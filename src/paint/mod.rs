//! Paint context and state for rendering views.
//!
//! This module contains the types used during the paint phase:
//! - [`PaintCx`] - Context for painting views
//! - [`PaintState`] - State for the renderer (pending or initialized)
//! - [`Renderer`] - Backend renderer abstraction

pub mod border_path_iter;
pub mod renderer;

pub use border_path_iter::{BorderPath, BorderPathEvent};
pub use renderer::Renderer;

use floem_renderer::Renderer as FloemRenderer;
use floem_renderer::gpu_resources::{GpuResourceError, GpuResources};
use peniko::kurbo::{Affine, RoundedRect, Shape, Size};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use understory_box_tree::NodeFlags;
use winit::window::Window;

#[cfg(feature = "crossbeam")]
use crossbeam::channel::Receiver;
#[cfg(not(feature = "crossbeam"))]
use std::sync::mpsc::Receiver;

use crate::ElementId;
use crate::view::ViewId;
use crate::view::stacking::{StackingContextItem, collect_stacking_context_items_into};
use crate::view::{paint_bg, paint_border, paint_outline};
use crate::window::state::WindowState;

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
    pub(crate) paint_state: &'a mut PaintState,
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
    paint_state: &'a mut PaintState,
    /// The target visual node being painted (CRITICAL for views with multiple visuals)
    pub target_id: ElementId,
    /// World transform for this visual node (from box tree)
    pub world_transform: Affine,
    /// Local layout bounds for this visual node (from box tree)
    pub layout_rect_local: peniko::kurbo::Rect,
    /// Optional clip for this visual node (from box tree)
    pub clip: Option<RoundedRect>,
}

pub(crate) enum PaintOrPost {
    Paint(ElementId),
    Post(ElementId),
}

/// Collect VisualIds in paint order (depth-first, z-index sorted) without recursion.
pub(crate) fn collect_visual_order(
    root_element_id: ElementId,
    box_tree: &mut crate::BoxTree,
    paint_order: &mut Vec<PaintOrPost>,
    is_drag_preview: bool,
    skip_element_id: Option<ElementId>,
) {
    enum TraversalStep {
        Visit(ElementId),
        Post(ElementId),
    }

    // Local closure instead of nested fn
    let should_paint = |element_id: ElementId, box_tree: &mut crate::BoxTree| {
        if is_drag_preview {
            return true;
        }

        box_tree
            .get_or_compute_world_bounds(element_id.0)
            .is_none_or(|bounds| bounds.area() != 0.0)
    };

    let mut stack = Vec::new();
    let mut stacking_scratch: Vec<StackingContextItem> = Vec::new();
    stack.push(TraversalStep::Visit(root_element_id));

    while let Some(step) = stack.pop() {
        match step {
            TraversalStep::Visit(element_id) => {
                // Skip specific element and subtree when not drag preview
                if !is_drag_preview && Some(element_id) == skip_element_id {
                    continue;
                }

                // If hidden skip this element and the subtree
                if box_tree
                    .flags(element_id.0)
                    .is_none_or(|f| !f.contains(NodeFlags::VISIBLE))
                {
                    continue;
                }

                let paints_this_node = should_paint(element_id, box_tree);
                if paints_this_node {
                    paint_order.push(PaintOrPost::Paint(element_id));
                    // Keep Paint/Post paired for the same node. If a node is culled
                    // (e.g. zero world bounds), emitting Post without Paint can pop clip
                    // state that belongs to an ancestor and corrupt sibling paint order.
                    stack.push(TraversalStep::Post(element_id));
                }

                // Push children in reverse so they are visited in forward order.
                collect_stacking_context_items_into(element_id, box_tree, &mut stacking_scratch);
                for item in stacking_scratch.iter().rev() {
                    stack.push(TraversalStep::Visit(item.element_id));
                }
            }
            TraversalStep::Post(element_id) => paint_order.push(PaintOrPost::Post(element_id)),
        }
    }
}

