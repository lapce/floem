use std::any::Any;

use floem_renderer::{cosmic_text::TextLayout, Renderer};
use glazier::kurbo::Point;
use leptos_reactive::create_effect;
use taffy::{prelude::Node, style::Dimension};

use crate::{
    app::AppContext,
    context::{EventCx, UpdateCx},
    event::Event,
    id::Id,
    style::{ComputedStyle, Style},
    view::{ChangeFlags, View},
};

pub struct RichText {
    id: Id,
    text_layout: TextLayout,
    text_node: Option<Node>,
}

pub fn rich_text(cx: AppContext, text_layout: impl Fn() -> TextLayout + 'static) -> RichText {
    let id = cx.new_id();
    let text = text_layout();
    create_effect(cx.scope, move |_| {
        let new_text_layout = text_layout();
        AppContext::update_state(id, new_text_layout, false);
    });
    RichText {
        id,
        text_layout: text,
        text_node: None,
    }
}

impl View for RichText {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&mut self, _id: Id) -> Option<&mut dyn View> {
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

    fn event(&mut self, _cx: &mut EventCx, _id_path: Option<&[Id]>, _event: Event) -> bool {
        false
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| {
            let size = self.text_layout.size();
            let width = size.width as f32;
            let height = size.height as f32;

            if self.text_node.is_none() {
                self.text_node = Some(
                    cx.app_state
                        .taffy
                        .new_leaf(taffy::style::Style::DEFAULT)
                        .unwrap(),
                );
            }
            let text_node = self.text_node.unwrap();

            let style = Style::BASE
                .width(Dimension::Points(width))
                .height(Dimension::Points(height))
                .compute(&ComputedStyle::default())
                .to_taffy_style();
            let _ = cx.app_state.taffy.set_style(text_node, style);
            vec![text_node]
        })
    }

    fn compute_layout(&mut self, _cx: &mut crate::context::LayoutCx) {}

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let text_node = self.text_node.unwrap();
        let location = cx.app_state.taffy.layout(text_node).unwrap().location;
        let point = Point::new(location.x as f64, location.y as f64);
        cx.draw_text(&self.text_layout, point);
    }
}
