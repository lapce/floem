# Window/UI Thread Split Plan

## Goal

Keep the main thread available for platform events and compositor commits while moving the maximum practical amount of UI state and UI work to a dedicated UI thread.

The main thread must remain able to commit compositor work even when style/layout/event work is expensive. In particular, compositor and external surfaces that explicitly opt into presenting without the Core Animation transaction must not be blocked behind UI style/layout work.

## Target Ownership

### Main Thread

The main thread owns only platform and compositor-runtime state:

- `winit::Window`, `WindowId`, monitor/scale/visibility/occlusion state needed by the platform.
- Frame source, frame tick receipt, timers, event-loop control flow, and deadline timers.
- `GpuResources`, scene renderer pool handles, and GPU polling.
- Subduction `LayerHost`, `LayerStore`, pending layer changes, pending scene publications, pending scene render jobs, and layer-host commit feedback.
- Compositor commit scheduling and present feedback routing.
- Platform side effects requested by UI code: cursor, IME, window title, menus, drag window, resize window, visibility, capture, etc.
- Independent compositor/external surface paths that can present without transaction.

### UI Thread

The UI thread owns view and reactive state:

- `WindowState` after main-thread compositor runtime state is extracted from it.
- Root view id, root view scope, `VIEW_STORAGE`, and per-window reactive work.
- Event routing through `GlobalEventCx`.
- Update message processing.
- Style, layout, box tree update/commit, and retained display-list recording.
- Composition plan production from the display list.
- Begin-frame callbacks and animation sampling.
- UI-side compositor surface placement/provider decisions.

The main thread should treat UI view ids as opaque ids. It should not borrow view state or route events directly after the split.

## Boundary Types

The split should be expressed with narrow message/artifact types before adding a real thread:

- `UiWindowDriver`: owns UI state and exposes the synchronous version of the future UI-thread API.
- `UiWindowEvent`: main-to-UI event input after platform reduction.
- `UiFrameRequest`: main-to-UI frame request containing timing, size/scale/theme changes, and demand.
- `UiFrameArtifact`: UI-to-main output containing a composition plan, timings/profile events, active surface placement metadata, and requested platform side effects.
- `PlatformRequest`: UI-to-main side effects such as cursor, IME, title, menu, drag/resize window, visibility, and capture requests.
- `CompositorRuntime`: main-owned runtime split out of current `WindowCompositor`.

## Required Refactors

### 1. Establish the UI Boundary In-Process

Introduce `UiWindowDriver` and move direct `WindowState` ownership behind it while still running on the main thread. This should be mechanical and behavior-preserving.

Success criteria:

- `WindowHandle` has an explicit UI-owned field instead of directly owning bare `WindowState`.
- Existing behavior and tests still pass.
- New direct `WindowState` access goes through the UI boundary, making future diffs easier to review.

### 2. Move Platform Side Effects Out of UI Update Processing

`process_update_messages` currently mutates both UI state and platform window state. Split those effects into `PlatformRequest`s.

Examples:

- `SetWindowTitle`
- `SetImeAllowed`
- `SetImeCursorArea`
- `ShowContextMenu`
- `WindowMenu`
- `DragWindow`
- `DragResizeWindow`
- `ToggleWindowMaximized`
- `SetWindowMaximized`
- `MinimizeWindow`
- `WindowVisible`
- `CaptureMetalFrame`
- `ToggleHud` if the HUD remains main-compositor-facing

Success criteria:

- UI processing can run without an owned `winit::Window`.
- Main applies platform requests after a UI turn.

### 3. Split `WindowCompositor`

Current `WindowCompositor` is mixed. It both consumes UI-derived composition plans and owns platform/subduction runtime state.

Split into:

- UI-produced `CompositionPlan` and compositor-surface placement metadata.
- Main-owned `CompositorRuntime` that owns `LayerHost`, `LayerStore`, surface ids, pending render/publication state, and commit logic.

Success criteria:

- `WindowState` no longer owns `LayerHost` or pending compositor commit/publication state.
- Main can commit independent compositor-surface content without entering UI state.

### 4. Add a Synchronous Adapter

Before using a real thread, make `WindowHandle` talk to `UiWindowDriver` through the same methods that will become channel messages.

