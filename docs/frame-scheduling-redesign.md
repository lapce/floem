# Frame Scheduling Redesign

## Goal

Make Floem's scheduling model correct by separating four different concepts that are currently entangled:

1. Animation scheduling
2. Draw/frame scheduling
3. Timer scheduling
4. Update scheduling

Correctness is the primary goal. Lower main-thread blocking, especially around swapchain acquisition, is a direct consequence of getting the frame model right.

This document proposes major architectural and API changes. It intentionally prefers a clean model over source compatibility.

## Problem Statement

Today Floem mixes:

- immediate update requests via `UpdateMessage`
- next-frame requests via `WindowState::scheduled_updates`
- wall-clock timers in `ApplicationHandle::timers`
- animation callbacks via `exec_after_animation_frame`, which are implemented as timers
- redraw pacing via `paced_redraw_timers`, which are also implemented as timers

These mechanisms are not equivalent, but they are routed through overlapping infrastructure. That causes correctness problems:

- There is no single authoritative per-window frame lifecycle.
- Animation callbacks are not real begin-frame callbacks.
- Redraw pacing is not a first-class clock. It is a special timer.
- `scheduled_updates` is a lossy, untyped "do this later" queue.
- Views can bypass pacing and directly request redraw.
- GPU rendering acquires the surface texture too early, so main-thread work can block on swapchain availability before the frame has even been built.

## Existing Model

### Immediate update path

Views call APIs like:

- `request_style`
- `request_layout`
- `request_box_tree_update`
- `request_box_tree_commit`
- `request_paint`

These enqueue `UpdateMessage`s. `WindowHandle::process_update_messages` converts them into `WindowState` dirtiness.

This is the "do it as soon as the next app update runs" path.

### Scheduled update path

Other code calls:

- `schedule_style`
- `schedule_layout`
- `schedule_box_tree_commit`
- `schedule_paint`

These push `FrameUpdate`s into `WindowState::scheduled_updates`.

Later, `WindowHandle::process_scheduled_updates` converts them back into the same dirtiness state used by immediate requests.

This is the "do it later" path, but it does not correspond to a principled frame phase. It is just another queue feeding the same work.

### Timer path

`ApplicationHandle` owns a global timer table. `exec_after` creates a timer with a wall-clock deadline. `exec_after_animation_frame` also creates a timer, but sets its deadline to `now + frame_duration_for_window`.

So "timer" and "next frame" are not actually separate abstractions in the runtime.

### Draw scheduling path

After update processing, `ApplicationHandle` decides whether a window needs a frame. If so, it either:

- directly calls `window.request_redraw()`, or
- arms a paced redraw timer that later wakes the update loop

This means draw scheduling lives above the window, but draw dirtiness lives inside the window. The coordination is heuristic rather than contractual.

## Conceptual Separation We Need

These need to become distinct systems.

### 1. Update scheduling

Purpose:

- process messages
- flush reactive work
- compute style/layout/box tree dirtiness

This is state convergence. It should not itself imply a draw deadline or an animation clock.

### 2. Animation scheduling

Purpose:

- advance semantic time for transitions, animations, drag settle effects, cursor blink if frame-tied, and user `requestAnimationFrame` callbacks

This should be driven by a begin-frame opportunity, not by wall-clock timers pretending to be frames.

### 3. Timer scheduling

Purpose:

- fire wall-clock deadlines
- wake the app when time-based work becomes eligible

This is independent of display refresh. Timers may cause state changes that mark visual work dirty, but a timer is not a frame.

### 4. Draw/frame scheduling

Purpose:

- decide when to ask the platform for a frame
- deliver begin-frame callbacks
- choose commit deadlines
- acquire the output target only when ready to submit
- observe present feedback

This should be a first-class per-window subsystem.

## Required Invariants

The redesign should enforce these invariants.

### Invariant 1: one authoritative per-window frame coordinator

Each window must have exactly one owner for:

- whether a frame is needed
- whether a frame is already requested
- whether a frame callback is in flight
- the current predicted present time and deadline
- the last presentation feedback

No other code path should directly call `window.request_redraw()` except through this coordinator.

### Invariant 2: wall-clock timers never masquerade as begin-frame

`set_timeout` and `set_interval` are wall-clock mechanisms.
`request_animation_frame` is a frame-clock mechanism.

They may wake the same outer event loop, but they must remain distinct in semantics and storage.

