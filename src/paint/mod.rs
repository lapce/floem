//! Paint context and state for rendering views.
//!
//! This module contains the types used during the paint phase:
//! - [`PaintCx`] - Context for painting views
//! - [`PaintState`] - State for the renderer (pending or initialized)

pub mod border_path_iter;
pub(crate) mod composition;
pub mod display_list;
pub mod renderer;

use crate::gpu_resources::{GpuResourceError, GpuResources};
pub use border_path_iter::{BorderPath, BorderPathEvent};
use imaging::{PaintSink, Painter};
use peniko::kurbo::{Affine, Point, RoundedRect, Size};
use rustc_hash::FxHashSet;
use std::sync::Arc;
use winit::window::Window;

#[cfg(feature = "crossbeam")]
use crossbeam::channel::Receiver;
#[cfg(not(feature = "crossbeam"))]
use std::sync::mpsc::Receiver;

use crate::ElementId;
use crate::style::FontSizeCx;
use crate::view::ViewId;
use crate::view::{paint_bg, paint_border, paint_outline};
use crate::window::state::WindowState;
use composition::CompositionItem;
use display_list::{ElementSnapshot, StageRecorder, replay_scene};

std::thread_local! {
    /// Holds the ID of a View being painted very briefly if it is being rendered as
    /// a moving drag image.  Since that is a relatively unusual thing to need, it
    /// makes more sense to use a thread local for it and avoid cluttering the fields
    /// and memory footprint of PaintCx or PaintState or ViewId with a field for it.
    /// This is ephemerally set before paint calls that are painting the view in a
    /// location other than its natural one for purposes of drag and drop.
    pub(crate) static CURRENT_DRAG_PAINTING_ID : std::cell::Cell<Option<ElementId>> = const { std::cell::Cell::new(None) };

    /// Paint order tracker for testing purposes.
    /// When enabled, records the ViewIds in the order they are painted.
    /// This is used by HeadlessHarness to verify paint order in tests.
    static PAINT_ORDER_TRACKER: std::cell::RefCell<PaintOrderTracker> = const { std::cell::RefCell::new(PaintOrderTracker::new()) };
}

/// Tracker for paint order, used in testing to verify views are painted in the correct order.
#[derive(Default)]
pub struct PaintOrderTracker {
    enabled: bool,
    order: Vec<ViewId>,
}

impl PaintOrderTracker {
    const fn new() -> Self {
        Self {
            enabled: false,
            order: Vec::new(),
        }
    }

