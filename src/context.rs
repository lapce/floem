use floem_reactive::Scope;
use floem_renderer::Renderer as FloemRenderer;
use floem_renderer::gpu_resources::{GpuResourceError, GpuResources};
use peniko::kurbo::{Affine, Point, Rect, RoundedRect, Shape, Size, Vec2};
use smallvec::SmallVec;
use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::Arc,
};
use winit::window::Window;

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

#[cfg(feature = "crossbeam")]
use crossbeam::channel::Receiver;
#[cfg(not(feature = "crossbeam"))]
use std::sync::mpsc::Receiver;

use taffy::prelude::NodeId;
use crate::animate::{AnimStateKind, RepeatMode};
use crate::easing::{Easing, Linear};
use crate::menu::Menu;
use crate::renderer::Renderer;
use crate::style::{Disabled, DisplayProp, Focusable, Hidden, OverflowX, OverflowY, ZIndex};
use crate::view::{IsHiddenState, StackingInfo};
use crate::{
    action::exec_after,
    event::{Event, EventPropagation},
    id::ViewId,
    inspector::CaptureState,
    style::{Style, StyleProp},
    view::{View, paint_bg, paint_border, paint_outline},
    view::ChangeFlags,
    window_state::WindowState,
};

pub type EventCallback = dyn FnMut(&Event) -> EventPropagation;
pub type ResizeCallback = dyn Fn(Rect);
pub type MenuCallback = dyn Fn() -> Menu;

#[derive(Default)]
pub(crate) struct ResizeListeners {
    pub(crate) rect: Rect,
    pub(crate) callbacks: Vec<Rc<ResizeCallback>>,
}

/// Listeners for when the view moves to a different position in the window
#[derive(Default)]
pub(crate) struct MoveListeners {
    pub(crate) window_origin: Point,
    pub(crate) callbacks: Vec<Rc<dyn Fn(Point)>>,
}

pub(crate) type CleanupListeners = Vec<Rc<dyn Fn()>>;

pub(crate) enum FrameUpdate {
    Style(ViewId),
    Layout(ViewId),
    Paint(ViewId),
}

// Re-export EventCx from event module for backward compatibility
pub use crate::event::EventCx;
// Re-export DragState from window_state
pub use crate::window_state::DragState;
// Re-export stacking context types from event module
pub(crate) use crate::event::{collect_stacking_context_items, invalidate_stacking_cache};
/// The interaction state of a view, used to determine which style selectors apply.
///
/// This struct captures the current state of user interaction with a view,
/// such as whether it's hovered, focused, being clicked, etc. This state is
/// used during style computation to apply conditional styles like `:hover`,
/// `:active`, `:focus`, etc.
#[derive(Default, Debug, Clone, Copy)]
pub struct InteractionState {
    /// Whether the pointer is currently over this view.
    pub is_hovered: bool,
    /// Whether this view is in a selected state.
    pub is_selected: bool,
    /// Whether this view is disabled.
    pub is_disabled: bool,
    /// Whether this view has keyboard focus.
    pub is_focused: bool,
    /// Whether this view is being clicked (pointer down but not yet up).
    pub is_clicking: bool,
    /// Whether dark mode is enabled.
    pub is_dark_mode: bool,
    /// Whether a file is being dragged over this view.
    pub is_file_hover: bool,
    /// Whether keyboard navigation is active.
    pub using_keyboard_navigation: bool,
}

pub struct StyleCx<'a> {
    pub window_state: &'a mut WindowState,
    pub(crate) current_view: ViewId,
    /// current is used as context for carrying inherited properties between views
    pub(crate) current: Rc<Style>,
    pub(crate) direct: Style,
    saved: Vec<Rc<Style>>,
    pub(crate) now: Instant,
    saved_disabled: Vec<bool>,
    saved_selected: Vec<bool>,
    saved_hidden: Vec<bool>,
    disabled: bool,
    hidden: bool,
    selected: bool,
}

impl<'a> StyleCx<'a> {
    pub(crate) fn new(window_state: &'a mut WindowState, root: ViewId) -> Self {
        Self {
            window_state,
            current_view: root,
            current: Default::default(),
            direct: Default::default(),
            saved: Default::default(),
            now: Instant::now(),
            saved_disabled: Default::default(),
            saved_selected: Default::default(),
            saved_hidden: Default::default(),
            disabled: false,
            hidden: false,
            selected: false,
        }
    }

    /// Marks the current context as selected.
    pub fn selected(&mut self) {
        self.selected = true;
    }

    pub fn hidden(&mut self) {
        self.hidden = true;
    }

    fn get_interact_state(&self, id: &ViewId) -> InteractionState {
        InteractionState {
            is_selected: self.selected || id.is_selected(),
            is_hovered: self.window_state.is_hovered(id),
            is_disabled: id.is_disabled() || self.disabled,
            is_focused: self.window_state.is_focused(id),
            is_clicking: self.window_state.is_clicking(id),
            is_dark_mode: self.window_state.is_dark_mode(),
            is_file_hover: self.window_state.is_file_hover(id),
            using_keyboard_navigation: self.window_state.keyboard_navigation,
        }
    }

