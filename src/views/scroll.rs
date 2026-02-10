#![deny(missing_docs)]
//! Scroll View

use floem_reactive::Effect;
use peniko::kurbo::{Axis, Point, Rect, RoundedRect, RoundedRectRadii, Stroke, Vec2};
use peniko::{Brush, Color};
use std::{cell::RefCell, rc::Rc};
use taffy::Overflow;
use ui_events::pointer::PointerEvent;

use crate::context::{LayoutChanged, LayoutChangedListener};
use crate::event::listener::EventListenerTrait;
use crate::event::{DispatchKind, PointerScrollEventExt};
use crate::style::ScrollbarWidth;
use crate::{
    BoxTree, ElementId, Renderer,
    context::{EventCx, PaintCx, StyleCx},
    event::{Event, EventPropagation, Phase},
    prop, prop_extractor,
    style::{
        Background, BorderColorProp, BorderRadiusProp, CustomStylable, CustomStyle, OverflowX,
        OverflowY, Style, StyleClass,
    },
    style_class,
    unit::{Px, PxPct},
    view::{IntoView, View},
};
use crate::{ViewId, WindowState, custom_event};
use understory_box_tree::NodeFlags;

use super::Decorators;

/// Event fired when a scroll view's scroll position changes
///
/// This event is fired whenever the visible viewport of the scroll view changes,
/// either through user interaction (scrolling with mouse wheel, dragging scrollbars)
/// or programmatic changes to the scroll offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollChanged {
    /// The scroll offset as a vector (how far scrolled from origin)
    pub offset: Vec2,
}
custom_event!(ScrollChanged);

enum ScrollState {
    EnsureVisible(Rect),
    ScrollDelta(Vec2),
    ScrollTo(Point),
    ScrollToPercent(f32),
    ScrollToView(ViewId),
}

trait Vec2Ext {
    /// Returns a new Vec2 with the maximum x and y components from self and other
    fn max_by_component(self, other: Self) -> Self;

    /// Returns a new Vec2 with the minimum x and y components from self and other
    fn min_by_component(self, other: Self) -> Self;
}

impl Vec2Ext for Vec2 {
    fn max_by_component(self, other: Self) -> Self {
        Vec2::new(self.x.max(other.x), self.y.max(other.y))
    }

    fn min_by_component(self, other: Self) -> Self {
        Vec2::new(self.x.min(other.x), self.y.min(other.y))
    }
}

#[derive(Debug, Clone)]
struct ScrollHandle {
    element_id: ElementId,
    box_tree: Rc<RefCell<BoxTree>>,
    axis: Axis,
    /// The initial scroll offset when dragging started
    drag_start_offset: Option<Vec2>,
    /// The initial pointer position when dragging started
    drag_start_point: Option<f64>,
    style: ScrollTrackStyle,
}

impl ScrollHandle {
    fn new(parent_id: ViewId, axis: Axis) -> Self {
        let box_tree = parent_id.box_tree();
        let element_id = parent_id.create_child_element_id();

        Self {
            element_id,
            box_tree,
            axis,
            drag_start_offset: None,
            drag_start_point: None,
            style: Default::default(),
        }
    }

    fn style(&mut self, cx: &mut StyleCx) {
        let interact_state = cx.get_interact_state(self.element_id);
        let resolved = cx.resolve_nested_maps_with_state(
            Style::new(),
            &[Handle::class_ref()],
            &interact_state,
        );
        self.style.read_style(cx, &resolved);
    }

    fn event(
        &mut self,
        cx: &mut EventCx,
        scroll_offset: &mut Vec2,
        is_scrolling_or_interacting: &mut bool,
        parent_id: ViewId,
        child_id: ViewId,
    ) {
        match &cx.event {
            Event::Pointer(PointerEvent::Down(e)) => {
                if let Some(pointer_id) = e.pointer.pointer_id {
                    cx.window_state
                        .set_pointer_capture(pointer_id, self.element_id);
                }
                let pos = e.state.logical_point();
                self.drag_start_point = Some(pos.get_coord(self.axis));
                self.drag_start_offset = Some(*scroll_offset);
                *is_scrolling_or_interacting = true;
                cx.window_state.request_paint(parent_id);
            }
            Event::Pointer(PointerEvent::Up(_)) => {
                self.drag_start_point = None;
                self.drag_start_offset = None;
            }
            Event::Pointer(PointerEvent::Move(u)) => {
                if cx.window_state.has_capture(self.element_id) {
                    if let (Some(start_point), Some(initial_offset)) =
                        (self.drag_start_point, self.drag_start_offset)
                    {
                        let pos = u.current.logical_point();

                        // Calculate scale (content_size / viewport_size)
                        let viewport_size = parent_id
                            .get_content_rect_local()
                            .size()
                            .get_coord(self.axis);
                        let content_size =
                            child_id.get_layout_rect_local().size().get_coord(self.axis);
                        let scale = content_size / viewport_size;

                        let scroll_delta = (pos.get_coord(self.axis) - start_point) * scale;

                        let mut new_offset = initial_offset;
                        new_offset.set_coord(
                            self.axis,
                            initial_offset.get_coord(self.axis) + scroll_delta,
                        );

                        // Apply scroll
                        let viewport_size_vec = parent_id.get_content_rect_local().size();
                        let content_size_vec = child_id.get_layout_rect_local().size();
                        let max_scroll = (content_size_vec.to_vec2() - viewport_size_vec.to_vec2())
                            .max_by_component(Vec2::ZERO);

                        *scroll_offset = new_offset
                            .max_by_component(Vec2::ZERO)
                            .min_by_component(max_scroll);
                        parent_id.set_child_translation(*scroll_offset);
                    }
                }
            }
            Event::Pointer(PointerEvent::Enter(_)) => {
                *is_scrolling_or_interacting = true;
                cx.window_state.request_paint(parent_id);
            }
            Event::Pointer(PointerEvent::Leave(_)) => {
                if !cx.window_state.has_capture(self.element_id) {
                    *is_scrolling_or_interacting = false;
                    cx.window_state.request_paint(parent_id);
                }
            }
            _ => {}
        }
    }

