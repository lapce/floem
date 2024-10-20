use floem_reactive::create_effect;
use floem_renderer::Renderer;
use peniko::kurbo::{Point, Rect, Size, Vec2};
use peniko::{Brush, Color};

use crate::style::CustomStylable;
use crate::unit::PxPct;
use crate::{
    app_state::AppState,
    context::{ComputeLayoutCx, PaintCx},
    event::{Event, EventPropagation},
    id::ViewId,
    prop, prop_extractor,
    style::{Background, BorderColor, BorderRadius, Style, StyleSelector},
    style_class,
    unit::Px,
    view::{IntoView, View},
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

style_class!(pub Handle);
style_class!(pub Track);

prop!(pub Rounded: bool {} = cfg!(target_os = "macos"));
prop!(pub Thickness: Px {} = Px(10.0));
prop!(pub Border: Px {} = Px(0.0));

prop_extractor! {
    ScrollTrackStyle {
        color: Background,
        border_radius: BorderRadius,
        border_color: BorderColor,
        border: Border,
        rounded: Rounded,
        thickness: Thickness,
    }
}

prop!(pub VerticalInset: Px {} = Px(0.0));
prop!(pub HorizontalInset: Px {} = Px(0.0));
prop!(pub HideBars: bool {} = false);
prop!(pub PropagatePointerWheel: bool {} = true);
prop!(pub VerticalScrollAsHorizontal: bool {} = false);
prop!(pub OverflowClip: bool {} = true);

prop_extractor!(ScrollStyle {
    vertical_bar_inset: VerticalInset,
    horizontal_bar_inset: HorizontalInset,
    hide_bar: HideBars,
    propagate_pointer_wheel: PropagatePointerWheel,
    vertical_scroll_as_horizontal: VerticalScrollAsHorizontal,
    overflow_clip: OverflowClip,
});

const HANDLE_COLOR: Brush = Brush::Solid(Color::rgba8(0, 0, 0, 120));

style_class!(pub ScrollClass);

pub struct Scroll {
    id: ViewId,
    child: ViewId,

    total_rect: Rect,

    /// the actual rect of the scroll view excluding padding and borders. The origin is relative to this view.
    content_rect: Rect,

    child_size: Size,

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
    handle_style: ScrollTrackStyle,
    handle_active_style: ScrollTrackStyle,
    handle_hover_style: ScrollTrackStyle,
    track_style: ScrollTrackStyle,
    track_hover_style: ScrollTrackStyle,
    scroll_style: ScrollStyle,
}

pub fn scroll<V: IntoView + 'static>(child: V) -> Scroll {
    let id = ViewId::new();
    let child = child.into_view();
    let child_id = child.id();
    id.set_children(vec![child]);

    Scroll {
        id,
        child: child_id,
        content_rect: Rect::ZERO,
        total_rect: Rect::ZERO,
        child_size: Size::ZERO,
        child_viewport: Rect::ZERO,
        computed_child_viewport: Rect::ZERO,
        onscroll: None,
        held: BarHeldState::None,
        v_handle_hover: false,
        h_handle_hover: false,
        v_track_hover: false,
        h_track_hover: false,
        handle_style: Default::default(),
        handle_active_style: Default::default(),
        handle_hover_style: Default::default(),
        track_style: Default::default(),
        track_hover_style: Default::default(),
        scroll_style: Default::default(),
    }
    .class(ScrollClass)
}

impl Scroll {
    pub fn on_scroll(mut self, onscroll: impl Fn(Rect) + 'static) -> Self {
        self.onscroll = Some(Box::new(onscroll));
        self
    }