### Invariant 3: frame callbacks run from begin-frame

Animation callbacks must receive a semantic frame time from a real frame opportunity. They should not be synthesized by a timer using a guessed monitor interval.

### Invariant 4: updates mark pending work, not redraw directly

Style/layout/paint APIs should only mark work pending. They should not schedule redraw themselves. The frame coordinator decides when a platform redraw should be requested.

### Invariant 5: surface acquisition is late

Swapchain or present-target acquisition must happen after:

- updates are converged
- animation has been advanced
- scene / render plan has been built

The output target should be acquired only when the frame is actually ready to composite/submit.

### Invariant 6: presentation feedback informs pacing

Heuristics based on `last_presented_at` are acceptable only as a fallback. When platform feedback exists, pacing should be driven by predicted present time and actual presentation feedback.

## Proposed Architecture

Introduce a first-class `FrameCoordinator` per window.

The `FrameCoordinator` is Floem-owned, but its timing source and pacing model should be designed so that `subduction` can drive it directly.

Concretely:

- Floem owns update convergence, style/layout, view state, scene generation, and renderer integration.
- `subduction` should own frame ticks, present hints, frame planning, and present feedback adaptation.
- The coordinator is the bridge between those worlds.

```rust
pub struct FrameCoordinator {
    phase: FramePhase,
    frame_requested: bool,
    frame_in_flight: bool,
    begin_frame_callbacks: Vec<FrameCallback>,
    pending_work: PendingWork,
    timer_state: TimerState,
    pacing: PacingState,
    presentation: PresentationState,
}
```

In the long-term target, `pacing` is not a Floem-invented heuristic type. It is effectively a thin wrapper around subduction's:

- `FrameTick`
- `PresentHints`
- `FramePlan`
- `PresentFeedback`
- `Scheduler`

### `FramePhase`

```rust
pub enum FramePhase {
    Idle,
    BeginFrame,
    Animate,
    Update,
    LayoutAndPrePaint,
    BuildRenderPlan,
    AcquireTarget,
    CompositeAndPresent,
}
```

This is primarily for internal correctness and assertions.

### `PendingWork` and `NextFrameWork`

The distinction between:

- dirty in the current convergence cycle
- dirty at the next frame opportunity

is real and necessary.

Floem currently runs update work in a loop until quiescent. Because of that, simply "marking dirty" is not enough for animations and other frame-driven work: some work must be deferred so it does not get pulled back into the same update loop iteration.

The redesign should preserve that distinction, but model it explicitly.

Replace `scheduled_updates: Vec<FrameUpdate>` with two typed structures:

```rust
pub struct PendingWork {
    pub needs_message_flush: bool,
    pub style_dirty: FxHashMap<ViewId, StyleReason>,
    pub needs_layout: bool,
    pub needs_box_tree_from_layout: bool,
    pub views_needing_box_tree_update: FxHashSet<ViewId>,
    pub needs_box_tree_commit: bool,
    pub dirty_paint_elements: FxHashSet<ElementId>,
    pub pending_damage_rects: Vec<Rect>,
}

pub struct NextFrameWork {
    pub style_dirty: FxHashMap<ViewId, StyleReason>,
    pub needs_layout: bool,
    pub needs_box_tree_from_layout: bool,
    pub views_needing_box_tree_update: FxHashSet<ViewId>,
    pub needs_box_tree_commit: bool,
    pub dirty_paint_elements: FxHashSet<ElementId>,
    pub begin_frame_callbacks: Vec<FrameCallback>,
}
```

`PendingWork` is eligible in the current update loop.
`NextFrameWork` is promoted into `PendingWork` only at the next begin-frame boundary.

This preserves the semantics of "schedule for next frame" without treating it as an unstructured queue.

### Why this split matters

If an active animation calls "schedule style for next frame", that must not just set `style_dirty` immediately or the current update loop will continue stepping the animation in the same turn until it converges or hits a guardrail.

So the correct model is not:

- one dirty bitset

It is:

- `PendingWork`: current-frame convergence
- `NextFrameWork`: deferred until the next frame opportunity

### API shape for deferral

That means Floem still needs two classes of APIs:

- request now
- request next frame

but they should be explicit about phase semantics.

Suggested names:

```rust
pub fn request_style_now(&mut self, id: ViewId, reason: StyleReason);
pub fn request_style_next_frame(&mut self, id: ViewId, reason: StyleReason);
```

and similarly for layout / box-tree / paint where needed.