    fn set_position(
        &mut self,
        scroll_offset: Vec2,
        viewport: Rect,
        full_rect: Rect,
        content_size: peniko::kurbo::Size,
        scrollbar_width: f64,
        bar_inset: f64,
    ) {
        let viewport_size = viewport.size().get_coord(self.axis);
        let content_size_val = content_size.get_coord(self.axis);
        let full_rect_size = full_rect.size().get_coord(self.axis);

        // No scrollbar if content fits in viewport
        if viewport_size >= (content_size_val - f64::EPSILON) {
            // Hide the handle
            self.box_tree
                .borrow_mut()
                .set_flags(self.element_id.0, NodeFlags::empty());
            return;
        }

        // Calculate scrollbar handle size and position
        let percent_visible = viewport_size / content_size_val;
        let max_scroll = content_size_val - viewport_size;
        let scroll_offset_val = scroll_offset.get_coord(self.axis);

        let percent_scrolled = if max_scroll > 0.0 {
            scroll_offset_val / max_scroll
        } else {
            0.0
        };

        let handle_length = (percent_visible * full_rect_size).ceil().max(15.);

        let track_length = full_rect_size;
        let available_travel = track_length - handle_length;
        let handle_offset = (available_travel * percent_scrolled).ceil();

        let rect = match self.axis {
            Axis::Vertical => {
                let x0 = full_rect.width() - scrollbar_width - bar_inset;
                let y0 = handle_offset;
                let x1 = full_rect.width() - bar_inset;
                let y1 = handle_offset + handle_length;
                Rect::new(x0, y0, x1, y1)
            }
            Axis::Horizontal => {
                let x0 = handle_offset;
                let y0 = full_rect.height() - scrollbar_width - bar_inset;
                let x1 = handle_offset + handle_length;
                let y1 = full_rect.height() - bar_inset;
                Rect::new(x0, y0, x1, y1)
            }
        };

        self.box_tree
            .borrow_mut()
            .set_local_bounds(self.element_id.0, rect);
        self.box_tree
            .borrow_mut()
            .set_flags(self.element_id.0, NodeFlags::VISIBLE | NodeFlags::PICKABLE);
        self.box_tree.borrow_mut().set_z_index(self.element_id.0, 2);
    }

