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
use peniko::kurbo::{Affine, RoundedRect, Shape, Size, Vec2};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use winit::window::Window;

use crate::platform::{Duration, Instant};

#[cfg(feature = "crossbeam")]
use crossbeam::channel::Receiver;
#[cfg(not(feature = "crossbeam"))]
use std::sync::mpsc::Receiver;

use crate::action::exec_after;
use crate::animate::{Easing, Linear};
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
    static CURRENT_DRAG_PAINTING_ID : std::cell::Cell<Option<ViewId>> = const { std::cell::Cell::new(None) };

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
    pub id: ViewId,
    pub base_transform: Affine,
}

pub struct PaintCx<'a> {
    pub window_state: &'a mut WindowState,
    pub(crate) paint_state: &'a mut PaintState,
    pub(crate) transform: Affine,
    pub(crate) clip: Option<RoundedRect>,
    pub(crate) saved_transforms: Vec<Affine>,
    pub(crate) saved_clips: Vec<Option<RoundedRect>>,
    /// Pending drag paint info, to be painted after the main tree.
    pub(crate) pending_drag_paint: Option<PendingDragPaint>,
    pub gpu_resources: Option<GpuResources>,
    pub window: Arc<dyn Window>,
    #[cfg(feature = "vello")]
    pub layer_count: usize,
    #[cfg(feature = "vello")]
    pub saved_layer_counts: Vec<usize>,
    /// Whether to record paint order for testing. Cached from thread-local at creation.
    pub(crate) record_paint_order: bool,
}

