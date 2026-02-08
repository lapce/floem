#![deny(missing_docs)]
//! Scroll View

use floem_reactive::Effect;
use peniko::kurbo::{Point, Rect, RoundedRectRadii, Size, Stroke, Vec2};
use peniko::{Brush, Color};
use ui_events::pointer::{PointerButton, PointerButtonEvent, PointerEvent, PointerScrollEvent};

use crate::style::{
    BorderColorProp, BorderRadiusProp, CustomStylable, CustomStyle, OverflowX, OverflowY,
};
use crate::unit::PxPct;
use crate::{
    Renderer,
    context::{ComputeLayoutCx, PaintCx},
    event::{Event, EventPropagation},
    prop, prop_extractor,
    style::{Background, Style, StyleSelector},
    style_class,
    unit::Px,
    view::ViewId,
    view::{IntoView, View},
    window::state::WindowState,
};

use super::Decorators;

enum ScrollState {
    EnsureVisible(Rect),
    ScrollDelta(Vec2),
    ScrollTo(Point),
    ScrollToPercent(f32),
    ScrollToView(ViewId),
}

/// Minimum length for any scrollbar to be when measured on that
/// scrollbar's primary axis.
const SCROLLBAR_MIN_SIZE: f64 = 10.0;

/// Denotes which scrollbar, if any, is currently being dragged.
#[derive(Debug, Copy, Clone)]
enum BarHeldState {
    /// Neither scrollbar is being dragged.
    None,
    /// Vertical scrollbar is being dragged. Contains an `f64` with
    /// the initial y-offset of the dragging input.
    Vertical(f64, Vec2),
    /// Horizontal scrollbar is being dragged. Contains an `f64` with
    /// the initial x-offset of the dragging input.
    Horizontal(f64, Vec2),
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
    /// Specifies the thickness of scroll handles in pixels.
    pub Thickness: Px {} = Px(10.0)
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
        thickness: Thickness,
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

prop!(
    /// Enables clipping of overflowing content when set to true.
    pub OverflowClip: bool {} = true
);

prop_extractor!(ScrollStyle {
    vertical_bar_inset: VerticalInset,
    horizontal_bar_inset: HorizontalInset,
    hide_bar: HideBars,
    show_bars_when_idle: ShowBarsWhenIdle,
    propagate_pointer_wheel: PropagatePointerWheel,
    vertical_scroll_as_horizontal: VerticalScrollAsHorizontal,
    overflow_clip: OverflowClip,
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

    total_rect: Rect,

    /// the actual rect of the scroll view excluding padding and borders. The origin is relative to this view.
    content_rect: Rect,

    child_size: Size,

    /// Callback called when child_size changes after layout
    on_child_size: Option<Box<dyn Fn(Size)>>,

    /// The origin is relative to `actual_rect`.
    child_viewport: Rect,

    /// This is the value of `child_viewport` for the last `compute_layout`. This is used in
    /// handling for `ScrollToView` as scrolling updates may mutate `child_viewport`.
    /// The origin is relative to `actual_rect`.
    computed_child_viewport: Rect,

    onscroll: Option<Box<dyn Fn(Rect)>>,
    held: BarHeldState,
    v_handle_hover: bool,
    h_handle_hover: bool,
    v_track_hover: bool,
    h_track_hover: bool,
    /// Tracks whether user is currently interacting with scrollbars or recently scrolled
    is_scrolling_or_interacting: bool,
    handle_style: ScrollTrackStyle,
    handle_active_style: ScrollTrackStyle,
    handle_hover_style: ScrollTrackStyle,
    track_style: ScrollTrackStyle,
    track_hover_style: ScrollTrackStyle,
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
        let child = child.into_view();
        let child_id = child.id();
        id.set_children([child]);

        Scroll {
            id,
            child: child_id,
            content_rect: Rect::ZERO,
            total_rect: Rect::ZERO,
            child_size: Size::ZERO,
            on_child_size: None,
            child_viewport: Rect::ZERO,
            computed_child_viewport: Rect::ZERO,
            onscroll: None,
            held: BarHeldState::None,
            v_handle_hover: false,
            h_handle_hover: false,
            v_track_hover: false,
            h_track_hover: false,
            is_scrolling_or_interacting: false,
            handle_style: Default::default(),
            handle_active_style: Default::default(),
            handle_hover_style: Default::default(),
            track_style: Default::default(),
            track_hover_style: Default::default(),
            scroll_style: Default::default(),
        }
        .class(ScrollClass)
    }

    /// Sets a callback that will be triggered whenever the scroll position changes.
    ///
    /// This callback receives the viewport rectangle that represents the currently
    /// visible portion of the scrollable content.
    pub fn on_scroll(mut self, onscroll: impl Fn(Rect) + 'static) -> Self {
        self.onscroll = Some(Box::new(onscroll));
        self
    }

    /// Sets a callback that will be triggered whenever the child's size changes after layout.
    ///
    /// This is useful for reactive code that needs to depend on the actual
    /// laid-out size of the scroll content, ensuring proper ordering when
    /// combined with `ensure_visible`.
    pub fn on_child_size(mut self, callback: impl Fn(Size) + 'static) -> Self {
        self.on_child_size = Some(Box::new(callback));
        self
    }

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

    fn do_scroll_delta(&mut self, window_state: &mut WindowState, delta: Vec2) {
        let new_origin = self.child_viewport.origin() + delta;
        self.clamp_child_viewport(window_state, self.child_viewport.with_origin(new_origin));
    }

    fn do_scroll_to(&mut self, window_state: &mut WindowState, origin: Point) {
        self.clamp_child_viewport(window_state, self.child_viewport.with_origin(origin));
    }

    /// Ensure that an entire area is visible in the scroll view.
    // TODO: remove duplilcation between this method and pan_to_visible
    pub fn ensure_area_visible(&mut self, window_state: &mut WindowState, rect: Rect) {
        // Refresh child_size to ensure we have the latest layout
        self.update_size();
        /// Given a position and the min and max edges of an axis,
        /// return a delta by which to adjust that axis such that the value
        /// falls between its edges.
        ///
        /// if the value already falls between the two edges, return 0.0.
        fn closest_on_axis(val: f64, min: f64, max: f64) -> f64 {
            assert!(min <= max);
            if val > min && val < max {
                0.0
            } else if val <= min {
                val - min
            } else {
                val - max
            }
        }

        // clamp the target region size to our own size.
        // this means we will show the portion of the target region that
        // includes the origin.
        let target_size = Size::new(
            rect.width().min(self.child_viewport.width()),
            rect.height().min(self.child_viewport.height()),
        );
        let rect = rect.with_size(target_size);

        let x0 = closest_on_axis(
            rect.min_x(),
            self.child_viewport.min_x(),
            self.child_viewport.max_x(),
        );
        let x1 = closest_on_axis(
            rect.max_x(),
            self.child_viewport.min_x(),
            self.child_viewport.max_x(),
        );
        let y0 = closest_on_axis(
            rect.min_y(),
            self.child_viewport.min_y(),
            self.child_viewport.max_y(),
        );
        let y1 = closest_on_axis(
            rect.max_y(),
            self.child_viewport.min_y(),
            self.child_viewport.max_y(),
        );

        let delta_x = if x0.abs() > x1.abs() { x0 } else { x1 };
        let delta_y = if y0.abs() > y1.abs() { y0 } else { y1 };
        let new_origin = self.child_viewport.origin() + Vec2::new(delta_x, delta_y);
        self.clamp_child_viewport(window_state, self.child_viewport.with_origin(new_origin));
    }

    /// Pan the smallest distance that makes the target [`Rect`] visible.
    ///
    /// If the target rect is larger than viewport size, we will prioritize
    /// the region of the target closest to its origin.
    pub fn pan_to_visible(&mut self, window_state: &mut WindowState, rect: Rect) {
        // If target is larger than viewport
        if rect.width() > self.child_viewport.width()
            || rect.height() > self.child_viewport.height()
        {
            // If there's any overlap at all, don't scroll
            if rect.min_x() < self.child_viewport.max_x()
                && rect.max_x() > self.child_viewport.min_x()
                && rect.min_y() < self.child_viewport.max_y()
                && rect.max_y() > self.child_viewport.min_y()
            {
                return;
            }
        } else if rect.area() > 0. {
            // For smaller elements, check if at least 50% is visible
            let intersection = rect.intersect(self.child_viewport);

            if intersection.area() >= rect.area() * 0.5 {
                return;
            }
        }

        /// Given a position and the min and max edges of an axis,
        /// return a delta by which to adjust that axis such that the value
        /// falls between its edges.
        ///
        /// if the value already falls between the two edges, return 0.0.
        fn closest_on_axis(val: f64, min: f64, max: f64) -> f64 {
            assert!(min <= max);
            if val > min && val < max {
                0.0
            } else if val <= min {
                val - min
            } else {
                val - max
            }
        }

        // clamp the target region size to our own size.
        // this means we will show the portion of the target region that
        // includes the origin.
        let target_size = Size::new(
            rect.width().min(self.child_viewport.width()),
            rect.height().min(self.child_viewport.height()),
        );
        let rect = rect.with_size(target_size);

        let x0 = closest_on_axis(
            rect.min_x(),
            self.child_viewport.min_x(),
            self.child_viewport.max_x(),
        );
        let x1 = closest_on_axis(
            rect.max_x(),
            self.child_viewport.min_x(),
            self.child_viewport.max_x(),
        );
        let y0 = closest_on_axis(
            rect.min_y(),
            self.child_viewport.min_y(),
            self.child_viewport.max_y(),
        );
        let y1 = closest_on_axis(
            rect.max_y(),
            self.child_viewport.min_y(),
            self.child_viewport.max_y(),
        );

        let delta_x = if x0.abs() > x1.abs() { x0 } else { x1 };
        let delta_y = if y0.abs() > y1.abs() { y0 } else { y1 };
        let new_origin = self.child_viewport.origin() + Vec2::new(delta_x, delta_y);
        self.clamp_child_viewport(window_state, self.child_viewport.with_origin(new_origin));
    }

    fn update_size(&mut self) {
        self.child_size = self.child_size();
        self.content_rect = self.id.get_content_rect();
        let new_total_rect = self.id.get_size().unwrap_or_default().to_rect();
        if new_total_rect != self.total_rect {
            self.total_rect = new_total_rect;
            // request style so that the paddig for the scroll bar can be shown
            self.id.request_style();
        }
    }

    fn clamp_child_viewport(
        &mut self,
        window_state: &mut WindowState,
        child_viewport: Rect,
    ) -> Option<()> {
        let actual_rect = self.content_rect;
        let actual_size = actual_rect.size();
        let width = actual_rect.width();
        let height = actual_rect.height();
        let child_size = self.child_size;

        let mut child_viewport = child_viewport;
        if width >= child_size.width {
            child_viewport.x0 = 0.0;
        } else if child_viewport.x0 > child_size.width - width {
            child_viewport.x0 = child_size.width - width;
        } else if child_viewport.x0 < 0.0 {
            child_viewport.x0 = 0.0;
        }

        if height >= child_size.height {
            child_viewport.y0 = 0.0;
        } else if child_viewport.y0 > child_size.height - height {
            child_viewport.y0 = child_size.height - height;
        } else if child_viewport.y0 < 0.0 {
            child_viewport.y0 = 0.0;
        }
        child_viewport = child_viewport.with_size(actual_size);

        if child_viewport != self.child_viewport {
            self.child.set_viewport(child_viewport);
            window_state.request_compute_layout_recursive(self.id());
            window_state.request_paint(self.id());
            self.child_viewport = child_viewport;
            // Mark as scrolling when viewport changes
            self.is_scrolling_or_interacting = true;
            if let Some(onscroll) = &self.onscroll {
                onscroll(child_viewport);
            }
        } else {
            return None;
        }
        Some(())
    }

    fn child_size(&self) -> Size {
        self.child
            .get_layout()
            .map(|layout| Size::new(layout.size.width as f64, layout.size.height as f64))
            .unwrap()
    }

    fn v_handle_style(&self) -> &ScrollTrackStyle {
        if let BarHeldState::Vertical(..) = self.held {
            &self.handle_active_style
        } else if self.v_handle_hover {
            &self.handle_hover_style
        } else {
            &self.handle_style
        }
    }

    fn h_handle_style(&self) -> &ScrollTrackStyle {
        if let BarHeldState::Horizontal(..) = self.held {
            &self.handle_active_style
        } else if self.h_handle_hover {
            &self.handle_hover_style
        } else {
            &self.handle_style
        }
    }

    fn draw_bars(&self, cx: &mut PaintCx) {
        // Check if scrollbars should be shown based on the show_bars_when_idle property
        if !self.scroll_style.show_bars_when_idle() && !self.is_scrolling_or_interacting {
            return;
        }

        let scroll_offset = self.child_viewport.origin().to_vec2();
        let radius = |style: &ScrollTrackStyle, rect: Rect, vertical| {
            if style.rounded() {
                if vertical {
                    RoundedRectRadii::from_single_radius((rect.x1 - rect.x0) / 2.)
                } else {
                    RoundedRectRadii::from_single_radius((rect.y1 - rect.y0) / 2.)
                }
            } else {
                let size = rect.size().min_side();
                let border_radius = style.border_radius();
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
            }
        };

        if let Some(bounds) = self.calc_vertical_bar_bounds() {
            let style = self.v_handle_style();
            let track_style =
                if self.v_track_hover || matches!(self.held, BarHeldState::Vertical(..)) {
                    &self.track_hover_style
                } else {
                    &self.track_style
                };

            if let Some(color) = track_style.color() {
                let mut bounds = bounds - scroll_offset;
                bounds.y0 = self.total_rect.y0;
                bounds.y1 = self.total_rect.y1;
                cx.fill(&bounds, &color, 0.0);
            }
            let edge_width = style.border().0;
            let rect = (bounds - scroll_offset).inset(-edge_width / 2.0);
            let rect = rect.to_rounded_rect(radius(style, rect, true));
            cx.fill(&rect, &style.color().unwrap_or(HANDLE_COLOR), 0.0);
            if edge_width > 0.0
                && let Some(color) = style.border_color().right
            {
                cx.stroke(&rect, &color, &Stroke::new(edge_width));
            }
        }

        // Horizontal bar
        if let Some(bounds) = self.calc_horizontal_bar_bounds() {
            let style = self.h_handle_style();
            let track_style =
                if self.h_track_hover || matches!(self.held, BarHeldState::Horizontal(..)) {
                    &self.track_hover_style
                } else {
                    &self.track_style
                };

            if let Some(color) = track_style.color() {
                let mut bounds = bounds - scroll_offset;
                bounds.x0 = self.total_rect.x0;
                bounds.x1 = self.total_rect.x1;
                cx.fill(&bounds, &color, 0.0);
            }
            let edge_width = style.border().0;
            let rect = (bounds - scroll_offset).inset(-edge_width / 2.0);
            let rect = rect.to_rounded_rect(radius(style, rect, false));
            cx.fill(&rect, &style.color().unwrap_or(HANDLE_COLOR), 0.0);
            if edge_width > 0.0
                && let Some(color) = style.border_color().right
            {
                cx.stroke(&rect, &color, &Stroke::new(edge_width));
            }
        }
    }

    fn calc_vertical_bar_bounds(&self) -> Option<Rect> {
        let viewport_size = self.child_viewport.size();
        let content_size = self.child_size;
        let scroll_offset = self.child_viewport.origin().to_vec2();

        if viewport_size.height >= content_size.height - 1. {
            return None;
        }

        let style = self.v_handle_style();

        let bar_width = style.thickness().0;
        let bar_pad = self.scroll_style.vertical_bar_inset().0;

        let percent_visible = viewport_size.height / content_size.height;
        let percent_scrolled = scroll_offset.y / (content_size.height - viewport_size.height);

        let length = (percent_visible * self.total_rect.height()).ceil();
        // Vertical scroll bar must have ast least the same height as it's width
        let length = length.max(style.thickness().0);

        let top_y_offset = ((self.total_rect.height() - length) * percent_scrolled).ceil();
        let bottom_y_offset = top_y_offset + length;

        let x0 = scroll_offset.x + self.total_rect.width() - bar_width - bar_pad;
        let y0 = scroll_offset.y + top_y_offset;

        let x1 = scroll_offset.x + self.total_rect.width() - bar_pad;
        let y1 = scroll_offset.y + bottom_y_offset;

        Some(Rect::new(x0, y0, x1, y1))
    }

    fn calc_horizontal_bar_bounds(&self) -> Option<Rect> {
        let viewport_size = self.child_viewport.size();
        let content_size = self.child_size;
        let scroll_offset = self.child_viewport.origin().to_vec2();

        if viewport_size.width >= content_size.width - 1. {
            return None;
        }

        let style = self.h_handle_style();

        let bar_width = style.thickness().0;
        let bar_pad = self.scroll_style.horizontal_bar_inset().0;

        let percent_visible = viewport_size.width / content_size.width;
        let percent_scrolled = scroll_offset.x / (content_size.width - viewport_size.width);

        let length = (percent_visible * self.total_rect.width()).ceil();
        let length = length.max(SCROLLBAR_MIN_SIZE);

        let horizontal_padding = if viewport_size.height >= content_size.height {
            0.0
        } else {
            bar_pad + bar_pad + bar_width
        };

        let left_x_offset =
            ((self.total_rect.width() - length - horizontal_padding) * percent_scrolled).ceil();
        let right_x_offset = left_x_offset + length;

        let x0 = scroll_offset.x + left_x_offset;
        let y0 = scroll_offset.y + self.total_rect.height() - bar_width - bar_pad;

        let x1 = scroll_offset.x + right_x_offset;
        let y1 = scroll_offset.y + self.total_rect.height() - bar_pad;

        Some(Rect::new(x0, y0, x1, y1))
    }

    fn click_vertical_bar_area(&mut self, window_state: &mut WindowState, pos: Point) {
        let new_y = (pos.y / self.content_rect.height()) * self.child_size.height
            - self.content_rect.height() / 2.0;
        let mut new_origin = self.child_viewport.origin();
        new_origin.y = new_y;
        self.do_scroll_to(window_state, new_origin);
    }

    fn click_horizontal_bar_area(&mut self, window_state: &mut WindowState, pos: Point) {
        let new_x = (pos.x / self.content_rect.width()) * self.child_size.width
            - self.content_rect.width() / 2.0;
        let mut new_origin = self.child_viewport.origin();
        new_origin.x = new_x;
        self.do_scroll_to(window_state, new_origin);
    }

    fn point_hits_vertical_bar(&self, pos: Point) -> bool {
        if let Some(mut bounds) = self.calc_vertical_bar_bounds() {
            // Stretch hitbox to edge of widget
            let scroll_offset = self.child_viewport.origin().to_vec2();
            bounds.x1 = self.total_rect.x1 + scroll_offset.x;
            pos.x >= bounds.x0 && pos.x <= bounds.x1
        } else {
            false
        }
    }

    fn point_hits_horizontal_bar(&self, pos: Point) -> bool {
        if let Some(mut bounds) = self.calc_horizontal_bar_bounds() {
            // Stretch hitbox to edge of widget
            let scroll_offset = self.child_viewport.origin().to_vec2();
            bounds.y1 = self.total_rect.y1 + scroll_offset.y;
            pos.y >= bounds.y0 && pos.y <= bounds.y1
        } else {
            false
        }
    }

    fn point_hits_vertical_handle(&self, pos: Point) -> bool {
        if let Some(mut bounds) = self.calc_vertical_bar_bounds() {
            // Stretch hitbox to edge of widget
            let scroll_offset = self.child_viewport.origin().to_vec2();
            bounds.x1 = self.total_rect.x1 + scroll_offset.x;
            bounds.contains(pos)
        } else {
            false
        }
    }

    fn point_hits_horizontal_handle(&self, pos: Point) -> bool {
        if let Some(mut bounds) = self.calc_horizontal_bar_bounds() {
            // Stretch hitbox to edge of widget
            let scroll_offset = self.child_viewport.origin().to_vec2();
            bounds.y1 = self.total_rect.y1 + scroll_offset.y;
            bounds.contains(pos)
        } else {
            false
        }
    }

    /// true if either scrollbar is currently held down/being dragged
    fn are_bars_held(&self) -> bool {
        !matches!(self.held, BarHeldState::None)
    }

    fn update_hover_states(&mut self, window_state: &mut WindowState, pos: Point) {
        let scroll_offset = self.child_viewport.origin().to_vec2();
        let pos = pos + scroll_offset;
        let hover = self.point_hits_vertical_handle(pos);
        if self.v_handle_hover != hover {
            self.v_handle_hover = hover;
            window_state.request_paint(self.id());
        }
        let hover = self.point_hits_horizontal_handle(pos);
        if self.h_handle_hover != hover {
            self.h_handle_hover = hover;
            window_state.request_paint(self.id());
        }
        let hover = self.point_hits_vertical_bar(pos);
        if self.v_track_hover != hover {
            self.v_track_hover = hover;
            window_state.request_paint(self.id());
        }
        let hover = self.point_hits_horizontal_bar(pos);
        if self.h_track_hover != hover {
            self.h_track_hover = hover;
            window_state.request_paint(self.id());
        }

        // Set scrolling/interacting state if hovering over scrollbars
        let any_hover =
            self.v_handle_hover || self.h_handle_hover || self.v_track_hover || self.h_track_hover;
        if any_hover != self.is_scrolling_or_interacting {
            self.is_scrolling_or_interacting = any_hover;
            window_state.request_paint(self.id());
        }
    }

    fn do_scroll_to_view(
        &mut self,
        window_state: &mut WindowState,
        target: ViewId,
        target_rect: Option<Rect>,
    ) {
        if target.get_layout().is_some() && !target.is_hidden() {
            let mut rect = target.layout_rect();

            if let Some(target_rect) = target_rect {
                rect = rect + target_rect.origin().to_vec2();

                let new_size = target_rect
                    .size()
                    .to_rect()
                    .intersect(rect.size().to_rect())
                    .size();
                rect = rect.with_size(new_size);
            }

            // `get_layout_rect` is window-relative so we have to
            // convert it to child view relative.

            // TODO: How to deal with nested viewports / scrolls?
            let rect = rect.with_origin(
                rect.origin()
                    - self.id.layout_rect().origin().to_vec2()
                    - self.content_rect.origin().to_vec2()
                    + self.computed_child_viewport.origin().to_vec2(),
            );

            self.pan_to_visible(window_state, rect);
        }
    }

    /// Sets the custom style properties of the `Scroll`.
    pub fn scroll_style(
        self,
        style: impl Fn(ScrollCustomStyle) -> ScrollCustomStyle + 'static,
    ) -> Self {
        self.custom_style(style)
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
                .set(OverflowX, taffy::Overflow::Scroll)
                .set(OverflowY, taffy::Overflow::Scroll),
        )
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<ScrollState>() {
            match *state {
                ScrollState::EnsureVisible(rect) => {
                    self.ensure_area_visible(cx.window_state, rect);
                }
                ScrollState::ScrollDelta(delta) => {
                    self.do_scroll_delta(cx.window_state, delta);
                }
                ScrollState::ScrollTo(origin) => {
                    self.do_scroll_to(cx.window_state, origin);
                }
                ScrollState::ScrollToPercent(percent) => {
                    let mut child_size = self.child_size;
                    child_size *= percent as f64;
                    let point = child_size.to_vec2().to_point();
                    self.do_scroll_to(cx.window_state, point);
                }
                ScrollState::ScrollToView(id) => {
                    self.do_scroll_to_view(cx.window_state, id, None);
                }
            }
            self.id.request_layout();
        }
    }