    pub fn ensure_visible(self, to: impl Fn() -> Rect + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            let rect = to();
            id.update_state_deferred(ScrollState::EnsureVisible(rect));
        });

        self
    }

    pub fn scroll_delta(self, delta: impl Fn() -> Vec2 + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            let delta = delta();
            id.update_state(ScrollState::ScrollDelta(delta));
        });

        self
    }

    pub fn scroll_to(self, origin: impl Fn() -> Option<Point> + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            if let Some(origin) = origin() {
                id.update_state_deferred(ScrollState::ScrollTo(origin));
            }
        });

        self
    }

    /// Scroll the scroll view to a percent (0-100)
    pub fn scroll_to_percent(self, percent: impl Fn() -> f32 + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            let percent = percent() / 100.;
            id.update_state_deferred(ScrollState::ScrollToPercent(percent));
        });
        self
    }

    pub fn scroll_to_view(self, view: impl Fn() -> Option<ViewId> + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            if let Some(view) = view() {
                id.update_state_deferred(ScrollState::ScrollToView(view));
            }
        });

        self
    }

    fn do_scroll_delta(&mut self, app_state: &mut AppState, delta: Vec2) {
        let new_origin = self.child_viewport.origin() + delta;
        self.clamp_child_viewport(app_state, self.child_viewport.with_origin(new_origin));
    }

    fn do_scroll_to(&mut self, app_state: &mut AppState, origin: Point) {
        self.clamp_child_viewport(app_state, self.child_viewport.with_origin(origin));
    }

    /// Pan the smallest distance that makes the target [`Rect`] visible.
    ///
    /// If the target rect is larger than viewport size, we will prioritize
    /// the region of the target closest to its origin.
    pub fn pan_to_visible(&mut self, app_state: &mut AppState, rect: Rect) {
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
        self.clamp_child_viewport(app_state, self.child_viewport.with_origin(new_origin));
    }

    fn update_size(&mut self) {
        self.child_size = self.child_size();
        self.content_rect = self.id.get_content_rect();
        self.total_rect = self.id.get_size().unwrap_or_default().to_rect();
    }

    fn clamp_child_viewport(
        &mut self,
        app_state: &mut AppState,
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
            app_state.request_compute_layout_recursive(self.id());
            app_state.request_paint(self.id());
            self.child_viewport = child_viewport;
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
        let scroll_offset = self.child_viewport.origin().to_vec2();
        let radius = |style: &ScrollTrackStyle, rect: Rect, vertical| {
            if style.rounded() {
                if vertical {
                    (rect.x1 - rect.x0) / 2.
                } else {
                    (rect.y1 - rect.y0) / 2.
                }
            } else {
                match style.border_radius() {
                    crate::unit::PxPct::Px(px) => px,
                    crate::unit::PxPct::Pct(pct) => rect.size().min_side() * (pct / 100.),
                }
            }
        };

        if let Some(bounds) = self.calc_vertical_bar_bounds(cx.app_state) {
            let style = self.v_handle_style();
            let track_style =
                if self.v_track_hover || matches!(self.held, BarHeldState::Vertical(..)) {
                    &self.track_hover_style
                } else {
                    &self.track_style
                };

            if let Some(color) = track_style.color() {
                let mut bounds = bounds - scroll_offset;
                bounds.y0 = self.content_rect.y0;
                bounds.y1 = self.content_rect.y1;
                cx.fill(&bounds, &color, 0.0);
            }
            let edge_width = style.border().0;
            let rect = (bounds - scroll_offset).inset(-edge_width / 2.0);
            let rect = rect.to_rounded_rect(radius(style, rect, true));
            cx.fill(&rect, &style.color().unwrap_or(HANDLE_COLOR), 0.0);
            if edge_width > 0.0 {
                cx.stroke(&rect, &style.border_color(), edge_width);
            }
        }

        // Horizontal bar
        if let Some(bounds) = self.calc_horizontal_bar_bounds(cx.app_state) {
            let style = self.h_handle_style();
            let track_style =
                if self.h_track_hover || matches!(self.held, BarHeldState::Horizontal(..)) {
                    &self.track_hover_style
                } else {
                    &self.track_style
                };

            if let Some(color) = track_style.color() {
                let mut bounds = bounds - scroll_offset;
                bounds.x0 = self.content_rect.x0;
                bounds.x1 = self.content_rect.x1;
                cx.fill(&bounds, &color, 0.0);
            }
            let edge_width = style.border().0;
            let rect = (bounds - scroll_offset).inset(-edge_width / 2.0);
            let rect = rect.to_rounded_rect(radius(style, rect, false));
            cx.fill(&rect, &style.color().unwrap_or(HANDLE_COLOR), 0.0);
            if edge_width > 0.0 {
                cx.stroke(&rect, &style.border_color(), edge_width);
            }
        }
    }

    fn calc_vertical_bar_bounds(&self, _app_state: &mut AppState) -> Option<Rect> {
        let viewport_size = self.child_viewport.size();
        let content_size = self.child_size;
        let scroll_offset = self.child_viewport.origin().to_vec2();

        // dbg!(viewport_size.height, content_size.height);
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

    fn calc_horizontal_bar_bounds(&self, _app_state: &mut AppState) -> Option<Rect> {
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

    fn click_vertical_bar_area(&mut self, app_state: &mut AppState, pos: Point) {
        let new_y = (pos.y / self.content_rect.height()) * self.child_size.height
            - self.content_rect.height() / 2.0;
        let mut new_origin = self.child_viewport.origin();
        new_origin.y = new_y;
        self.do_scroll_to(app_state, new_origin);
    }

    fn click_horizontal_bar_area(&mut self, app_state: &mut AppState, pos: Point) {
        let new_x = (pos.x / self.content_rect.width()) * self.child_size.width
            - self.content_rect.width() / 2.0;
        let mut new_origin = self.child_viewport.origin();
        new_origin.x = new_x;
        self.do_scroll_to(app_state, new_origin);
    }

    fn point_within_vertical_bar(&self, app_state: &mut AppState, pos: Point) -> bool {
        let viewport_size = self.child_viewport.size();
        let scroll_offset = self.child_viewport.origin().to_vec2();

        if let Some(mut bounds) = self.calc_vertical_bar_bounds(app_state) {
            // Stretch hitbox to edge of widget
            bounds.x1 = scroll_offset.x + viewport_size.width;
            pos.x >= bounds.x0 && pos.x <= bounds.x1
        } else {
            false
        }
    }

    fn point_within_horizontal_bar(&self, app_state: &mut AppState, pos: Point) -> bool {
        let viewport_size = self.child_viewport.size();
        let scroll_offset = self.child_viewport.origin().to_vec2();

        if let Some(mut bounds) = self.calc_horizontal_bar_bounds(app_state) {
            // Stretch hitbox to edge of widget
            bounds.y1 = scroll_offset.y + viewport_size.height;
            pos.y >= bounds.y0 && pos.y <= bounds.y1
        } else {
            false
        }
    }

    fn point_hits_vertical_bar(&self, app_state: &mut AppState, pos: Point) -> bool {
        let viewport_size = self.child_viewport.size();
        let scroll_offset = self.child_viewport.origin().to_vec2();

        if let Some(mut bounds) = self.calc_vertical_bar_bounds(app_state) {
            // Stretch hitbox to edge of widget
            bounds.x1 = scroll_offset.x + viewport_size.width;
            bounds.contains(pos)
        } else {
            false
        }
    }

    fn point_hits_horizontal_bar(&self, app_state: &mut AppState, pos: Point) -> bool {
        let viewport_size = self.child_viewport.size();
        let scroll_offset = self.child_viewport.origin().to_vec2();

        if let Some(mut bounds) = self.calc_horizontal_bar_bounds(app_state) {
            // Stretch hitbox to edge of widget
            bounds.y1 = scroll_offset.y + viewport_size.height;
            bounds.contains(pos)
        } else {
            false
        }
    }

    /// true if either scrollbar is currently held down/being dragged
    fn are_bars_held(&self) -> bool {
        !matches!(self.held, BarHeldState::None)
    }

    fn update_hover_states(&mut self, app_state: &mut AppState, pos: Point) {
        let scroll_offset = self.child_viewport.origin().to_vec2();
        let pos = pos + scroll_offset;
        let hover = self.point_hits_vertical_bar(app_state, pos);
        if self.v_handle_hover != hover {
            self.v_handle_hover = hover;
            app_state.request_paint(self.id());
        }
        let hover = self.point_hits_horizontal_bar(app_state, pos);
        if self.h_handle_hover != hover {
            self.h_handle_hover = hover;
            app_state.request_paint(self.id());
        }
        let hover = self.point_within_vertical_bar(app_state, pos);
        if self.v_track_hover != hover {
            self.v_track_hover = hover;
            app_state.request_paint(self.id());
        }
        let hover = self.point_within_horizontal_bar(app_state, pos);
        if self.h_track_hover != hover {
            self.h_track_hover = hover;
            app_state.request_paint(self.id());
        }
    }

    fn do_scroll_to_view(
        &mut self,
        app_state: &mut AppState,
        target: ViewId,
        target_rect: Option<Rect>,
    ) {
        if target.get_layout().is_some() && !target.is_hidden_recursive() {
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

            self.pan_to_visible(app_state, rect);
        }
    }

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
        Some(Style::new().items_start())
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<ScrollState>() {
            match *state {
                ScrollState::EnsureVisible(rect) => {
                    self.pan_to_visible(cx.app_state, rect);
                }
                ScrollState::ScrollDelta(delta) => {
                    self.do_scroll_delta(cx.app_state, delta);
                }
                ScrollState::ScrollTo(origin) => {
                    self.do_scroll_to(cx.app_state, origin);
                }
                ScrollState::ScrollToPercent(percent) => {
                    let mut child_size = self.child_size;
                    child_size *= percent as f64;
                    let point = child_size.to_vec2().to_point();
                    self.do_scroll_to(cx.app_state, point);
                }
                ScrollState::ScrollToView(id) => {
                    self.do_scroll_to_view(cx.app_state, id, None);
                }
            }
            self.id.request_layout();
        }
    }

    fn scroll_to(&mut self, cx: &mut AppState, target: ViewId, rect: Option<Rect>) -> bool {
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

        cx.style_view(self.child);
    }

    fn compute_layout(&mut self, cx: &mut ComputeLayoutCx) -> Option<Rect> {
        self.update_size();
        self.clamp_child_viewport(cx.app_state_mut(), self.child_viewport);
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
            Event::PointerDown(event) => {
                if !self.scroll_style.hide_bar() && event.button.is_primary() {
                    self.held = BarHeldState::None;

                    let pos = event.pos + scroll_offset;

                    if self.point_within_vertical_bar(cx.app_state, pos) {
                        if self.point_hits_vertical_bar(cx.app_state, pos) {
                            self.held = BarHeldState::Vertical(
                                // The bounds must be non-empty, because the point hits the scrollbar.
                                event.pos.y,
                                scroll_offset,
                            );
                            cx.update_active(self.id());
                            // Force a repaint.
                            self.id.request_paint();
                            return EventPropagation::Stop;
                        }
                        self.click_vertical_bar_area(cx.app_state, event.pos);
                        let scroll_offset = self.child_viewport.origin().to_vec2();
                        self.held = BarHeldState::Vertical(
                            // The bounds must be non-empty, because the point hits the scrollbar.
                            event.pos.y,
                            scroll_offset,
                        );
                        cx.update_active(self.id());
                        return EventPropagation::Stop;
                    } else if self.point_within_horizontal_bar(cx.app_state, pos) {
                        if self.point_hits_horizontal_bar(cx.app_state, pos) {
                            self.held = BarHeldState::Horizontal(
                                // The bounds must be non-empty, because the point hits the scrollbar.
                                event.pos.x,
                                scroll_offset,
                            );
                            cx.update_active(self.id());
                            // Force a repaint.
                            cx.app_state.request_paint(self.id());
                            return EventPropagation::Stop;
                        }
                        self.click_horizontal_bar_area(cx.app_state, event.pos);
                        let scroll_offset = self.child_viewport.origin().to_vec2();
                        self.held = BarHeldState::Horizontal(
                            // The bounds must be non-empty, because the point hits the scrollbar.
                            event.pos.x,
                            scroll_offset,
                        );
                        cx.update_active(self.id());
                        return EventPropagation::Stop;
                    }
                }
            }
            Event::PointerUp(_event) => {
                if self.are_bars_held() {
                    self.held = BarHeldState::None;
                    // Force a repaint.
                    cx.app_state.request_paint(self.id());
                }
            }
            Event::PointerMove(event) => {
                if !self.scroll_style.hide_bar() {
                    let pos = event.pos + scroll_offset;
                    self.update_hover_states(cx.app_state, event.pos);

                    if self.are_bars_held() {
                        match self.held {
                            BarHeldState::Vertical(offset, initial_scroll_offset) => {
                                let scale_y = viewport_size.height / content_size.height;
                                let y = initial_scroll_offset.y + (event.pos.y - offset) / scale_y;
                                self.clamp_child_viewport(
                                    cx.app_state,
                                    self.child_viewport
                                        .with_origin(Point::new(initial_scroll_offset.x, y)),
                                );
                            }
                            BarHeldState::Horizontal(offset, initial_scroll_offset) => {
                                let scale_x = viewport_size.width / content_size.width;
                                let x = initial_scroll_offset.x + (event.pos.x - offset) / scale_x;
                                self.clamp_child_viewport(
                                    cx.app_state,
                                    self.child_viewport
                                        .with_origin(Point::new(x, initial_scroll_offset.y)),
                                );
                            }
                            BarHeldState::None => {}
                        }
                    } else if self.point_within_vertical_bar(cx.app_state, pos)
                        || self.point_within_horizontal_bar(cx.app_state, pos)
                    {
                        return EventPropagation::Continue;
                    }
                }
            }
            Event::PointerLeave => {
                self.v_handle_hover = false;
                self.h_handle_hover = false;
                self.v_track_hover = false;
                self.h_track_hover = false;
                cx.app_state.request_paint(self.id());
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
        if let Event::PointerWheel(pointer_event) = &event {
            if let Some(listener) = event.listener() {
                if self
                    .id
                    .apply_event(&listener, event)
                    .is_some_and(|prop| prop.is_processed())
                {
                    return EventPropagation::Stop;
                }
            }
            let delta = pointer_event.delta;
            let delta = if self.scroll_style.vertical_scroll_as_horizontal()
                && delta.x == 0.0
                && delta.y != 0.0
            {
                Vec2::new(delta.y, delta.x)
            } else {
                delta
            };
            let any_change = self.clamp_child_viewport(cx.app_state, self.child_viewport + delta);

            // Check if the scroll bars now hover
            self.update_hover_states(cx.app_state, pointer_event.pos);

            return if self.scroll_style.propagate_pointer_wheel() && any_change.is_none() {
                EventPropagation::Continue
            } else {
                EventPropagation::Stop
            };
        }

        EventPropagation::Continue
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.save();
        let radius = match self.id.state().borrow().combined_style.get(BorderRadius) {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => self.total_rect.size().min_side() * (pct / 100.),
        };
        if self.scroll_style.overflow_clip() {
            if radius > 0.0 {
                let rect = self.total_rect.to_rounded_rect(radius);
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
/// Represents a custom style for a `Label`.
#[derive(Default, Debug, Clone)]
pub struct ScrollCustomStyle(Style);
impl From<ScrollCustomStyle> for Style {
    fn from(value: ScrollCustomStyle) -> Self {
        value.0
    }
}

impl CustomStylable<ScrollCustomStyle> for Scroll {
    type DV = Self;
}

impl ScrollCustomStyle {
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
}

pub trait ScrollExt {
    fn scroll(self) -> Scroll;
}

impl<T: IntoView + 'static> ScrollExt for T {
    fn scroll(self) -> Scroll {
        scroll(self)
    }
}
