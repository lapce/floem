# External Surfaces and Compositor Integration

## Goal

Add OS-composited and compositor-managed external surfaces to Floem without
making the feature video-specific.

Video is the motivating use case because smooth playback requires frame timing,
visibility, presentation feedback, and low-copy texture/image presentation.
However, the primitive should also support other external content such as native
controls, web views, camera previews, maps, embedded renderers, and GPU texture
producers.

The core design principle is:

> An external surface is ordered like a draw command, but presented like a
> compositor layer.

## Non-Goals

- Floem core should not implement video cadence, A/V sync, decoder policy, or
  frame-selection algorithms.
- `imaging` should not learn about OS compositor layers.
- Paint should not create native resources.
- Video worker threads should not mutate Floem view, display-list, or compositor
  state directly.

## High-Level Data Flow

```text
View tree
  -> style/layout/box tree
  -> retained display list
  -> composition lowering
  -> compositor diff
  -> subduction LayerStore diff
  -> platform presenter
  -> present
  -> frame outcome feedback
```

Normal rendering remains the fast path:

```text
No external surfaces:
  retained display list -> imaging sink -> window surface
```

When external surfaces are present:

```text
Has external surfaces:
  retained display list
    -> ordered CompositionItems
    -> render normal drawing chunks into compositor surfaces
    -> map external surfaces into compositor layers
    -> compositor/platform present
```

## Core Concepts

### ExternalSurface

`ExternalSurface` is a stable handle to a window-local content slot.

It has identity and content, but no paint position by itself.

```rust
pub struct ExternalSurface {
    id: ExternalSurfaceId,
}

pub struct ExternalSurfaceConfig {
    pub kind: ExternalSurfaceKind,
    pub alpha_mode: AlphaMode,
    pub color_space: Option<ColorSpace>,
    pub preferred_format: Option<SurfaceFormat>,
}

pub enum ExternalSurfaceKind {
    NativeTexture,
    WgpuTexture,
    CpuImageFallback,
}
```

Creation should happen outside paint, for example during view construction or
view setup:

```rust
let surface = create_external_surface(ExternalSurfaceConfig::video());
```

### Paint Placement

Paint places an existing external surface at a precise paint-order position.

```rust
impl PaintCx<'_> {
    pub fn draw_external_surface(
        &mut self,
        surface: &ExternalSurface,
        rect: Rect,
        options: ExternalSurfacePaintOptions,
    );
}

pub struct ExternalSurfacePaintOptions {
    pub opacity: f32,
    pub hit_test: bool,
}
```

Example:

```rust
impl View for VideoView {
    fn paint(&mut self, cx: &mut PaintCx) {
        cx.draw_external_surface(
            &self.surface,
            cx.layout_rect_local,
            ExternalSurfacePaintOptions::default(),
        );
    }
}
```

### Content

The compositor should only need texture/image-like content.

```rust
pub enum ExternalSurfaceContent {
    Empty,
    Texture(ExternalTexture),
    Image(ImageData),
}
```

No `submit_video_frame` API is needed. Video is responsible for selecting,
decoding, and uploading/wrapping frames into one of the supported content forms.

## Display List Integration

External surfaces should be represented in the retained display list at the same
ordering granularity as a single draw.

Conceptually:

```rust
enum FloemDisplayCommand {
    PushContext(ContextId),
    PopContext,
    PushClip(ClipId),
    PopClip,
    PushGroup(GroupId),
    PopGroup,
    Draw(DrawId),
    ExternalSurface(ExternalSurfaceId),
}
```

This does not require changing `imaging::record::Scene`. `imaging::Scene`
remains the drawing language. Floem's display list owns paint order and can
retain external surface commands alongside normal drawing.

The invariant is:

> `ExternalSurface` is a paint-order barrier.

That means:

- normal draw commands before it may be grouped together
- normal draw commands after it may be grouped together
- normal draw commands must never be reordered across it
- active clips/groups/context annotations at the command apply to the external
  surface placement
- compositor layer order must match lowered display-list order exactly

## Composition Lowering

The display-list lowering pass walks retained paint order and emits
composition items.

```rust
pub struct CompositionPlan {
    pub items: Vec<CompositionItem>,
}

pub enum CompositionItem {
    Scene(SceneLayer),
    ExternalSurface(ExternalSurfaceLayer),
}

pub struct SceneLayer {
    pub key: CompositionKey,
    pub scene: imaging::record::Scene,
    pub bounds: Rect,
    pub transform: Affine,
    pub clip: Option<RoundedRect>,
    pub opacity: f32,
}

pub struct ExternalSurfaceLayer {
    pub key: CompositionKey,
    pub surface_id: ExternalSurfaceId,
    pub rect: Rect,
    pub transform: Affine,
    pub clip: Option<RoundedRect>,
    pub opacity: f32,
}
```

