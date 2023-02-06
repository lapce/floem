use std::any::Any;

use glazier::kurbo::Point;
use leptos_reactive::create_effect;
use taffy::{prelude::Node, style::Dimension};
use vello::peniko::{Brush, Color};

use crate::{
    app::{AppContext, UpdateMessage},
    context::{EventCx, UpdateCx},
    event::Event,
    id::Id,
    style::Style,
    text::ParleyBrush,
    view::{ChangeFlags, View},
};

pub struct Label {
    id: Id,
    label: String,
    text_layout: Option<parley::Layout<ParleyBrush>>,
    text_node: Option<Node>,
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
        text_node: None,
    }
}

impl View for Label {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&mut self, id: Id) -> Option<&mut dyn View> {
        None
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) -> ChangeFlags {
        if let Ok(state) = state.downcast() {
            self.label = *state;
            self.text_layout = None;
            cx.request_layout(self.id());
            ChangeFlags::LAYOUT
        } else {
            ChangeFlags::empty()
        }
    }

    fn event(&mut self, cx: &mut EventCx, id_path: Option<&[Id]>, event: Event) {}

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
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

        let text_node = cx
            .layout_state
            .taffy
            .new_leaf(
                (&Style {
                    width: Dimension::Points(width),
                    height: Dimension::Points(height),
                    ..Default::default()
                })
                    .into(),
            )
            .unwrap();
        self.text_node = Some(text_node);

        let style: taffy::style::Style =
            cx.get_style(self.id).map(|s| s.into()).unwrap_or_default();
        let node = cx
            .layout_state
            .taffy
            .new_with_children(style, &[text_node])
            .unwrap();
        let layout = cx.layout_state.view_states.entry(self.id()).or_default();
        layout.node = Some(node);
        node
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let location = cx
            .layout_state
            .taffy
            .layout(self.text_node.unwrap())
            .unwrap()
            .location;
        let point = Point::new(location.x as f64, location.y as f64);
        cx.render_text(self.text_layout.as_ref().unwrap(), point);
    }
}