    fn paint(&self, cx: &mut PaintCx) {
        let box_tree = self.box_tree.borrow();
        let bounds = match box_tree.world_bounds(self.element_id.0) {
            Ok(bounds) => bounds,
            Err(bounds) => bounds.value().unwrap(),
        };

        let transform = match box_tree.world_transform(self.element_id.0) {
            Ok(transform) => transform,
            Err(transform) => transform.value().unwrap(),
        };
        let rect = transform.transform_rect_bbox(bounds);
        cx.set_transform(transform);

        let radius = if self.style.rounded() {
            match self.axis {
                Axis::Vertical => RoundedRectRadii::from_single_radius((rect.x1 - rect.x0) / 2.),
                Axis::Horizontal => RoundedRectRadii::from_single_radius((rect.y1 - rect.y0) / 2.),
            }
        } else {
            let size = rect.size().min_side();
            let border_radius = self.style.border_radius();
            RoundedRectRadii {
                top_left: crate::view::border_radius(
                    border_radius.top_left.unwrap_or(PxPct::Px(0.)),
                    size,
                ),
                top_right: crate::view::border_radius(
                    border_radius.top_right.unwrap_or(PxPct::Px(0.)),
                    size,
                ),
                bottom_left: crate::view::border_radius(
                    border_radius.bottom_left.unwrap_or(PxPct::Px(0.)),
                    size,
                ),
                bottom_right: crate::view::border_radius(
                    border_radius.bottom_right.unwrap_or(PxPct::Px(0.)),
                    size,
                ),
            }
        };

        let edge_width = self.style.border().0;
        let rect_with_border = rect.inset(-edge_width / 2.0);
        let rounded_rect = rect_with_border.to_rounded_rect(radius);

        cx.fill(
            &rounded_rect,
            &self.style.color().unwrap_or(HANDLE_COLOR),
            0.0,
        );

        if edge_width > 0.0 {
            if let Some(color) = self.style.border_color().right {
                cx.stroke(&rounded_rect, &color, &Stroke::new(edge_width));
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ScrollTrack {
    element_id: ElementId,
    box_tree: Rc<RefCell<BoxTree>>,
    axis: Axis,
    style: ScrollTrackStyle,
}

impl ScrollTrack {
    fn new(parent_id: ViewId, axis: Axis) -> Self {
        let box_tree = parent_id.box_tree();
        let element_id = parent_id.create_child_element_id();

        Self {
            element_id,
            box_tree,
            axis,
            style: Default::default(),
        }
    }

    fn style(&mut self, cx: &mut StyleCx) {
        let interact_state = cx.get_interact_state(self.element_id);
        let resolved =
            cx.resolve_nested_maps_with_state(Style::new(), &[Track::class_ref()], &interact_state);
        self.style.read_style(cx, &resolved);
    }

    fn event(
        &mut self,
        cx: &mut EventCx,
        scroll_offset: &mut Vec2,
        is_scrolling_or_interacting: &mut bool,
        parent_id: ViewId,
        child_id: ViewId,
    ) {
        match &cx.event {
            Event::Pointer(PointerEvent::Down(e)) => {
                let pos = e.state.logical_point();

                // Inline click_track logic
                let viewport = parent_id.get_content_rect_local();
                let full_rect = parent_id.get_layout_rect_local();
                let content_size = child_id.get_layout_rect_local().size();

                let pos_val = pos.get_coord(self.axis);
                let viewport_size = viewport.size().get_coord(self.axis);
                let content_size_val = content_size.get_coord(self.axis);
                let full_rect_size = full_rect.size().get_coord(self.axis);

                let percent_visible = viewport_size / content_size_val;
                let handle_length = (percent_visible * full_rect_size).ceil().max(15.);
                let max_scroll = content_size_val - viewport_size;

                let track_length = full_rect_size;
                let available_travel = track_length - handle_length;

                let target_handle_offset = (pos_val - handle_length / 2.0)
                    .max(0.0)
                    .min(available_travel);
                let target_percent = if available_travel > 0.0 {
                    target_handle_offset / available_travel
                } else {
                    0.0
                };

                let new_offset = (target_percent * max_scroll).clamp(0.0, max_scroll);

                scroll_offset.set_coord(self.axis, new_offset);
                parent_id.set_child_translation(*scroll_offset);

                *is_scrolling_or_interacting = true;
                cx.window_state.request_paint(parent_id);
            }
            Event::Pointer(PointerEvent::Enter(_)) => {
                *is_scrolling_or_interacting = true;
                cx.window_state.request_paint(parent_id);
            }
            Event::Pointer(PointerEvent::Leave(_)) => {
                *is_scrolling_or_interacting = false;
                cx.window_state.request_paint(parent_id);
            }
            _ => {}
        }
    }

    fn set_position(
        &mut self,
        viewport: Rect,
        full_rect: Rect,
        content_size: peniko::kurbo::Size,
        scrollbar_width: f64,
        bar_inset: f64,
    ) {
        let viewport_size = viewport.size().get_coord(self.axis);
        let content_size_val = content_size.get_coord(self.axis);

        // No scrollbar if content fits in viewport
        if viewport_size >= (content_size_val - f64::EPSILON) {
            // Hide the track
            self.box_tree
                .borrow_mut()
                .set_flags(self.element_id.0, NodeFlags::empty());
            return;
        }

        let rect = match self.axis {
            Axis::Vertical => {
                let x0 = full_rect.width() - scrollbar_width - bar_inset;
                let y0 = 0.0;
                let x1 = full_rect.width() - bar_inset;
                let y1 = full_rect.height();
                Rect::new(x0, y0, x1, y1)
            }
            Axis::Horizontal => {
                let x0 = 0.0;
                let y0 = full_rect.height() - scrollbar_width - bar_inset;
                let x1 = full_rect.width();
                let y1 = full_rect.height() - bar_inset;
                Rect::new(x0, y0, x1, y1)
            }
        };

        self.box_tree
            .borrow_mut()
            .set_local_bounds(self.element_id.0, rect);
        self.box_tree
            .borrow_mut()
            .set_flags(self.element_id.0, NodeFlags::VISIBLE | NodeFlags::PICKABLE);
        self.box_tree.borrow_mut().set_z_index(self.element_id.0, 1);
    }

    fn paint(&self, cx: &mut PaintCx) {
        let box_tree = self.box_tree.borrow();
        let bounds = match box_tree.world_bounds(self.element_id.0) {
            Ok(bounds) => bounds,
            Err(bounds) => bounds.value().unwrap(),
        };

        let transform = match box_tree.world_transform(self.element_id.0) {
            Ok(transform) => transform,
            Err(transform) => transform.value().unwrap(),
        };
        let rect = transform.transform_rect_bbox(bounds);

        if let Some(color) = self.style.color() {
            cx.fill(&rect, &color, 0.0);
        }
    }
}

style_class!(
    /// Style class that will be applied to the handles of the scroll view
    pub Handle
);
style_class!(
    /// Style class that will be applied to the scroll tracks of the scroll view
    pub Track
);

prop!(
    /// Determines if scroll handles should be rounded (defaults to true on macOS).
    pub Rounded: bool {} = cfg!(target_os = "macos")
);
prop!(
    /// Defines the border width of a scroll track in pixels.
    pub Border: Px {} = Px(0.0)
);

prop_extractor! {
    ScrollTrackStyle {
        color: Background,
        border_radius: BorderRadiusProp,
        border_color: BorderColorProp,
        border: Border,
        rounded: Rounded,
    }
}

prop!(
    /// Specifies the vertical inset of the scrollable area in pixels.
    pub VerticalInset: Px {} = Px(0.0)
);

prop!(
    /// Defines the horizontal inset of the scrollable area in pixels.
    pub HorizontalInset: Px {} = Px(0.0)
);

prop!(
    /// Controls the visibility of scroll bars. When true, bars are hidden.
    pub HideBars: bool {} = false
);

prop!(
    /// Controls whether scroll bars are shown when not scrolling. When false, bars are only shown during scroll interactions.
    pub ShowBarsWhenIdle: bool {} = true
);

prop!(
    /// Determines if pointer wheel events should propagate to parent elements.
    pub PropagatePointerWheel: bool {} = true
);

prop!(
    /// When true, vertical scroll input is interpreted as horizontal scrolling.
    pub VerticalScrollAsHorizontal: bool {} = false
);

prop_extractor!(ScrollStyle {
    vertical_bar_inset: VerticalInset,
    horizontal_bar_inset: HorizontalInset,
    hide_bar: HideBars,
    show_bars_when_idle: ShowBarsWhenIdle,
    propagate_pointer_wheel: PropagatePointerWheel,
    vertical_scroll_as_horizontal: VerticalScrollAsHorizontal,
    overflow_x: OverflowX,
    overflow_y: OverflowY,
    scrollbar_width: ScrollbarWidth,
});

const HANDLE_COLOR: Brush = Brush::Solid(Color::from_rgba8(0, 0, 0, 120));

style_class!(
    /// Style class that is applied to every scroll view
    pub ScrollClass
);

/// A scroll view
pub struct Scroll {
    id: ViewId,
    child: ViewId,
    // any time this changes, we must update the scroll_offset in the ViewState.
    scroll_offset: Vec2,
    v_handle: ScrollHandle,
    h_handle: ScrollHandle,
    v_track: ScrollTrack,
    h_track: ScrollTrack,
    /// Tracks whether user is currently interacting with scrollbars or recently scrolled
    is_scrolling_or_interacting: bool,
    scroll_style: ScrollStyle,
}

/// Create a new scroll view
#[deprecated(since = "0.2.0", note = "Use Scroll::new() instead")]
pub fn scroll<V: IntoView + 'static>(child: V) -> Scroll {
    Scroll::new(child)
}

impl Scroll {
    /// Creates a new scroll view wrapping the given child view.
    ///
    /// ## Example
    /// ```rust
    /// use floem::views::*;
    ///
    /// let content = Label::new("Scrollable content");
    /// let scrollable = Scroll::new(content);
    /// ```
    pub fn new(child: impl IntoView) -> Self {
        let id = ViewId::new();
        id.register_listener(LayoutChangedListener::listener_key());

        let child = child.into_any();
        let child_id = child.id();
        id.add_child(child);
        // we need to first set the clip rect to zero so that virtual items don't set a large initial size
        id.set_box_tree_clip(Some(RoundedRect::from_rect(Rect::ZERO, 0.)));

        Scroll {
            id,
            child: child_id,
            scroll_offset: Vec2::ZERO,
            v_handle: ScrollHandle::new(id, Axis::Vertical),
            h_handle: ScrollHandle::new(id, Axis::Horizontal),
            v_track: ScrollTrack::new(id, Axis::Vertical),
            h_track: ScrollTrack::new(id, Axis::Horizontal),
            is_scrolling_or_interacting: false,
            scroll_style: Default::default(),
        }
        .class(ScrollClass)
    }
}

impl Scroll {
    /// Ensures that a specific rectangular area is visible within the scroll view by automatically
    /// scrolling to it if necessary.
    ///
    /// # Reactivity
    /// The viewport will automatically update to include the target rectangle whenever the rectangle's
    /// position or size changes, as determined by the `to` function which will update any time there are
    /// changes in the signals that it depends on.
    pub fn ensure_visible(self, to: impl Fn() -> Rect + 'static) -> Self {
        let id = self.id();
        Effect::new(move |_| {
            let rect = to();
            id.update_state_deferred(ScrollState::EnsureVisible(rect));
        });

        self
    }

    /// Scrolls the view by the specified delta vector.
    ///
    /// # Reactivity
    /// The scroll position will automatically update whenever the delta vector changes,
    /// as determined by the `delta` function which will update any time there are changes in the signals that it depends on.
    pub fn scroll_delta(self, delta: impl Fn() -> Vec2 + 'static) -> Self {
        let id = self.id();
        Effect::new(move |_| {
            let delta = delta();
            id.update_state(ScrollState::ScrollDelta(delta));
        });

        self
    }

    /// Scrolls the view to the specified target point.
    ///
    /// # Reactivity
    /// The scroll position will automatically update whenever the target point changes,
    /// as determined by the `origin` function which will update any time there are changes in the signals that it depends on.
    pub fn scroll_to(self, origin: impl Fn() -> Option<Point> + 'static) -> Self {
        let id = self.id();
        Effect::new(move |_| {
            if let Some(origin) = origin() {
                id.update_state_deferred(ScrollState::ScrollTo(origin));
            }
        });

        self
    }

    /// Scrolls the view to the specified percentage (0-100) of its scrollable content.
    ///
    /// # Reactivity
    /// The scroll position will automatically update whenever the target percentage changes,
    /// as determined by the `percent` function which will update any time there are changes in the signals that it depends on.
    pub fn scroll_to_percent(self, percent: impl Fn() -> f32 + 'static) -> Self {
        let id = self.id();
        Effect::new(move |_| {
            let percent = percent() / 100.;
            id.update_state_deferred(ScrollState::ScrollToPercent(percent));
        });
        self
    }

    /// Scrolls the view to make a specific view visible.
    ///
    /// # Reactivity
    /// The scroll position will automatically update whenever the target view changes,
    /// as determined by the `view` function which will update any time there are changes in the signals that it depends on.
    pub fn scroll_to_view(self, view: impl Fn() -> Option<ViewId> + 'static) -> Self {
        let id = self.id();
        Effect::new(move |_| {
            if let Some(view) = view() {
                id.update_state_deferred(ScrollState::ScrollToView(view));
            }
        });

        self
    }
}

/// internal methods
impl Scroll {
    /// this applies a delta, set the viewport in the window state and returns the delta that was actually applied
    ///
    /// If the delta is positive, the view will scroll down, negative will scroll up.
    fn apply_scroll_delta(&mut self, delta: Vec2) -> Option<Vec2> {
        let viewport_size = self.id.get_content_rect_local().size();
        let content_size = self.child.get_layout_rect_local().size();

        // Calculate max scroll based on overflow settings
        let mut max_scroll =
            (content_size.to_vec2() - viewport_size.to_vec2()).max_by_component(Vec2::ZERO);

        // Zero out scroll in axes that aren't scrollable
        let can_scroll_x = matches!(self.scroll_style.overflow_x(), taffy::Overflow::Scroll);
        let can_scroll_y = matches!(self.scroll_style.overflow_y(), taffy::Overflow::Scroll);

        let mut new_scroll_offset = self.scroll_offset + delta;
        if !can_scroll_x {
            new_scroll_offset.x = 0.0;
            max_scroll.x = 0.0;
        }
        if !can_scroll_y {
            new_scroll_offset.y = 0.0;
            max_scroll.y = 0.0;
        }

        let old_scroll_offset = self.scroll_offset;
        self.scroll_offset = new_scroll_offset
            .max_by_component(Vec2::ZERO)
            .min_by_component(max_scroll);
        let change = self.id.set_child_translation(self.scroll_offset);
        if change {
            self.id.dispatch_event(
                Event::new_custom(ScrollChanged {
                    offset: self.scroll_offset,
                }),
                DispatchKind::Directed {
                    target: self.id.get_element_id(),
                    phases: crate::context::Phases::TARGET,
                },
            );
        }

        if change {
            Some(self.scroll_offset - old_scroll_offset)
        } else {
            None
        }
    }

    /// Scroll to a specific offset position.
    ///
    /// Sets the scroll offset to the given point, clamping to valid scroll bounds.
    /// The offset represents how much content has scrolled out of view at the top-left.
    ///
    /// # Arguments
    /// * `offset` - The desired scroll offset. Will be clamped to valid range [0, max_scroll]
    fn do_scroll_to(&mut self, offset: Point) {
        self.apply_scroll_delta(offset.to_vec2() - self.scroll_offset);
    }

    /// Ensure that an entire area is visible in the scroll view.
    ///
    /// Scrolls the minimum distance necessary to make the entire rect visible.
    /// If the rect is larger than the viewport, prioritizes showing the top-left.
    ///
    /// # Arguments
    /// * `rect` - The rectangle in content coordinates (relative to the child's layout)
    pub fn do_ensure_visible(&mut self, rect: Rect) {
        let viewport = self.id.get_content_rect_local();
        let viewport_size = viewport.size();

        // Calculate the rect's position relative to current scroll position
        let visible_rect = Rect::from_origin_size(self.scroll_offset.to_point(), viewport_size);

        // If rect is already fully visible, no need to scroll
        if visible_rect.contains_rect(rect) {
            return;
        }

        let mut new_offset = self.scroll_offset;

        // Scroll horizontally if needed
        if rect.width() > viewport_size.width {
            // Rect is wider than viewport - show left edge
            new_offset.x = rect.x0;
        } else if rect.x0 < visible_rect.x0 {
            // Rect is cut off on left - scroll left
            new_offset.x = rect.x0;
        } else if rect.x1 > visible_rect.x1 {
            // Rect is cut off on right - scroll right
            new_offset.x = rect.x1 - viewport_size.width;
        }

        // Scroll vertically if needed
        if rect.height() > viewport_size.height {
            // Rect is taller than viewport - show top edge
            new_offset.y = rect.y0;
        } else if rect.y0 < visible_rect.y0 {
            // Rect is cut off on top - scroll up
            new_offset.y = rect.y0;
        } else if rect.y1 > visible_rect.y1 {
            // Rect is cut off on bottom - scroll down
            new_offset.y = rect.y1 - viewport_size.height;
        }

        self.do_scroll_to(new_offset.to_point());
    }

    // TODO: make this work
    fn do_scroll_to_view(&mut self, _target: ViewId, _target_rect: Option<Rect>) {
        // todo!()
        // if target.layout().is_none() || target.is_hidden() {
        //     return;
        // }

        // // Get target's rect relative to its parent
        // let mut rect = target.layout_rect();

        // // If a specific sub-rect within the target is specified, adjust
        // if let Some(target_rect) = target_rect {
        //     rect = rect.with_origin(rect.origin() + target_rect.origin().to_vec2());
        //     let new_size = target_rect
        //         .size()
        //         .to_rect()
        //         .intersect(rect.size().to_rect())
        //         .size();
        //     rect = rect.with_size(new_size);
        // }

        // // Convert from window coordinates to child content coordinates
        // // We need to find the position relative to the scrollable child
        // let target_window_rect = rect;
        // let child_window_origin = self.child.layout_rect().origin();

        // // Convert to child-relative coordinates
        // let rect_in_child = target_window_rect
        //     .with_origin(target_window_rect.origin() - child_window_origin.to_vec2());

        // self.do_ensure_visible(rect_in_child, lcx);
    }

    fn post_layout(&mut self, layout: &LayoutChanged) {
        let viewport = layout.content_box_local();
        let full_rect = layout.box_local();
        let content_size = self.child.get_layout_rect_local().size();
        let scrollbar_width = self.scroll_style.scrollbar_width().0;
        let v_bar_inset = self.scroll_style.vertical_bar_inset().0;
        let h_bar_inset = self.scroll_style.horizontal_bar_inset().0;

        self.v_track.set_position(
            viewport,
            full_rect,
            content_size,
            scrollbar_width,
            v_bar_inset,
        );
        self.h_track.set_position(
            viewport,
            full_rect,
            content_size,
            scrollbar_width,
            h_bar_inset,
        );

        self.v_handle.set_position(
            self.scroll_offset,
            viewport,
            full_rect,
            content_size,
            scrollbar_width,
            v_bar_inset,
        );
        self.h_handle.set_position(
            self.scroll_offset,
            viewport,
            full_rect,
            content_size,
            scrollbar_width,
            h_bar_inset,
        );
    }
}

impl View for Scroll {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Scroll".into()
    }

    fn view_style(&self) -> Option<Style> {
        Some(
            Style::new()
                .items_start()
                .overflow_x(Overflow::Scroll)
                .overflow_y(Overflow::Scroll),
        )
    }

    fn update(&mut self, _cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<ScrollState>() {
            match *state {
                ScrollState::EnsureVisible(rect) => {
                    self.do_ensure_visible(rect);
                }
                ScrollState::ScrollDelta(delta) => {
                    self.apply_scroll_delta(delta);
                }
                ScrollState::ScrollTo(origin) => {
                    self.do_scroll_to(origin);
                }
                ScrollState::ScrollToPercent(percent) => {
                    let content_size = self.child.get_layout_rect_local().size();
                    let viewport_size = self.id.get_content_rect_local().size();

                    // Calculate max scroll (content size - viewport size)
                    let max_scroll = (content_size.to_vec2() - viewport_size.to_vec2())
                        .max_by_component(Vec2::ZERO);

                    // Apply percentage to max scroll
                    let target_offset = max_scroll * (percent as f64);

                    self.do_scroll_to(target_offset.to_point());
                }
                ScrollState::ScrollToView(id) => {
                    self.do_scroll_to_view(id, None);
                }
            }
            self.id.request_box_tree_update_for_view();
        }
    }

    fn scroll_to(&mut self, cx: &mut WindowState, target: ViewId, rect: Option<Rect>) -> bool {
        let found = self.child.view().borrow_mut().scroll_to(cx, target, rect);
        if found {
            self.do_scroll_to_view(target, rect);
        }
        found
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        self.scroll_style.read(cx);

        self.v_handle.style(cx);
        self.h_handle.style(cx);
        self.v_track.style(cx);
        self.h_track.style(cx);
    }

    fn event(&mut self, cx: &mut EventCx) -> EventPropagation {
        // in order to use this we had to set `id.has_layout_listener`.
        if let Some(new_layout) = LayoutChangedListener::extract(&cx.event) {
            self.post_layout(new_layout);
            return EventPropagation::Stop;
        }
        // Handle events targeted at our visual IDs (handles and tracks)
        if cx.phase == Phase::Target {
            if cx.target == self.v_handle.element_id {
                self.v_handle.event(
                    cx,
                    &mut self.scroll_offset,
                    &mut self.is_scrolling_or_interacting,
                    self.id,
                    self.child,
                );
                return EventPropagation::Stop;
            }
            if cx.target == self.h_handle.element_id {
                self.h_handle.event(
                    cx,
                    &mut self.scroll_offset,
                    &mut self.is_scrolling_or_interacting,
                    self.id,
                    self.child,
                );
                return EventPropagation::Stop;
            }
            if cx.target == self.v_track.element_id {
                self.v_track.event(
                    cx,
                    &mut self.scroll_offset,
                    &mut self.is_scrolling_or_interacting,
                    self.id,
                    self.child,
                );
                return EventPropagation::Stop;
            }
            if cx.target == self.h_track.element_id {
                self.h_track.event(
                    cx,
                    &mut self.scroll_offset,
                    &mut self.is_scrolling_or_interacting,
                    self.id,
                    self.child,
                );
                return EventPropagation::Stop;
            }
        }

        // Handle scroll wheel events in bubble phase
        if cx.phase == Phase::Bubble {
            if let Event::Pointer(PointerEvent::Scroll(pse)) = &cx.event {
                let size = self.id.get_layout_rect_local().size();
                let delta = pse.resolve_to_points(None, Some(size));
                let delta = -if self.scroll_style.vertical_scroll_as_horizontal()
                    && delta.x == 0.0
                    && delta.y != 0.0
                {
                    Vec2::new(delta.y, delta.x)
                } else {
                    delta
                };

                let change = self.apply_scroll_delta(delta);

                if change.is_some() {
                    self.is_scrolling_or_interacting = true;
                    cx.window_state.request_paint(self.id);
                }

                return if self.scroll_style.propagate_pointer_wheel() && change.is_none() {
                    EventPropagation::Continue
                } else {
                    EventPropagation::Stop
                };
            }
        }

        EventPropagation::Continue
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        // this apply scroll delta of zero is cheap.
        // it is here in the case that the available delta changed, this will catch it and update it to a better size
        self.apply_scroll_delta(Vec2::ZERO);

        // Check which visual node we're painting
        // Scroll view creates multiple visual IDs for scrollbars/tracks
        if cx.target_id == self.id.get_element_id() {
            // Main scroll container - children painted automatically by traversal
        } else if cx.target_id == self.v_handle.element_id {
            // Painting vertical scrollbar handle
            if !self.scroll_style.hide_bar()
                && (self.scroll_style.show_bars_when_idle() || self.is_scrolling_or_interacting)
            {
                self.v_handle.paint(cx);
            }
        } else if cx.target_id == self.h_handle.element_id {
            // Painting horizontal scrollbar handle
            if !self.scroll_style.hide_bar()
                && (self.scroll_style.show_bars_when_idle() || self.is_scrolling_or_interacting)
            {
                self.h_handle.paint(cx);
            }
        } else if cx.target_id == self.v_track.element_id {
            // Painting vertical scrollbar track
            if !self.scroll_style.hide_bar()
                && (self.scroll_style.show_bars_when_idle() || self.is_scrolling_or_interacting)
            {
                self.v_track.paint(cx);
            }
        } else if cx.target_id == self.h_track.element_id {
            // Painting horizontal scrollbar track
            if !self.scroll_style.hide_bar()
                && (self.scroll_style.show_bars_when_idle() || self.is_scrolling_or_interacting)
            {
                self.h_track.paint(cx);
            }
        }
    }
}
/// Represents a custom style for a `Scroll`.
#[derive(Default, Debug, Clone)]
pub struct ScrollCustomStyle(Style);
impl From<ScrollCustomStyle> for Style {
    fn from(value: ScrollCustomStyle) -> Self {
        value.0
    }
}
impl From<Style> for ScrollCustomStyle {
    fn from(value: Style) -> Self {
        Self(value)
    }
}
impl CustomStyle for ScrollCustomStyle {
    type StyleClass = ScrollClass;
}

impl CustomStylable<ScrollCustomStyle> for Scroll {
    type DV = Self;
}

impl ScrollCustomStyle {
    /// Creates a new `ScrollCustomStyle`.
    pub fn new() -> Self {
        Self(Style::new())
    }

    /// Configures the scroll view to allow the viewport to be smaller than the inner content,
    /// while still taking up the full available space in its container.
    ///
    /// Use this when you need a scroll view that can shrink its viewport size to fit within
    /// the container, ensuring the content remains scrollable even if the inner content is
    /// greater than the parent size.
    ///
    /// Internally this does a `s.min_size(0., 0.).size_full()`.
    pub fn shrink_to_fit(mut self) -> Self {
        self = Self(self.0.min_size(0., 0.).size_full());
        self
    }

    /// Sets the background color for the handle.
    pub fn handle_background(mut self, color: impl Into<Brush>) -> Self {
        self = Self(self.0.class(Handle, |s| s.background(color.into())));
        self
    }

    /// Sets the border radius for the handle.
    pub fn handle_border_radius(mut self, border_radius: impl Into<PxPct>) -> Self {
        self = Self(self.0.class(Handle, |s| s.border_radius(border_radius)));
        self
    }

    /// Sets the border color for the handle.
    pub fn handle_border_color(mut self, border_color: impl Into<Brush>) -> Self {
        self = Self(self.0.class(Handle, |s| s.border_color(border_color)));
        self
    }

    /// Sets the border thickness for the handle.
    pub fn handle_border(mut self, border: impl Into<Px>) -> Self {
        self = Self(self.0.class(Handle, |s| s.set(Border, border)));
        self
    }

    /// Sets whether the handle should have rounded corners.
    pub fn handle_rounded(mut self, rounded: impl Into<bool>) -> Self {
        self = Self(self.0.class(Handle, |s| s.set(Rounded, rounded)));
        self
    }

    /// Sets the background color for the track.
    pub fn track_background(mut self, color: impl Into<Brush>) -> Self {
        self = Self(self.0.class(Track, |s| s.background(color.into())));
        self
    }

    /// Sets the border radius for the track.
    pub fn track_border_radius(mut self, border_radius: impl Into<PxPct>) -> Self {
        self = Self(self.0.class(Track, |s| s.border_radius(border_radius)));
        self
    }

    /// Sets the border color for the track.
    pub fn track_border_color(mut self, border_color: impl Into<Brush>) -> Self {
        self = Self(self.0.class(Track, |s| s.border_color(border_color)));
        self
    }

    /// Sets the border thickness for the track.
    pub fn track_border(mut self, border: impl Into<Px>) -> Self {
        self = Self(self.0.class(Track, |s| s.set(Border, border)));
        self
    }

    /// Sets whether the track should have rounded corners.
    pub fn track_rounded(mut self, rounded: impl Into<bool>) -> Self {
        self = Self(self.0.class(Track, |s| s.set(Rounded, rounded)));
        self
    }

    /// Sets the vertical track inset.
    pub fn vertical_track_inset(mut self, inset: impl Into<Px>) -> Self {
        self = Self(self.0.set(VerticalInset, inset));
        self
    }

    /// Sets the horizontal track inset.
    pub fn horizontal_track_inset(mut self, inset: impl Into<Px>) -> Self {
        self = Self(self.0.set(HorizontalInset, inset));
        self
    }

    /// Controls the visibility of the scroll bars.
    pub fn hide_bars(mut self, hide: impl Into<bool>) -> Self {
        self = Self(self.0.set(HideBars, hide));
        self
    }

    /// Sets whether the pointer wheel events should be propagated.
    pub fn propagate_pointer_wheel(mut self, propagate: impl Into<bool>) -> Self {
        self = Self(self.0.set(PropagatePointerWheel, propagate));
        self
    }

    /// Sets whether vertical scrolling should be interpreted as horizontal scrolling.
    pub fn vertical_scroll_as_horizontal(mut self, vert_as_horiz: impl Into<bool>) -> Self {
        self = Self(self.0.set(VerticalScrollAsHorizontal, vert_as_horiz));
        self
    }

    /// Controls whether scroll bars are shown when not scrolling. When false, bars are only shown during scroll interactions.
    pub fn show_bars_when_idle(mut self, show: impl Into<bool>) -> Self {
        self = Self(self.0.set(ShowBarsWhenIdle, show));
        self
    }
}

/// A trait that adds a `scroll` method to any type that implements `IntoView`.
pub trait ScrollExt {
    /// Wrap the view in a scroll view.
    fn scroll(self) -> Scroll;
}

impl<T: IntoView + 'static> ScrollExt for T {
    fn scroll(self) -> Scroll {
        Scroll::new(self)
    }
}
