use std::{any::Any, fmt::Display, mem::swap};

use crate::{
    context::UpdateCx,
    cosmic_text::{Attrs, AttrsList, FamilyOwned, TextLayout},
    event::{Event, EventPropagation},
    id::ViewId,
    prop, prop_extractor,
    style::{FontProps, LineHeight, Style, TextColor, TextOverflow, TextOverflowProp},
    unit::PxPct,
    view::View,
};
use floem_reactive::create_updater;
use floem_renderer::{cosmic_text::HitPosition, Renderer};
use peniko::kurbo::{Point, Rect};
use peniko::Color;
use taffy::tree::NodeId;

prop!(pub Selectable: bool {} = false);

prop_extractor! {
    Extractor {
        color: TextColor,
        text_overflow: TextOverflowProp,
        line_height: LineHeight,
        text_selectable: Selectable,
    }
}

struct TextOverflowListener {
    last_is_overflown: Option<bool>,
    on_change_fn: Box<dyn Fn(bool) + 'static>,
}

#[derive(Debug, Clone)]
enum SelectionState {
    None,
    Ready(Point),
    Selecting(Point, Point),
    Selected(Point, Point),
}

/// A View that can display text from a [`String`]. See [`label`], [`text`], and [`static_label`].
pub struct Label {
    id: ViewId,
    label: String,
    text_layout: Option<TextLayout>,
    text_node: Option<NodeId>,
    available_text: Option<String>,
    available_width: Option<f32>,
    available_text_layout: Option<TextLayout>,
    text_overflow_listener: Option<TextOverflowListener>,
    selection_pos: SelectionState,
    selection_range: Option<core::ops::RangeInclusive<usize>>,
    selection_rect: Option<Rect>,
    font: FontProps,
    style: Extractor,
}