    /// Internal method used by Floem to compute the styles for the view.
    pub fn style_view(&mut self, view_id: ViewId) {
        self.save();
        let view = view_id.view();
        let view_state = view_id.state();
        {
            let mut view_state = view_state.borrow_mut();
            if !view_state.requested_changes.contains(ChangeFlags::STYLE)
                && !view_state
                    .requested_changes
                    .contains(ChangeFlags::VIEW_STYLE)
            {
                self.restore();
                return;
            }
            view_state.requested_changes.remove(ChangeFlags::STYLE);
        }
        let view_class = view.borrow().view_class();
        {
            let mut view_state = view_state.borrow_mut();
            if view_state
                .requested_changes
                .contains(ChangeFlags::VIEW_STYLE)
            {
                view_state.requested_changes.remove(ChangeFlags::VIEW_STYLE);
                if let Some(view_style) = view.borrow().view_style() {
                    let offset = view_state.view_style_offset;
                    view_state.style.set(offset, view_style);
                }
            }
            // Propagate style requests to children if needed.
            if view_state.request_style_recursive {
                view_state.request_style_recursive = false;
                let children = view_id.children();
                for child in children {
                    let view_state = child.state();
                    let mut state = view_state.borrow_mut();
                    state.request_style_recursive = true;
                    state.requested_changes.insert(ChangeFlags::STYLE);
                }
            }
        }

        let view_interact_state = self.get_interact_state(&view_id);
        self.disabled = view_interact_state.is_disabled;
        let (mut new_frame, classes_applied) = view_id.state().borrow_mut().compute_combined(
            view_interact_state,
            self.window_state.screen_size_bp,
            view_class,
            &self.current,
            self.hidden,
        );
        if classes_applied {
            let children = view_id.children();
            for child in children {
                let view_state = child.state();
                let mut state = view_state.borrow_mut();
                state.request_style_recursive = true;
                state.requested_changes.insert(ChangeFlags::STYLE);
            }
        }

        self.direct = view_state.borrow().combined_style.clone();
        Style::apply_only_inherited(&mut self.current, &self.direct);
        let mut computed_style = (*self.current).clone();
        computed_style.apply_mut(self.direct.clone());
        CaptureState::capture_style(view_id, self, computed_style.clone());
        if computed_style.get(Focusable)
            && !computed_style.get(Disabled)
            && !computed_style.get(Hidden)
            && computed_style.get(DisplayProp) != taffy::Display::None
        {
            self.window_state.focusable.insert(view_id);
        } else {
            self.window_state.focusable.remove(&view_id);
        }
        view_state.borrow_mut().computed_style = computed_style;
        self.hidden |= view_id.is_hidden();

        // This is used by the `request_transition` and `style` methods below.
        self.current_view = view_id;

        {
            let mut view_state = view_state.borrow_mut();
            // Extract the relevant layout properties so the content rect can be calculated
            // when painting.
            view_state.layout_props.read_explicit(
                &self.direct,
                &self.current,
                &self.now,
                &mut new_frame,
            );
            if new_frame {
                // If any transitioning layout props, schedule layout.
                self.window_state.schedule_layout(view_id);
            }

            view_state.view_style_props.read_explicit(
                &self.direct,
                &self.current,
                &self.now,
                &mut new_frame,
            );
            if new_frame && !self.hidden {
                self.window_state.schedule_style(view_id);
            }
        }
        // If there's any changes to the Taffy style, request layout.
        let layout_style = view_state.borrow().layout_props.to_style();
        let taffy_style = self.direct.clone().apply(layout_style).to_taffy_style();
        if taffy_style != view_state.borrow().taffy_style {
            view_state.borrow_mut().taffy_style = taffy_style;
            self.window_state.schedule_layout(view_id);
        }

        view.borrow_mut().style_pass(self);

        let old_is_hidden_state = view_state.borrow().is_hidden_state;
        let mut is_hidden_state = old_is_hidden_state;
        let computed_display = view_state.borrow().combined_style.get(DisplayProp);
        is_hidden_state.transition(
            computed_display,
            || {
                let count = animations_on_remove(view_id, Scope::current());
                view_state.borrow_mut().num_waiting_animations = count;
                count > 0
            },
            || {
                animations_on_create(view_id);
            },
            || {
                stop_reset_remove_animations(view_id);
            },
            || view_state.borrow().num_waiting_animations,
        );

        // Invalidate stacking cache if hidden state changed
        if old_is_hidden_state != is_hidden_state {
            invalidate_stacking_cache(view_id);
        }

        view_state.borrow_mut().is_hidden_state = is_hidden_state;
        let modified = view_state
            .borrow()
            .combined_style
            .clone()
            .apply_opt(is_hidden_state.get_display(), Style::display);

        view_state.borrow_mut().combined_style = modified;

        let mut transform = Affine::IDENTITY;

        let transform_x = match view_state.borrow().layout_props.translate_x() {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => pct / 100.,
        };
        let transform_y = match view_state.borrow().layout_props.translate_y() {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => pct / 100.,
        };
        transform *= Affine::translate(Vec2 {
            x: transform_x,
            y: transform_y,
        });

        let scale_x = view_state.borrow().layout_props.scale_x().0 / 100.;
        let scale_y = view_state.borrow().layout_props.scale_y().0 / 100.;
        let size = view_id.layout_rect();
        let center_x = size.width() / 2.;
        let center_y = size.height() / 2.;
        transform *= Affine::translate(Vec2 {
            x: center_x,
            y: center_y,
        });
        transform *= Affine::scale_non_uniform(scale_x, scale_y);
        let rotation = view_state.borrow().layout_props.rotation().0;
        transform *= Affine::rotate(rotation);
        transform *= Affine::translate(Vec2 {
            x: -center_x,
            y: -center_y,
        });

        view_state.borrow_mut().transform = transform;

        // Compute stacking context info
        // A view creates a stacking context if it has:
        // - Any explicit z-index value (including 0, since None means "auto" in CSS terms)
        // - Any non-identity transform
        // - A viewport (scroll views) - these offset their children's coordinates
        // - Overflow set to Scroll or Hidden (scroll views, clip views) - they manage child painting
        // Note: Unlike our previous implementation, `position: absolute` alone does NOT
        // create a stacking context per CSS spec. It needs explicit z-index to do so.
        // Missing CSS triggers (not implemented in floem): opacity < 1, filter, clip-path,
        // mask, isolation: isolate, mix-blend-mode, contain.
        let z_index = view_state.borrow().combined_style.get(ZIndex);
        let has_transform = transform != Affine::IDENTITY;
        let has_viewport = view_state.borrow().viewport.is_some();
        // Check if overflow is set to Scroll or Hidden - these views manage their own child painting
        let overflow_x = view_state.borrow().combined_style.get(OverflowX);
        let overflow_y = view_state.borrow().combined_style.get(OverflowY);
        let has_scroll_overflow = matches!(
            overflow_x,
            taffy::Overflow::Scroll | taffy::Overflow::Hidden
        ) || matches!(
            overflow_y,
            taffy::Overflow::Scroll | taffy::Overflow::Hidden
        );

        let creates_context =
            z_index.is_some() || has_transform || has_viewport || has_scroll_overflow;

        let new_stacking_info = StackingInfo {
            creates_context,
            effective_z_index: z_index.unwrap_or(0),
        };

        // Invalidate stacking cache if stacking info changed
        {
            let mut vs = view_state.borrow_mut();
            let old_info = vs.stacking_info;
            if old_info.creates_context != new_stacking_info.creates_context
                || old_info.effective_z_index != new_stacking_info.effective_z_index
            {
                invalidate_stacking_cache(view_id);
            }
            vs.stacking_info = new_stacking_info;
        }

        self.restore();
    }

    pub fn now(&self) -> Instant {
        self.now
    }

    pub fn save(&mut self) {
        self.saved.push(self.current.clone());
        self.saved_disabled.push(self.disabled);
        self.saved_selected.push(self.selected);
        self.saved_hidden.push(self.hidden);
    }

    pub fn restore(&mut self) {
        self.current = self.saved.pop().unwrap_or_default();
        self.disabled = self.saved_disabled.pop().unwrap_or_default();
        self.selected = self.saved_selected.pop().unwrap_or_default();
        self.hidden = self.saved_hidden.pop().unwrap_or_default();
    }

    pub fn get_prop<P: StyleProp>(&self, _prop: P) -> Option<P::Type> {
        self.direct
            .get_prop::<P>()
            .or_else(|| self.current.get_prop::<P>())
    }

    pub fn style(&self) -> Style {
        (*self.current).clone().apply(self.direct.clone())
    }

    pub fn direct_style(&self) -> &Style {
        &self.direct
    }

    pub fn indirect_style(&self) -> &Style {
        &self.current
    }

    pub fn request_transition(&mut self) {
        let id = self.current_view;
        self.window_state.schedule_style(id);
    }
}

pub struct ComputeLayoutCx<'a> {
    pub window_state: &'a mut WindowState,
    pub(crate) viewport: Rect,
    pub(crate) window_origin: Point,
    /// The accumulated clip rect in window coordinates. Views outside this rect
    /// are clipped by ancestor overflow:hidden/scroll containers.
    pub(crate) clip_rect: Rect,
    pub(crate) saved_viewports: Vec<Rect>,
    pub(crate) saved_window_origins: Vec<Point>,
    pub(crate) saved_clip_rects: Vec<Rect>,
}

impl<'a> ComputeLayoutCx<'a> {
    pub(crate) fn new(window_state: &'a mut WindowState, viewport: Rect) -> Self {
        Self {
            window_state,
            viewport,
            window_origin: Point::ZERO,
            // Start with a large clip rect that effectively means "no clipping"
            clip_rect: Rect::new(-1e9, -1e9, 1e9, 1e9),
            saved_viewports: Vec::new(),
            saved_window_origins: Vec::new(),
            saved_clip_rects: Vec::new(),
        }
    }