impl PaintCx<'_> {
    pub fn save(&mut self) {
        self.saved_transforms.push(self.transform);
        self.saved_clips.push(self.clip);
        #[cfg(feature = "vello")]
        self.saved_layer_counts.push(self.layer_count);
    }

    pub fn restore(&mut self) {
        #[cfg(feature = "vello")]
        {
            let saved_count = self.saved_layer_counts.pop().unwrap_or_default();
            while self.layer_count > saved_count {
                self.pop_layer();
                self.layer_count -= 1;
            }
        }

        self.transform = self.saved_transforms.pop().unwrap_or_default();
        self.clip = self.saved_clips.pop().unwrap_or_default();
        self.paint_state
            .renderer_mut()
            .set_transform(self.transform);

        #[cfg(not(feature = "vello"))]
        {
            if let Some(rect) = self.clip {
                self.paint_state.renderer_mut().clip(&rect);
            } else {
                self.paint_state.renderer_mut().clear_clip();
            }
        }
    }

    /// Allows a `View` to determine if it is being called in order to
    /// paint a *draggable* image of itself during a drag (likely
    /// `draggable()` was called on the `View` or `ViewId`) as opposed
    /// to a normal paint in order to alter the way it renders itself.
    pub fn is_drag_paint(&self, id: ViewId) -> bool {
        // This could be an associated function, but it is likely
        // a Good Thing to restrict access to cases when the caller actually
        // has a PaintCx, and that doesn't make it a breaking change to
        // use instance methods in the future.
        if let Some(dragging) = CURRENT_DRAG_PAINTING_ID.get() {
            return dragging == id;
        }
        false
    }

    /// Paint the children of this view using simplified stacking semantics.
    ///
    /// In the simplified stacking model:
    /// - Every view is implicitly a stacking context
    /// - z-index only competes with siblings
    /// - Children are always bounded within their parent (no "escaping")
    pub fn paint_children(&mut self, id: ViewId) {
        // Collect direct children sorted by z-index
        let items = collect_stacking_context_items(id);

        for item in items.iter() {
            if item.view_id.is_hidden() {
                continue;
            }

            // Paint the child view (which will recursively paint its own children)
            self.paint_view(item.view_id);
        }
    }

    /// The entry point for painting a view. You shouldn't need to implement this yourself. Instead, implement [`View::paint`].
    /// It handles the internal work before and after painting [`View::paint`] implementations.
    /// It is responsible for
    /// - managing hidden status
    /// - clipping
    /// - painting computed styles like background color, border, font-styles, and z-index and handling painting requirements of drag and drop
    pub fn paint_view(&mut self, id: ViewId) {
        if id.is_hidden() {
            return;
        }

        // Record paint order for testing (fast path: skip if not recording)
        if self.record_paint_order {
            record_paint(id);
        }

        let view = id.view();
        let view_state = id.state();

        self.save();
        let size = self.transform(id);
        let is_empty = self
            .clip
            .map(|rect| rect.rect().intersect(size.to_rect()).is_zero_area())
            .unwrap_or(false);
        if !is_empty {
            let view_style_props = view_state.borrow().view_style_props.clone();
            let layout_props = view_state.borrow().layout_props.clone();

            paint_bg(self, &view_style_props, size);

            view.borrow_mut().paint(self);
            paint_border(self, &layout_props, &view_style_props, size);
            paint_outline(self, &view_style_props, size)
        }
        // Check if this view is being dragged and needs deferred painting
        if let Some(dragging) = self.window_state.dragging.as_ref()
            && dragging.id == id
        {
            // Store the pending drag paint info - actual painting happens after tree traversal
            self.pending_drag_paint = Some(PendingDragPaint {
                id,
                base_transform: self.transform,
            });
        }

        self.restore();
    }

    /// Paint the drag overlay after the main tree has been painted.
    /// This ensures the dragged view always appears on top of all other content.
    pub fn paint_pending_drag(&mut self) {
        let Some(pending) = self.pending_drag_paint.take() else {
            return;
        };

        let id = pending.id;
        let base_transform = pending.base_transform;

        let Some(dragging) = self.window_state.dragging.as_ref() else {
            return;
        };

        if dragging.id != id {
            return;
        }

        let mut drag_set_to_none = false;

        let transform = if let Some((released_at, release_location)) =
            dragging.released_at.zip(dragging.release_location)
        {
            let easing = Linear;
            const ANIMATION_DURATION_MS: f64 = 300.0;
            let elapsed = released_at.elapsed().as_millis() as f64;
            let progress = elapsed / ANIMATION_DURATION_MS;

            if !(easing.finished(progress)) {
                let offset_scale = 1.0 - easing.eval(progress);
                let release_offset = release_location.to_vec2() - dragging.offset;

                // Schedule next animation frame
                exec_after(Duration::from_millis(8), move |_| {
                    id.request_paint();
                });

                Some(base_transform * Affine::translate(release_offset * offset_scale))
            } else {
                drag_set_to_none = true;
                None
            }
        } else {
            // Handle active dragging
            let translation = self.window_state.last_cursor_location.to_vec2() - dragging.offset;
            Some(base_transform.with_translation(translation))
        };

        if let Some(transform) = transform {
            let view = id.view();
            let view_state = id.state();

            self.save();
            self.transform = transform;
            self.paint_state
                .renderer_mut()
                .set_transform(self.transform);
            self.clear_clip();

            // Get size from layout
            let size = if let Some(layout) = id.get_layout() {
                Size::new(layout.size.width as f64, layout.size.height as f64)
            } else {
                Size::ZERO
            };

            // Apply styles
            let style = view_state.borrow().combined_style.clone();
            let mut view_style_props = view_state.borrow().view_style_props.clone();

            if let Some(dragging_style) = view_state.borrow().dragging_style.clone() {
                let style = style.apply(dragging_style);
                let mut _new_frame = false;
                view_style_props.read_explicit(&style, &style, &Instant::now(), &mut _new_frame);
            }

            // Paint with drag styling
            let layout_props = view_state.borrow().layout_props.clone();

            // Important: If any method early exit points are added in this
            // code block, they MUST call CURRENT_DRAG_PAINTING_ID.take() before
            // returning.

            CURRENT_DRAG_PAINTING_ID.set(Some(id));

            paint_bg(self, &view_style_props, size);
            view.borrow_mut().paint(self);
            paint_border(self, &layout_props, &view_style_props, size);
            paint_outline(self, &view_style_props, size);

            self.restore();

            CURRENT_DRAG_PAINTING_ID.take();
        }

        if drag_set_to_none {
            self.window_state.dragging = None;
        }
    }

    /// Paint all registered overlays for the given root view.
    ///
    /// Overlays are painted at the root level, above all regular content but below
    /// drag overlays. They are sorted by z-index (lower z-index painted first).
    ///
    /// The overlay views are skipped during normal tree traversal (in `collect_stacking_context_items`)
    /// and painted here at root level so they appear above all other content.
    pub fn paint_overlays(&mut self, root_id: ViewId) {
        let overlays = collect_overlays(root_id);

        for overlay_id in overlays {
            if overlay_id.is_hidden() {
                continue;
            }

            // Check if the overlay itself is fixed, or if its first child is fixed.
            // When using Overlay::new(content.style(|s| s.fixed()...)), the fixed style
            // is on the child, not the overlay itself.
            let overlay_is_fixed = overlay_id
                .state()
                .borrow()
                .combined_style
                .get(crate::style::IsFixed);

            let first_child_is_fixed = overlay_id.children().first().is_some_and(|child| {
                child
                    .state()
                    .borrow()
                    .combined_style
                    .get(crate::style::IsFixed)
            });

            let is_fixed = overlay_is_fixed || first_child_is_fixed;

            // Set up the transform for the overlay.
            // This ensures the overlay is painted at the correct window position.
            self.save();

            // For fixed-positioned overlays, we reset to identity since they're
            // positioned relative to the viewport, not their parent.
            // For regular overlays, we use the parent's pre-computed visual_transform.
            if is_fixed {
                self.transform = Affine::IDENTITY;
            } else if let Some(parent) = overlay_id.parent() {
                // Use parent's pre-computed transform directly (O(1) instead of O(depth))
                self.transform = parent.state().borrow().visual_transform;
            }

            self.paint_state
                .renderer_mut()
                .set_transform(self.transform);

            // Paint the overlay view.
            // If the first child is fixed (but not the overlay itself), we paint
            // children directly to avoid adding the overlay's layout.location.
            // The overlay wrapper doesn't render anything itself.
            if first_child_is_fixed && !overlay_is_fixed {
                for child in overlay_id.children() {
                    self.paint_view(child);
                }
            } else {
                self.paint_view(overlay_id);
            }

            self.restore();
        }
    }

    /// Clip the drawing area to the given shape.
    pub fn clip(&mut self, shape: &impl Shape) {
        #[cfg(feature = "vello")]
        {
            use peniko::Mix;

            self.push_layer(Mix::Normal, 1.0, Affine::IDENTITY, shape);
            self.layer_count += 1;
            self.clip = Some(shape.bounding_box().to_rounded_rect(0.0));
        }

        #[cfg(not(feature = "vello"))]
        {
            let rect = if let Some(rect) = shape.as_rect() {
                rect.to_rounded_rect(0.0)
            } else if let Some(rect) = shape.as_rounded_rect() {
                rect
            } else {
                let rect = shape.bounding_box();
                rect.to_rounded_rect(0.0)
            };

            let rect = if let Some(existing) = self.clip {
                let rect = existing.rect().intersect(rect.rect());
                self.paint_state.renderer_mut().clip(&rect);
                rect.to_rounded_rect(0.0)
            } else {
                self.paint_state.renderer_mut().clip(&shape);
                rect
            };

            self.clip = Some(rect);
        }
    }

    /// Remove clipping so the entire window can be rendered to.
    pub fn clear_clip(&mut self) {
        self.clip = None;
        self.paint_state.renderer_mut().clear_clip();
    }

    pub fn offset(&mut self, offset: (f64, f64)) {
        let mut new = self.transform.as_coeffs();
        new[4] += offset.0;
        new[5] += offset.1;
        self.transform = Affine::new(new);
        self.paint_state
            .renderer_mut()
            .set_transform(self.transform);
        if let Some(rect) = self.clip.as_mut() {
            let raidus = rect.radii();
            *rect = rect
                .rect()
                .with_origin(rect.origin() - Vec2::new(offset.0, offset.1))
                .to_rounded_rect(raidus);
        }
    }

    pub fn transform(&mut self, id: ViewId) -> Size {
        if let Some(layout) = id.get_layout() {
            let offset = layout.location;

            // Use the pre-computed visual_transform directly instead of accumulating.
            // This transform is computed during layout and includes all ancestor transforms.
            self.transform = id.state().borrow().visual_transform;

            self.paint_state
                .renderer_mut()
                .set_transform(self.transform);

            // Adjust clip rect to local coordinates by subtracting the layout offset.
            // This keeps clips in the current view's coordinate space for intersection tests.
            if let Some(rect) = self.clip.as_mut() {
                let raidus = rect.radii();
                *rect = rect
                    .rect()
                    .with_origin(rect.origin() - Vec2::new(offset.x as f64, offset.y as f64))
                    .to_rounded_rect(raidus);
            }

            Size::new(layout.size.width as f64, layout.size.height as f64)
        } else {
            Size::ZERO
        }
    }

    pub fn is_focused(&self, id: ViewId) -> bool {
        self.window_state.is_focused(&id)
    }
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