Public view-facing API does not necessarily need to expose both families, but internally the engine absolutely does.

The important change is that "next frame" should mean:

- place work in `NextFrameWork`
- ensure the frame coordinator requests a future frame opportunity

not:

- push an opaque `FrameUpdate` into a shared vector

### Promotion rule

At begin-frame:

1. move `NextFrameWork` into `PendingWork`
2. run begin-frame callbacks
3. animate
4. converge updates until quiescent

That gives correct "one step per frame opportunity" behavior.

### `TimerState`

Wall-clock timers remain their own structure:

```rust
pub struct TimerState {
    pub timers: BTreeMap<TimerDeadline, SmallVec<[TimerId; 4]>>,
    pub entries: HashMap<TimerId, TimerEntry>,
}

pub enum TimerKind {
    Timeout,
    Interval,
}
```

This state can stay application-global or move per window plus a small app-global table for non-window timers.

### `PacingState`

Pacing should stop being "redraw timer". It should instead track the current frame model:

```rust
pub struct PacingState {
    pub last_tick: Option<FrameTick>,
    pub last_feedback: Option<PresentFeedback>,
    pub scheduler: FrameScheduler,
    pub visibility: VisibilityState,
}
```

The naming here intentionally aligns with the subduction model.

The intended end state is:

- `FrameTick`, `PresentHints`, `FramePlan`, and `PresentFeedback` come from `subduction` types directly, or from a local compatibility layer that matches them exactly.
- `FrameScheduler` is subduction's scheduler or a thin adapter around it.

In other words, this is where subduction fits in to drive timing.

### `PresentationState`

```rust
pub struct PresentationState {
    pub target_state: PresentTargetState,
    pub last_submission: Option<PendingPresentationFeedback>,
    pub output_size_px: (u32, u32),
}
```

This owns output target lifecycle and feedback, not general update logic.

## Proposed Frame Lifecycle

Each window frame should follow this sequence:

1. `BeginFrame`
2. `Animate`
3. `Update`
4. `LayoutAndPrePaint`
5. `BuildRenderPlan`
6. `AcquireTarget`
7. `CompositeAndPresent`
8. `ObserveFeedback`

### 1. BeginFrame

Input:

- frame tick from the backend or redraw callback
- predicted present time if known
- previous actual present if known

Actions:

- compute present hints
- plan semantic frame time and commit deadline
- store current frame plan

Long-term, this phase should be driven by subduction:

- platform backend produces `FrameTick`
- subduction computes `PresentHints`
- subduction scheduler produces `FramePlan`
- Floem consumes the result

### 2. Animate

Actions:

- run `requestAnimationFrame` callbacks for this window
- advance transitions and animations using `FramePlan::semantic_time`
- mark any resulting style/layout/paint dirtiness

This is where `exec_after_animation_frame` semantics belong.

### 3. Update

Actions:

- process update messages
- drain runtime/reactive work up to policy
- apply timer-fired state changes

This phase is about converging application state. It should not acquire any present target.

### 4. LayoutAndPrePaint

Actions:

- style
- layout
- box-tree update
- box-tree commit
- produce final visual tree state
- compute paint damage / render plan invalidation

### 5. BuildRenderPlan

Actions:

- produce a retained scene or explicit render plan
- prepare offscreen surfaces or layer targets

This is the key place where Floem should move away from rendering directly into the window target.

### 6. AcquireTarget

Actions:

- acquire swapchain image or CPU present buffer only now

This is the main correction for the current GPU path.

### 7. CompositeAndPresent

Actions:

- composite offscreen results into the acquired output target
- submit
- present

### 8. ObserveFeedback

Actions:

- create feedback object from submission timestamps and actual/predicted present info
- feed the scheduler
- clear `frame_requested` / update `frame_in_flight`

## API Changes

### `action.rs`

Current:

- `exec_after`
- `exec_after_animation_frame`

Proposed:

```rust
pub fn set_timeout(duration: Duration, f: impl FnOnce(TimeoutId) + 'static) -> TimeoutId;
pub fn set_interval(duration: Duration, f: impl FnMut(IntervalId) + 'static) -> IntervalId;
pub fn cancel_timeout(id: TimeoutId);
pub fn cancel_interval(id: IntervalId);

pub fn request_animation_frame(f: impl FnOnce(FrameTime) + 'static) -> AnimationFrameId;
pub fn cancel_animation_frame(id: AnimationFrameId);
```

