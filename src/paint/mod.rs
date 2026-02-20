//! Paint context and state for rendering views.
//!
//! This module contains the types used during the paint phase:
//! - [`PaintCx`] - Context for painting views
//! - [`PaintState`] - State for the renderer (pending or initialized)
//! - [`Renderer`] - Backend renderer abstraction

#[cfg(feature = "vello")]
pub mod border_path_iter;
pub mod renderer;

#[cfg(feature = "vello")]
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
use crate::view::stacking::{collect_overlays, collect_stacking_context_items};
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

/// Information needed to paint a dragged view overlay after the main tree painting.
/// This ensures the drag overlay always appears on top of all other content.
pub(crate) struct PendingDragPaint {
    pub id: ElementId,
    pub base_transform: Affine,
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

/// Recursively collect VisualIds in paint order (depth-first, z-index sorted)
pub(crate) fn collect_visual_recursive(
    element_id: ElementId,
    box_tree: &crate::BoxTree,
    paint_order: &mut Vec<PaintOrPost>,
    is_drag_preview: bool,
    skip_element_id: Option<ElementId>,
) {
    // CRITICAL: Query box tree directly, NOT element_id.view_id()!
    // Multiple VisualIds can map to the same ViewId (e.g., scroll view scrollbars).

    if !is_drag_preview && Some(element_id) == skip_element_id {
        return;
    }

    // We must check visibility and bounds from the box tree for THIS specific visual node.
    // Skip invisible nodes (check NodeFlags from box tree)
    if let Some(flags) = box_tree.flags(element_id.0) {
        if !flags.contains(NodeFlags::VISIBLE) {
            return;
        }
    }

    // Skip zero-area nodes (optimization - check bounds from box tree)
    if let Ok(bounds) = box_tree.world_bounds(element_id.0) {
        if !is_drag_preview && bounds.area() == 0. {
            return;
        }
    }

    // Add this visual node to paint order
    paint_order.push(PaintOrPost::Paint(element_id));

    // Get children from box tree (sorted by z-index)
    let items = collect_stacking_context_items(element_id, box_tree);

    // Recursively collect children (painting uses VisualIds from box tree)
    for item in items.iter() {
        collect_visual_recursive(
            item.element_id,
            box_tree,
            paint_order,
            is_drag_preview,
            skip_element_id,
        );
    }
    paint_order.push(PaintOrPost::Post(element_id));
}

impl GlobalPaintCx<'_> {
    /// Build explicit paint order for entire view tree including overlays.
    ///
    /// Returns a flat list of VisualIds in paint order (back-to-front, respecting z-index).
    /// Filters out hidden views and views with zero-area bounds.
    /// Includes overlays sorted by their z-index in the appropriate position.
    ///
    /// # Arguments
    /// * `root` - The root VisualId to start traversal from
    /// * `box_tree` - The box tree for querying spatial information
    ///
    /// # Returns
    /// Vector of VisualIds in paint order (back-to-front)
    fn build_paint_order_with_overlays(
        &self,
        root: ElementId,
        box_tree: &crate::BoxTree,
    ) -> Vec<PaintOrPost> {
        let mut paint_order = Vec::new();

        let dragging_element_id = self
            .window_state
            .drag_tracker
            .active_drag
            .as_ref()
            .and_then(|ad| ad.dragging_preview.as_ref().map(|p| p.element_id));
        // Recursively collect main tree
        collect_visual_recursive(root, box_tree, &mut paint_order, false, dragging_element_id);

        collect_overlays(root, box_tree, &mut paint_order, dragging_element_id);

        // Paint drag overlay separately (always on top)
        if let Some(preview) = self
            .window_state
            .drag_tracker
            .active_drag
            .as_ref()
            .and_then(|ad| ad.dragging_preview.as_ref().map(|p| p.element_id))
        {
            crate::paint::collect_visual_recursive(preview, box_tree, &mut paint_order, true, None);
        }

        paint_order
    }
    /// Paint entire tree using explicit traversal
    pub(crate) fn paint_with_traversal(&mut self, root_id: ViewId) {
        let root_element_id = root_id.get_element_id();
        let box_tree = self.window_state.box_tree.borrow();
        let paint_order = self.build_paint_order_with_overlays(root_element_id, &box_tree);
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
        let box_tree = self.window_state.box_tree.borrow();
        let world_transform = match box_tree.world_transform(element_id.0) {
            Ok(t) => t,
            Err(e) => e.value().unwrap(),
        };
        let layout_rect_local = box_tree.local_bounds(element_id.0).unwrap_or_default();
        let clip = box_tree.local_clip(element_id.0).flatten();
        drop(box_tree);

        // Set absolute transform on renderer
        self.paint_state
            .renderer_mut()
            .set_transform(world_transform);

        // Only access view state if this is a view element
        let style_data = if element_id.is_view() {
            let view_id = element_id.owning_id();
            let view_state = view_id.state();
            let view_style_props = view_state.borrow().view_style_props.clone();
            let layout_props = view_state.borrow().layout_props.clone();
            Some((view_style_props, layout_props))
        } else {
            None
        };

        let layout_rect = layout_rect_local;
        let view_id = element_id.owning_id();
        let view = view_id.view();

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
            if let Some((view_style_props, layout_props)) = &style_data {
                paint_bg(&mut cx, view_style_props, layout_rect);
                paint_border(&mut cx, layout_props, view_style_props, layout_rect);
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
            if let Some((view_style_props, _)) = &style_data {
                paint_outline(&mut cx, view_style_props, layout_rect);
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
        #[cfg(feature = "vello")]
        {
            use peniko::Mix;
            self.push_layer(Mix::Normal, 1.0, Affine::IDENTITY, shape);
        }
        #[cfg(not(feature = "vello"))]
        {
            self.paint_state.renderer_mut().clip(shape);
        }
    }

    /// Clear clip
    pub fn clear_clip(&mut self) {
        #[cfg(feature = "vello")]
        {
            self.pop_layer();
        }
        #[cfg(not(feature = "vello"))]
        {
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
        scale: f64,
        size: Size,
        font_embolden: f32,
    ) -> Self {
        Self::PendingGpuResources {
            window,
            rx,
            font_embolden,
            renderer: Renderer::Uninitialized { scale, size },
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
