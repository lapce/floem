use floem_renderer::Renderer;
use glazier::kurbo::{Point, Rect, Size, Vec2};
use leptos_reactive::create_effect;
use taffy::{
    prelude::Node,
    style::{Dimension, Position},
};
use vello::peniko::Color;

use crate::{
    app::AppContext,
    context::{AppState, LayoutCx, PaintCx},
    event::Event,
    id::Id,
    style::{ComputedStyle, Style, StyleValue},
    view::{ChangeFlags, View},
};

enum ScrollState {
    EnsureVisble(Rect),
    ScrollDelta(Vec2),
    ScrollTo(Point),
    ScrollBarColor(Color),
    HiddenBar(bool),
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

pub struct Scroll<V: View> {
    id: Id,
    child: V,
    size: Size,
    child_size: Size,
    child_viewport: Rect,
    onscroll: Option<Box<dyn Fn(Rect)>>,
    held: BarHeldState,
    virtual_node: Option<Node>,
    hide_bar: bool,
    opacity: f64,
    scroll_bar_color: Color,
}

pub fn scroll<V: View>(cx: AppContext, child: impl Fn(AppContext) -> V) -> Scroll<V> {
    let id = cx.new_id();

    let mut child_cx = cx;
    child_cx.id = id;
    let child = child(child_cx);

    Scroll {
        id,
        child,
        size: Size::ZERO,
        child_size: Size::ZERO,
        child_viewport: Rect::ZERO,
        onscroll: None,
        held: BarHeldState::None,
        virtual_node: None,
        hide_bar: false,
        opacity: 0.7,
        scroll_bar_color: Color::BLACK,
    }
}

impl<V: View> Scroll<V> {
    pub fn scroll_bar_color(self, cx: AppContext, color: impl Fn() -> Color + 'static) -> Self {
        let id = self.id;
        create_effect(cx.scope, move |_| {
            let color = color();
            AppContext::update_state(id, ScrollState::ScrollBarColor(color), false);
        });

        self
    }

    pub fn onscroll(mut self, onscroll: impl Fn(Rect) + 'static) -> Self {
        self.onscroll = Some(Box::new(onscroll));
        self
    }

    pub fn on_ensure_visible(self, cx: AppContext, to: impl Fn() -> Rect + 'static) -> Self {
        let id = self.id;
        create_effect(cx.scope, move |_| {
            let rect = to();
            AppContext::update_state(id, ScrollState::EnsureVisble(rect), true);
        });

        self
    }

    pub fn on_scroll_delta(self, cx: AppContext, delta: impl Fn() -> Vec2 + 'static) -> Self {
        let id = self.id;
        create_effect(cx.scope, move |_| {
            let delta = delta();
            AppContext::update_state(id, ScrollState::ScrollDelta(delta), false);
        });

        self
    }

    pub fn on_scroll_to(
        self,
        cx: AppContext,
        origin: impl Fn() -> Option<Point> + 'static,
    ) -> Self {
        let id = self.id;
        create_effect(cx.scope, move |_| {
            if let Some(origin) = origin() {
                AppContext::update_state(id, ScrollState::ScrollTo(origin), true);
            }
        });

        self
    }

    pub fn hide_bar(self, cx: AppContext, value: impl Fn() -> bool + 'static) -> Self {
        let id = self.id;
        create_effect(cx.scope, move |_| {
            AppContext::update_state(id, ScrollState::HiddenBar(value()), false);
        });
        self
    }

    fn scroll_delta(&mut self, app_state: &mut AppState, delta: Vec2) {
        let new_origin = self.child_viewport.origin() + delta;
        self.clamp_child_viewport(app_state, self.child_viewport.with_origin(new_origin));
    }

