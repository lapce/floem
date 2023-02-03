use std::any::Any;

use bitflags::bitflags;
use glazier::kurbo::{RoundedRect, Shape};
use taffy::prelude::Node;
use vello::peniko::Color;

use crate::{
    context::{EventCx, LayoutCx, PaintCx, UpdateCx},
    event::Event,
    id::Id,
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

    fn update(&mut self, cx: &mut UpdateCx, id_path: &[Id], state: Box<dyn Any>) -> ChangeFlags;

    fn layout(&mut self, cx: &mut LayoutCx) -> Node;

    fn event(&mut self, cx: &mut EventCx, event: Event);

    fn paint_main(&mut self, cx: &mut PaintCx) {
        cx.save();
        let id = self.id();
        let size = cx.transform(id);
        self.paint(cx);

        if let Some(style) = cx.get_style(id) {
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
            if left == top && top == right && right == bottom && bottom == left && left > 0.0 {
                let border_color = Color::rgb8(0xa1, 0xa1, 0xa1);
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
            }
        }

        cx.restore();
    }

    fn paint(&mut self, cx: &mut PaintCx);
}