Notes:

- `request_animation_frame` should be window-scoped and invalid outside a window context.
- The callback should receive semantic frame time, not a timer token.
- `exec_after_animation_frame` should be deprecated and removed.

### `view` / `window_state` request APIs

Keep request APIs, but redefine semantics:

- `request_style`
- `request_layout`
- `request_box_tree_update`
- `request_box_tree_commit`
- `request_paint`

These only mark `PendingWork`. They do not trigger redraw directly.

Remove:

- `schedule_style`
- `schedule_layout`
- `schedule_box_tree_commit`
- `schedule_paint`
- `WindowHandle::schedule_repaint`

Replace their use sites with either:

- immediate dirtiness, if the work belongs to the current convergence cycle
- explicit `*_next_frame` APIs, if the work must happen on the next frame boundary
- `request_animation_frame`, if the work is a begin-frame callback rather than deferred dirtiness

### `WindowHandle`

Current `WindowHandle` combines:

- state convergence
- input dispatch
- render timing accumulation
- painting
- redraw triggering

Split it into clearer responsibilities:

```rust
pub struct WindowHandle {
    pub window: Arc<dyn Window>,
    pub state: WindowState,
    pub frame: FrameCoordinator,
    pub renderer: WindowRendererHost,
    pub input: InputRouter,
}
```

`WindowHandle` can still own them, but code should stop mixing their responsibilities.

### `ApplicationHandle`

Current `ApplicationHandle` owns:

- global timers
- paced redraw timers
- control flow
- window iteration budget

Proposed:

- keep app-global timer wakeup scheduling
- remove `paced_redraw_timers`
- make control flow derive from:
  - earliest wall-clock timer
  - whether any window has a pending frame deadline or queued frame callback

`ApplicationHandle` should not own frame pacing policy anymore. It should only multiplex wakeups from:

- wall-clock timers
- per-window frame coordinators

The per-window frame coordinators are where the subduction-backed timing model plugs in.

The app should ask each window for:

```rust
pub enum WakeupRequest {
    None,
    At(Instant),
    Immediate,
}
```

Then the app computes the earliest wakeup across all windows plus timers.

## Renderer Changes

The renderer boundary must change if we want correct late acquisition.

### Current renderer boundary

Current GPU rendering API effectively means:

1. acquire window surface texture
2. render content into it
3. present

That is wrong for minimizing main-thread stalls.

### Proposed renderer boundary

Change the rendering model to:

1. build a render plan or scene
2. render into offscreen targets
3. acquire output target
4. composite/blit into output target
5. present

Suggested trait split:

```rust
pub trait SceneRenderer {
    type PreparedFrame;

    fn resize(&mut self, width: u32, height: u32);
    fn prepare_frame(&mut self, scene: &Scene, size: Size) -> Self::PreparedFrame;
}

pub trait WindowPresenter {
    fn acquire_target(&mut self) -> Result<PresentTarget, PresentError>;
    fn present_prepared(
        &mut self,
        prepared: &PreparedFrameHandle,
        target: PresentTarget,
    ) -> PresentResult;
}
```

In practice, a combined type may implement both traits, but the phase split matters.

### GPU path

Preferred design:

- render all app content into an offscreen texture or a retained set of layer textures
- only then call `get_current_texture()`
- perform a final composite pass into the surface texture

This minimizes the time between surface acquisition and present.

### CPU path

Current CPU path is already much closer:

- render into scratch RGBA image
- acquire softbuffer buffer
- copy and present

This can be generalized behind the same presenter interface.

## Video-Capable Core Plan

Floem core should not implement video playback policy or frame-selection algorithms.

Those belong in a video widget or media subsystem built on top of core.

What core must do is provide the timing, frame, visibility, and presentation primitives that such a widget would need. The Chrome "Project Butter" design is still useful here, but as a guide for what interfaces core must expose, not for what algorithms core must own.

### Core lessons to adopt

#### 1. Core must support pull-from-frame behavior

A video widget should be able to choose a frame from the begin-frame/render path instead of being forced into a push model.

Core therefore needs a frame pipeline that can:

- deliver begin-frame opportunities
- expose the current presentation interval
- let widgets prepare content for that interval

#### 2. Core must expose interval semantics

Video widgets need more than "current time".

Core should expose a frame interval object like:

```rust
pub struct PresentationInterval {
    pub deadline_min: Instant,
    pub deadline_max: Instant,
    pub predicted_present: Option<Instant>,
    pub background_rendering: bool,
}
```

This should come from the frame coordinator, derived from subduction timing.

#### 3. Core must expose visibility-aware frame modes

Core should make it possible for a widget to know whether it is in:

- normal visible begin-frame-driven mode
- background / fallback timer-driven mode

Core owns that scheduling mode switch. A video widget can then decide what playback behavior to use.

#### 4. Core must expose frame outcome feedback

A video widget needs to know whether the frame it prepared:

- was considered for draw
- actually drew
- likely missed deadline

Core should expose:

```rust
pub struct FrameOutcome {
    pub draw_attempted: bool,
    pub draw_completed: bool,
    pub missed_deadline: Option<bool>,
}
```

That lets a widget implement cadence/coverage/drift or any other policy outside core.

### Core responsibilities for video enablement

Floem core should provide:

- begin-frame callbacks
- subduction-driven frame timing and deadlines
- `PresentationInterval`
- visibility/background frame mode signaling
- `FrameOutcome`
- late-acquire presentation so widgets can prepare content before output-target acquisition

Floem core should not provide:

- cadence estimators
- coverage/drift frame choice
- A/V sync policy
- decoder/media-clock policy

### Required hooks for a future video widget

At minimum, core should make a future widget able to do this:

1. receive begin-frame with interval data
2. select or synthesize a frame for that interval
3. attach that frame to the scene/render plan
4. learn whether it actually got drawn
5. fall back to background cadence when no visible begin-frame arrives

### Required migration work in core

#### Phase V1: define timing/output primitives

- add `PresentationInterval`
- add `FrameOutcome`
- expose begin-frame callbacks with interval data

#### Phase V2: add visibility-aware scheduling modes

- visible widgets get begin-frame-driven opportunities
- invisible widgets can opt into background timer-driven opportunities

#### Phase V3: preserve pull-based content preparation

- ensure widgets can prepare frame content during the frame pipeline
- ensure output-target acquisition remains late

#### Phase V4: add telemetry hooks

Core should expose enough instrumentation that a future video widget can record:

- missed-frame counts
- draw suppression
- frame deadlines
- actual-present feedback when available

## Blink-Inspired Model

The important Blink idea to borrow is not specific class names. It is the separation between:

- begin-frame delivery
- semantic frame time
- lifecycle/update phases
- commit deadlines
- actual presentation feedback

Floem should adopt the same logical split:

- a frame source delivers opportunities
- a scheduler plans work against deadlines
- lifecycle work converges state
- rendering builds output independent of the final present target
- presentation happens late

## Subduction Fit

Subduction is a strong fit for the timing and presentation architecture.

### What maps cleanly

Subduction already has:

- `FrameTick`
- `PresentHints`
- `FramePlan`
- `PresentFeedback`
- `Scheduler`
- `Presenter`

Those concepts map almost exactly to what Floem needs.

Floem should strongly consider adopting this model directly instead of reinventing a parallel one.

### Exact ownership split

This is the intended division of responsibility.

#### Floem owns

- update/message convergence
- reactive draining policy
- style/layout/box-tree work
- visual dirtiness tracking
- scene building / render plan construction
- renderer backend integration
- window-local bridge logic in `FrameCoordinator`

#### Subduction owns

- frame opportunity delivery model
- timing confidence model
- predicted present / commit deadline calculation
- semantic frame planning
- presentation feedback processing
- pacing adaptation logic

#### Boundary

The boundary between them should look like:

```rust
pub trait FrameClockBackend {
    fn poll_tick(&mut self) -> Option<FrameTick>;
    fn compute_present_hints(&self, tick: &FrameTick) -> PresentHints;
    fn observe_feedback(&mut self, feedback: &PresentFeedback);
}
```

Floem's `FrameCoordinator` would then:

1. receive a `FrameTick`
2. obtain `PresentHints`
3. ask the scheduler for a `FramePlan`
4. run Floem lifecycle/update/render phases
5. submit/present
6. feed `PresentFeedback` back

That is the concrete place where subduction drives timing.

### Recommended use of subduction

#### Timing and scheduling

Use subduction's timing model directly or mirror it closely.

This is the highest-confidence adoption area.

This is not just inspiration. It should be the intended driver for:

- `request_animation_frame`
- frame deadlines
- pacing adaptation
- actual-present feedback handling

#### GPU presenter