Lowering algorithm:

```rust
for command in display_order {
    match command {
        normal_imaging_command => {
            current_scene_chunk.push(command);
        }
        ExternalSurface(surface_id) => {
            flush_current_scene_chunk_as_scene_layer();
            emit_external_surface_layer(surface_id, active_state);
        }
    }
}

flush_current_scene_chunk_as_scene_layer();
```

Example:

```text
Draw(background)
ExternalSurface(video)
Draw(caption)
```

lowers to:

```text
Layer 0: imaging chunk containing background
Layer 1: external video surface
Layer 2: imaging chunk containing caption
```

## Compositor Diff

Compositor integration should be diff-driven. Floem should not rebuild the
native layer tree every frame.

There are two diffs:

- Floem composition diff: `CompositionPlan` to stable compositor layers.
- Subduction layer diff: `LayerStore::evaluate()` to platform presenter
  changes.

Subduction already owns the second part:

```rust
let changes = layer_store.evaluate();
presenter.apply(&layer_store, &changes);
```

Floem needs the first part.

```rust
struct WindowCompositor {
    layer_store: subduction_core::layer::LayerStore,
    layers_by_key: HashMap<CompositionKey, LayerId>,
    previous_order: Vec<CompositionKey>,
    scene_surfaces: HashMap<CompositionKey, SceneSurface>,
    external_surfaces: HashMap<ExternalSurfaceId, ExternalSurfaceEntry>,
}
```

Stable keys are required to avoid destroy/recreate churn.

```rust
pub enum CompositionKey {
    SceneChunk {
        owner: ElementId,
        stage: PaintStage,
        chunk_index: u32,
    },
    ExternalSurface(ExternalSurfaceId),
}
```

Apply flow:

```rust
fn apply_plan(&mut self, plan: CompositionPlan) {
    remove_layers_not_in_new_plan();
    create_layers_for_new_items();
    update_changed_layer_properties();
    reorder_layers_to_match_plan();

    let changes = self.layer_store.evaluate();
    self.presenter.apply(&self.layer_store, &changes);
}
```

Layer properties should be updated only when changed:

```rust
if old.bounds != new.bounds {
    layer_store.set_bounds(layer, new.bounds);
}
if old.transform != new.transform {
    layer_store.set_transform(layer, new.transform);
}
if old.clip != new.clip {
    layer_store.set_clip(layer, new.clip);
}
if old.opacity != new.opacity {
    layer_store.set_opacity(layer, new.opacity);
}
if old.surface != new.surface {
    layer_store.set_content(layer, Some(surface_id));
}
```

The compositor should have one root layer per window, with composition items as
children ordered by paint order:

```text
window compositor root
  child 0: scene chunk
  child 1: external video surface
  child 2: overlay scene chunk
```

If subduction does not expose an efficient `set_child_order` API, add one or
implement reordering by remove/reinsert as an initial implementation.

## External Surface Registry

`ExternalSurface` communicates with the compositor through a window-local
registry, not by directly calling platform presenter APIs.

```rust
struct ExternalSurfaceEntry {
    config: ExternalSurfaceConfig,
    content: ExternalSurfaceContent,
    dirty: ExternalSurfaceDirty,
    last_outcome: Option<FrameOutcome>,
}
```

Producer-side APIs update this registry:

```rust
impl ExternalSurface {
    pub fn submit_texture(&self, texture: ExternalTexture);
    pub fn submit_image(&self, image: ImageData);
    pub fn clear(&self);
    pub fn request_frame(&self);
}
```

Internally:

```text
submit_texture()
  -> update ExternalSurfaceEntry.content
  -> mark surface content dirty
  -> request frame/render
```

The compositor consumes the registry during plan application:

```rust
match registry.content(surface_id) {
    ExternalSurfaceContent::Empty => hide_layer(),
    ExternalSurfaceContent::Texture(texture) => bind_texture_to_layer(texture),
    ExternalSurfaceContent::Image(image) => upload_or_render_image_to_layer(image),
}
```

## Threaded Producers

Video decoding/upload may run on another thread. That thread must not touch
Floem display-list or compositor state directly.

Use a sendable surface handle:

```rust
#[derive(Clone)]
pub struct ExternalSurfaceHandle {
    id: ExternalSurfaceId,
    proxy: AppUpdateProxy,
}

impl ExternalSurfaceHandle {
    pub fn submit_texture(&self, texture: ExternalTexture);
    pub fn submit_image(&self, image: ImageData);
    pub fn clear(&self);
    pub fn request_frame(&self);
}
```