    fn scroll_to(&mut self, cx: &mut WindowState, target: ViewId, rect: Option<Rect>) -> bool {
        let found = self.child.view().borrow_mut().scroll_to(cx, target, rect);
        if found {
            self.do_scroll_to_view(cx, target, rect);
        }
        found
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        let style = cx.style();

        self.scroll_style.read(cx);

        let handle_style = style.clone().apply_class(Handle);
        self.handle_style.read_style(cx, &handle_style);
        self.handle_hover_style.read_style(
            cx,
            &handle_style
                .clone()
                .apply_selectors(&[StyleSelector::Hover]),
        );
        self.handle_active_style
            .read_style(cx, &handle_style.apply_selectors(&[StyleSelector::Active]));

        let track_style = style.apply_class(Track);
        self.track_style.read_style(cx, &track_style);
        self.track_hover_style
            .read_style(cx, &track_style.apply_selectors(&[StyleSelector::Hover]));
    }

    fn compute_layout(&mut self, cx: &mut ComputeLayoutCx) -> Option<Rect> {
        let old_child_size = self.child_size;
        self.update_size();
        // Call callback if child size changed
        if old_child_size != self.child_size
            && let Some(callback) = &self.on_child_size
        {
            callback(self.child_size);
        }
        self.clamp_child_viewport(cx.window_state, self.child_viewport);
        self.computed_child_viewport = self.child_viewport;
        cx.compute_view_layout(self.child);
        None
    }