impl Label {
    fn new(id: ViewId, label: String) -> Self {
        Label {
            id,
            label,
            text_layout: None,
            text_node: None,
            available_text: None,
            available_width: None,
            available_text_layout: None,
            text_overflow_listener: None,
            selection_pos: SelectionState::None,
            selection_range: None,
            selection_rect: None,
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
    Label::new(ViewId::new(), label.into())
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
    let id = ViewId::new();
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

    fn get_hit_point(&self, point: Point) -> usize {
        let layout = self.id.get_layout().unwrap_or_default();
        let view_state = self.id.state();
        let view_state = view_state.borrow();
        let style = view_state.combined_style.builtin();

        let padding_left = match style.padding_left() {
            PxPct::Px(padding) => padding as f32,
            PxPct::Pct(pct) => pct as f32 * layout.size.width,
        };
        let padding_top = match style.padding_top() {
            PxPct::Px(padding) => padding as f32,
            PxPct::Pct(pct) => pct as f32 * layout.size.width,
        };
        if self.available_text_layout.is_some() {
            println!("There is an available text layout");
        }
        self.text_layout
            .as_ref()
            .unwrap()
            .hit_point(Point::new(
                point.x - padding_left as f64,
                // TODO: prevent cursor incorrectly going to end of buffer when clicking
                // slightly below the text
                point.y - padding_top as f64,
            ))
            .index
    }

    fn set_selection_range(&mut self) {
        if let SelectionState::Selecting(start, end) | SelectionState::Selected(start, end) =
            self.selection_pos
        {
            let mut start_index = self.get_hit_point(start);
            let mut end_index = self.get_hit_point(end);
            if start_index > end_index {
                swap(&mut start_index, &mut end_index);
            }

            self.selection_range = Some(start_index..=end_index);
        }
    }
}

impl View for Label {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        format!("Label: {:?}", self.label).into()
    }

    fn update(&mut self, _cx: &mut UpdateCx, state: Box<dyn Any>) {
        if let Ok(state) = state.downcast() {
            self.label = *state;
            self.text_layout = None;
            self.available_text = None;
            self.available_width = None;
            self.available_text_layout = None;
            self.selection_rect = None;
            self.id.request_layout();
        }
    }

    fn event_before_children(
        &mut self,
        _cx: &mut crate::context::EventCx,
        event: &Event,
    ) -> crate::event::EventPropagation {
        match event {
            Event::PointerDown(pe) => {
                self.selection_rect = None;
                self.selection_pos = SelectionState::Ready(pe.pos);
                self.id.request_active();
            }
            Event::PointerMove(pme) => {
                let (SelectionState::Selecting(start, _) | SelectionState::Ready(start)) =
                    self.selection_pos
                else {
                    return EventPropagation::Continue;
                };
                self.selection_pos = SelectionState::Selecting(start, pme.pos);
                self.id.request_layout();
            }
            Event::PointerUp(_) => {
                self.id.clear_active();
                if let SelectionState::Selecting(start, end) = self.selection_pos {
                    self.selection_pos = SelectionState::Selected(start, end);
                } else {
                    self.selection_pos = SelectionState::None;
                    self.id.request_paint();
                }
            }
            _ => {}
        }
        EventPropagation::Continue
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.font.read(cx) | self.style.read(cx) {
            self.text_layout = None;
            self.available_text = None;
            self.available_width = None;
            self.available_text_layout = None;
            self.id.request_layout();
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::tree::NodeId {
        cx.layout_node(self.id(), true, |_cx| {
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
                    self.id
                        .taffy()
                        .borrow_mut()
                        .new_leaf(taffy::style::Style::DEFAULT)
                        .unwrap(),
                );
            }
            let text_node = self.text_node.unwrap();

            let style = Style::new().width(width).height(height).to_taffy_style();
            let _ = self.id.taffy().borrow_mut().set_style(text_node, style);

            vec![text_node]
        })
    }

    fn compute_layout(&mut self, _cx: &mut crate::context::ComputeLayoutCx) -> Option<Rect> {
        if self.label.is_empty() {
            return None;
        }

        let layout = self.id.get_layout().unwrap_or_default();
        let (text_overflow, padding) = {
            let view_state = self.id.state();
            let view_state = view_state.borrow();
            let style = view_state.combined_style.builtin();
            let padding_left = match style.padding_left() {
                PxPct::Px(padding) => padding as f32,
                PxPct::Pct(pct) => pct as f32 * layout.size.width,
            };
            let padding_right = match style.padding_right() {
                PxPct::Px(padding) => padding as f32,
                PxPct::Pct(pct) => pct as f32 * layout.size.width,
            };
            let text_overflow = style.text_overflow();
            (text_overflow, padding_left + padding_right)
        };
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
                    self.id.request_layout();
                }
            } else {
                if self.available_text_layout.is_some() {
                    self.id.request_layout();
                }
                self.available_text = None;
                self.available_width = None;
                self.available_text_layout = None;
            }
        }

        self.selection_rect = None;
        self.set_selection_range();
        let text_layout = self.text_layout.as_ref().unwrap();
        if let Some(range) = &self.selection_range {
            let HitPosition {
                point:
                    Point {
                        x: start_x,
                        y: start_y,
                    },
                ..
            } = text_layout.hit_position(*range.start());
            let HitPosition {
                point: Point { x: end_x, y: end_y },
                ..
            } = text_layout.hit_position(*range.end());
            self.selection_rect = Some(Rect::new(
                start_x,
                start_y - text_layout.layout_runs().next().unwrap().line_height as f64,
                end_x,
                end_y,
            ));
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
        let location = self
            .id
            .taffy()
            .borrow()
            .layout(text_node)
            .cloned()
            .unwrap_or_default()
            .location;
        let point = Point::new(location.x as f64, location.y as f64);
        if let Some(text_layout) = self.available_text_layout.as_ref() {
            cx.draw_text(text_layout, point);
        } else {
            cx.draw_text(self.text_layout.as_ref().unwrap(), point);
        }
        if let Some(rect) = self.selection_rect {
            cx.fill(&rect, Color::GRAY.with_alpha_factor(0.5), 0.);
        }
    }
}