Success criteria:

- Main/UI ordering is explicit.
- Stale frame artifacts can be dropped by generation.
- Resize and monitor changes carry generation ids.

### 5. Move UI Driver to a Dedicated Thread

Once the boundary compiles cleanly, move `UiWindowDriver` to a dedicated thread. The thread owns `VIEW_STORAGE` and the reactive runtime state for the window.

Success criteria:

- Main thread receives events, forwards UI events/frame requests, and keeps compositor commits responsive.
- UI thread produces frame artifacts asynchronously.
- Main can keep servicing `present_without_transaction` surfaces while UI is busy.

## Review Checklist While Working

- Does this code need `winit::Window` or Core Animation/Subduction calls? If not, it should trend toward UI-owned.
- Does this code borrow `VIEW_STORAGE`, route events, or mutate style/layout/paint state? If yes, it should trend toward UI-owned.
- Does this code commit, publish, or attach compositor resources? If yes, it should remain main-owned.
- Can a slow style/layout pass block an external surface that opted into present-without-transaction? If yes, the ownership is still wrong.
- Is there a synchronous call from main to UI in a path that must stay responsive? If yes, it needs a queued message or stale artifact handling.

## Current Progress

### Done

- Added `WindowUiDriver` as the explicit UI-owned in-process state owner.
- Moved bare `WindowState` ownership behind `WindowUiDriver`.
- Moved root view id and reactive scope behind `WindowUiDriver`.
- Added `PlatformRequest` plumbing for low-risk main-thread side effects.
- Moved `compositor.apply_plan(...)` out of `GlobalPaintCx`; paint now updates the `CompositionPlan`, and `WindowHandle` applies that plan to the compositor runtime.
- Added `UiFrameArtifact` as the in-process artifact boundary for compositor consumption.
- Moved `WindowCompositor` out of `WindowState`; `WindowHandle` now owns it as `compositor_runtime`.
- Added `CompositorRuntime` as the main-thread ownership name for the staged compositor runtime.
- Moved `WindowCompositorSurfaces` out of `WindowState`; `WindowHandle` now owns compositor-surface providers/content/frame-pull state.
- Changed compositor-surface user events to update main-owned compositor-surface state through `WindowHandle` methods.
- Changed frame rendering/commit paths to pass `UiFrameArtifact` explicitly into compositor work.
- Moved style/layout/box-tree frame preparation bodies onto `WindowUiDriver`; `WindowHandle` now delegates those UI-owned phases.
- Moved next-frame promotion and begin-frame callback execution behind `WindowUiDriver`.
- Moved UI dirty/prepare predicates (`needs_style`, layout/box-tree checks, pending box-tree updates, current prepare work) behind `WindowUiDriver`.
- Added a `WindowUiDriver::route_window_event` boundary so normal window-event routing no longer happens open-coded in `WindowHandle`.
- Added `UiFrameStatus` so compositor-critical frame paths consume a UI snapshot instead of borrowing `WindowState` fields directly.
- Moved `UpdateMessage` processing into `WindowUiDriver`; `WindowHandle` now receives only `UiUpdateOutcome` and `PlatformRequest`s for main-thread side effects.
- Removed the old proxy/deferred context-menu path after replacing it with `PlatformRequest`.
- Routed these update messages through `PlatformRequest`:
  - `DragWindow`
  - `FocusWindow`
  - `DragResizeWindow`
  - `ToggleWindowMaximized`
  - `SetWindowMaximized`
  - `MinimizeWindow`
  - `SetWindowDelta`
  - `SetWindowTitle`
  - `ShowContextMenu`
  - `WindowMenu`
  - `SetImeAllowed`
  - `SetImeCursorArea`
  - `Inspect`
  - `CaptureMetalFrame`
  - `WindowVisible`

### Next

- Expand `UiFrameArtifact` so frame preparation returns all UI-produced frame output instead of letting `WindowHandle` pull it from UI state.
- Continue splitting `WindowCompositor` internals now that ownership has moved to main.
- Move UI event/update driving behind `WindowUiDriver` methods instead of keeping those methods on `WindowHandle`.
- Add the real UI-thread worker/proxy now that frame/update loops have explicit UI outputs.
