//! Paint context and state for rendering views.
//!
//! This module contains the types used during the paint phase:
//! - [`PaintCx`] - Context for painting views
//! - [`PaintState`] - State for the renderer (pending or initialized)

pub mod border_path_iter;
pub(crate) mod composition;
pub mod display_list;
pub mod renderer;

use crate::effects::{Composite, Filter};
use crate::gpu_resources::{GpuResourceError, GpuResources};
pub use border_path_iter::{BorderPath, BorderPathEvent};
use imaging::Painter;
use peniko::kurbo::{Affine, RoundedRect};

#[cfg(feature = "crossbeam")]
use crossbeam::channel::Receiver;
#[cfg(not(feature = "crossbeam"))]
use std::sync::mpsc::Receiver;

use crate::ElementId;
use crate::style::FontSizeCx;
use crate::view::ViewId;
use crate::view::{paint_bg, paint_border, paint_outline};
use crate::window::state::WindowState;
use composition::clip_scene_layers_to_viewport;
use display_list::{ElementSnapshot, StageRecorder};

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

/// Global paint context - holds shared state for entire paint pass
/// Similar to GlobalEventCx in event dispatch
pub struct GlobalPaintCx<'a> {
    pub window_state: &'a mut WindowState,
    pub gpu_resources: Option<GpuResources>,
}

/// Per-target paint context - created for each visual node
/// Similar to EventCx in event dispatch
pub struct PaintCx<'a> {
    /// Reference to global paint state
    pub window_state: &'a mut WindowState,
    gpu_resources: Option<&'a GpuResources>,
    pub painter: Painter<'a, StageRecorder, Filter, Composite, crate::effects::Brush>,
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
        self.update_composition_plan();
    }

    fn update_composition_plan(&mut self) {
        let effective_scale = self.window_state.effective_scale();
        let mut plan = self.window_state.display_list.lower_composition_plan(
            effective_scale,
            &self.window_state.surface_image_registry.borrow(),
        );
        clip_scene_layers_to_viewport(&mut plan, self.window_state.root_size);
        self.window_state.composition_plan = plan;
    }

    fn element_base_transform(&mut self, element_id: ElementId) -> Affine {
        // Get state from box tree for this visual node
        let box_tree = self.window_state.box_tree.borrow();
        box_tree.world_transform(element_id.0).unwrap_or_default()
    }

    /// Record a single visual node in local coordinates.
    pub(crate) fn record_visual_node(&mut self, element_id: ElementId, is_post: bool) {
        let box_tree = self.window_state.box_tree.borrow_mut();
        let layout_rect_local = box_tree.local_bounds(element_id.0).unwrap_or_default();
        let snapshot = ElementSnapshot::from_box_tree(&box_tree, element_id);
        drop(box_tree);

        let view_id = element_id.owning_id();
        let view = view_id.view();
        let view_state = view_id.state();
        let font_size_cx = view_state.borrow().layout_props.font_size_cx();
        let font_embolden = view_state.borrow().view_style_props.font_embolden();
        let effective_scale = self.window_state.effective_scale();
        let mut recorder = {
            let element = self.window_state.display_list.element_mut(element_id);
            let stage = if is_post {
                &mut element.post
            } else {
                &mut element.paint
            };
            let mut recorder = StageRecorder::from_stage(
                stage,
                self.window_state.surface_image_registry.clone(),
                font_size_cx,
                effective_scale,
            );
            recorder.clear();
            recorder
        };

        let layout_rect = layout_rect_local;
        let is_vger = false;
        let world_transform = self.element_base_transform(element_id);
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
                font_embolden,
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

        {
            let element = self.window_state.display_list.element_mut(element_id);
            let stage = if is_post {
                &mut element.post
            } else {
                &mut element.paint
            };
            recorder.finish(stage);
        }
        self.window_state
            .display_list
            .element_mut(element_id)
            .snapshot = Some(snapshot);
        self.window_state
            .display_list
            .mark_composed_dirty(element_id);
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

    pub fn is_vger(&self) -> bool {
        self.is_vger
    }
}

pub(crate) enum PaintState {
    /// The renderer is not yet initialized. This state is used to wait for the GPU resources to be acquired.
    PendingGpuResources {
        rx: Receiver<
            Result<(GpuResources, subduction::wgpu::ExternalSurfaceCapabilities), GpuResourceError>,
        >,
    },
    /// The renderer is initialized and ready to paint.
    Initialized,
    Headless,
}

impl PaintState {
    pub fn new_pending(
        rx: Receiver<
            Result<(GpuResources, subduction::wgpu::ExternalSurfaceCapabilities), GpuResourceError>,
        >,
    ) -> Self {
        Self::PendingGpuResources { rx }
    }

    pub(crate) fn is_initialized(&self) -> bool {
        matches!(self, Self::Initialized | Self::Headless)
    }
}
