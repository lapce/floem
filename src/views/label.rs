use std::any::Any;

use crate::{
    cosmic_text::{Attrs, AttrsList, FamilyOwned, TextLayout},
    style::{ComputedStyle, TextOverflow},
};
use floem_renderer::{
    cosmic_text::{LineHeightValue, Style as FontStyle, Weight},
    Renderer,
};
use glazier::kurbo::{Point, Rect};
use leptos_reactive::create_effect;
use taffy::{prelude::Node, style::Dimension};
use vello::peniko::Color;

use crate::{
    app_handle::ViewContext,
    context::{EventCx, UpdateCx},
    event::Event,
    id::Id,
    style::Style,
    view::{ChangeFlags, View},
};

pub struct Label {
    id: Id,
    label: String,
    text_layout: Option<TextLayout>,
    text_node: Option<Node>,
    available_text: Option<String>,
    available_width: Option<f32>,
    available_text_layout: Option<TextLayout>,
    color: Option<Color>,
    font_size: Option<f32>,
    font_family: Option<String>,
    font_weight: Option<Weight>,
    font_style: Option<FontStyle>,
    line_height: Option<LineHeightValue>,
    text_overflow: TextOverflow,
}

pub fn label(label: impl Fn() -> String + 'static) -> Label {
    let cx = ViewContext::get_current();
    let id = cx.new_id();
    create_effect(cx.scope, move |_| {
        let new_label = label();
        id.update_state(new_label, false);
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
        font_family: None,
        font_weight: None,
        font_style: None,
        line_height: None,
        text_overflow: TextOverflow::Wrap,
    }
}

impl Label {
    fn get_attrs_list(&self) -> AttrsList {
        let mut attrs = Attrs::new().color(self.color.unwrap_or(Color::BLACK));
        if let Some(font_size) = self.font_size {
            attrs = attrs.font_size(font_size);
        }
        if let Some(font_style) = self.font_style {
            attrs = attrs.style(font_style);
        }
        let font_family = self.font_family.as_ref().map(|font_family| {
            let family: Vec<FamilyOwned> = FamilyOwned::parse_list(font_family).collect();
            family
        });
        if let Some(font_family) = font_family.as_ref() {
            attrs = attrs.family(font_family);
        }
        if let Some(font_weight) = self.font_weight {
            attrs = attrs.weight(font_weight);
        }
        if let Some(line_height) = self.line_height {
            attrs = attrs.line_height(line_height);
        }
        AttrsList::new(attrs)
    }

    fn set_text_layout(&mut self) {
        let mut text_layout = TextLayout::new();
        let attrs_list = self.get_attrs_list();
        text_layout.set_text(self.label.as_str(), attrs_list.clone());
        self.text_layout = Some(text_layout);

        if let Some(new_text) = self.available_text.as_ref() {
            let mut text_layout = TextLayout::new();
            text_layout.set_text(new_text, attrs_list);
            self.available_text_layout = Some(text_layout);
        }
    }
}

impl View for Label {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&self, _id: Id) -> Option<&dyn View> {
        None
    }

    fn child_mut(&mut self, _id: Id) -> Option<&mut dyn View> {
        None
    }

    fn children(&self) -> Vec<&dyn View> {
        Vec::new()
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        Vec::new()
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        format!("Label: {:?}", self.label).into()
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

    fn event(&mut self, _cx: &mut EventCx, _id_path: Option<&[Id]>, _event: Event) -> bool {
        false
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| {
            let (width, height) = if self.label.is_empty() {
                (0.0, cx.current_font_size().unwrap_or(14.0))
            } else {
                let text_overflow = cx.app_state_mut().get_computed_style(self.id).text_overflow;
                if self.color != cx.color
                    || self.font_size != cx.current_font_size()
                    || self.font_family.as_deref() != cx.current_font_family()
                    || self.font_weight != cx.font_weight
                    || self.font_style != cx.font_style
                    || self.line_height != cx.line_height
                    || self.text_overflow != text_overflow
                {
                    self.color = cx.color;
                    self.font_size = cx.current_font_size();
                    self.font_family = cx.current_font_family().map(|s| s.to_string());
                    self.font_weight = cx.font_weight;
                    self.font_style = cx.font_style;
                    self.line_height = cx.line_height;
                    self.text_overflow = text_overflow;
                    self.set_text_layout();
                }
                if self.text_layout.is_none() {
                    self.set_text_layout();
                }
                let text_layout = self.text_layout.as_ref().unwrap();
                let size = text_layout.size();
                let width = size.width.ceil() as f32;
                let mut height = size.height as f32;

                if text_overflow == TextOverflow::Wrap {
                    if let Some(t) = self.available_text_layout.as_ref() {
                        height = height.max(t.size().height as f32);
                    }
                }

                (width, height)
            };

            if self.text_node.is_none() {
                self.text_node = Some(
                    cx.app_state_mut()
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
            let _ = cx.app_state_mut().taffy.set_style(text_node, style);

            vec![text_node]
        })
    }

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) -> Option<Rect> {
        if self.label.is_empty() {
            return None;
        }

        let layout = cx.get_layout(self.id()).unwrap();
        let style = cx.app_state_mut().get_computed_style(self.id);
        let text_overflow = style.text_overflow;
        let padding_left = match style.padding_left {
            taffy::style::LengthPercentage::Points(padding) => padding,
            taffy::style::LengthPercentage::Percent(pct) => pct * layout.size.width,
        };
        let padding_right = match style.padding_right {
            taffy::style::LengthPercentage::Points(padding) => padding,
            taffy::style::LengthPercentage::Percent(pct) => pct * layout.size.width,
        };
        let padding = padding_left + padding_right;

        let text_layout = self.text_layout.as_ref().unwrap();
        let width = text_layout.size().width as f32;
        let available_width = layout.size.width - padding;
        if text_overflow == TextOverflow::Ellipsis {
            if width > available_width {
                if self.available_width != Some(available_width) {
                    let mut dots_text = TextLayout::new();
                    dots_text.set_text("...", self.get_attrs_list());

                    let dots_width = dots_text.size().width as f32;
                    let width_left = available_width - dots_width;
                    let hit_point = text_layout.hit_point(Point::new(width_left as f64, 0.0));
                    let index = hit_point.index;

                    let new_text = if index > 0 {
                        format!("{}...", &self.label[..index])
                    } else {
                        "".to_string()
                    };
                    self.available_text = Some(new_text);
                    self.available_width = Some(available_width);
                    self.set_text_layout();
                }
            } else {
                self.available_text = None;
                self.available_width = None;
                self.available_text_layout = None;
            }
        } else if text_overflow == TextOverflow::Wrap {
            if width > available_width {
                if self.available_width != Some(available_width) {
                    let mut text_layout = text_layout.clone();
                    text_layout.set_size(available_width, f32::MAX);
                    self.available_text_layout = Some(text_layout);
                    self.available_width = Some(available_width);
                    cx.app_state_mut().request_layout(self.id());
                }
            } else {
                if self.available_text_layout.is_some() {
                    cx.app_state_mut().request_layout(self.id());
                }
                self.available_text = None;
                self.available_width = None;
                self.available_text_layout = None;
            }
        }
        None
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if self.label.is_empty() {
            return;
        }

        if self.color != cx.color
            || self.font_size != cx.font_size
            || self.font_family.as_deref() != cx.font_family.as_deref()
            || self.font_weight != cx.font_weight
            || self.font_style != cx.font_style
            || self.line_height != cx.line_height
        {
            self.color = cx.color;
            self.font_size = cx.font_size;
            self.font_family = cx.font_family.clone();
            self.font_weight = cx.font_weight;
            self.font_style = cx.font_style;
            self.line_height = cx.line_height;
            self.set_text_layout();
        }
        let text_node = self.text_node.unwrap();
        let location = cx.app_state.taffy.layout(text_node).unwrap().location;
        let point = Point::new(location.x as f64, location.y as f64);
        if let Some(text_layout) = self.available_text_layout.as_ref() {
            cx.draw_text(text_layout, point);
        } else {
            cx.draw_text(self.text_layout.as_ref().unwrap(), point);
        }
    }
}
