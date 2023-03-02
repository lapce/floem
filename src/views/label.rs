use std::any::Any;

use glazier::kurbo::Point;
use leptos_reactive::create_effect;
use parley::{layout::Cursor, style::StyleProperty};
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
    available_text: Option<String>,
    available_width: Option<f32>,
    available_text_layout: Option<parley::Layout<ParleyBrush>>,
    color: Option<Color>,
    font_size: Option<f32>,
}

pub fn label(cx: AppContext, label: impl Fn() -> String + 'static) -> Label {
    let id = cx.new_id();
    create_effect(cx.scope, move |_| {
        let new_label = label();
        AppContext::update_state(id, new_label);
    });
    Label {
        id,
        label: "".to_string(),
        text_layout: None,
        text_node: None,
        available_text: None,
        available_width: None,
        available_text_layout: None,
        color: None,
        font_size: None,
    }
}

impl Label {
    fn set_text_layout(&mut self) {
        let mut text_layout_builder = parley::LayoutContext::builder(self.label.as_str(), 1.0);
        text_layout_builder.push_default(&parley::style::StyleProperty::Brush(ParleyBrush(
            Brush::Solid(self.color.unwrap_or_else(|| Color::rgb8(0xf0, 0xf0, 0xea))),
        )));
        if let Some(font_size) = self.font_size {
            text_layout_builder.push_default(&StyleProperty::FontSize(font_size));
        }
        let mut text_layout = text_layout_builder.build();
        text_layout.break_all_lines(None, parley::layout::Alignment::Start);
        self.text_layout = Some(text_layout);

        if let Some(new_text) = self.available_text.as_ref() {
            let mut text_layout_builder = parley::LayoutContext::builder(new_text.as_str(), 1.0);
            text_layout_builder.push_default(&parley::style::StyleProperty::Brush(ParleyBrush(
                Brush::Solid(self.color.unwrap_or_else(|| Color::rgb8(0xf0, 0xf0, 0xea))),
            )));
            if let Some(font_size) = self.font_size {
                text_layout_builder.push_default(&StyleProperty::FontSize(font_size));
            }
            let mut new_text = text_layout_builder.build();
            new_text.break_all_lines(None, parley::layout::Alignment::Start);
            self.available_text_layout = Some(new_text);
        }
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

    fn event(&mut self, cx: &mut EventCx, id_path: Option<&[Id]>, event: Event) -> bool {
        false
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        // if self.label.is_empty() {
        //     return cx.layout_node(self.id, false, |cx| vec![]);
        // }

        cx.layout_node(self.id, true, |cx| {
            let (width, height) = if self.label.is_empty() {
                (0.0, cx.current_font_size().unwrap_or(12.0))
            } else {
                if self.font_size != cx.current_font_size() {
                    self.font_size = cx.current_font_size();
                    self.set_text_layout();
                }
                if self.text_layout.is_none() {
                    self.set_text_layout();
                }
                let text_layout = self.text_layout.as_ref().unwrap();
                let width = text_layout.width().ceil();
                let height = text_layout.height().ceil();
                (width, height)
            };

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

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) {
        if self.label.is_empty() {
            return;
        }

        let text_node = self.text_node.unwrap();
        let layout = cx.app_state.taffy.layout(text_node).unwrap();
        let text_layout = self.text_layout.as_ref().unwrap();
        let width = text_layout.width();
        if width > layout.size.width {
            if self.available_width != Some(layout.size.width) {
                let mut text_layout_builder = parley::LayoutContext::builder("...", 1.0);
                text_layout_builder.push_default(&parley::style::StyleProperty::Brush(
                    ParleyBrush(Brush::Solid(Color::rgb8(0xf0, 0xf0, 0xea))),
                ));
                if let Some(font_size) = self.font_size {
                    text_layout_builder.push_default(&StyleProperty::FontSize(font_size));
                }
                let mut dots_text = text_layout_builder.build();
                dots_text.break_all_lines(None, parley::layout::Alignment::Start);
                let dots_width = dots_text.width();
                let width_left = layout.size.width - dots_width;
                let cursor = Cursor::from_point(text_layout, width_left, 0.0);
                let range = cursor.text_range();
                let index = if cursor.is_trailing() {
                    range.end
                } else {
                    range.start
                };

                let new_text = if index > 0 {
                    format!("{}...", &self.label[..index])
                } else {
                    "".to_string()
                };
                self.available_text = Some(new_text);
                self.available_width = Some(layout.size.width);
                self.set_text_layout();
            }
        } else {
            self.available_text = None;
            self.available_width = None;
            self.available_text_layout = None;
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if self.label.is_empty() {
            return;
        }

        if self.color != cx.color || self.font_size != cx.font_size {
            self.color = cx.color;
            self.font_size = cx.font_size;
            self.set_text_layout();
        }
        let text_node = self.text_node.unwrap();
        let location = cx.app_state.taffy.layout(text_node).unwrap().location;
        let point = Point::new(location.x as f64, location.y as f64);
        if let Some(text_layout) = self.available_text_layout.as_ref() {
            cx.render_text(text_layout, point);
        } else {
            cx.render_text(self.text_layout.as_ref().unwrap(), point);
        }
    }
}
