use std::any::Any;

use glazier::kurbo::Point;
use leptos_reactive::create_effect;
use taffy::style::Style;
use vello::peniko::{Brush, Color};

use crate::{
    app::{AppContext, UpdateMessage},
    event::Event,
    id::Id,
    text::ParleyBrush,
    view::{ChangeFlags, View},
};

pub struct Label {
    id: Id,
    label: String,
    text_layout: Option<parley::Layout<ParleyBrush>>,
}

pub fn label(cx: AppContext, label: impl Fn() -> String + 'static + Copy) -> Label {
    let id = cx.new_id();
    create_effect(cx.scope, move |_| {
        let new_label = label();
        AppContext::add_update(UpdateMessage::new(id, new_label));
    });
    Label {
        id,
        label: label(),
        text_layout: None,
    }
}

impl View for Label {
    type State = String;

    fn id(&self) -> Id {
        self.id
    }

    fn update(&mut self, id: &[Id], state: Box<dyn Any>) -> ChangeFlags {
        if let Ok(state) = state.downcast() {
            self.label = *state;
            self.text_layout = None;
            ChangeFlags::LAYOUT
        } else {
            ChangeFlags::empty()
        }
    }

    fn event(&mut self, event: Event) {}

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) {
        let layout = cx.layout_state.layouts.entry(self.id()).or_default();
        layout.layout = *cx.layout_state.taffy.layout(layout.node).unwrap();
    }

    fn build_layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        let mut lcx = parley::LayoutContext::new();
        let mut text_layout_builder = lcx.ranged_builder(cx.font_cx, &self.label, 1.0);
        text_layout_builder.push_default(&parley::style::StyleProperty::Brush(ParleyBrush(
            Brush::Solid(Color::rgb8(0xf0, 0xf0, 0xea)),
        )));
        let mut text_layout = text_layout_builder.build();
        text_layout.break_all_lines(None, parley::layout::Alignment::Start);
        let width = text_layout.width();
        let height = text_layout.height();
        self.text_layout = Some(text_layout);

        let node = cx
            .layout_state
            .taffy
            .new_leaf(Style {
                size: taffy::prelude::Size {
                    width: taffy::style::Dimension::Points(width),
                    height: taffy::style::Dimension::Points(height),
                },
                ..Default::default()
            })
            .unwrap();
        let layout = cx.layout_state.layouts.entry(self.id()).or_default();
        layout.node = node;
        node
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.save();
        let size = cx.transform(self.id());
        cx.render_text(self.text_layout.as_ref().unwrap(), Point::ZERO);
        cx.restore();
    }
}
