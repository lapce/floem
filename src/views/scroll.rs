use glazier::kurbo::{Point, Rect, Size, Vec2};
use taffy::{prelude::Node, style::Position};
use vello::peniko::Color;

use crate::{
    app::AppContext,
    context::{AppState, LayoutCx, PaintCx},
    event::Event,
    id::Id,
    view::{ChangeFlags, View},
};

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
    child_viewport: Rect,
    onscroll: Option<Box<dyn Fn(Rect)>>,
    virtual_child_node: Option<Node>,
    held: BarHeldState,
}

pub fn scroll<V: View>(cx: AppContext, child: impl Fn(AppContext) -> V) -> Scroll<V> {
    let id = cx.new_id();

    let mut child_cx = cx;
    child_cx.id = id;
    let child = child(child_cx);

    Scroll {
        id,
        child,
        child_viewport: Rect::ZERO,
        onscroll: None,
        virtual_child_node: None,
        held: BarHeldState::None,
    }
}

impl<V: View> Scroll<V> {
    pub fn onscroll(mut self, onscroll: impl Fn(Rect) + 'static) -> Self {
        self.onscroll = Some(Box::new(onscroll));
        self
    }

    fn clamp_child_viewport(
        &mut self,
        app_state: &mut AppState,
        child_viewport: Rect,
    ) -> Option<()> {
        let size = self.size(app_state)?;
        let child_size = self.child_size(app_state)?;

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
            .and_then(|nodes| nodes.get(0))
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

        let color = Color::rgb8(0xa1, 0xa1, 0xa1);
        if let Some(bounds) = self.calc_vertical_bar_bounds(cx.layout_state) {
            let rect = (bounds - scroll_offset).inset(-edge_width / 2.0);
            cx.fill(&rect, color);
            if edge_width > 0.0 {
                cx.stroke(&rect, color, edge_width);
            }
        }

        // Horizontal bar
        if let Some(bounds) = self.calc_horizontal_bar_bounds(cx.layout_state) {
            let rect = (bounds - scroll_offset).inset(-edge_width / 2.0);
            cx.fill(&rect, color);
            if edge_width > 0.0 {
                cx.stroke(&rect, color, edge_width);
            }
        }
    }

    fn calc_vertical_bar_bounds(&self, app_state: &mut AppState) -> Option<Rect> {
        let viewport_size = self.child_viewport.size();
        let content_size = self.child_size(app_state)?;
        let scroll_offset = self.child_viewport.origin().to_vec2();

        if viewport_size.height >= content_size.height {
            return None;
        }

        let bar_width = 20.0;
        let bar_pad = 2.0;

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

    fn calc_horizontal_bar_bounds(&self, app_state: &mut AppState) -> Option<Rect> {
        let viewport_size = self.child_viewport.size();
        let content_size = self.child_size(app_state)?;
        let scroll_offset = self.child_viewport.origin().to_vec2();

        if viewport_size.width >= content_size.width {
            return None;
        }

        let bar_width = if viewport_size.height < 40.0 {
            5.0
        } else {
            20.0
        };
        let bar_pad = 2.0;

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
        ChangeFlags::empty()
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| {
            if self.virtual_child_node.is_none() {
                self.virtual_child_node = Some(
                    cx.app_state
                        .taffy
                        .new_leaf(taffy::prelude::Style {
                            position: Position::Absolute,
                            ..Default::default()
                        })
                        .unwrap(),
                );
            }
            let virtual_child_node = self.virtual_child_node.unwrap();
            let child_node = self.child.layout(cx);

            cx.app_state
                .taffy
                .set_children(virtual_child_node, &[child_node]);

            vec![virtual_child_node]
        })
    }

    fn compute_layout(&mut self, cx: &mut LayoutCx) {
        self.clamp_child_viewport(cx.app_state, self.child_viewport);
        self.child.compute_layout(cx);
    }

    fn event(
        &mut self,
        cx: &mut crate::context::EventCx,
        id_path: Option<&[Id]>,
        event: crate::event::Event,
    ) -> bool {
        let viewport_size = self.child_viewport.size();
        let scroll_offset = self.child_viewport.origin().to_vec2();
        let content_size = self.child_size(cx.app_state).unwrap_or_default();

        match &event {
            Event::MouseDown(event) => {
                let pos = event.pos + scroll_offset;

                if self.point_hits_vertical_bar(cx.app_state, pos) {
                    self.held = BarHeldState::Vertical(
                        // The bounds must be non-empty, because the point hits the scrollbar.
                        event.pos.y,
                        scroll_offset,
                    );
                    cx.update_active(self.id);
                } else if self.point_hits_horizontal_bar(cx.app_state, pos) {
                    self.held = BarHeldState::Horizontal(
                        // The bounds must be non-empty, because the point hits the scrollbar.
                        event.pos.x,
                        scroll_offset,
                    );
                    cx.update_active(self.id);
                } else {
                    self.held = BarHeldState::None;
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
                }
            }
            _ => {}
        }

        if id_path.is_some() {
            return true;
        }

        if self.child.event_main(
            cx,
            id_path,
            event.clone().offset((-scroll_offset.x, -scroll_offset.y)),
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
        cx.offset((-self.child_viewport.x0, -self.child_viewport.y0));
        self.child.paint_main(cx);
        cx.restore();

        self.draw_bars(cx);
    }
}
