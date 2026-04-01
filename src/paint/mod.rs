//! Paint context and state for rendering views.
//!
//! This module contains the types used during the paint phase:
//! - [`PaintCx`] - Context for painting views
//! - [`PaintState`] - State for the renderer (pending or initialized)
//! - [`Rasterizer`] - Backend rasterizer abstraction

pub mod border_path_iter;
pub mod display_list;
pub mod renderer;

pub use border_path_iter::{BorderPath, BorderPathEvent};
pub use floem_renderer::RenderCore as Rasterizer;

use floem_renderer::{
    RenderCore,
    gpu_resources::{GpuResourceError, GpuResources},
};
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
use display_list::{ElementSnapshot, RecordingRenderer, replay_stage, transform_diff_class};

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
    pub painter: Painter<'a, RecordingRenderer<'a>>,
    is_vger: bool,
    /// The target visual node being painted (CRITICAL for views with multiple visuals)
    pub target_id: ElementId,
    /// World transform for this visual node (from box tree)
    pub world_transform: Affine,
    /// Local layout bounds for this visual node (from box tree)
    pub layout_rect_local: peniko::kurbo::Rect,
    /// Optional clip for this visual node (from box tree)
    pub clip: Option<RoundedRect>,
    pub font_size_cx: FontSizeCx,
}

pub trait PainterExt {
    fn dyn_painter(&mut self) -> Painter<'_, dyn PaintSink + '_>;
}

impl<S: PaintSink> PainterExt for Painter<'_, S> {
    fn dyn_painter(&mut self) -> Painter<'_, dyn PaintSink + '_> {
        let sink: &mut dyn PaintSink = self.sink_mut();
        Painter::new(sink)
    }
}

impl PaintCx<'_> {
    pub fn dyn_painter(&mut self) -> Painter<'_, dyn PaintSink + '_> {
        self.painter.dyn_painter()
    }
}