Those methods post app/window events:

```rust
enum AppUpdateEvent {
    ExternalSurfaceContent {
        surface_id: ExternalSurfaceId,
        content: ExternalSurfaceContent,
    },
    ExternalSurfaceRequestFrame {
        surface_id: ExternalSurfaceId,
    },
}
```

UI thread handling:

```text
receive ExternalSurfaceContent
  -> registry[surface_id].content = content
  -> mark dirty
  -> request frame
```

## Video Presentation Model

Project Butter recommends that visible video presentation be pulled by the
compositor/frame pipeline, not pushed by a video thread.

In Floem terms:

```text
video decode/upload thread
  -> maintain queue of ready textures/images

Floem frame pipeline
  -> finds visible external surface placement
  -> gives video provider presentation interval
  -> provider selects best ready texture/image
  -> compositor presents selected content
  -> provider receives outcome feedback
```

The provider API should look like:

```rust
pub trait ExternalSurfaceProvider {
    fn update_current_content(
        &mut self,
        args: ExternalSurfaceFrameArgs,
    ) -> ExternalSurfaceFrameUpdate;

    fn current_content(&self) -> Option<ExternalSurfaceContent>;

    fn release_current_content(&mut self, outcome: ExternalSurfaceOutcome);
}

pub struct ExternalSurfaceFrameArgs {
    pub surface_id: ExternalSurfaceId,
    pub interval: PresentationInterval,
    pub visible: bool,
    pub rect: Rect,
    pub size_px: Size,
    pub previous_outcome: Option<ExternalSurfaceOutcome>,
}

pub struct ExternalSurfaceFrameUpdate {
    pub content_changed: bool,
    pub request_next_frame: bool,
}

pub struct ExternalSurfaceOutcome {
    pub surface_id: ExternalSurfaceId,
    pub frame_index: u64,
    pub visible: bool,
    pub outcome: FrameOutcome,
}
```

Video selection policy stays outside Floem core:

```rust
impl ExternalSurfaceProvider for VideoSurface {
    fn update_current_content(
        &mut self,
        args: ExternalSurfaceFrameArgs,
    ) -> ExternalSurfaceFrameUpdate {
        self.current = self.algorithm.select_texture(
            args.interval.deadline_min,
            args.interval.deadline_max,
            args.interval.predicted_present,
            args.previous_outcome,
            &self.uploaded_frames,
        );

        ExternalSurfaceFrameUpdate {
            content_changed: self.current.is_some(),
            request_next_frame: self.is_playing(),
        }
    }

    fn current_content(&self) -> Option<ExternalSurfaceContent> {
        self.current.clone().map(ExternalSurfaceContent::Texture)
    }

    fn release_current_content(&mut self, outcome: ExternalSurfaceOutcome) {
        self.algorithm.observe(outcome);
    }
}
```

The algorithm may implement Project Butter's priority order:

1. cadence-based selection when stable
2. coverage-based selection for the presentation interval
3. drift-based selection when no frame overlaps the interval

Floem core only supplies timing, visibility, placement, and outcome feedback.
If a provider returns `request_next_frame`, Floem keeps scheduling frame work so
visible video is pulled by the compositor clock instead of by a sleeping video
thread.
If a provider returns `content_changed` and `current_content()` returns `None`,
Floem clears the surface content.

Provider pulls are not ordinary paint invalidation. They are frame-scheduler
work:

- With a display-link/subduction frame clock, each external tick may advance the
  provider pull if the window has external-surface frame work.
- With the heuristic frame clock, provider pulls arm a paced update wake.
- If only provider content changes, Floem applies a compositor diff and does
  not rerender the main window surface.
- If retained Floem content above an external surface changes, Floem rerenders
  only the promoted compositor-owned scene layer when the window prefix before
  the first external surface is unchanged.
- Promoted compositor-owned scene layers are rendered through the same renderer
  backend instance as the main window. For the threaded window renderer this is
  the existing render worker; GPU results are blitted into the layer surface,
  and CPU results are uploaded as RGBA and then blitted.
- If the prefix before the first external surface changes, the main window
  surface must be rerendered because that content lives in the window surface.

## Background Rendering

Begin-frame callbacks are only reliable for visible/active surfaces. Video still
needs a lower-frequency background path so media state advances without full
compositor-rate work.

Core should expose:

```rust
pub enum ExternalSurfaceFrameMode {
    Visible,
    Background,
}
```

In background mode:

- callbacks can be timer-driven
- `background_rendering` is true
- the provider may select/update content, but the result may not be displayed
- cadence-sensitive algorithms should treat the interval as lower-confidence