impl GlobalPaintCx<'_> {
    /// Build explicit paint order for entire view tree.
    ///
    /// Returns a flat list of VisualIds in paint order (back-to-front, respecting z-index).
    /// Filters out hidden views and views with zero-area bounds.
    ///
    /// # Arguments
    /// * `root` - The root VisualId to start traversal from
    /// * `box_tree` - The box tree for querying spatial information
    ///
    /// # Returns
    /// Vector of VisualIds in paint order (back-to-front)
    fn build_paint_order(
        &self,
        root: ElementId,
        box_tree: &mut crate::BoxTree,
    ) -> Vec<PaintOrPost> {
        let mut paint_order = Vec::new();

        let dragging_element_id = self
            .window_state
            .drag_tracker
            .active_drag
            .as_ref()
            .and_then(|ad| ad.dragging_preview.as_ref().map(|p| p.element_id));
        // Collect main tree
        collect_visual_order(root, box_tree, &mut paint_order, false, dragging_element_id);

        // Paint drag overlay separately (always on top)
        if let Some(preview) = self
            .window_state
            .drag_tracker
            .active_drag
            .as_ref()
            .and_then(|ad| ad.dragging_preview.as_ref().map(|p| p.element_id))
        {
            crate::paint::collect_visual_order(preview, box_tree, &mut paint_order, true, None);
        }

        paint_order
    }
    /// Paint entire tree using explicit traversal
    pub(crate) fn paint_with_traversal(&mut self, root_id: ViewId) {
        let root_element_id = root_id.get_element_id();
        let mut box_tree = self.window_state.box_tree.borrow_mut();
        let paint_order = self.build_paint_order(root_element_id, &mut box_tree);
        drop(box_tree);

        for id_or_pop in paint_order {
            match id_or_pop {
                PaintOrPost::Paint(element_id) => {
                    // Record for testing
                    if self.record_paint_order {
                        record_paint(element_id.owning_id());
                    }

                    // Create per-target PaintCx and paint this visual node
                    self.paint_visual_node(element_id, false);
                }
                PaintOrPost::Post(element_id) => {
                    self.paint_visual_node(element_id, true);
                }
            }
        }
    }

    /// Paint a single visual node with its absolute transform
    pub(crate) fn paint_visual_node(&mut self, element_id: ElementId, is_post: bool) {
        // Get state from box tree for this visual node
        let mut box_tree = self.window_state.box_tree.borrow_mut();
        let world_transform = box_tree
            .get_or_compute_world_transform(element_id.0)
            .unwrap_or_default();
        let layout_rect_local = box_tree.local_bounds(element_id.0).unwrap_or_default();
        let clip = box_tree.clipped_local_clip(element_id.0).flatten();
        drop(box_tree);

        // Set absolute transform on renderer
        let device_transform = world_transform.then_scale(self.window_state.effective_scale());
        self.paint_state
            .renderer_mut()
            .set_transform(device_transform);

        let layout_rect = layout_rect_local;
        let view_id = element_id.owning_id();
        let view = view_id.view();
        let view_state = element_id.is_view().then(|| view_id.state());

        // Create per-target PaintCx
        let mut cx = PaintCx {
            window_state: self.window_state,
            paint_state: self.paint_state,
            target_id: element_id,
            world_transform,
            layout_rect_local,
            clip,
        };

        if !is_post {
            if let Some(view_state) = view_state.as_ref() {
                let state = view_state.borrow();
                paint_bg(&mut cx, &state.view_style_props, layout_rect);
                paint_border(
                    &mut cx,
                    &state.layout_props,
                    &state.view_style_props,
                    layout_rect,
                );
            }
            // Apply overflow clip (stays active through children)
            if let Some(clip_shape) = clip {
                cx.clip(&clip_shape);
            }
            view.borrow_mut().paint(&mut cx);
        } else {
            if clip.is_some() {
                cx.clear_clip();
            }
            view.borrow_mut().post_paint(&mut cx);
            if let Some(view_state) = view_state.as_ref() {
                let state = view_state.borrow();
                paint_outline(&mut cx, &state.view_style_props, layout_rect);
            }
        }
    }
}