    fn event_before_children(
        &mut self,
        cx: &mut crate::context::EventCx,
        event: &Event,
    ) -> EventPropagation {
        let viewport_size = self.child_viewport.size();
        let scroll_offset = self.child_viewport.origin().to_vec2();
        let content_size = self.child_size;

        match &event {
            Event::Pointer(PointerEvent::Down(PointerButtonEvent { button, state, .. })) => {
                if !self.scroll_style.hide_bar()
                    && button.is_some_and(|b| b == PointerButton::Primary)
                {
                    self.held = BarHeldState::None;

                    let pos = state.logical_point() + scroll_offset;

                    if self.point_hits_vertical_bar(pos) {
                        if self.point_hits_vertical_handle(pos) {
                            self.held = BarHeldState::Vertical(
                                // The bounds must be non-empty, because the point hits the scrollbar.
                                state.logical_point().y,
                                scroll_offset,
                            );
                            cx.update_active(self.id());
                            // Force a repaint.
                            self.id.request_paint();
                            return EventPropagation::Stop;
                        }
                        self.click_vertical_bar_area(cx.window_state, state.logical_point());
                        let scroll_offset = self.child_viewport.origin().to_vec2();
                        self.held = BarHeldState::Vertical(
                            // The bounds must be non-empty, because the point hits the scrollbar.
                            state.logical_point().y,
                            scroll_offset,
                        );
                        cx.update_active(self.id());
                        return EventPropagation::Stop;
                    } else if self.point_hits_horizontal_bar(pos) {
                        if self.point_hits_horizontal_handle(pos) {
                            self.held = BarHeldState::Horizontal(
                                // The bounds must be non-empty, because the point hits the scrollbar.
                                state.logical_point().x,
                                scroll_offset,
                            );
                            cx.update_active(self.id());
                            // Force a repaint.
                            cx.window_state.request_paint(self.id());
                            return EventPropagation::Stop;
                        }
                        self.click_horizontal_bar_area(cx.window_state, state.logical_point());
                        let scroll_offset = self.child_viewport.origin().to_vec2();
                        self.held = BarHeldState::Horizontal(
                            // The bounds must be non-empty, because the point hits the scrollbar.
                            state.logical_point().x,
                            scroll_offset,
                        );
                        cx.update_active(self.id());
                        return EventPropagation::Stop;
                    }
                }
            }
            Event::Pointer(PointerEvent::Up { .. }) => {
                if self.are_bars_held() {
                    self.held = BarHeldState::None;
                    // Force a repaint.
                    cx.window_state.request_paint(self.id());
                }
            }
            Event::Pointer(PointerEvent::Move(pu)) => {
                if !self.scroll_style.hide_bar() {
                    let pos = pu.current.logical_point() + scroll_offset;
                    self.update_hover_states(cx.window_state, pu.current.logical_point());

                    if self.are_bars_held() {
                        match self.held {
                            BarHeldState::Vertical(offset, initial_scroll_offset) => {
                                let scale_y = viewport_size.height / content_size.height;
                                let y = initial_scroll_offset.y
                                    + (pu.current.logical_point().y - offset) / scale_y;
                                self.clamp_child_viewport(
                                    cx.window_state,
                                    self.child_viewport
                                        .with_origin(Point::new(initial_scroll_offset.x, y)),
                                );
                            }
                            BarHeldState::Horizontal(offset, initial_scroll_offset) => {
                                let scale_x = viewport_size.width / content_size.width;
                                let x = initial_scroll_offset.x
                                    + (pu.current.logical_point().x - offset) / scale_x;
                                self.clamp_child_viewport(
                                    cx.window_state,
                                    self.child_viewport
                                        .with_origin(Point::new(x, initial_scroll_offset.y)),
                                );
                            }
                            BarHeldState::None => {}
                        }
                    } else if self.point_hits_vertical_bar(pos)
                        || self.point_hits_horizontal_bar(pos)
                    {
                        return EventPropagation::Stop;
                    }
                }
            }
            Event::Pointer(PointerEvent::Leave(_)) => {
                self.v_handle_hover = false;
                self.h_handle_hover = false;
                self.v_track_hover = false;
                self.h_track_hover = false;
                self.is_scrolling_or_interacting = false;
                cx.window_state.request_paint(self.id());
            }
            _ => {}
        }
        EventPropagation::Continue
    }