    fn scroll_to(&mut self, app_state: &mut AppState, origin: Point) {
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

    fn update_size(&mut self, app_state: &mut AppState) {
        let child_size = self.child_size;
        let new_child_size = self.child_size(app_state).unwrap_or_default();
        self.child_size = new_child_size;
        self.size = self.size(app_state).unwrap_or_default();
        if child_size != new_child_size {
            app_state.request_layout(self.id);
        }
    }

    fn clamp_child_viewport(
        &mut self,
        app_state: &mut AppState,
        child_viewport: Rect,
    ) -> Option<()> {
        let size = self.size;
        let child_size = self.child_size;

        let mut child_viewport = child_viewport;
        if size.width >= child_size.width {
            child_viewport.x0 = 0.0;
        } else if child_viewport.x0 > child_size.width - size.width {
            child_viewport.x0 = child_size.width - size.width;
        } else if child_viewport.x0 < 0.0 {
            child_viewport.x0 = 0.0;
        }

        if size.height >= child_size.height {
            child_viewport.y0 = 0.0;
        } else if child_viewport.y0 > child_size.height - size.height {
            child_viewport.y0 = child_size.height - size.height;
        } else if child_viewport.y0 < 0.0 {
            child_viewport.y0 = 0.0;
        }
        child_viewport = child_viewport.with_size(size);

        if child_viewport != self.child_viewport {
            app_state.set_viewport(self.child.id(), child_viewport);
            app_state.request_layout(self.id);
            self.child_viewport = child_viewport;
            if let Some(onscroll) = &self.onscroll {
                onscroll(child_viewport);
            }
        }
        Some(())
    }

    fn child_size(&self, app_state: &mut AppState) -> Option<Size> {
        app_state
            .view_states
            .get(&self.id)
            .map(|view| &view.children_nodes)
            .and_then(|nodes| nodes.get(1))
            .and_then(|node| app_state.taffy.layout(*node).ok())
            .map(|layout| Size::new(layout.size.width as f64, layout.size.height as f64))
    }

    fn size(&self, app_state: &mut AppState) -> Option<Size> {
        app_state
            .get_layout(self.id)
            .map(|layout| Size::new(layout.size.width as f64, layout.size.height as f64))
    }

    fn draw_bars(&self, cx: &mut PaintCx) {
        let edge_width = 0.0;
        let scroll_offset = self.child_viewport.origin().to_vec2();

        let color = self.scroll_bar_color.with_alpha_factor(self.opacity as f32);
        if let Some(bounds) = self.calc_vertical_bar_bounds(cx.app_state) {
            let rect = (bounds - scroll_offset).inset(-edge_width / 2.0);
            cx.fill(&rect, color);
            if edge_width > 0.0 {
                cx.stroke(&rect, color, edge_width);
            }
        }

        // Horizontal bar
        if let Some(bounds) = self.calc_horizontal_bar_bounds(cx.app_state) {
            let rect = (bounds - scroll_offset).inset(-edge_width / 2.0);
            cx.fill(&rect, color);
            if edge_width > 0.0 {
                cx.stroke(&rect, color, edge_width);
            }
        }
    }

    fn calc_vertical_bar_bounds(&self, _app_state: &mut AppState) -> Option<Rect> {
        let viewport_size = self.child_viewport.size();
        let content_size = self.child_size;
        let scroll_offset = self.child_viewport.origin().to_vec2();

        if viewport_size.height >= content_size.height {
            return None;
        }

        let bar_width = 10.0;
        let bar_pad = 0.0;

        let percent_visible = viewport_size.height / content_size.height;
        let percent_scrolled = scroll_offset.y / (content_size.height - viewport_size.height);

        let length = (percent_visible * viewport_size.height).ceil();
        let length = length.max(SCROLLBAR_MIN_SIZE);

        let top_y_offset = ((viewport_size.height - length) * percent_scrolled).ceil();
        let bottom_y_offset = top_y_offset + length;

        let x0 = scroll_offset.x + viewport_size.width - bar_width - bar_pad;
        let y0 = scroll_offset.y + top_y_offset;

        let x1 = scroll_offset.x + viewport_size.width - bar_pad;
        let y1 = scroll_offset.y + bottom_y_offset;

        Some(Rect::new(x0, y0, x1, y1))
    }

    fn calc_horizontal_bar_bounds(&self, _app_state: &mut AppState) -> Option<Rect> {
        let viewport_size = self.child_viewport.size();
        let content_size = self.child_size;
        let scroll_offset = self.child_viewport.origin().to_vec2();

        if viewport_size.width >= content_size.width {
            return None;
        }

        let bar_width = if viewport_size.height < 40.0 {
            5.0
        } else {
            10.0
        };
        let bar_pad = 0.0;

        let percent_visible = viewport_size.width / content_size.width;
        let percent_scrolled = scroll_offset.x / (content_size.width - viewport_size.width);

        let length = (percent_visible * viewport_size.width).ceil();
        let length = length.max(SCROLLBAR_MIN_SIZE);

        let horizontal_padding = if viewport_size.height >= content_size.height {
            0.0
        } else {
            bar_pad + bar_pad + bar_width
        };

        let left_x_offset =
            ((viewport_size.width - length - horizontal_padding) * percent_scrolled).ceil();
        let right_x_offset = left_x_offset + length;

        let x0 = scroll_offset.x + left_x_offset;
        let y0 = scroll_offset.y + viewport_size.height - bar_width - bar_pad;

        let x1 = scroll_offset.x + right_x_offset;
        let y1 = scroll_offset.y + viewport_size.height - bar_pad;

        Some(Rect::new(x0, y0, x1, y1))
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
}

impl<V: View> View for Scroll<V> {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&mut self, id: Id) -> Option<&mut dyn View> {
        if self.child.id() == id {
            Some(&mut self.child)
        } else {
            None
        }
    }

    fn update(
        &mut self,
        cx: &mut crate::context::UpdateCx,
        state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        if let Ok(state) = state.downcast::<ScrollState>() {
            match *state {
                ScrollState::EnsureVisble(rect) => {
                    self.pan_to_visible(cx.app_state, rect);
                }
                ScrollState::ScrollDelta(delta) => {
                    self.scroll_delta(cx.app_state, delta);
                }
                ScrollState::ScrollTo(origin) => {
                    self.scroll_to(cx.app_state, origin);
                }
                ScrollState::ScrollBarColor(color) => {
                    self.scroll_bar_color = color;
                }
                ScrollState::HiddenBar(value) => {
                    self.hide_bar = value;
                }
            }
            cx.request_layout(self.id());
            ChangeFlags::LAYOUT
        } else {
            ChangeFlags::empty()
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| {
            let child_id = self.child.id();
            let mut child_view = cx.app_state.view_state(child_id);
            child_view.style.position = StyleValue::Val(Position::Absolute);
            let child_node = self.child.layout_main(cx);

            let virtual_style = Style::default()
                .width(Dimension::Points(self.child_size.width as f32))
                .height(Dimension::Points(self.child_size.height as f32))
                .min_width(Dimension::Points(0.0))
                .min_height(Dimension::Points(0.0))
                .compute(&ComputedStyle::default())
                .to_taffy_style();
            if self.virtual_node.is_none() {
                self.virtual_node =
                    Some(cx.app_state.taffy.new_leaf(virtual_style.clone()).unwrap());
            }
            let virtual_node = self.virtual_node.unwrap();
            let _ = cx.app_state.taffy.set_style(virtual_node, virtual_style);

            vec![virtual_node, child_node]
        })
    }

    fn compute_layout(&mut self, cx: &mut LayoutCx) {
        self.update_size(cx.app_state);
        self.clamp_child_viewport(cx.app_state, self.child_viewport);
        self.child.compute_layout_main(cx);
    }

    fn event(
        &mut self,
        cx: &mut crate::context::EventCx,
        id_path: Option<&[Id]>,
        event: crate::event::Event,
    ) -> bool {
        let viewport_size = self.child_viewport.size();
        let scroll_offset = self.child_viewport.origin().to_vec2();
        let content_size = self.child_size;

        match &event {
            Event::MouseDown(event) => {
                if !self.hide_bar {
                    let pos = event.pos + scroll_offset;

                    if self.point_hits_vertical_bar(cx.app_state, pos) {
                        self.held = BarHeldState::Vertical(
                            // The bounds must be non-empty, because the point hits the scrollbar.
                            event.pos.y,
                            scroll_offset,
                        );
                        cx.update_active(self.id);
                        return true;
                    } else if self.point_hits_horizontal_bar(cx.app_state, pos) {
                        self.held = BarHeldState::Horizontal(
                            // The bounds must be non-empty, because the point hits the scrollbar.
                            event.pos.x,
                            scroll_offset,
                        );
                        cx.update_active(self.id);
                        return true;
                    } else {
                        self.held = BarHeldState::None;
                    }
                }
            }
            Event::MouseUp(_event) => self.held = BarHeldState::None,
            Event::MouseMove(event) => {
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
                } else {
                    let pos = event.pos + scroll_offset;
                    if self.point_hits_vertical_bar(cx.app_state, pos)
                        || self.point_hits_horizontal_bar(cx.app_state, pos)
                    {
                        return true;
                    }
                }
            }
            _ => {}
        }

        if id_path.is_some() {
            return true;
        }

        if self.child.event_main(
            cx,
            id_path, // event.clone().offset((-scroll_offset.x, -scroll_offset.y)),
            event.clone(),
        ) {
            return true;
        }

        if let Event::MouseWheel(mouse_event) = event {
            self.clamp_child_viewport(cx.app_state, self.child_viewport + mouse_event.wheel_delta);
        }

        true
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.save();
        let size = cx
            .get_layout(self.id)
            .map(|layout| Size::new(layout.size.width as f64, layout.size.height as f64))
            .unwrap_or_default();
        cx.clip(&size.to_rect());
        cx.offset((-self.child_viewport.x0, -self.child_viewport.y0));
        self.child.paint_main(cx);
        cx.restore();

        if !self.hide_bar {
            self.draw_bars(cx);
        }
    }
}