impl PaintCx<'_> {
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

    /// Clip the drawing area (delegates to helper methods)
    pub fn clip(&mut self, shape: &impl Shape) {
        if self.paint_state.renderer().uses_layer_clip() {
            use peniko::Mix;
            self.push_layer(Mix::Normal, 1.0, Affine::IDENTITY, shape);
        } else {
            self.paint_state.renderer_mut().clip(shape);
        }
    }

    /// Clear clip
    pub fn clear_clip(&mut self) {
        if self.paint_state.renderer().uses_layer_clip() {
            self.pop_layer();
        } else {
            self.paint_state.renderer_mut().clear_clip();
        }
    }

    // Note: get_transform/set_transform removed as Renderer doesn't expose transform()
    // Views that previously used save/restore should use clip/clear_clip instead
}

// TODO: should this be private?
pub enum PaintState {
    /// The renderer is not yet initialized. This state is used to wait for the GPU resources to be acquired.
    PendingGpuResources {
        window: Arc<dyn Window>,
        rx: Receiver<Result<(GpuResources, wgpu::Surface<'static>), GpuResourceError>>,
        font_embolden: f32,
        /// This field holds an instance of `Renderer::Uninitialized` until the GPU resources are acquired,
        /// which will be returned in `PaintState::renderer` and `PaintState::renderer_mut`.
        /// All calls to renderer methods will be no-ops until the renderer is initialized.
        ///
        /// Previously, `PaintState::renderer` and `PaintState::renderer_mut` would panic if called when the renderer was uninitialized.
        /// However, this turned out to be hard to handle properly and led to panics, especially since the rest of the application code can't control when the renderer is initialized.
        renderer: Renderer,
    },
    /// The renderer is initialized and ready to paint.
    Initialized { renderer: Renderer },
}

impl PaintState {
    pub fn new_pending(
        window: Arc<dyn Window>,
        rx: Receiver<Result<(GpuResources, wgpu::Surface<'static>), GpuResourceError>>,
        size: Size,
        font_embolden: f32,
    ) -> Self {
        Self::PendingGpuResources {
            window,
            rx,
            font_embolden,
            renderer: Renderer::Uninitialized { size },
        }
    }

    pub fn new(
        window: Arc<dyn Window>,
        surface: wgpu::Surface<'static>,
        gpu_resources: GpuResources,
        scale: f64,
        size: Size,
        font_embolden: f32,
    ) -> Self {
        let renderer = Renderer::new(
            window.clone(),
            gpu_resources,
            surface,
            scale,
            size,
            font_embolden,
        );
        Self::Initialized { renderer }
    }

    #[cfg(feature = "skia")]
    pub fn new_skia(window: Arc<dyn Window>, scale: f64, size: Size, font_embolden: f32) -> Self {
        let renderer = Renderer::new_skia(window.clone(), scale, size, font_embolden);
        Self::Initialized { renderer }
    }

    pub(crate) fn renderer(&self) -> &Renderer {
        match self {
            PaintState::PendingGpuResources { renderer, .. } => renderer,
            PaintState::Initialized { renderer } => renderer,
        }
    }

    pub(crate) fn renderer_mut(&mut self) -> &mut Renderer {
        match self {
            PaintState::PendingGpuResources { renderer, .. } => renderer,
            PaintState::Initialized { renderer } => renderer,
        }
    }

    pub(crate) fn resize(&mut self, scale: f64, size: Size) {
        self.renderer_mut().resize(scale, size);
    }

    pub(crate) fn set_scale(&mut self, scale: f64) {
        self.renderer_mut().set_scale(scale);
    }
}

impl Deref for PaintCx<'_> {
    type Target = Renderer;

    fn deref(&self) -> &Self::Target {
        self.paint_state.renderer()
    }
}

impl DerefMut for PaintCx<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.paint_state.renderer_mut()
    }
}