    fn record(&mut self, id: ViewId) {
        if self.enabled {
            self.order.push(id);
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PaintStats {
    pub active_ids: usize,
    pub explicit_dirty_ids: usize,
    pub reusable_descendants: usize,
    pub rerecord_ids: usize,
    pub replay_steps: usize,
}

/// Enable paint order tracking. When enabled, all painted ViewIds are recorded in order.
pub fn enable_paint_order_tracking() {
    PAINT_ORDER_TRACKER.with(|tracker| {
        let mut t = tracker.borrow_mut();
        t.enabled = true;
        t.order.clear();
    });
}

/// Disable paint order tracking.
pub fn disable_paint_order_tracking() {
    PAINT_ORDER_TRACKER.with(|tracker| {
        tracker.borrow_mut().enabled = false;
    });
}

/// Clear the recorded paint order without disabling tracking.
pub fn clear_paint_order() {
    PAINT_ORDER_TRACKER.with(|tracker| {
        tracker.borrow_mut().order.clear();
    });
}

/// Get a copy of the recorded paint order.
pub fn get_paint_order() -> Vec<ViewId> {
    PAINT_ORDER_TRACKER.with(|tracker| tracker.borrow().order.clone())
}

/// Check if paint order tracking is enabled.
pub fn is_paint_order_tracking_enabled() -> bool {
    PAINT_ORDER_TRACKER.with(|tracker| tracker.borrow().enabled)
}

/// Record a view being painted (internal use).
#[inline]
fn record_paint(id: ViewId) {
    PAINT_ORDER_TRACKER.with(|tracker| {
        tracker.borrow_mut().record(id);
    });
}

/// Global paint context - holds shared state for entire paint pass
/// Similar to GlobalEventCx in event dispatch
pub struct GlobalPaintCx<'a> {
    pub window_state: &'a mut WindowState,
    pub gpu_resources: Option<GpuResources>,
    pub window: Arc<dyn Window>,
    /// Whether to record paint order for testing. Cached from thread-local at creation.
    pub(crate) record_paint_order: bool,
}

/// Per-target paint context - created for each visual node
/// Similar to EventCx in event dispatch
pub struct PaintCx<'a> {
    /// Reference to global paint state
    pub window_state: &'a mut WindowState,
    gpu_resources: Option<&'a GpuResources>,
    pub painter: Painter<'a, StageRecorder>,
    is_vger: bool,
    /// The target visual node being painted
    pub target_id: ElementId,
    /// World transform for this visual node (from box tree)
    pub world_transform: Affine,
    /// Local layout bounds for this visual node (from box tree)
    pub layout_rect_local: peniko::kurbo::Rect,
    /// Optional clip for this visual node (from box tree)
    pub clip: Option<RoundedRect>,
    pub font_size_cx: FontSizeCx,
    pub font_embolden: peniko::kurbo::Vec2,
    pub effective_scale: f64,
}

impl<'a> PaintCx<'a> {
    /// Returns the WGPU resources used by Floem's renderer for this window, if
    /// the active renderer has a GPU context.
    pub fn gpu_resources(&self) -> Option<&'a GpuResources> {
        self.gpu_resources
    }
}

