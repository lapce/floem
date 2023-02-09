use std::any::Any;

use bitflags::bitflags;
use glazier::kurbo::{Line, Point, RoundedRect, Shape, Size};
use taffy::prelude::Node;
use vello::peniko::Color;

use crate::{
    context::{EventCx, LayoutCx, PaintCx, UpdateCx},
    event::Event,
    id::Id,
    style::Style,
};

bitflags! {
    #[derive(Default)]
    #[must_use]
    pub struct ChangeFlags: u8 {
        const UPDATE = 1;
        const LAYOUT = 2;
        const ACCESSIBILITY = 4;
        const PAINT = 8;
    }
}

pub trait View {
    fn id(&self) -> Id;

    fn child(&mut self, id: Id) -> Option<&mut dyn View>;

    fn update_main(
        &mut self,
        cx: &mut UpdateCx,
        id_path: &[Id],
        state: Box<dyn Any>,
    ) -> ChangeFlags {
        let id = id_path[0];
        let id_path = &id_path[1..];
        if id == self.id() {
            if id_path.is_empty() {
                return self.update(cx, state);
            } else if let Some(child) = self.child(id_path[0]) {
                return child.update_main(cx, id_path, state);
            }
        }
        ChangeFlags::empty()
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) -> ChangeFlags;

    fn layout(&mut self, cx: &mut LayoutCx) -> Node;

    fn compute_layout(&mut self, cx: &mut LayoutCx);

    fn event_main(&mut self, cx: &mut EventCx, id_path: Option<&[Id]>, event: Event) -> bool {
        if let Some(id_path) = id_path {
            let id = id_path[0];
            let id_path = &id_path[1..];
            if id == self.id() && !id_path.is_empty() {
                if let Some(child) = self.child(id_path[0]) {
                    return child.event_main(cx, Some(id_path), event);
                }
            }
        }
        if let Some(listener) = event.listener() {
            if let Some(listeners) = cx.get_event_listener(self.id()) {
                if let Some(action) = listeners.get(&listener) {
                    if (*action)(&event) {
                        return true;
                    }
                }
            }
        }
        self.event(cx, None, event)
    }

    fn event(&mut self, cx: &mut EventCx, id_path: Option<&[Id]>, event: Event) -> bool;

    fn paint_main(&mut self, cx: &mut PaintCx) {
        cx.save();
        let id = self.id();
        let size = cx.transform(id);
        self.paint(cx);

        if let Some(style) = cx.get_style(id).cloned() {
            paint_border(cx, &style, size);
        }

        cx.restore();
    }

    fn paint(&mut self, cx: &mut PaintCx);
}

fn paint_border(cx: &mut PaintCx, style: &Style, size: Size) {
    let left = if style.border_left > 0.0 {
        style.border_left
    } else {
        style.border
    };
    let top = if style.border_top > 0.0 {
        style.border_top
    } else {
        style.border
    };
    let right = if style.border_right > 0.0 {
        style.border_right
    } else {
        style.border
    };
    let bottom = if style.border_bottom > 0.0 {
        style.border_bottom
    } else {
        style.border
    };

    let border_color = Color::rgb8(0xa1, 0xa1, 0xa1);
    if left == top && top == right && right == bottom && bottom == left && left > 0.0 {
        let half = left as f64 / 2.0;
        let rect = size.to_rect().inflate(-half, -half);
        let radius = style.border_radius;
        if radius > 0.0 {
            cx.stroke(
                &rect.to_rounded_rect(radius as f64),
                border_color,
                left as f64,
            );
        } else {
            cx.stroke(&rect, border_color, left as f64);
        }
    } else if left > 0.0 {
        let half = left as f64 / 2.0;
        cx.stroke(
            &Line::new(Point::new(half, 0.0), Point::new(half, size.height)),
            border_color,
            left as f64,
        );
    } else if right > 0.0 {
        let half = right as f64 / 2.0;
        cx.stroke(
            &Line::new(
                Point::new(size.width - half, 0.0),
                Point::new(size.width - half, size.height),
            ),
            border_color,
            right as f64,
        );
    } else if top > 0.0 {
        let half = top as f64 / 2.0;
        cx.stroke(
            &Line::new(Point::new(0.0, half), Point::new(size.width, half)),
            border_color,
            top as f64,
        );
    } else if bottom > 0.0 {
        let half = bottom as f64 / 2.0;
        cx.stroke(
            &Line::new(
                Point::new(0.0, size.height - half),
                Point::new(size.width, size.height - half),
            ),
            border_color,
            bottom as f64,
        );
    }
}