    fn event_after_children(
        &mut self,
        cx: &mut crate::context::EventCx,
        event: &Event,
    ) -> EventPropagation {
        if let Event::Pointer(PointerEvent::Scroll(PointerScrollEvent { state, .. })) = &event {
            if let Some(listener) = event.listener()
                && self
                    .id
                    .apply_event(&listener, event)
                    .is_some_and(|prop| prop.is_processed())
            {
                return EventPropagation::Stop;
            }
            if let Some(delta) = event.pixel_scroll_delta_vec2() {
                let delta = -if self.scroll_style.vertical_scroll_as_horizontal()
                    && delta.x == 0.0
                    && delta.y != 0.0
                {
                    Vec2::new(delta.y, delta.x)
                } else {
                    delta
                };
                let any_change =
                    self.clamp_child_viewport(cx.window_state, self.child_viewport + delta);

                // Check if the scroll bars now hover
                self.update_hover_states(cx.window_state, state.logical_point());

                return if self.scroll_style.propagate_pointer_wheel() && any_change.is_none() {
                    EventPropagation::Continue
                } else {
                    EventPropagation::Stop
                };
            }
        }

        EventPropagation::Continue
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.save();
        let radii = crate::view::border_to_radii(
            &self.id.state().borrow().combined_style,
            self.total_rect.size(),
        );
        if self.scroll_style.overflow_clip() {
            if crate::view::radii_max(radii) > 0.0 {
                let rect = self.total_rect.to_rounded_rect(radii);
                cx.clip(&rect);
            } else {
                cx.clip(&self.total_rect);
            }
        }
        cx.offset((-self.child_viewport.x0, -self.child_viewport.y0));
        cx.paint_view(self.child);
        cx.restore();

        if !self.scroll_style.hide_bar() {
            self.draw_bars(cx);
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

    /// Conditionally configures the scroll view to clip the overflow of the content.
    pub fn overflow_clip(mut self, clip: bool) -> Self {
        self = Self(self.0.set(OverflowClip, clip));
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

    /// Sets the thickness of the handle.
    pub fn handle_thickness(mut self, thickness: impl Into<Px>) -> Self {
        self = Self(self.0.class(Handle, |s| s.set(Thickness, thickness)));
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

    /// Sets the thickness of the track.
    pub fn track_thickness(mut self, thickness: impl Into<Px>) -> Self {
        self = Self(self.0.class(Track, |s| s.set(Thickness, thickness)));
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
