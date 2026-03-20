//! Paint context and state for rendering views.
//!
//! This module contains the types used during the paint phase:
//! - [`PaintCx`] - Context for painting views
//! - [`PaintState`] - State for the renderer (pending or initialized)
//! - [`Renderer`] - Backend renderer abstraction

pub mod border_path_iter;
pub mod display_list;
pub mod renderer;

pub use border_path_iter::{BorderPath, BorderPathEvent};
pub use renderer::Renderer;

use floem_renderer::Renderer as _;
use floem_renderer::gpu_resources::{GpuResourceError, GpuResources};
use peniko::kurbo::{Affine, Point, RoundedRect, Shape, Size};
use rustc_hash::FxHashSet;
use std::sync::Arc;
use understory_box_tree::NodeFlags;
use winit::window::Window;

#[cfg(feature = "crossbeam")]
use crossbeam::channel::Receiver;
#[cfg(not(feature = "crossbeam"))]
use std::sync::mpsc::Receiver;

use crate::ElementId;
use crate::style::FontSizeCx;
use crate::view::ViewId;
use crate::view::stacking::{StackingContextItem, collect_stacking_context_items_into};
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
    recorder: &'a mut RecordingRenderer<'a>,
    uses_layer_clip: bool,
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

#[derive(Clone, Copy)]
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
            .world_bounds(element_id.0)
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
    fn replay_element_overflow_clip(
        &mut self,
        element_id: ElementId,
        target_origin: Point,
        render_size: Option<Size>,
    ) {
        let box_tree = self.window_state.box_tree.borrow();
        let Some(clip) = box_tree.local_clip(element_id.0).flatten() else {
            return;
        };
        drop(box_tree);

        let base_transform = self
            .element_base_transform(element_id)
            .then_scale(self.window_state.effective_scale())
            .then_translate(-target_origin.to_vec2());
        let render_size = render_size.unwrap_or_else(|| self.paint_state.renderer().size());
        let renderer = self.paint_state.renderer_mut();

        // Overflow clip must be replayed at traversal time rather than recorded into the
        // element stage so it can stay active across descendant element replay.
        display_list::replay_view_clip(renderer, clip, base_transform, render_size);
    }

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
        let replay_order = self.prepare_display_list(root_id);
        self.replay_display_list(&replay_order, None, Point::ZERO, None);
    }

    pub(crate) fn prepare_display_list(&mut self, root_id: ViewId) -> Vec<PaintOrPost> {
        let root_element_id = root_id.get_element_id();
        let mut box_tree = self.window_state.box_tree.borrow_mut();
        let paint_order = self.build_paint_order(root_element_id, &mut box_tree);
        drop(box_tree);

        let active_ids = paint_order
            .iter()
            .map(|step| match *step {
                PaintOrPost::Paint(id) | PaintOrPost::Post(id) => id,
            })
            .collect();

        self.window_state.display_list.retain_only(&active_ids);
        self.window_state
            .display_list
            .set_paint_order(paint_order.clone());

        let mut dirty_ids = self.window_state.take_dirty_paint_elements();
        let explicit_dirty_ids = dirty_ids.len();
        let reusable_descendants =
            self.collect_retained_subtree_descendants(&active_ids, &dirty_ids);
        let snapshots = {
            let box_tree = self.window_state.box_tree.borrow();
            active_ids
                .iter()
                .copied()
                .map(|element_id| (element_id, ElementSnapshot::from_box_tree(&box_tree, element_id)))
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
                // Retained commands can be reused across pure transform/clip changes, but
                // downstream systems like promoted compositor bounds still need the current
                // snapshot every frame. Keep the artifact metadata fresh even when we skip
                // rerecording the stage commands.
                self.window_state.display_list.element_mut(element_id).snapshot = Some(snapshot);
            }
        }
        for element_id in dirty_ids {
            self.record_visual_node(element_id, false);
            self.record_visual_node(element_id, true);
        }

        let replay_order = self.window_state.display_list.paint_order().to_vec();
        self.window_state.last_paint_stats = PaintStats {
            active_ids: active_ids.len(),
            explicit_dirty_ids,
            reusable_descendants: reusable_descendants.len(),
            rerecord_ids,
            replay_steps: replay_order.len(),
        };
        replay_order
    }

    pub(crate) fn replay_display_list(
        &mut self,
        replay_order: &[PaintOrPost],
        included_ids: Option<&FxHashSet<ElementId>>,
        target_origin: Point,
        render_size: Option<Size>,
    ) {
        for &id_or_pop in replay_order {
            let element_id = match id_or_pop {
                PaintOrPost::Paint(id) | PaintOrPost::Post(id) => id,
            };
            if included_ids.is_some_and(|ids| !ids.contains(&element_id)) {
                continue;
            }

            match id_or_pop {
                PaintOrPost::Paint(element_id) => {
                    // Record for testing
                    if self.record_paint_order {
                        record_paint(element_id.owning_id());
                    }
                    if element_id.is_view() {
                        self.replay_element_overflow_clip(
                            element_id,
                            target_origin,
                            render_size,
                        );
                    }
                    // Damage-aware chunk replay is intentionally disabled here for now.
                    // Every backend rebuilds a fresh frame/scene on `begin()`, so skipping
                    // undamaged chunks would drop previously visible content and cause flashing.
                    self.replay_visual_node(element_id, false, None, target_origin, render_size);
                }
                PaintOrPost::Post(element_id) => {
                    self.replay_visual_node(element_id, true, None, target_origin, render_size);
                    if element_id.is_view() {
                        let has_clip = self
                            .window_state
                            .box_tree
                            .borrow()
                            .local_clip(element_id.0)
                            .flatten()
                            .is_some();
                        if has_clip {
                            self.paint_state.renderer_mut().clear_clip();
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn collect_subtree_elements(&self, root: ElementId) -> FxHashSet<ElementId> {
        let box_tree = self.window_state.box_tree.borrow();
        let mut elements = FxHashSet::default();
        let mut stack = vec![root.0];

        while let Some(node_id) = stack.pop() {
            if let Some(element_id) = box_tree.element_id_of(node_id) {
                elements.insert(element_id);
            }
            stack.extend(box_tree.children_of(node_id).iter().copied());
        }

        elements
    }

    fn element_base_transform(&mut self, element_id: ElementId) -> Affine {
        // Get state from box tree for this visual node
        let box_tree = self.window_state.box_tree.borrow();
        box_tree.world_transform(element_id.0).unwrap_or_default()
    }

    fn replay_visual_node(
        &mut self,
        element_id: ElementId,
        is_post: bool,
        damage_rects: Option<&[peniko::kurbo::Rect]>,
        target_origin: Point,
        render_size: Option<Size>,
    ) {
        let base_transform = self
            .element_base_transform(element_id)
            .then_scale(self.window_state.effective_scale())
            .then_translate(-target_origin.to_vec2());
        let Some(element) = self.window_state.display_list.element(element_id) else {
            return;
        };
        let stage = if is_post {
            &element.post
        } else {
            &element.paint
        };
        let local_damage = damage_rects.map(|rects| {
            let inverse = base_transform.inverse();
            rects.iter()
                .map(|rect| inverse.transform_rect_bbox(*rect))
                .collect::<Vec<_>>()
        });
        let render_size = render_size.unwrap_or_else(|| self.paint_state.renderer().size());
        let renderer = self.paint_state.renderer_mut();
        replay_stage(
            stage,
            renderer,
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
        let mut commands = Vec::new();
        let mut recorder = RecordingRenderer::new(&mut commands);
        let uses_layer_clip = self.paint_state.renderer().uses_layer_clip();
        let is_vger = self.paint_state.renderer().is_vger();
        let world_transform = self.element_base_transform(element_id);
        let font_size_cx = view_state.borrow().layout_props.font_size_cx();

        {
            // Create per-target PaintCx
            let mut cx = PaintCx {
                window_state: self.window_state,
                recorder: &mut recorder,
                uses_layer_clip,
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
        stage.set_commands(commands, snapshot.layer_candidate());
        element.snapshot = Some(snapshot);
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
        if self.uses_layer_clip {
            use peniko::Mix;
            self.push_layer(Mix::Normal, 1.0, Affine::IDENTITY, shape);
        } else {
            self.recorder.clip(shape);
        }
    }

    /// Clear clip
    pub fn pop_clip(&mut self) {
        if self.uses_layer_clip {
            self.pop_layer();
        } else {
            self.recorder.clear_clip();
        }
    }

    // Note: get_transform/set_transform removed as Renderer doesn't expose transform()
    // Views that previously used save/restore should use clip/clear_clip instead

    pub fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<peniko::BrushRef<'b>>,
        stroke: &'s peniko::kurbo::Stroke,
    ) {
        self.recorder.stroke(shape, brush, stroke);
    }

    pub fn fill<'b>(
        &mut self,
        path: &impl peniko::kurbo::Shape,
        brush: impl Into<peniko::BrushRef<'b>>,
        blur_radius: f64,
    ) {
        self.recorder.fill(path, brush, blur_radius);
    }

    pub fn push_layer(
        &mut self,
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        self.recorder.push_layer(blend, alpha, transform, clip);
    }

    pub fn pop_layer(&mut self) {
        self.recorder.pop_layer();
    }

    pub fn draw_img(&mut self, img: floem_renderer::Img<'_>, rect: peniko::kurbo::Rect) {
        self.recorder.draw_img(img, rect);
    }

    pub fn draw_glyphs<'a>(
        &mut self,
        origin: peniko::kurbo::Point,
        props: &floem_renderer::text::GlyphRunProps<'a>,
        glyphs: impl Iterator<Item = floem_renderer::text::Glyph> + 'a,
    ) {
        self.recorder.draw_glyphs(origin, props, glyphs);
    }

    pub fn draw_svg<'b>(
        &mut self,
        svg: floem_renderer::Svg<'b>,
        rect: peniko::kurbo::Rect,
        brush: Option<impl Into<peniko::BrushRef<'b>>>,
    ) {
        self.recorder.draw_svg(svg, rect, brush);
    }

    pub fn set_transform(&mut self, transform: Affine) {
        self.recorder.set_transform(transform);
    }

    pub fn set_z_index(&mut self, z_index: i32) {
        self.recorder.set_z_index(z_index);
    }

    pub fn is_vger(&self) -> bool {
        self.is_vger
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