    pub fn window_origin(&self) -> Point {
        self.window_origin
    }

    pub fn save(&mut self) {
        self.saved_viewports.push(self.viewport);
        self.saved_window_origins.push(self.window_origin);
        self.saved_clip_rects.push(self.clip_rect);
    }

    pub fn restore(&mut self) {
        self.viewport = self.saved_viewports.pop().unwrap_or_default();
        self.window_origin = self.saved_window_origins.pop().unwrap_or_default();
        self.clip_rect = self.saved_clip_rects.pop().unwrap_or(Rect::new(-1e9, -1e9, 1e9, 1e9));
    }

    pub fn current_viewport(&self) -> Rect {
        self.viewport
    }

    /// Internal method used by Floem. This method derives its calculations based on the [Taffy Node](taffy::tree::NodeId) returned by the `View::layout` method.
    ///
    /// It's responsible for:
    /// - calculating and setting the view's origin (local coordinates and window coordinates)
    /// - calculating and setting the view's viewport
    /// - invoking any attached `context::ResizeListener`s
    ///
    /// Returns the bounding rect that encompasses this view and its children
    pub fn compute_view_layout(&mut self, id: ViewId) -> Option<Rect> {
        let view_state = id.state();

        if view_state.borrow().is_hidden_state == IsHiddenState::Hidden {
            view_state.borrow_mut().layout_rect = Rect::ZERO;
            return None;
        }

        self.save();

        let layout = id.get_layout().unwrap_or_default();
        let origin = Point::new(layout.location.x as f64, layout.location.y as f64);
        let this_viewport = view_state.borrow().viewport;
        let this_viewport_origin = this_viewport.unwrap_or_default().origin().to_vec2();
        let size = Size::new(layout.size.width as f64, layout.size.height as f64);
        let parent_viewport = self.viewport.with_origin(
            Point::new(
                self.viewport.x0 - layout.location.x as f64,
                self.viewport.y0 - layout.location.y as f64,
            ) + this_viewport_origin,
        );
        self.viewport = parent_viewport.intersect(size.to_rect());
        if let Some(this_viewport) = this_viewport {
            self.viewport = self.viewport.intersect(this_viewport);
        }

        let window_origin = origin + self.window_origin.to_vec2() - this_viewport_origin;
        self.window_origin = window_origin;
        {
            view_state.borrow_mut().window_origin = window_origin;
        }

        // Compute this view's clip_rect in window coordinates.
        // It's the intersection of the parent's clip_rect with this view's visible area.
        let view_rect_in_window = size.to_rect().with_origin(window_origin);
        let view_clip_rect = self.clip_rect.intersect(view_rect_in_window);

        // If this view has a viewport (scroll view), it clips its children.
        // Update self.clip_rect for child layout.
        if this_viewport.is_some() {
            self.clip_rect = view_clip_rect;
        }

        {
            let view_state = view_state.borrow();
            let mut resize_listeners = view_state.resize_listeners.borrow_mut();

            let new_rect = size.to_rect().with_origin(origin);
            if new_rect != resize_listeners.rect {
                resize_listeners.rect = new_rect;

                let callbacks = resize_listeners.callbacks.clone();

                // explicitly dropping borrows before using callbacks
                std::mem::drop(resize_listeners);
                std::mem::drop(view_state);

                for callback in callbacks {
                    (*callback)(new_rect);
                }
            }
        }

        {
            let view_state = view_state.borrow();
            let mut move_listeners = view_state.move_listeners.borrow_mut();

            if window_origin != move_listeners.window_origin {
                move_listeners.window_origin = window_origin;

                let callbacks = move_listeners.callbacks.clone();

                // explicitly dropping borrows before using callbacks
                std::mem::drop(move_listeners);
                std::mem::drop(view_state);

                for callback in callbacks {
                    (*callback)(window_origin);
                }
            }
        }

        let view = id.view();
        let child_layout_rect = view.borrow_mut().compute_layout(self);

        let layout_rect = size.to_rect().with_origin(self.window_origin);
        let layout_rect = if let Some(child_layout_rect) = child_layout_rect {
            layout_rect.union(child_layout_rect)
        } else {
            layout_rect
        };

        let transform = view_state.borrow().transform;
        let layout_rect = transform.transform_rect_bbox(layout_rect);

        // Compute the cumulative transform from local coordinates to root (window) coordinates.
        // This combines translation to window_origin with the view's CSS transform.
        // To convert from root coords to local: local = local_to_root.inverse() * root
        let local_to_root_transform =
            Affine::translate((self.window_origin.x, self.window_origin.y)) * transform;

        {
            let mut vs = view_state.borrow_mut();
            vs.layout_rect = layout_rect;
            vs.clip_rect = view_clip_rect;
            vs.local_to_root_transform = local_to_root_transform;
        }

        self.restore();

        Some(layout_rect)
    }
}

/// Holds current layout state for given position in the tree.
/// You'll use this in the `View::layout` implementation to call `layout_node` on children and to access any font
pub struct LayoutCx<'a> {
    pub window_state: &'a mut WindowState,
}

impl<'a> LayoutCx<'a> {
    pub(crate) fn new(window_state: &'a mut WindowState) -> Self {
        Self { window_state }
    }

    /// Responsible for invoking the recalculation of style and thus the layout and
    /// creating or updating the layout of child nodes within the closure.
    ///
    /// You should ensure that all children are laid out within the closure and/or whatever
    /// other work you need to do to ensure that the layout for the returned nodes is correct.
    pub fn layout_node(
        &mut self,
        id: ViewId,
        has_children: bool,
        mut children: impl FnMut(&mut LayoutCx) -> Vec<NodeId>,
    ) -> NodeId {
        let view_state = id.state();
        let node = view_state.borrow().node;
        if !view_state
            .borrow()
            .requested_changes
            .contains(ChangeFlags::LAYOUT)
        {
            return node;
        }
        view_state
            .borrow_mut()
            .requested_changes
            .remove(ChangeFlags::LAYOUT);
        let layout_style = view_state.borrow().layout_props.to_style();
        let animate_out_display = view_state.borrow().is_hidden_state.get_display();
        let style = view_state
            .borrow()
            .combined_style
            .clone()
            .apply(layout_style)
            .apply_opt(animate_out_display, Style::display)
            .to_taffy_style();
        let _ = id.taffy().borrow_mut().set_style(node, style);

        if has_children {
            let nodes = children(self);
            let _ = id.taffy().borrow_mut().set_children(node, &nodes);
        }

        node
    }

    /// Internal method used by Floem to invoke the user-defined `View::layout` method.
    pub fn layout_view(&mut self, view: &mut dyn View) -> NodeId {
        view.layout(self)
    }
}