A `subduction_backend_wgpu` style presenter is a good fit for Floem if Floem moves to:

- offscreen layer or surface textures
- final composition into the window target

This would let Floem separate scene generation from final output submission.

#### CPU presenter

This likely requires new work upstream or in Floem:

- a CPU presenter abstraction
- likely a software surface presenter backend analogous to softbuffer

So subduction is not yet a full replacement for all current present paths unless that work is added.

#### Platform backends

If the upstream checkout has the Windows backend, that strengthens the case for converging on subduction's platform timing/presenter abstractions.

Even then, Floem should treat subduction as:

- the timing and presenter substrate

not necessarily:

- the complete rendering engine

## Migration Plan

### Phase 0: stop making it worse

- forbid new direct `request_redraw()` call sites outside the frame coordinator
- forbid new uses of `exec_after_animation_frame`
- stop adding new `schedule_*` call sites

### Phase 1: introduce frame coordinator types

- add `FrameCoordinator`
- add `PendingWork`
- add a `request_animation_frame` API
- route redraw requests through the coordinator
- add a frame-clock abstraction whose target implementation is subduction-backed

No renderer changes yet.

### Phase 2: remove scheduled update queue

- delete `WindowState::scheduled_updates`
- convert all `schedule_*` users
- move transition/animation re-arming to `request_animation_frame`

### Phase 3: late-acquire rendering

- split scene building from present target acquisition
- change the GPU renderer to offscreen-first
- keep CPU on scratch-image-first

This is the phase that directly addresses swapchain blocking.

### Phase 4: adopt present feedback-driven pacing

- replace heuristic redraw deadline logic with frame-plan / feedback logic
- make subduction the timing engine behind the frame coordinator
- use subduction `FrameTick` / `PresentHints` / `Scheduler` / `PresentFeedback`
- keep heuristic fallback only for platforms that do not yet have the backend wired

### Phase 5: presenter unification

- unify CPU and GPU present under a `WindowPresenter` abstraction
- optionally converge on subduction presenters where feasible

## Concrete Type Sketch

This is one possible shape for the new APIs.

```rust
pub struct FrameTime {
    pub semantic_time: Instant,
    pub predicted_present: Option<Instant>,
    pub commit_deadline: Instant,
    pub frame_index: u64,
}

pub struct WindowFramePlan {
    pub time: FrameTime,
    pub should_render: bool,
}

pub struct PendingWork {
    pub style_dirty: FxHashMap<ViewId, StyleReason>,
    pub needs_layout: bool,
    pub needs_box_tree_update: bool,
    pub views_needing_box_tree_update: FxHashSet<ViewId>,
    pub needs_box_tree_commit: bool,
    pub dirty_paint_elements: FxHashSet<ElementId>,
    pub pending_damage_rects: Vec<Rect>,
}

pub struct FrameCoordinator {
    pub pending_work: PendingWork,
    pub raf_callbacks: Vec<(AnimationFrameId, Box<dyn FnOnce(FrameTime)>)>,
    pub frame_requested: bool,
    pub current_plan: Option<WindowFramePlan>,
    pub scheduler: SchedulerLike,
}

impl FrameCoordinator {
    pub fn mark_visual_dirty(&mut self);
    pub fn request_animation_frame(&mut self, cb: Box<dyn FnOnce(FrameTime)>)
        -> AnimationFrameId;
    pub fn on_begin_frame(&mut self, tick: FrameTick) -> WindowFramePlan;
    pub fn should_request_platform_frame(&self) -> bool;
}
```

## Expected Benefits

- Correct separation of semantics
- Fewer accidental feedback loops between updates, timers, and drawing
- Better visibility handling
- Better pacing under real presentation feedback
- Less main-thread blocking on swapchain acquisition
- Cleaner platform abstraction boundary
- Easier integration with subduction-style backends

## Non-Goals

- Preserving current internal API shape
- Minimizing refactor size
- Keeping `schedule_*` semantics for compatibility

Those would work against the core goal here.

## Recommended Immediate Next Steps

1. Introduce `request_animation_frame` and convert transition re-arming to use it.
2. Add a per-window `FrameCoordinator` that becomes the only owner of redraw requests.
3. Remove direct redraw calls from update code paths.
4. Redesign the GPU renderer boundary so `get_current_texture()` happens after scene preparation.
5. Evaluate whether to adopt subduction's scheduler types directly in-tree or via a dependency.
