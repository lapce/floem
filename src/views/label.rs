use std::{any::Any, fmt::Display};

use crate::{
    context::UpdateCx,
    cosmic_text::{Attrs, AttrsList, FamilyOwned, TextLayout},
    id::Id,
    prop_extractor,
    style::Style,
    style::{FontProps, LineHeight, TextColor, TextOverflow, TextOverflowProp},
    unit::PxPct,
    view::{View, ViewData, Widget},
};
use floem_peniko::Color;
use floem_reactive::create_updater;
use floem_renderer::Renderer;
use kurbo::{Point, Rect};
use taffy::tree::NodeId;

prop_extractor! {
    Extracter {
        color: TextColor,
        text_overflow: TextOverflowProp,
        line_height: LineHeight,
    }
}

struct TextOverflowListener {
    last_is_overflown: Option<bool>,
    on_change_fn: Box<dyn Fn(bool) + 'static>,
}

/// A View that can display text from a [`String`]. See [`label`], [`text`], and [`static_label`].
pub struct Label {
    data: ViewData,
    label: String,
    text_layout: Option<TextLayout>,
    text_node: Option<NodeId>,
    available_text: Option<String>,
    available_width: Option<f32>,
    available_text_layout: Option<TextLayout>,
    text_overflow_listener: Option<TextOverflowListener>,
    font: FontProps,
    style: Extracter,
}

impl Label {
    fn new(id: Id, label: String) -> Self {
        Label {
            data: ViewData::new(id),
            label,
            text_layout: None,
            text_node: None,
            available_text: None,
            available_width: None,
            available_text_layout: None,
            text_overflow_listener: None,
            font: FontProps::default(),
            style: Default::default(),
        }
    }
}

/// A non-reactive view that can display text from an item that implements [`Display`]. See also [`label`].
///
/// ## Example
/// ```rust
/// use floem::views::*;
///
/// stack((
///    text("non-reactive-text"),
///    text(505),
/// ));
/// ```
pub fn text<S: Display>(text: S) -> Label {
    static_label(text.to_string())
}

/// A non-reactive view that can display text from an item that can be turned into a [`String`]. See also [`label`].
pub fn static_label(label: impl Into<String>) -> Label {
    Label::new(Id::next(), label.into())
}

/// A view that can reactively display text from an item that implements [`Display`]. See also [`text`] for a non-reactive label.
///
/// ## Example
/// ```rust
/// use floem::{reactive::*, views::*};
///
/// let text = RwSignal::new("Reactive text to be displayed".to_string());
///
/// label(move || text.get());
/// ```
pub fn label<S: Display + 'static>(label: impl Fn() -> S + 'static) -> Label {
    let id = Id::next();
    let initial_label = create_updater(
        move || label().to_string(),
        move |new_label| id.update_state(new_label),
    );
    Label::new(id, initial_label)
}

impl Label {
    pub fn on_text_overflow(mut self, is_text_overflown_fn: impl Fn(bool) + 'static) -> Self {
        self.text_overflow_listener = Some(TextOverflowListener {
            on_change_fn: Box::new(is_text_overflown_fn),
            last_is_overflown: None,
        });
        self
    }

    fn get_attrs_list(&self) -> AttrsList {
        let mut attrs = Attrs::new().color(self.style.color().unwrap_or(Color::BLACK));
        if let Some(font_size) = self.font.size() {
            attrs = attrs.font_size(font_size);
        }
        if let Some(font_style) = self.font.style() {
            attrs = attrs.style(font_style);
        }
        let font_family = self.font.family().as_ref().map(|font_family| {
            let family: Vec<FamilyOwned> = FamilyOwned::parse_list(font_family).collect();
            family
        });
        if let Some(font_family) = font_family.as_ref() {
            attrs = attrs.family(font_family);
        }
        if let Some(font_weight) = self.font.weight() {
            attrs = attrs.weight(font_weight);
        }
        if let Some(line_height) = self.style.line_height() {
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
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn build(self) -> Box<dyn Widget> {
        Box::new(self)
    }
}

impl Widget for Label {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        format!("Label: {:?}", self.label).into()
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) {
        if let Ok(state) = state.downcast() {
            self.label = *state;
            self.text_layout = None;
            self.available_text = None;
            self.available_width = None;
            self.available_text_layout = None;
            cx.request_layout(self.id());
        }
    }

    fn style(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.font.read(cx) | self.style.read(cx) {
            self.text_layout = None;
            self.available_text = None;
            self.available_width = None;
            self.available_text_layout = None;
            cx.app_state_mut().request_layout(self.id());
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::tree::NodeId {
        cx.layout_node(self.id(), true, |cx| {
            let (width, height) = if self.label.is_empty() {
                (0.0, self.font.size().unwrap_or(14.0))
            } else {
                if self.text_layout.is_none() {
                    self.set_text_layout();
                }
                let text_layout = self.text_layout.as_ref().unwrap();
                let size = text_layout.size();
                let width = size.width.ceil() as f32;
                let mut height = size.height as f32;

                if self.style.text_overflow() == TextOverflow::Wrap {
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

            let style = Style::new().width(width).height(height).to_taffy_style();
            let _ = cx.app_state_mut().taffy.set_style(text_node, style);

            vec![text_node]
        })
    }

    fn compute_layout(&mut self, cx: &mut crate::context::ComputeLayoutCx) -> Option<Rect> {
        if self.label.is_empty() {
            return None;
        }

        let layout = cx.get_layout(self.id()).unwrap();
        let style = cx.app_state_mut().get_builtin_style(self.id());
        let text_overflow = style.text_overflow();
        let padding_left = match style.padding_left() {
            PxPct::Px(padding) => padding as f32,
            PxPct::Pct(pct) => pct as f32 * layout.size.width,
        };
        let padding_right = match style.padding_right() {
            PxPct::Px(padding) => padding as f32,
            PxPct::Pct(pct) => pct as f32 * layout.size.width,
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

        if let Some(listener) = self.text_overflow_listener.as_mut() {
            let was_overflown = listener.last_is_overflown;
            let now_overflown = width > available_width;

            if was_overflown != Some(now_overflown) {
                (listener.on_change_fn)(now_overflown);
                listener.last_is_overflown = Some(now_overflown);
            }
        }
        None
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if self.label.is_empty() {
            return;
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
