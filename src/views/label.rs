use std::{any::Any, fmt::Display, mem::swap};

use crate::{
    Clipboard,
    context::{PaintCx, UpdateCx},
    event::{Event, EventListener, EventPropagation},
    prop_extractor,
    style::{
        CursorColor, CustomStylable, CustomStyle, FontProps, LineHeight, Selectable,
        SelectionCornerRadius, SelectionStyle, Style, TextAlignProp, TextColor, TextOverflow,
        TextOverflowProp,
    },
    style_class,
    text::{Attrs, AttrsList, FamilyOwned, TextLayout},
    unit::PxPct,
    view::View,
    view::ViewId,
};
use floem_reactive::UpdaterEffect;
use floem_renderer::{Renderer, text::Cursor};
use peniko::{
    Brush,
    color::palette,
    kurbo::{Point, Rect},
};
use taffy::tree::NodeId;
use ui_events::{
    keyboard::{Key, KeyState, KeyboardEvent},
    pointer::{PointerButtonEvent, PointerEvent},
};

use super::{Decorators, TextCommand};

prop_extractor! {
    Extractor {
        color: TextColor,
        text_overflow: TextOverflowProp,
        line_height: LineHeight,
        text_selectable: Selectable,
        text_align: TextAlignProp,
    }
}

style_class!(
    /// The style class that is applied to labels.
    pub LabelClass
);

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
    selection_state: SelectionState,
    selection_range: Option<(Cursor, Cursor)>,
    selection_style: SelectionStyle,
    font: FontProps,
    style: Extractor,
}

impl Label {
    fn new_internal(id: ViewId, label: String) -> Self {
        Label {
            id,
            label,
            text_layout: None,
            text_node: None,
            available_text: None,
            available_width: None,
            available_text_layout: None,
            text_overflow_listener: None,
            selection_state: SelectionState::None,
            selection_range: None,
            selection_style: Default::default(),
            font: FontProps::default(),
            style: Default::default(),
        }
        .class(LabelClass)
    }

    /// Creates a new non-reactive label from any type that implements [`Display`].
    ///
    /// ## Example
    /// ```rust
    /// use floem::views::*;
    ///
    /// Label::new("Hello, world!");
    /// Label::new(42);
    /// ```
    pub fn new<S: Display>(label: S) -> Self {
        Self::new_internal(ViewId::new(), label.to_string())
    }

    /// Creates a new non-reactive label with a pre-existing [`ViewId`].
    ///
    /// This is useful for lazy view construction where the `ViewId` is created
    /// before the view itself.
    pub fn with_id<S: Display>(id: ViewId, label: S) -> Self {
        Self::new_internal(id, label.to_string())
    }