std::thread_local! {
    /// Holds the ID of a View being painted very briefly if it is being rendered as
    /// a moving drag image.  Since that is a relatively unusual thing to need, it
    /// makes more sense to use a thread local for it and avoid cluttering the fields
    /// and memory footprint of PaintCx or PaintState or ViewId with a field for it.
    /// This is ephemerally set before paint calls that are painting the view in a
    /// location other than its natural one for purposes of drag and drop.
    static CURRENT_DRAG_PAINTING_ID : std::cell::Cell<Option<ViewId>> = const { std::cell::Cell::new(None) };
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
    /// When painting a stacking context, this tracks views whose children are handled
    /// by the parent stacking context (not by the view itself). When paint_children is
    /// called for such a view, it should be a no-op.
    pub(crate) skip_children_for: Option<ViewId>,
    pub gpu_resources: Option<GpuResources>,
    pub window: Arc<dyn Window>,
    #[cfg(feature = "vello")]
    pub layer_count: usize,
    #[cfg(feature = "vello")]
    pub saved_layer_counts: Vec<usize>,
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

    /// Paint the children of this view using true CSS stacking context semantics.
    ///
    /// Views that create stacking contexts have their children bounded within them.
    /// Views that don't create stacking contexts allow their children to "escape"
    /// and participate in the parent's stacking context (z-index sorting).
    pub fn paint_children(&mut self, id: ViewId) {
        // If this view's children are being handled by a parent stacking context, skip
        if self.skip_children_for == Some(id) {
            return;
        }

        // Collect all items participating in this stacking context
        let items = collect_stacking_context_items(id);

        // Track currently applied transforms in root-to-item order.
        // Using diff-based approach: only push/pop transforms that change between items.
        // This is much faster for siblings (common case) since they share the same parent chain.
        let mut current_chain: SmallVec<[ViewId; 8]> = SmallVec::new();

        for item in items.iter() {
            if item.view_id.is_hidden() {
                continue;
            }

            // Find common prefix length between current_chain and item's parent chain.
            // parent_chain is [immediate_parent, ..., root_child], we compare in root-to-item order.
            // current_chain[i] should match parent_chain[len - 1 - i]
            let item_chain_len = item.parent_chain.len();
            let common_len = current_chain
                .iter()
                .zip(item.parent_chain.iter().rev())
                .take_while(|(a, b)| a == b)
                .count();

            // Pop transforms that are no longer in the path
            for _ in common_len..current_chain.len() {
                self.restore();
            }
            current_chain.truncate(common_len);

            // Push new transforms for the remaining ancestors
            // Iterate from common_len to item_chain_len in root-to-item order
            for i in common_len..item_chain_len {
                let ancestor = item.parent_chain[item_chain_len - 1 - i];
                self.save();
                self.transform(ancestor);
                current_chain.push(ancestor);
            }

            // If this item doesn't create a stacking context, mark it so its
            // paint_children call will be a no-op (children are in our flat list)
            if !item.creates_context {
                self.skip_children_for = Some(item.view_id);
            }

            // Paint the view
            self.paint_view(item.view_id);

            // Clear the skip flag
            self.skip_children_for = None;

            // Don't pop transforms here - leave them for the next item to potentially reuse
        }

        // Pop all remaining transforms after processing all items
        for _ in 0..current_chain.len() {
            self.restore();
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
        if let Some(dragging) = self.window_state.dragging.as_ref() {
            if dragging.id == id {
                // Store the pending drag paint info - actual painting happens after tree traversal
                self.pending_drag_paint = Some(PendingDragPaint {
                    id,
                    base_transform: self.transform,
                });
            }
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
            self.transform *= Affine::translate(Vec2 {
                x: offset.x as f64,
                y: offset.y as f64,
            });
            self.transform *= id.state().borrow().transform;

            self.paint_state
                .renderer_mut()
                .set_transform(self.transform);

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
        renderer: crate::renderer::Renderer,
    },
    /// The renderer is initialized and ready to paint.
    Initialized { renderer: crate::renderer::Renderer },
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
        let renderer = crate::renderer::Renderer::new(
            window.clone(),
            gpu_resources,
            surface,
            scale,
            size,
            font_embolden,
        );
        Self::Initialized { renderer }
    }

    pub(crate) fn renderer(&self) -> &crate::renderer::Renderer {
        match self {
            PaintState::PendingGpuResources { renderer, .. } => renderer,
            PaintState::Initialized { renderer } => renderer,
        }
    }

    pub(crate) fn renderer_mut(&mut self) -> &mut crate::renderer::Renderer {
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

pub struct UpdateCx<'a> {
    pub window_state: &'a mut WindowState,
}

impl Deref for PaintCx<'_> {
    type Target = crate::renderer::Renderer;

    fn deref(&self) -> &Self::Target {
        self.paint_state.renderer()
    }
}

impl DerefMut for PaintCx<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.paint_state.renderer_mut()
    }
}

fn animations_on_remove(id: ViewId, scope: Scope) -> u16 {
    let mut wait_for = 0;
    let state = id.state();
    let mut state = state.borrow_mut();
    state.num_waiting_animations = 0;
    let animations = &mut state.animations.stack;
    let mut request_style = false;
    for anim in animations {
        if anim.run_on_remove && !matches!(anim.repeat_mode, RepeatMode::LoopForever) {
            anim.reverse_mut();
            request_style = true;
            wait_for += 1;
            let trigger = anim.on_visual_complete;
            scope.create_updater(
                move || trigger.track(),
                move |_| {
                    id.transition_anim_complete();
                },
            );
        }
    }
    drop(state);
    if request_style {
        id.request_style();
    }

    id.children()
        .into_iter()
        .fold(wait_for, |acc, id| acc + animations_on_remove(id, scope))
}
fn stop_reset_remove_animations(id: ViewId) {
    let state = id.state();
    let mut state = state.borrow_mut();
    let animations = &mut state.animations.stack;
    let mut request_style = false;
    for anim in animations {
        if anim.run_on_remove
            && anim.state_kind() == AnimStateKind::PassInProgress
            && !matches!(anim.repeat_mode, RepeatMode::LoopForever)
        {
            anim.start_mut();
            request_style = true;
        }
    }
    drop(state);
    if request_style {
        id.request_style();
    }

    id.children()
        .into_iter()
        .for_each(stop_reset_remove_animations)
}