impl GlobalPaintCx<'_> {
    fn collect_retained_subtree_descendants(
        &mut self,
        active_ids: &FxHashSet<ElementId>,
        explicit_dirty: &FxHashSet<ElementId>,
    ) -> FxHashSet<ElementId> {
        let mut reusable_descendants = FxHashSet::default();
        let display_list = &self.window_state.display_list;

        let box_tree = self.window_state.box_tree.borrow();
        let mut stack = Vec::new();

        for &element_id in active_ids {
            let Some(boundary) = box_tree.retained_transform_boundary(element_id.0) else {
                continue;
            };

            if explicit_dirty.contains(&element_id) {
                continue;
            }

            let snapshot = ElementSnapshot::from_box_tree(&box_tree, element_id);

            let Some(previous) = display_list.element(element_id).and_then(|e| e.snapshot) else {
                continue;
            };
            let diff = transform_diff_class(previous.world_transform, snapshot.world_transform);
            if !previous.supports_reuse(snapshot) || !boundary.supports(diff) {
                continue;
            }

            stack.clear();
            stack.extend(box_tree.children_of(element_id.0).iter().copied());
            while let Some(node_id) = stack.pop() {
                let Some(descendant) = box_tree.element_id_of(node_id) else {
                    continue;
                };
                if !active_ids.contains(&descendant)
                    || explicit_dirty.contains(&descendant)
                    || display_list.element(descendant).is_none()
                {
                    stack.extend(box_tree.children_of(node_id).iter().copied());
                    continue;
                }
                reusable_descendants.insert(descendant);
                stack.extend(box_tree.children_of(node_id).iter().copied());
            }
        }

        reusable_descendants
    }

    pub(crate) fn paint_with_traversal_into(
        &mut self,
        root_id: ViewId,
        renderer: &mut dyn RenderCore,
    ) {
        self.prepare_display_list(root_id);
        renderer.render(&mut |sink| {
            Self::replay_display_list_to_sink_with_state(
                self.window_state,
                self.record_paint_order,
                sink,
                None,
                Point::ZERO,
                None,
            );
        });
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
        let reusable_descendants =
            self.collect_retained_subtree_descendants(&active_ids, &dirty_ids);
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
            if reusable_descendants.contains(&element_id) {
                continue;
            }
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
            }
        }
        for element_id in dirty_ids {
            self.record_visual_node(element_id, false);
            self.record_visual_node(element_id, true);
        }

        self.window_state.last_paint_stats = PaintStats {
            active_ids: active_ids.len(),
            explicit_dirty_ids,
            reusable_descendants: reusable_descendants.len(),
            rerecord_ids,
            replay_steps: self.window_state.display_list.replay_step_count(),
        };
    }

    fn replay_display_list_to_sink_with_state(
        window_state: &mut WindowState,
        record_paint_order: bool,
        sink: &mut dyn PaintSink,
        included_ids: Option<&FxHashSet<ElementId>>,
        target_origin: Point,
        render_size: Option<Size>,
    ) {
        let mut stack = window_state
            .display_list
            .root_slots()
            .iter()
            .rev()
            .filter_map(|&slot| {
                window_state
                    .display_list
                    .node_element_id(slot)
                    .map(|id| (slot, false, id))
            })
            .collect::<Vec<_>>();

        while let Some((slot, is_post, element_id)) = stack.pop() {
            if included_ids.is_some_and(|ids| !ids.contains(&element_id)) {
                continue;
            }

            if !is_post {
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
                    None,
                    target_origin,
                    render_size,
                );

                stack.push((slot, true, element_id));
                let children = window_state
                    .display_list
                    .child_slots(slot)
                    .map(|children| children.to_vec())
                    .unwrap_or_default();
                for child in children.into_iter().rev() {
                    if let Some(child_id) = window_state.display_list.node_element_id(child) {
                        stack.push((child, false, child_id));
                    }
                }
                continue;
            }

            Self::replay_visual_node_to_sink_with_state(
                window_state,
                sink,
                element_id,
                true,
                None,
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
        damage_rects: Option<&[peniko::kurbo::Rect]>,
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
        let local_damage = damage_rects.map(|rects| {
            let inverse = base_transform.inverse();
            rects
                .iter()
                .map(|rect| inverse.transform_rect_bbox(*rect))
                .collect::<Vec<_>>()
        });
        let render_size =
            render_size.unwrap_or_else(|| window_state.root_size * window_state.os_scale);
        replay_stage(
            stage,
            sink,
            base_transform,
            render_size,
            local_damage.as_deref(),
        );
    }

    /// Record a single visual node in local coordinates.
    pub(crate) fn record_visual_node(&mut self, element_id: ElementId, is_post: bool) {
        let box_tree = self.window_state.box_tree.borrow_mut();
        let layout_rect_local = box_tree.local_bounds(element_id.0).unwrap_or_default();
        let snapshot = ElementSnapshot::from_box_tree(&box_tree, element_id);
        drop(box_tree);

        let layout_rect = layout_rect_local;
        let view_id = element_id.owning_id();
        let view = view_id.view();
        let view_state = view_id.state();
        let mut scene = imaging::record::Scene::new();
        let mut recorder = RecordingRenderer::new(&mut scene);
        let is_vger = false;
        let world_transform = self.element_base_transform(element_id);
        let font_size_cx = view_state.borrow().layout_props.font_size_cx();

        {
            // Create per-target PaintCx
            let mut cx = PaintCx {
                window_state: self.window_state,
                painter: Painter::new(&mut recorder),
                is_vger,
                target_id: element_id,
                world_transform,
                layout_rect_local,
                clip: snapshot.clip,
                font_size_cx,
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
        stage.set_scene(scene);
        element.snapshot = Some(snapshot);
    }
}

impl<'a> PaintCx<'a> {
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
        font_embolden: f32,
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
        font_embolden: f32,
    ) -> Self {
        Self::PendingGpuResources {
            window,
            rx,
            font_embolden,
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