impl GlobalPaintCx<'_> {
    pub(crate) fn paint_with_traversal_into(&mut self, root_id: ViewId, sink: &mut dyn PaintSink) {
        self.prepare_display_list(root_id);
        #[cfg(feature = "subduction")]
        if self.window_state.composition_plan.has_external_surfaces() {
            Self::replay_composition_prefix_to_sink(self.window_state, sink, Point::ZERO, None);
            return;
        }
        Self::replay_display_list_to_sink_with_state(
            self.window_state,
            self.record_paint_order,
            sink,
            None,
            Point::ZERO,
            None,
        );
    }

    pub(crate) fn prepare_display_list(&mut self, root_id: ViewId) {
        let root_element_id = root_id.get_element_id();
        let dragging_element_id = self
            .window_state
            .drag_tracker
            .active_drag
            .as_ref()
            .and_then(|ad| ad.dragging_preview.as_ref().map(|p| p.element_id));
        let sync = {
            let box_tree = self.window_state.box_tree.borrow();
            self.window_state.display_list.sync_structure(
                root_element_id,
                &box_tree,
                dragging_element_id,
            )
        };
        let active_ids = sync.active_ids;

        let mut dirty_ids = self.window_state.take_dirty_paint_elements();
        let explicit_dirty_ids = dirty_ids.len();
        dirty_ids.extend(sync.newly_active_ids);
        let snapshots = {
            let box_tree = self.window_state.box_tree.borrow();
            active_ids
                .iter()
                .copied()
                .map(|element_id| {
                    (
                        element_id,
                        ElementSnapshot::from_box_tree(&box_tree, element_id),
                    )
                })
                .collect::<Vec<_>>()
        };
        let mut reused_snapshots = Vec::new();

        for &(element_id, snapshot) in &snapshots {
            if self
                .window_state
                .display_list
                .needs_stage_rerecord(element_id, snapshot)
            {
                dirty_ids.insert(element_id);
            } else {
                reused_snapshots.push((element_id, snapshot));
            }
        }
        let rerecord_ids = dirty_ids.len();
        for (element_id, snapshot) in reused_snapshots {
            if let Some(element) = self.window_state.display_list.element(element_id)
                && element.snapshot != Some(snapshot)
            {
                // Retained commands can be reused across pure transform/clip changes, but the
                // current snapshot still needs to track the element's latest geometry/transform.
                self.window_state
                    .display_list
                    .element_mut(element_id)
                    .snapshot = Some(snapshot);
                self.window_state
                    .display_list
                    .mark_composed_dirty(element_id);
            }
        }
        for element_id in dirty_ids {
            self.record_visual_node(element_id, false);
            self.record_visual_node(element_id, true);
        }

        self.window_state.last_paint_stats = PaintStats {
            active_ids: active_ids.len(),
            explicit_dirty_ids,
            reusable_descendants: 0,
            rerecord_ids,
            replay_steps: self.window_state.display_list.replay_step_count(),
        };
        self.window_state.composition_plan =
            self.window_state.display_list.lower_composition_plan();
        let effective_scale = self.window_state.effective_scale();
        self.window_state
            .external_surfaces
            .update_providers(&self.window_state.composition_plan, effective_scale);
        let _composition_diff = self.window_state.compositor.apply_plan(
            &self.window_state.composition_plan,
            self.window_state.external_surfaces.entries(),
            self.gpu_resources.as_ref(),
        );
    }

    #[cfg(feature = "subduction")]
    fn replay_composition_prefix_to_sink(
        window_state: &mut WindowState,
        sink: &mut dyn PaintSink,
        target_origin: Point,
        render_size: Option<Size>,
    ) {
        let effective_scale = window_state.effective_scale();
        let root_size = window_state.root_size;
        let os_scale = window_state.os_scale;
        let render_size = render_size.unwrap_or_else(|| root_size * os_scale);

        for item in &window_state.composition_plan.items {
            match item {
                CompositionItem::Scene(layer) => {
                    let base_transform = layer
                        .transform
                        .then_scale(effective_scale)
                        .then_translate(-target_origin.to_vec2());
                    if let Some(clip) = layer.clip {
                        display_list::replay_view_clip(sink, clip, base_transform, render_size);
                    }
                    replay_scene(&layer.scene, sink, base_transform, render_size);
                    if layer.clip.is_some() {
                        PaintSink::pop_clip(sink);
                    }
                }
                CompositionItem::ExternalSurface(_) => break,
            }
        }
    }

    fn replay_display_list_to_sink_with_state(
        window_state: &mut WindowState,
        record_paint_order: bool,
        sink: &mut dyn PaintSink,
        included_ids: Option<&FxHashSet<ElementId>>,
        target_origin: Point,
        render_size: Option<Size>,
    ) {
        let root_slots = window_state.display_list.root_slots().to_vec();
        for slot in root_slots {
            Self::replay_display_slot_to_sink_with_state(
                window_state,
                record_paint_order,
                sink,
                slot,
                included_ids,
                target_origin,
                render_size,
            );
        }
    }

    fn replay_display_slot_to_sink_with_state(
        window_state: &mut WindowState,
        record_paint_order: bool,
        sink: &mut dyn PaintSink,
        slot: display_list::DisplayNodeSlot,
        included_ids: Option<&FxHashSet<ElementId>>,
        target_origin: Point,
        render_size: Option<Size>,
    ) {
        let Some(element_id) = window_state.display_list.node_element_id(slot) else {
            return;
        };
        if included_ids.is_some_and(|ids| !ids.contains(&element_id)) {
            return;
        }

        if !record_paint_order
            && included_ids.is_none()
            && window_state.display_list.slot_has_composed_scene(slot)
        {
            window_state.display_list.ensure_composed_scene(slot);
            let effective_scale = window_state.effective_scale();
            let root_size = window_state.root_size;
            let os_scale = window_state.os_scale;
            let display_list = &window_state.display_list;
            if let (Some(snapshot), Some(scene)) = (
                display_list.snapshot_for_slot(slot),
                display_list.composed_scene(slot),
            ) {
                let base_transform = snapshot
                    .world_transform
                    .then_scale(effective_scale)
                    .then_translate(-target_origin.to_vec2());
                let render_size = render_size.unwrap_or_else(|| root_size * os_scale);
                replay_scene(scene, sink, base_transform, render_size);
                return;
            }
        }

        if record_paint_order {
            record_paint(element_id.owning_id());
        }
        if element_id.is_view() {
            Self::replay_element_overflow_clip_to_sink_with_state(
                window_state,
                sink,
                element_id,
                target_origin,
                render_size,
            );
        }
        Self::replay_visual_node_to_sink_with_state(
            window_state,
            sink,
            element_id,
            false,
            target_origin,
            render_size,
        );

        let children = window_state
            .display_list
            .child_slots(slot)
            .map(|children| children.to_vec())
            .unwrap_or_default();
        for child in children {
            Self::replay_display_slot_to_sink_with_state(
                window_state,
                record_paint_order,
                sink,
                child,
                included_ids,
                target_origin,
                render_size,
            );
        }

        Self::replay_visual_node_to_sink_with_state(
            window_state,
            sink,
            element_id,
            true,
            target_origin,
            render_size,
        );
        if element_id.is_view() {
            let has_clip = window_state
                .box_tree
                .borrow()
                .local_clip(element_id.0)
                .flatten()
                .is_some();
            if has_clip {
                PaintSink::pop_clip(sink);
            }
        }
    }

    fn replay_element_overflow_clip_to_sink_with_state(
        window_state: &mut WindowState,
        sink: &mut dyn PaintSink,
        element_id: ElementId,
        target_origin: Point,
        render_size: Option<Size>,
    ) {
        let box_tree = window_state.box_tree.borrow();
        let Some(clip) = box_tree.local_clip(element_id.0).flatten() else {
            return;
        };
        drop(box_tree);

        let base_transform = Self::element_base_transform_from_state(window_state, element_id)
            .then_scale(window_state.effective_scale())
            .then_translate(-target_origin.to_vec2());
        let render_size =
            render_size.unwrap_or_else(|| window_state.root_size * window_state.os_scale);
        display_list::replay_view_clip(sink, clip, base_transform, render_size);
    }

    fn element_base_transform(&mut self, element_id: ElementId) -> Affine {
        // Get state from box tree for this visual node
        let box_tree = self.window_state.box_tree.borrow();
        box_tree.world_transform(element_id.0).unwrap_or_default()
    }

    fn element_base_transform_from_state(
        window_state: &mut WindowState,
        element_id: ElementId,
    ) -> Affine {
        let box_tree = window_state.box_tree.borrow();
        box_tree.world_transform(element_id.0).unwrap_or_default()
    }

    fn replay_visual_node_to_sink_with_state(
        window_state: &mut WindowState,
        sink: &mut dyn PaintSink,
        element_id: ElementId,
        is_post: bool,
        target_origin: Point,
        render_size: Option<Size>,
    ) {
        let base_transform = Self::element_base_transform_from_state(window_state, element_id)
            .then_scale(window_state.effective_scale())
            .then_translate(-target_origin.to_vec2());
        let Some(element) = window_state.display_list.element(element_id) else {
            return;
        };
        let stage = if is_post {
            &element.post
        } else {
            &element.paint
        };
        let render_size =
            render_size.unwrap_or_else(|| window_state.root_size * window_state.os_scale);
        replay_scene(&stage.scene, sink, base_transform, render_size);
    }

    /// Record a single visual node in local coordinates.
    pub(crate) fn record_visual_node(&mut self, element_id: ElementId, is_post: bool) {
        let box_tree = self.window_state.box_tree.borrow_mut();
        let layout_rect_local = box_tree.local_bounds(element_id.0).unwrap_or_default();
        let snapshot = ElementSnapshot::from_box_tree(&box_tree, element_id);
        drop(box_tree);

        let mut recorder = {
            let element = self.window_state.display_list.element_mut(element_id);
            let stage = if is_post {
                &mut element.post
            } else {
                &mut element.paint
            };
            let mut recorder = StageRecorder::from_stage(stage);
            recorder.clear();
            recorder
        };

        let layout_rect = layout_rect_local;
        let view_id = element_id.owning_id();
        let view = view_id.view();
        let view_state = view_id.state();
        let is_vger = false;
        let world_transform = self.element_base_transform(element_id);
        let font_size_cx = view_state.borrow().layout_props.font_size_cx();
        let effective_scale = self.window_state.effective_scale();

        {
            // Create per-target PaintCx
            let mut cx = PaintCx {
                window_state: self.window_state,
                gpu_resources: self.gpu_resources.as_ref(),
                painter: Painter::new(&mut recorder),
                is_vger,
                target_id: element_id,
                world_transform,
                layout_rect_local,
                clip: snapshot.clip,
                font_size_cx,
                font_embolden: view_state
                    .borrow()
                    .computed_style
                    .get(crate::style::FontEmbolden),
                effective_scale,
            };

            if !is_post {
                if element_id.is_view() {
                    let state = view_state.borrow();
                    paint_bg(&mut cx, &state.view_style_props, layout_rect);
                    paint_border(
                        &mut cx,
                        &state.layout_props,
                        &state.view_style_props,
                        layout_rect,
                    );
                    drop(state);
                }
                view.borrow_mut().paint(&mut cx);
            } else {
                view.borrow_mut().post_paint(&mut cx);
                if element_id.is_view() {
                    let state = view_state.borrow();
                    paint_outline(&mut cx, &state.view_style_props, layout_rect);
                }
            }
        }

        let element = self.window_state.display_list.element_mut(element_id);
        let stage = if is_post {
            &mut element.post
        } else {
            &mut element.paint
        };
        recorder.finish(stage);
        element.snapshot = Some(snapshot);
        self.window_state
            .display_list
            .mark_composed_dirty(element_id);
    }
}