fn animations_on_create(id: ViewId) {
    let state = id.state();
    let mut state = state.borrow_mut();
    state.num_waiting_animations = 0;
    let animations = &mut state.animations.stack;
    let mut request_style = false;
    for anim in animations {
        if anim.run_on_create && !matches!(anim.repeat_mode, RepeatMode::LoopForever) {
            anim.start_mut();
            request_style = true;
        }
    }
    drop(state);
    if request_style {
        id.request_style();
    }

    id.children().into_iter().for_each(animations_on_create);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::StackingContextItem;

    /// Helper to create a ViewId and set its z-index
    /// Views with explicit z-index create stacking contexts
    fn create_view_with_z_index(z_index: Option<i32>) -> ViewId {
        let id = ViewId::new();
        let state = id.state();
        state.borrow_mut().stacking_info = StackingInfo {
            creates_context: z_index.is_some(),
            effective_z_index: z_index.unwrap_or(0),
        };
        id
    }

    /// Helper to create a ViewId that does NOT create a stacking context
    /// Its children will participate in the parent's stacking context
    fn create_view_no_stacking_context() -> ViewId {
        let id = ViewId::new();
        let state = id.state();
        state.borrow_mut().stacking_info = StackingInfo {
            creates_context: false,
            effective_z_index: 0,
        };
        id
    }

    /// Helper to set up parent with children (also sets parent pointers)
    fn setup_parent_with_children(children: Vec<ViewId>) -> ViewId {
        let parent = ViewId::new();
        set_children_with_parents(parent, children);
        parent
    }

    /// Helper to set children AND parent pointers (for test purposes)
    fn set_children_with_parents(parent: ViewId, children: Vec<ViewId>) {
        for child in &children {
            child.set_parent(parent);
        }
        parent.set_children_ids(children);
    }

    /// Helper to extract view IDs from stacking context items
    fn get_view_ids(items: &[StackingContextItem]) -> Vec<ViewId> {
        items.iter().map(|item| item.view_id).collect()
    }

    /// Helper to extract z-indices from stacking context items
    fn get_z_indices_from_items(items: &[StackingContextItem]) -> Vec<i32> {
        items.iter().map(|item| item.z_index).collect()
    }

    #[test]
    fn test_no_children() {
        let parent = ViewId::new();
        let result = collect_stacking_context_items(parent);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_child() {
        let child = create_view_with_z_index(Some(5));
        let parent = setup_parent_with_children(vec![child]);

        let result = collect_stacking_context_items(parent);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].view_id, child);
    }

    #[test]
    fn test_children_no_z_index_preserves_dom_order() {
        // All children with default z-index (0) should preserve DOM order
        // Note: children without explicit z-index don't create stacking contexts
        let child1 = create_view_no_stacking_context();
        let child2 = create_view_no_stacking_context();
        let child3 = create_view_no_stacking_context();
        let parent = setup_parent_with_children(vec![child1, child2, child3]);

        let result = collect_stacking_context_items(parent);
        assert_eq!(get_view_ids(&result), vec![child1, child2, child3]);
    }

    #[test]
    fn test_basic_z_index_sorting() {
        // Children with different z-indices should be sorted ascending
        let child_z10 = create_view_with_z_index(Some(10));
        let child_z1 = create_view_with_z_index(Some(1));
        let child_z5 = create_view_with_z_index(Some(5));
        // DOM order: z10, z1, z5
        let parent = setup_parent_with_children(vec![child_z10, child_z1, child_z5]);

        let result = collect_stacking_context_items(parent);
        // Paint order should be: z1, z5, z10 (ascending)
        assert_eq!(get_z_indices_from_items(&result), vec![1, 5, 10]);
        assert_eq!(get_view_ids(&result), vec![child_z1, child_z5, child_z10]);
    }

    #[test]
    fn test_negative_z_index() {
        // Negative z-index should sort before positive
        let child_pos = create_view_with_z_index(Some(1));
        let child_neg = create_view_with_z_index(Some(-1));
        let child_zero = create_view_with_z_index(Some(0));
        // DOM order: pos, neg, zero
        let parent = setup_parent_with_children(vec![child_pos, child_neg, child_zero]);

        let result = collect_stacking_context_items(parent);
        // Paint order: -1, 0, 1
        assert_eq!(get_z_indices_from_items(&result), vec![-1, 0, 1]);
    }

    #[test]
    fn test_equal_z_index_preserves_dom_order() {
        // Children with same z-index should preserve DOM order (stable sort)
        let child1 = create_view_with_z_index(Some(5));
        let child2 = create_view_with_z_index(Some(5));
        let child3 = create_view_with_z_index(Some(5));
        let parent = setup_parent_with_children(vec![child1, child2, child3]);

        let result = collect_stacking_context_items(parent);
        // Same z-index, so DOM order preserved
        assert_eq!(get_view_ids(&result), vec![child1, child2, child3]);
    }

    #[test]
    fn test_mixed_z_index_and_default() {
        // Mix of explicit z-index and default (None = 0)
        let child_default = create_view_no_stacking_context(); // effective 0, no stacking context
        let child_z5 = create_view_with_z_index(Some(5));
        let child_z_neg = create_view_with_z_index(Some(-1));
        // DOM order: default, z5, z_neg
        let parent = setup_parent_with_children(vec![child_default, child_z5, child_z_neg]);

        let result = collect_stacking_context_items(parent);
        // Paint order: -1, 0, 5
        assert_eq!(get_z_indices_from_items(&result), vec![-1, 0, 5]);
    }

    #[test]
    fn test_large_z_index_values() {
        // Test with large z-index values
        let child_max = create_view_with_z_index(Some(i32::MAX));
        let child_min = create_view_with_z_index(Some(i32::MIN));
        let child_zero = create_view_with_z_index(Some(0));
        let parent = setup_parent_with_children(vec![child_max, child_min, child_zero]);

        let result = collect_stacking_context_items(parent);
        assert_eq!(get_z_indices_from_items(&result), vec![i32::MIN, 0, i32::MAX]);
    }

    #[test]
    fn test_event_dispatch_order_is_reverse_of_paint() {
        // Event dispatch iterates in reverse, so highest z-index receives events first
        let child_z1 = create_view_with_z_index(Some(1));
        let child_z10 = create_view_with_z_index(Some(10));
        let child_z5 = create_view_with_z_index(Some(5));
        let parent = setup_parent_with_children(vec![child_z1, child_z10, child_z5]);

        let paint_order = collect_stacking_context_items(parent);
        // Paint order: 1, 5, 10 (ascending)
        assert_eq!(get_z_indices_from_items(&paint_order), vec![1, 5, 10]);

        // Event dispatch order (reverse): 10, 5, 1
        let event_order: Vec<i32> = paint_order.iter().rev().map(|item| item.z_index).collect();
        assert_eq!(event_order, vec![10, 5, 1]);
    }

    #[test]
    fn test_many_children_sorting() {
        // Test with many children to ensure sorting is stable and correct
        let children: Vec<_> = (0..10)
            .map(|i| create_view_with_z_index(Some(9 - i))) // z-indices: 9, 8, 7, ..., 0
            .collect();
        let parent = setup_parent_with_children(children.clone());

        let result = collect_stacking_context_items(parent);
        // Should be sorted ascending: 0, 1, 2, ..., 9
        let z_indices = get_z_indices_from_items(&result);
        assert_eq!(z_indices, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn test_all_same_nonzero_z_index_preserves_dom_order() {
        // When all children have the same non-zero z-index, DOM order should be preserved
        let child1 = create_view_with_z_index(Some(-5));
        let child2 = create_view_with_z_index(Some(-5));
        let child3 = create_view_with_z_index(Some(-5));
        let parent = setup_parent_with_children(vec![child1, child2, child3]);

        let result = collect_stacking_context_items(parent);
        // All same z-index, DOM order preserved
        assert_eq!(get_view_ids(&result), vec![child1, child2, child3]);
        assert_eq!(get_z_indices_from_items(&result), vec![-5, -5, -5]);
    }

    // ========== True CSS Stacking Context Tests ==========

    #[test]
    fn test_stacking_context_children_escape() {
        // Children of a non-stacking-context view should participate in the
        // parent's stacking context and can interleave with siblings
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context, z=0)
        //   │   ├── A1 (z=5, creates context)
        //   │   └── A2 (z=-1, creates context)
        //   └── B (z=3, creates context)
        //
        // Expected paint order: A2 (z=-1), A (z=0), B (z=3), A1 (z=5)

        let a = create_view_no_stacking_context();
        let a1 = create_view_with_z_index(Some(5));
        let a2 = create_view_with_z_index(Some(-1));
        a.set_children_ids(vec![a1, a2]);

        let b = create_view_with_z_index(Some(3));

        let root = setup_parent_with_children(vec![a, b]);

        let result = collect_stacking_context_items(root);

        // A2 should be first (z=-1), then A (z=0), then B (z=3), then A1 (z=5)
        assert_eq!(get_z_indices_from_items(&result), vec![-1, 0, 3, 5]);
        assert_eq!(get_view_ids(&result), vec![a2, a, b, a1]);
    }

    #[test]
    fn test_stacking_context_bounds_children() {
        // Children of a stacking-context view should NOT escape
        //
        // Structure:
        //   Root
        //   ├── A (z=1, creates stacking context)
        //   │   └── A1 (z=100, creates context) - bounded within A
        //   └── B (z=2, creates context)
        //
        // Expected paint order: A (z=1), B (z=2)
        // A1's z=100 doesn't matter - it's inside A's stacking context

        let a = create_view_with_z_index(Some(1));
        let a1 = create_view_with_z_index(Some(100));
        a.set_children_ids(vec![a1]);

        let b = create_view_with_z_index(Some(2));

        let root = setup_parent_with_children(vec![a, b]);

        let result = collect_stacking_context_items(root);

        // Only A and B should be in root's stacking context
        // A1 is bounded within A's stacking context
        assert_eq!(result.len(), 2);
        assert_eq!(get_z_indices_from_items(&result), vec![1, 2]);
        assert_eq!(get_view_ids(&result), vec![a, b]);
    }

    #[test]
    fn test_deeply_nested_stacking_context_escape() {
        // Deeply nested children should escape multiple levels
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context)
        //   │   └── A1 (no stacking context)
        //   │       └── A1a (z=10, creates context)
        //   └── B (z=5, creates context)
        //
        // Expected paint order: A (z=0), A1 (z=0), B (z=5), A1a (z=10)

        let a = create_view_no_stacking_context();
        let a1 = create_view_no_stacking_context();
        let a1a = create_view_with_z_index(Some(10));
        a1.set_children_ids(vec![a1a]);
        a.set_children_ids(vec![a1]);

        let b = create_view_with_z_index(Some(5));

        let root = setup_parent_with_children(vec![a, b]);

        let result = collect_stacking_context_items(root);

        // A1a escapes through A1 and A to participate in root's stacking context
        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 5, 10]);
        assert_eq!(get_view_ids(&result), vec![a, a1, b, a1a]);
    }

    #[test]
    fn test_ancestor_path_tracking() {
        // Verify that ancestor paths are correctly tracked for nested items
        //
        // Structure:
        //   Root
        //   └── A (no stacking context)
        //       └── A1 (z=5, creates context)

        let a = create_view_no_stacking_context();
        let a1 = create_view_with_z_index(Some(5));
        a.set_children_ids(vec![a1]);

        let root = setup_parent_with_children(vec![a]);

        let result = collect_stacking_context_items(root);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].view_id, a);
        assert_eq!(result[1].view_id, a1);
    }

    #[test]
    fn test_negative_z_index_escapes_and_interleaves() {
        // Negative z-index children should escape and sort before z=0
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context)
        //   │   └── A1 (z=-5)
        //   ├── B (z=-2)
        //   └── C (no stacking context)
        //       └── C1 (z=-10)
        //
        // Expected: C1 (-10), A1 (-5), B (-2), A (0), C (0)

        let a = create_view_no_stacking_context();
        let a1 = create_view_with_z_index(Some(-5));
        a.set_children_ids(vec![a1]);

        let b = create_view_with_z_index(Some(-2));

        let c = create_view_no_stacking_context();
        let c1 = create_view_with_z_index(Some(-10));
        c.set_children_ids(vec![c1]);

        let root = setup_parent_with_children(vec![a, b, c]);
        let result = collect_stacking_context_items(root);

        assert_eq!(get_z_indices_from_items(&result), vec![-10, -5, -2, 0, 0]);
        assert_eq!(get_view_ids(&result), vec![c1, a1, b, a, c]);
    }

    #[test]
    fn test_dom_order_preserved_for_escaped_children_same_z() {
        // When escaped children have the same z-index, DOM order should be preserved
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context)
        //   │   └── A1 (z=5)
        //   ├── B (no stacking context)
        //   │   └── B1 (z=5)
        //   └── C (no stacking context)
        //       └── C1 (z=5)
        //
        // Expected: A (0), B (0), C (0), A1 (5), B1 (5), C1 (5)
        // DOM order: A1 before B1 before C1

        let a = create_view_no_stacking_context();
        let a1 = create_view_with_z_index(Some(5));
        a.set_children_ids(vec![a1]);

        let b = create_view_no_stacking_context();
        let b1 = create_view_with_z_index(Some(5));
        b.set_children_ids(vec![b1]);

        let c = create_view_no_stacking_context();
        let c1 = create_view_with_z_index(Some(5));
        c.set_children_ids(vec![c1]);

        let root = setup_parent_with_children(vec![a, b, c]);
        let result = collect_stacking_context_items(root);

        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 0, 5, 5, 5]);
        // A1 comes before B1 comes before C1 due to DOM order
        assert_eq!(get_view_ids(&result), vec![a, b, c, a1, b1, c1]);
    }

    #[test]
    fn test_empty_non_stacking_context_view() {
        // Non-stacking-context views with no children should work correctly
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context, no children)
        //   └── B (z=1)

        let a = create_view_no_stacking_context();
        let b = create_view_with_z_index(Some(1));

        let root = setup_parent_with_children(vec![a, b]);
        let result = collect_stacking_context_items(root);

        assert_eq!(get_z_indices_from_items(&result), vec![0, 1]);
        assert_eq!(get_view_ids(&result), vec![a, b]);
    }

    #[test]
    fn test_creates_context_flag_correctness() {
        // Verify the creates_context flag is set correctly for different view types

        let with_z = create_view_with_z_index(Some(5));
        let without_z = create_view_no_stacking_context();
        let with_z_zero = create_view_with_z_index(Some(0));

        let root = setup_parent_with_children(vec![with_z, without_z, with_z_zero]);
        let result = collect_stacking_context_items(root);

        // View with explicit z-index creates context
        assert!(result[1].creates_context); // with_z (sorted to middle due to z=5)
        // View without z-index doesn't create context
        assert!(!result[0].creates_context); // without_z (z=0)
        // View with explicit z-index: 0 DOES create context (unlike z-index: auto)
        assert!(result[2].creates_context); // with_z_zero (z=0 but explicit)
    }

    #[test]
    fn test_complex_nested_stacking_contexts() {
        // Complex scenario with multiple levels of stacking contexts
        //
        // Structure:
        //   Root
        //   ├── A (z=1, creates context)
        //   │   ├── A1 (no stacking context)
        //   │   │   └── A1a (z=100) -- bounded within A
        //   │   └── A2 (z=50) -- bounded within A
        //   ├── B (no stacking context)
        //   │   └── B1 (z=2, creates context)
        //   │       └── B1a (z=999) -- bounded within B1
        //   └── C (z=3)
        //
        // Root's stacking context: A (1), B (0), B1 (2), C (3)
        // A1a and A2 are in A's stacking context
        // B1a is in B1's stacking context

        let a = create_view_with_z_index(Some(1));
        let a1 = create_view_no_stacking_context();
        let a1a = create_view_with_z_index(Some(100));
        a1.set_children_ids(vec![a1a]);
        let a2 = create_view_with_z_index(Some(50));
        a.set_children_ids(vec![a1, a2]);

        let b = create_view_no_stacking_context();
        let b1 = create_view_with_z_index(Some(2));
        let b1a = create_view_with_z_index(Some(999));
        b1.set_children_ids(vec![b1a]);
        b.set_children_ids(vec![b1]);

        let c = create_view_with_z_index(Some(3));

        let root = setup_parent_with_children(vec![a, b, c]);
        let result = collect_stacking_context_items(root);

        // Root's stacking context should have: B (0), A (1), B1 (2), C (3)
        // Note: B escapes but B1 is in root's context because B doesn't create one
        assert_eq!(result.len(), 4);
        assert_eq!(get_z_indices_from_items(&result), vec![0, 1, 2, 3]);
        assert_eq!(get_view_ids(&result), vec![b, a, b1, c]);
    }

    #[test]
    fn test_siblings_interleave_with_escaped_cousins() {
        // Test that escaped children interleave correctly with their parent's siblings
        //
        // Structure:
        //   Root
        //   ├── A (z=5)
        //   ├── B (no stacking context)
        //   │   ├── B1 (z=3)
        //   │   └── B2 (z=7)
        //   └── C (z=6)
        //
        // Expected order: B (0), B1 (3), A (5), C (6), B2 (7)

        let a = create_view_with_z_index(Some(5));

        let b = create_view_no_stacking_context();
        let b1 = create_view_with_z_index(Some(3));
        let b2 = create_view_with_z_index(Some(7));
        b.set_children_ids(vec![b1, b2]);

        let c = create_view_with_z_index(Some(6));

        let root = setup_parent_with_children(vec![a, b, c]);
        let result = collect_stacking_context_items(root);

        assert_eq!(get_z_indices_from_items(&result), vec![0, 3, 5, 6, 7]);
        assert_eq!(get_view_ids(&result), vec![b, b1, a, c, b2]);
    }

    #[test]
    fn test_all_non_stacking_context_tree() {
        // When no view creates a stacking context, all should be collected with z=0
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context)
        //   │   └── A1 (no stacking context)
        //   │       └── A1a (no stacking context)
        //   └── B (no stacking context)
        //
        // All should be in paint order with z=0, DOM order preserved

        let a = create_view_no_stacking_context();
        let a1 = create_view_no_stacking_context();
        let a1a = create_view_no_stacking_context();
        a1.set_children_ids(vec![a1a]);
        a.set_children_ids(vec![a1]);

        let b = create_view_no_stacking_context();

        let root = setup_parent_with_children(vec![a, b]);
        let result = collect_stacking_context_items(root);

        // All z=0, DOM order: A, A1, A1a, B
        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 0, 0]);
        assert_eq!(get_view_ids(&result), vec![a, a1, a1a, b]);
    }

    #[test]
    fn test_stacking_context_at_leaf() {
        // Stacking context at leaf level (no children)
        //
        // Structure:
        //   Root
        //   └── A (no stacking context)
        //       └── A1 (no stacking context)
        //           └── A1a (z=5, leaf with no children)

        let a = create_view_no_stacking_context();
        let a1 = create_view_no_stacking_context();
        let a1a = create_view_with_z_index(Some(5));
        a1.set_children_ids(vec![a1a]);
        a.set_children_ids(vec![a1]);

        let root = setup_parent_with_children(vec![a]);
        let result = collect_stacking_context_items(root);

        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 5]);
        assert_eq!(get_view_ids(&result), vec![a, a1, a1a]);
    }

    #[test]
    fn test_event_dispatch_order_with_escaping() {
        // Event dispatch should be reverse of paint order, even with escaped children
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context)
        //   │   └── A1 (z=10)
        //   └── B (z=5)
        //
        // Paint order: A (0), B (5), A1 (10)
        // Event order: A1 (10), B (5), A (0)

        let a = create_view_no_stacking_context();
        let a1 = create_view_with_z_index(Some(10));
        a.set_children_ids(vec![a1]);

        let b = create_view_with_z_index(Some(5));

        let root = setup_parent_with_children(vec![a, b]);
        let paint_order = collect_stacking_context_items(root);

        assert_eq!(get_z_indices_from_items(&paint_order), vec![0, 5, 10]);

        // Reverse for event dispatch
        let event_z_indices: Vec<i32> = paint_order.iter().rev().map(|item| item.z_index).collect();
        let event_view_ids: Vec<ViewId> = paint_order.iter().rev().map(|item| item.view_id).collect();
        assert_eq!(event_z_indices, vec![10, 5, 0]);
        assert_eq!(event_view_ids, vec![a1, b, a]);
    }

    #[test]
    fn test_multiple_children_escape_same_parent() {
        // Multiple children of a non-stacking-context parent all escape
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context)
        //   │   ├── A1 (z=-1)
        //   │   ├── A2 (z=0)
        //   │   ├── A3 (z=1)
        //   │   └── A4 (z=2)
        //   └── B (z=1)
        //
        // Expected: A1 (-1), A (0), A2 (0), A3 (1), B (1), A4 (2)
        // Note: A3 and B both have z=1, A3 comes first due to DOM order

        let a = create_view_no_stacking_context();
        let a1 = create_view_with_z_index(Some(-1));
        let a2 = create_view_with_z_index(Some(0));
        let a3 = create_view_with_z_index(Some(1));
        let a4 = create_view_with_z_index(Some(2));
        a.set_children_ids(vec![a1, a2, a3, a4]);

        let b = create_view_with_z_index(Some(1));

        let root = setup_parent_with_children(vec![a, b]);
        let result = collect_stacking_context_items(root);

        assert_eq!(get_z_indices_from_items(&result), vec![-1, 0, 0, 1, 1, 2]);
        // A comes before A2 at z=0 because A is the parent (encountered first in DOM)
        // A3 comes before B at z=1 because A3's dom_order is smaller
        assert_eq!(get_view_ids(&result), vec![a1, a, a2, a3, b, a4]);
    }

    // ========== Stacking Context Cache Tests ==========

    #[test]
    fn test_stacking_cache_hit_on_second_call() {
        // Second call should return cached value (same result)
        let a = create_view_with_z_index(Some(1));
        let b = create_view_with_z_index(Some(2));
        let root = setup_parent_with_children(vec![a, b]);

        let result1 = collect_stacking_context_items(root);
        let result2 = collect_stacking_context_items(root);

        // Results should be identical
        assert_eq!(get_view_ids(&result1), get_view_ids(&result2));
        assert_eq!(get_z_indices_from_items(&result1), get_z_indices_from_items(&result2));
    }

    #[test]
    fn test_stacking_cache_invalidation_on_z_index_change() {
        // Cache should be invalidated when z-index changes
        let a = create_view_with_z_index(Some(1));
        let b = create_view_with_z_index(Some(2));
        let root = setup_parent_with_children(vec![a, b]);

        let result1 = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&result1), vec![a, b]);

        // Change a's z-index to be higher than b
        {
            let state = a.state();
            let old_info = state.borrow().stacking_info;
            state.borrow_mut().stacking_info = StackingInfo {
                creates_context: true,
                effective_z_index: 10,
            };
            // Simulate what happens during style computation
            if old_info.effective_z_index != 10 {
                invalidate_stacking_cache(a);
            }
        }

        let result2 = collect_stacking_context_items(root);
        // Now a should come after b due to higher z-index
        assert_eq!(get_view_ids(&result2), vec![b, a]);
        assert_eq!(get_z_indices_from_items(&result2), vec![2, 10]);
    }

    #[test]
    fn test_stacking_cache_invalidation_on_children_change() {
        // Cache should be invalidated when children are added/removed
        let a = create_view_with_z_index(Some(1));
        let root = setup_parent_with_children(vec![a]);

        let result1 = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&result1), vec![a]);

        // Add a new child
        let b = create_view_with_z_index(Some(2));
        root.set_children_ids(vec![a, b]); // This calls invalidate_stacking_cache

        let result2 = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&result2), vec![a, b]);
    }

    #[test]
    fn test_stacking_cache_invalidation_propagates_to_ancestors() {
        // Invalidating a child should also invalidate ancestor caches
        //
        // Structure:
        //   Root
        //   └── A (no stacking context)
        //       └── A1 (z=5)
        //
        // When A1's z-index changes, root's cache should also be invalidated

        let a = create_view_no_stacking_context();
        let a1 = create_view_with_z_index(Some(5));
        set_children_with_parents(a, vec![a1]);
        let root = setup_parent_with_children(vec![a]);

        let result1 = collect_stacking_context_items(root);
        assert_eq!(get_z_indices_from_items(&result1), vec![0, 5]);

        // Change A1's z-index
        {
            let state = a1.state();
            state.borrow_mut().stacking_info = StackingInfo {
                creates_context: true,
                effective_z_index: -1,
            };
            invalidate_stacking_cache(a1); // Should invalidate root's cache too
        }

        let result2 = collect_stacking_context_items(root);
        // A1 now has negative z-index, should come first
        assert_eq!(get_z_indices_from_items(&result2), vec![-1, 0]);
        assert_eq!(get_view_ids(&result2), vec![a1, a]);
    }

    #[test]
    fn test_stacking_cache_invalidation_on_creates_context_change() {
        // Cache should be invalidated when creates_context flag changes
        //
        // Structure:
        //   Root
        //   ├── A (initially creates stacking context)
        //   │   └── A1 (z=100)
        //   └── B (z=2)
        //
        // When A stops creating a stacking context, A1 should escape

        let a = create_view_with_z_index(Some(1));
        let a1 = create_view_with_z_index(Some(100));
        a.set_children_ids(vec![a1]);

        let b = create_view_with_z_index(Some(2));
        let root = setup_parent_with_children(vec![a, b]);

        let result1 = collect_stacking_context_items(root);
        // A1 is bounded within A's stacking context
        assert_eq!(result1.len(), 2);
        assert_eq!(get_view_ids(&result1), vec![a, b]);

        // Change A to NOT create a stacking context
        {
            let state = a.state();
            let old_info = state.borrow().stacking_info;
            state.borrow_mut().stacking_info = StackingInfo {
                creates_context: false,
                effective_z_index: 0,
            };
            if old_info.creates_context != false {
                invalidate_stacking_cache(a);
            }
        }

        let result2 = collect_stacking_context_items(root);
        // A1 should now escape and be in root's stacking context
        assert_eq!(result2.len(), 3);
        assert_eq!(get_z_indices_from_items(&result2), vec![0, 2, 100]);
        assert_eq!(get_view_ids(&result2), vec![a, b, a1]);
    }

    #[test]
    fn test_stacking_cache_multiple_roots_independent() {
        // Different stacking context roots should have independent caches
        let a1 = create_view_with_z_index(Some(1));
        let a2 = create_view_with_z_index(Some(2));
        let root_a = setup_parent_with_children(vec![a1, a2]);

        let b1 = create_view_with_z_index(Some(10));
        let b2 = create_view_with_z_index(Some(20));
        let root_b = setup_parent_with_children(vec![b1, b2]);

        let result_a = collect_stacking_context_items(root_a);
        let result_b = collect_stacking_context_items(root_b);

        assert_eq!(get_view_ids(&result_a), vec![a1, a2]);
        assert_eq!(get_view_ids(&result_b), vec![b1, b2]);

        // Invalidate root_a's cache
        invalidate_stacking_cache(a1);

        // root_b's cache should still be valid (returns same result)
        let result_b2 = collect_stacking_context_items(root_b);
        assert_eq!(get_view_ids(&result_b2), vec![b1, b2]);
    }

    #[test]
    fn test_stacking_cache_invalidation_on_child_removal() {
        // Cache should be invalidated when a child is removed
        let a = create_view_with_z_index(Some(1));
        let b = create_view_with_z_index(Some(2));
        let c = create_view_with_z_index(Some(3));
        let root = setup_parent_with_children(vec![a, b, c]);

        let result1 = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&result1), vec![a, b, c]);

        // Remove b from children
        root.set_children_ids(vec![a, c]); // This calls invalidate_stacking_cache

        let result2 = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&result2), vec![a, c]);
    }

    #[test]
    fn test_stacking_cache_invalidation_nested_escaping_child_change() {
        // When a deeply nested child changes, ancestor caches should be invalidated
        //
        // Structure:
        //   Root
        //   └── A (no stacking context)
        //       └── A1 (no stacking context)
        //           └── A1a (z=10, creates context)
        //
        // When A1a changes, root's cache should be invalidated

        let a = create_view_no_stacking_context();
        let a1 = create_view_no_stacking_context();
        let a1a = create_view_with_z_index(Some(10));
        set_children_with_parents(a1, vec![a1a]);
        set_children_with_parents(a, vec![a1]);
        let root = setup_parent_with_children(vec![a]);

        let result1 = collect_stacking_context_items(root);
        assert_eq!(get_z_indices_from_items(&result1), vec![0, 0, 10]);

        // Change A1a's z-index to negative
        {
            let state = a1a.state();
            state.borrow_mut().stacking_info = StackingInfo {
                creates_context: true,
                effective_z_index: -5,
            };
            invalidate_stacking_cache(a1a);
        }

        let result2 = collect_stacking_context_items(root);
        // A1a should now be first due to negative z-index
        assert_eq!(get_z_indices_from_items(&result2), vec![-5, 0, 0]);
        assert_eq!(get_view_ids(&result2), vec![a1a, a, a1]);
    }

    // ========== Fast Path Tests ==========

    #[test]
    fn test_fast_path_all_zero_z_index_preserves_dom_order() {
        // When all z-indices are zero, items should be in DOM order (no sorting needed)
        let a = create_view_no_stacking_context();
        let b = create_view_no_stacking_context();
        let c = create_view_no_stacking_context();
        let root = setup_parent_with_children(vec![a, b, c]);

        let result = collect_stacking_context_items(root);

        // All z-indices are 0, should be in DOM order
        assert_eq!(get_view_ids(&result), vec![a, b, c]);
        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 0]);
    }

    #[test]
    fn test_fast_path_nested_all_zero_z_index() {
        // Nested structure with all z-indices zero should preserve DOM order
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context)
        //   │   ├── A1 (no stacking context)
        //   │   └── A2 (no stacking context)
        //   └── B (no stacking context)

        let a = create_view_no_stacking_context();
        let a1 = create_view_no_stacking_context();
        let a2 = create_view_no_stacking_context();
        set_children_with_parents(a, vec![a1, a2]);

        let b = create_view_no_stacking_context();

        let root = setup_parent_with_children(vec![a, b]);
        let result = collect_stacking_context_items(root);

        // All z-indices are 0, DOM order: A, A1, A2, B
        assert_eq!(get_view_ids(&result), vec![a, a1, a2, b]);
        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 0, 0]);
    }

    #[test]
    fn test_sorting_triggered_by_single_non_zero_z_index() {
        // Even a single non-zero z-index should trigger sorting
        let a = create_view_no_stacking_context();
        let b = create_view_with_z_index(Some(1)); // Only one with z-index
        let c = create_view_no_stacking_context();
        let root = setup_parent_with_children(vec![a, b, c]);

        let result = collect_stacking_context_items(root);

        // b has z=1, so it should come after a and c (which have z=0)
        assert_eq!(get_view_ids(&result), vec![a, c, b]);
        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 1]);
    }

    #[test]
    fn test_sorting_triggered_by_negative_z_index() {
        // Negative z-index should also trigger sorting
        let a = create_view_no_stacking_context();
        let b = create_view_with_z_index(Some(-1)); // Negative z-index
        let c = create_view_no_stacking_context();
        let root = setup_parent_with_children(vec![a, b, c]);

        let result = collect_stacking_context_items(root);

        // b has z=-1, so it should come before a and c (which have z=0)
        assert_eq!(get_view_ids(&result), vec![b, a, c]);
        assert_eq!(get_z_indices_from_items(&result), vec![-1, 0, 0]);
    }
}