    /// Creates a derived label that automatically updates when its dependencies change.
    ///
    /// ## Example
    /// ```rust
    /// use floem::{reactive::*, views::*};
    ///
    /// let count = RwSignal::new(0);
    /// Label::derived(move || format!("Count: {}", count.get()));
    /// ```
    pub fn derived<S: Display + 'static>(label: impl Fn() -> S + 'static) -> Self {
        let id = ViewId::new();
        let initial_label = UpdaterEffect::new(
            move || label().to_string(),
            move |new_label| id.update_state(new_label),
        );
        Self::new_internal(id, initial_label).on_event_cont(EventListener::FocusLost, move |_| {
            id.request_layout();
        })
    }

    fn effective_text_layout(&self) -> &TextLayout {
        self.available_text_layout
            .as_ref()
            .unwrap_or_else(|| self.text_layout.as_ref().unwrap())
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
#[deprecated(since = "0.2.0", note = "Use Label::new() instead")]
pub fn text<S: Display>(text: S) -> Label {
    Label::new(text)
}

/// A non-reactive view that can display text from an item that can be turned into a [`String`]. See also [`label`].
#[deprecated(since = "0.2.0", note = "Use Label::new() instead")]
pub fn static_label(label: impl Into<String>) -> Label {
    Label::new(label.into())
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
#[deprecated(since = "0.2.0", note = "Use Label::derived() instead")]
pub fn label<S: Display + 'static>(label: impl Fn() -> S + 'static) -> Label {
    Label::derived(label)
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
        let mut attrs = Attrs::new().color(self.style.color().unwrap_or(palette::css::BLACK));
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
        let align = self.style.text_align();
        text_layout.set_text(self.label.as_str(), attrs_list.clone(), align);
        self.text_layout = Some(text_layout);

        if let Some(new_text) = self.available_text.as_ref() {
            let mut text_layout = TextLayout::new();
            text_layout.set_text(new_text, attrs_list, align);
            self.available_text_layout = Some(text_layout);
        }
    }

    fn get_hit_point(&self, point: Point) -> Option<Cursor> {
        let text_node = self.text_node?;
        let location = self
            .id
            .taffy()
            .borrow()
            .layout(text_node)
            .map_or(taffy::Layout::new().location, |layout| layout.location);
        self.effective_text_layout().hit(
            point.x as f32 - location.x,
            // TODO: prevent cursor incorrectly going to end of buffer when clicking
            // slightly below the text
            point.y as f32 - location.y,
        )
    }

    fn set_selection_range(&mut self) {
        match self.selection_state {
            SelectionState::None => {
                self.selection_range = None;
            }
            SelectionState::Selecting(start, end) | SelectionState::Selected(start, end) => {
                let mut start_cursor = self.get_hit_point(start).expect("Start position is valid");
                if let Some(mut end_cursor) = self.get_hit_point(end) {
                    if start_cursor.line > end_cursor.line
                        || (start_cursor.line == end_cursor.line
                            && start_cursor.index > end_cursor.index)
                    {
                        swap(&mut start_cursor, &mut end_cursor);
                    }

                    self.selection_range = Some((start_cursor, end_cursor));
                }
            }
            _ => {}
        }
    }

    fn handle_modifier_cmd(&mut self, command: &TextCommand) -> bool {
        match command {
            TextCommand::Copy => {
                if let Some((start_c, end_c)) = &self.selection_range
                    && let Some(ref text_layout) = self.text_layout
                {
                    let start_line_idx = text_layout.lines_range()[start_c.line].start;
                    let end_line_idx = text_layout.lines_range()[end_c.line].start;
                    let start_idx = start_line_idx + start_c.index;
                    let end_idx = end_line_idx + end_c.index;
                    let selection_txt = self.label[start_idx..end_idx].into();
                    let _ = Clipboard::set_contents(selection_txt);
                }
                true
            }
            _ => false,
        }
    }
    fn handle_key_down(&mut self, event: &KeyboardEvent) -> bool {
        if event.modifiers.is_empty() {
            return false;
        }
        if !matches!(event.key, Key::Character(_)) {
            return false;
        }

        self.handle_modifier_cmd(&event.into())
    }

    fn paint_selection(&self, text_layout: &TextLayout, paint_cx: &mut PaintCx) {
        if let Some((start_c, end_c)) = &self.selection_range {
            let location = self
                .id
                .taffy()
                .borrow()
                .layout(self.text_node.unwrap())
                .cloned()
                .unwrap_or_default()
                .location;
            let ss = &self.selection_style;
            let selection_color = ss.selection_color();

            for run in text_layout.layout_runs() {
                if let Some((mut start_x, width)) = run.highlight(*start_c, *end_c) {
                    start_x += location.x;
                    let end_x = width + start_x;
                    let start_y = location.y as f64 + run.line_top as f64;
                    let end_y = start_y + run.line_height as f64;
                    let rect = Rect::new(start_x.into(), start_y, end_x.into(), end_y)
                        .to_rounded_rect(ss.corner_radius());
                    paint_cx.fill(&rect, &selection_color, 0.0);
                }
            }
        }
    }

    pub fn label_style(
        self,
        style: impl Fn(LabelCustomStyle) -> LabelCustomStyle + 'static,
    ) -> Self {
        self.custom_style(style)
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
            self.id.request_layout();
        }
    }

    fn event_before_children(
        &mut self,
        _cx: &mut crate::context::EventCx,
        event: &Event,
    ) -> crate::event::EventPropagation {
        match event {
            Event::Pointer(PointerEvent::Down(PointerButtonEvent { state, .. })) => {
                if self.style.text_selectable() {
                    self.selection_range = None;
                    self.selection_state = SelectionState::Ready(state.logical_point());
                    self.id.request_layout();
                }
            }
            Event::Pointer(PointerEvent::Move(pu)) => {
                if !self.style.text_selectable() {
                    if self.selection_range.is_some() {
                        self.selection_state = SelectionState::None;
                        self.selection_range = None;
                        self.id.request_layout();
                    }
                } else {
                    let (SelectionState::Selecting(start, _) | SelectionState::Ready(start)) =
                        self.selection_state
                    else {
                        return EventPropagation::Continue;
                    };
                    // this check is here to make it so that text selection doesn't eat pointer events on very small move events
                    if start.distance(pu.current.logical_point()).abs() > 2. {
                        self.selection_state =
                            SelectionState::Selecting(start, pu.current.logical_point());
                        self.id.request_active();
                        self.id.request_focus();
                        self.id.request_layout();
                    }
                }
            }
            Event::Pointer(PointerEvent::Up { .. }) => {
                if let SelectionState::Selecting(start, end) = self.selection_state {
                    self.selection_state = SelectionState::Selected(start, end);
                } else {
                    self.selection_state = SelectionState::None;
                }
                self.id.clear_active();
                self.id.request_layout();
            }
            Event::Key(
                ke @ KeyboardEvent {
                    state: KeyState::Down,
                    ..
                },
            ) => {
                if self.handle_key_down(ke) {
                    return EventPropagation::Stop;
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
        if self.selection_style.read(cx) {
            self.id.request_paint();
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

                if self.style.text_overflow() == TextOverflow::Wrap
                    && let Some(t) = self.available_text_layout.as_ref()
                {
                    height = height.max(t.size().height as f32);
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
                PxPct::Pct(pct) => (pct / 100.) as f32 * layout.size.width,
            };
            let padding_right = match style.padding_right() {
                PxPct::Px(padding) => padding as f32,
                PxPct::Pct(pct) => (pct / 100.) as f32 * layout.size.width,
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
                    dots_text.set_text("...", self.get_attrs_list(), self.style.text_align());

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

        self.set_selection_range();

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
            .map_or(taffy::Layout::new().location, |layout| layout.location);

        let point = Point::new(location.x as f64, location.y as f64);

        let text_layout = self.effective_text_layout();
        cx.draw_text(text_layout, point);
        if cx.window_state.is_focused(&self.id()) {
            self.paint_selection(text_layout, cx);
        }
    }
}

/// Represents a custom style for a `Label`.
#[derive(Debug, Clone)]
pub struct LabelCustomStyle(Style);
impl From<LabelCustomStyle> for Style {
    fn from(value: LabelCustomStyle) -> Self {
        value.0
    }
}
impl From<Style> for LabelCustomStyle {
    fn from(value: Style) -> Self {
        Self(value)
    }
}
impl CustomStyle for LabelCustomStyle {
    type StyleClass = LabelClass;
}

impl CustomStylable<LabelCustomStyle> for Label {
    type DV = Self;
}

impl LabelCustomStyle {
    pub fn new() -> Self {
        Self(Style::new())
    }

    pub fn selectable(mut self, selectable: impl Into<bool>) -> Self {
        self = Self(self.0.set(Selectable, selectable));
        self
    }

    pub fn selection_corner_radius(mut self, corner_radius: impl Into<f64>) -> Self {
        self = Self(self.0.set(SelectionCornerRadius, corner_radius));
        self
    }

    pub fn selection_color(mut self, color: impl Into<Brush>) -> Self {
        self = Self(self.0.set(CursorColor, color));
        self
    }
}
impl Default for LabelCustomStyle {
    fn default() -> Self {
        Self::new()
    }
}
