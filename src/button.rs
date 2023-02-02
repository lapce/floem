use std::any::Any;

use glazier::kurbo::{Rect, Size};
use leptos_reactive::create_effect;
use taffy::style::Style;
use vello::peniko::Color;

use crate::{
    app::{AppContext, UpdateMessage},
    context::{EventCx, UpdateCx},
    event::Event,
    id::Id,
    view::{ChangeFlags, View},
};

pub struct Button {
    id: Id,
    label: String,
    onclick: Box<dyn Fn()>,
}

pub fn button(
    cx: AppContext,
    label: impl Fn() -> String + 'static + Copy,
    onclick: impl Fn() + 'static,
) -> Button {
    let id = cx.new_id();
    create_effect(cx.scope, move |_| {
        let new_label = label();
        AppContext::add_update(UpdateMessage::new(id, new_label));
    });
    Button {
        id,
        label: label(),
        onclick: Box::new(onclick),
    }
}

impl View for Button {
    fn id(&self) -> Id {
        self.id
    }

    fn update(&mut self, cx: &mut UpdateCx, id: &[Id], state: Box<dyn Any>) -> ChangeFlags {
        if let Ok(state) = state.downcast() {
            self.label = *state;
            cx.request_layout(self.id());
            ChangeFlags::LAYOUT
        } else {
            ChangeFlags::empty()
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: Event) {
        if let Event::MouseDown(mouse_event) = event {
            (self.onclick)();
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, false, |_| Vec::new())
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.save();
        let size = cx.transform(self.id());
        let border_color = Color::rgb8(0xa1, 0xa1, 0xa1);
        cx.stroke(&Rect::ZERO.with_size(size), border_color, 3.0);
        cx.restore();
    }
}
