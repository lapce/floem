use std::any::Any;

use glazier::kurbo::Point;
use leptos_reactive::create_effect;
use parley::{layout::Cursor, Layout};
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

pub struct TextLayout {
    id: Id,
    text_layout: parley::Layout<ParleyBrush>,
    text_node: Option<Node>,
}

pub fn text_layout(
    cx: AppContext,
    text_layout: impl Fn() -> Layout<ParleyBrush> + 'static,
) -> TextLayout {
    let id = cx.new_id();
    let text = text_layout();
    create_effect(cx.scope, move |_| {
        let new_text_layout = text_layout();
        AppContext::update_state(id, new_text_layout);
    });
    TextLayout {
        id,
        text_layout: text,
        text_node: None,
    }
}

impl View for TextLayout {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&mut self, id: Id) -> Option<&mut dyn View> {
        None
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) -> ChangeFlags {
        if let Ok(state) = state.downcast() {
            self.text_layout = *state;
            cx.request_layout(self.id());
            ChangeFlags::LAYOUT
        } else {
            ChangeFlags::empty()
        }
    }

    fn event(&mut self, cx: &mut EventCx, id_path: Option<&[Id]>, event: Event) -> bool {
        false
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| {
            let width = self.text_layout.width().ceil();
            let height = self.text_layout.height().ceil();

            if self.text_node.is_none() {
                self.text_node = Some(
                    cx.app_state
                        .taffy
                        .new_leaf(taffy::style::Style::DEFAULT)
                        .unwrap(),
                );
            }
            let text_node = self.text_node.unwrap();

            cx.app_state.taffy.set_style(
                text_node,
                (&Style {
                    width: Dimension::Points(width),
                    height: Dimension::Points(height),
                    ..Default::default()
                })
                    .into(),
            );
            vec![text_node]
        })
    }

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) {}

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let text_node = self.text_node.unwrap();
        let location = cx.app_state.taffy.layout(text_node).unwrap().location;
        let point = Point::new(location.x as f64, location.y as f64);
        cx.render_text(&self.text_layout, point);
    }
}