The initial background cadence can be conservative, with an explicit opt-in for
clients that need higher-frequency invisible pulls, such as texture-copy clients.

## Subduction Fit

Subduction should own native/compositor layer application:

- `LayerStore` stores ordered compositor layers.
- `FrameChanges` describes layer diffs.
- `Presenter::apply()` maps diffs to platform layers.

Floem should own:

- display-list paint order
- external surface registry
- composition lowering
- scene chunk rendering into compositor surfaces
- mapping composition keys to subduction layers
- frame timing and outcome delivery to providers

Platform paths:

- Apple: `LayerPresenter` / `MetalLayerPresenter`
- Wayland: subsurface presenter
- Windows: DirectComposition-style presenter when available
- fallback: `subduction_backend_wgpu::WgpuPresenter`

## GPU Resource Ownership

External surfaces that Floem owns or renders into must use the same WGPU
`Instance`, `Adapter`, `Device`, and `Queue` as the window renderer. Creating a
separate device for compositor-owned scene layers is incorrect because it breaks
resource sharing assumptions and can choose a different adapter from the app.

Floem therefore exposes `AppConfig::gpu_resources(GpuResources)` for embedding
apps that already own WGPU state. If no resources are supplied, Floem requests
them once and shares that `GpuResources` through:

- the main window renderer
- compositor-owned scene surfaces above/between external surfaces
- `WindowEvent::GpuResourcesReady(GpuResources)` / `WindowGpuResourcesReady`
  for producers that should start after Floem has acquired the window GPU
  context
- subduction external surface targets created through
  `SubductionWgpuSurface::create_target_with_gpu_resources`

The standalone `create_target` path exists only as a convenience for independent
producers. Video integrations that need zero-copy/shared-resource behavior
should use the app/Floem `GpuResources` path.

## Implementation Plan

Status:

- Phases 1-4 are implemented as the current vertical slice.
- Phase 5 is implemented for visible compositor-pulled surfaces: providers are
  registered on `ExternalSurface`, called after display-list lowering with the
  current `PresentationInterval`, may update content, may request the next
  compositor pull, and receive frame outcome feedback.
- Phase 5 background mode is still pending.
- Phase 6 remains outside Floem core: a video crate/view should implement the
  provider and own cadence/coverage/drift frame selection.

### Phase 1: Internal Display-List Command

- Add retained external surface placement commands to Floem display-list stages.
- Add `PaintCx::draw_external_surface`.
- Keep `imaging::Scene` unchanged.
- Add tests proving exact order with draw commands before and after an external
  surface.

### Phase 2: Composition Lowering

- Add `CompositionPlan` and `CompositionItem`.
- Lower retained display-list order into scene chunks and external surface
  items.
- Preserve active transform, clip, group opacity/composite state where supported.
- Keep existing direct-render fast path when no external surfaces exist.

### Phase 3: WindowCompositor Diff

- Add `WindowCompositor`.
- Map stable `CompositionKey`s to subduction `LayerId`s.
- Diff create/remove/update/reorder against the previous plan.
- Render scene chunks into compositor-owned surfaces/textures.
- Apply `LayerStore::evaluate()` and `Presenter::apply()`.

### Phase 4: External Surface Registry

- Add window-local registry for `ExternalSurfaceEntry`.
- Add UI-thread `ExternalSurface` and sendable `ExternalSurfaceHandle`.
- Route cross-thread content updates through app/window events.
- Support `Texture`, `Image`, and `Empty` content.

### Phase 5: Provider Pull API

- Add `ExternalSurfaceProvider`.
- During frame prepare/lowering, call providers for visible placements with
  `PresentationInterval`.
- Call outcome feedback after draw/present.
- Let providers request the next compositor pull.
- Keep provider pulls separate from ordinary paint invalidation.
- Only rerender the main window surface when the scene prefix before the first
  external surface changes.
- Add background rendering mode.

### Phase 6: Video Prototype

- Build a video surface provider outside core.
- Decode/upload frames on a worker thread into a ready queue.
- Select ready textures on the UI/frame path using presentation interval.
- Start with coverage/drift selection, then add cadence estimation.

## Open Questions

- How much of clip/group/composite state can native OS layers preserve exactly?
- Should unsupported clip/group states force a fallback texture-compositor path?
- Does subduction need an explicit `set_child_order` API?
- What is the minimum useful `ExternalTexture` abstraction across wgpu, native
  handles, and platform video textures?
- Should one `ExternalSurfaceId` be allowed to appear multiple times in one
  frame, or should V1 enforce one visible placement per surface?
- What should the exact background rendering cadence and opt-in policy be?