impl<'a> PaintCx<'a> {
    pub fn draw_external_surface(
        &mut self,
        surface: &crate::external_surface::ExternalSurface,
        rect: peniko::kurbo::Rect,
        options: crate::external_surface::ExternalSurfacePaintOptions,
    ) {
        self.painter
            .sink_mut()
            .draw_external_surface(surface.id(), rect, options);
    }

    /// Allows a `View` to determine if it is being called in order to
    /// paint a *draggable* image of itself during a drag (likely
    /// `draggable()` was called on the `View` or `ViewId`) as opposed
    /// to a normal paint in order to alter the way it renders itself.
    pub fn is_drag_paint(&self, id: impl Into<ElementId>) -> bool {
        let id = id.into();
        // This could be an associated function, but it is likely
        // a Good Thing to restrict access to cases when the caller actually
        // has a PaintCx, and that doesn't make it a breaking change to
        // use instance methods in the future.
        if let Some(dragging) = CURRENT_DRAG_PAINTING_ID.get() {
            return dragging == id;
        }
        false
    }

    pub fn is_vger(&self) -> bool {
        self.is_vger
    }
}

pub(crate) enum PaintState {
    /// The renderer is not yet initialized. This state is used to wait for the GPU resources to be acquired.
    PendingGpuResources {
        window: Arc<dyn Window>,
        rx: Receiver<Result<(GpuResources, wgpu::Surface<'static>), GpuResourceError>>,
        backend: renderer::WindowBackend,
    },
    /// The renderer is initialized and ready to paint.
    Initialized { backend: renderer::WindowBackend },
}

impl PaintState {
    pub fn new_pending(
        window: Arc<dyn Window>,
        rx: Receiver<Result<(GpuResources, wgpu::Surface<'static>), GpuResourceError>>,
        _size: Size,
    ) -> Self {
        Self::PendingGpuResources {
            window,
            rx,
            backend: renderer::uninitialized_backend(),
        }
    }

    pub(crate) fn backend(&self) -> &(dyn renderer::WindowRenderer + '_) {
        match self {
            PaintState::PendingGpuResources { backend, .. } => backend.as_ref(),
            PaintState::Initialized { backend } => backend.as_ref(),
        }
    }

    pub(crate) fn backend_mut(&mut self) -> &mut (dyn renderer::WindowRenderer + '_) {
        match self {
            PaintState::PendingGpuResources { backend, .. } => backend.as_mut(),
            PaintState::Initialized { backend } => backend.as_mut(),
        }
    }
}
