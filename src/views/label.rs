use std::{any::Any, cell::RefCell, fmt::Display, mem::swap, rc::Rc};

use crate::{
    Clipboard, ViewId,
    context::{EventCx, LayoutChangedListener, PaintCx, UpdateCx},
    event::{Event, EventPropagation, FocusEvent, Phase, listener},
    prelude::EventListenerTrait,
    prop_extractor,
    style::{
        CustomStylable, CustomStyle, FontProps, LineHeight, Selectable, SelectionCornerRadius,
        SelectionStyle, Style, TextAlignProp, TextColor, TextOverflow, TextOverflowProp,
    },
    style_class,
    text::{Attrs, AttrsList, Cursor, FamilyOwned, TextLayout, TextLayoutData, WordBreakStrength},
    view::{LayoutNodeCx, View},
    views::editor::SelectionColor,
};
use floem_reactive::UpdaterEffect;
use floem_renderer::Renderer;
use peniko::{
    Brush,
    color::palette::{self},
    kurbo::Point,
};
use ui_events::{
    keyboard::{Key, KeyState, KeyboardEvent},
    pointer::{PointerButtonEvent, PointerEvent},
};

use super::{Decorators, TextCommand};

prop_extractor! {
    LabelProps {
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

/// Event fired when a text view's overflow state changes.
///
/// This is fired when text transitions between fitting within its bounds and overflowing,
/// or vice versa. The overflow state depends on the `text_overflow` style property:
///
/// - `TextOverflow::NoWrap(NoWrapOverflow::Clip)`: Text is clipped at the boundary
/// - `TextOverflow::NoWrap(NoWrapOverflow::Ellipsis)`: Text is truncated with "..." when it overflows
/// - `TextOverflow::Wrap { .. }`: Text wraps to multiple lines (changes line count, not overflow state)
///
/// # Use Cases
///
/// - Show/hide a "more" button when text is truncated
/// - Toggle between single-line and multi-line display modes
/// - Display tooltips with full text when content is clipped
/// - Update UI indicators when text fits or overflows
///
/// # Example
///
/// ```rust
/// # use floem::event::EventPropagation;
/// # use floem::prelude::*;
/// # use floem::style::{NoWrapOverflow, TextOverflow};
/// Label::derived(|| "Some long text that might overflow")
///     .style(|s| s.text_overflow(TextOverflow::NoWrap(NoWrapOverflow::Ellipsis)))
///     .on_event(TextOverflowChanged::listener(), |cx, event| {
///         if event.is_overflowing {
///             println!("Text is now overflowing and truncated");
///         } else {
///             println!("Text fits completely");
///         }
///         EventPropagation::Continue
///     });
/// ```
#[derive(Debug, Clone, PartialEq)]
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
    /// Layout data containing text layouts and overflow handling logic
    layout_data: Rc<RefCell<TextLayoutData>>,
    selection_state: SelectionState,
    selection_range: Option<(Cursor, Cursor)>,
    selection_style: SelectionStyle,
    font_props: FontProps,
    label_props: LabelProps,
    text_node: Option<taffy::NodeId>,
    layout_node: Option<taffy::NodeId>,
}

impl Label {
    fn new_internal(id: ViewId, label: String) -> Self {
        id.register_listener(LayoutChangedListener::listener_key());
        let layout_data = Rc::new(RefCell::new(TextLayoutData::new(Some(id))));
        let mut label = Label {
            id,
            label,
            layout_data,
            text_node: None,
            layout_node: None,
            selection_state: SelectionState::None,
            selection_range: None,
            selection_style: Default::default(),
            font_props: FontProps::default(),
            label_props: Default::default(),
        };
        label.set_text_layout();
        label.set_taffy_layout();
        label.class(LabelClass)
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
        Self::new_internal(id, initial_label).on_event_cont(listener::FocusLost, move |_, _| {
            id.request_layout();
        })
    }

    fn with_effective_text_layout<O>(&self, with: impl FnOnce(&TextLayout) -> O) -> O {
        self.layout_data.borrow().with_effective_text_layout(with)
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
    fn get_attrs_list(&self) -> AttrsList {
        let mut attrs = Attrs::new().color(self.label_props.color().unwrap_or(palette::css::BLACK));
        if let Some(font_size) = self.font_props.size() {
            attrs = attrs.font_size(font_size);
        }
        if let Some(font_style) = self.font_props.style() {
            attrs = attrs.font_style(font_style);
        }
        let font_family = self.font_props.family().as_ref().map(|font_family| {
            let family: Vec<FamilyOwned> = FamilyOwned::parse_list(font_family).collect();
            family
        });
        if let Some(font_family) = font_family.as_ref() {
            attrs = attrs.family(font_family);
        }
        if let Some(font_weight) = self.font_props.weight() {
            attrs = attrs.weight(font_weight);
        }
        if let Some(line_height) = self.label_props.line_height() {
            attrs = attrs.line_height(line_height);
        }
        if let TextOverflow::Wrap { word_break, .. } = self.label_props.text_overflow()
            && word_break != WordBreakStrength::Normal
        {
            attrs = attrs.word_break(word_break);
        }
        AttrsList::new(attrs)
    }

    fn set_text_layout(&mut self) {
        let attrs_list = self.get_attrs_list();
        let align = self.label_props.text_align();
        let text_overflow = self.label_props.text_overflow();

        let mut layout_data = self.layout_data.borrow_mut();
        layout_data.set_text(&self.label, attrs_list, align);
        layout_data.set_text_overflow(text_overflow);

        let _ = self.id.mark_view_layout_dirty();
    }

    fn get_hit_point(&self, point: Point) -> Option<Cursor> {
        let (Some(parent_node), Some(text_node)) = (self.layout_node, self.text_node) else {
            return None;
        };

        let text_loc = self.get_text_origin(parent_node, text_node);
        self.with_effective_text_layout(|l| l.hit_test(point - text_loc.to_vec2()))
    }

    fn get_text_origin(&self, parent_node: taffy::NodeId, text_node: taffy::NodeId) -> Point {
        let content_rect = self
            .id
            .get_content_rect_relative(text_node, parent_node)
            .unwrap_or_default();
        self.layout_data.borrow().centered_text_origin(content_rect)
    }

    fn set_selection_range(&mut self) {
        match self.selection_state {
            SelectionState::None => {
                self.selection_range = None;
            }
            SelectionState::Selecting(start, end) | SelectionState::Selected(start, end) => {
                let mut start_cursor = self.get_hit_point(start).expect("Start position is valid");
                if let Some(mut end_cursor) = self.get_hit_point(end) {
                    if start_cursor.index() > end_cursor.index() {
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
                if let Some((start_c, end_c)) = &self.selection_range {
                    let layout_data = self.layout_data.borrow();
                    if let Some(text_layout) = layout_data.get_text_layout() {
                        let start_idx = text_layout.cursor_to_byte_index(start_c);
                        let end_idx = text_layout.cursor_to_byte_index(end_c);
                        let selection_txt = self.label[start_idx..end_idx].into();
                        let _ = Clipboard::set_contents(selection_txt);
                    }
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

    fn paint_selection(&self, text_loc: Point, paint_cx: &mut PaintCx) {
        if let Some((start_c, end_c)) = &self.selection_range {
            let selection_color = self.selection_style.selection_color();
            self.layout_data.borrow().selection_rects_for_cursors(
                start_c,
                end_c,
                text_loc,
                |rect| {
                    paint_cx.fill(&rect, &selection_color, 0.0);
                },
            );
        }
    }

    fn set_taffy_layout(&mut self) {
        let taffy_node = self.id.taffy_node();
        let taffy = self.id.taffy();
        let mut taffy = taffy.borrow_mut();
        let text_node = taffy
            .new_leaf(taffy::Style {
                ..taffy::Style::DEFAULT
            })
            .unwrap();

        let layout_fn = TextLayoutData::create_taffy_layout_fn(self.layout_data.clone());
        let finalize_fn = TextLayoutData::create_finalize_fn(self.layout_data.clone());
        self.text_node = Some(text_node);
        self.layout_node = Some(taffy_node);

        taffy
            .set_node_context(
                text_node,
                Some(LayoutNodeCx::Custom {
                    measure: layout_fn,
                    finalize: Some(finalize_fn),
                }),
            )
            .unwrap();
        taffy.set_children(taffy_node, &[text_node]).unwrap();
    }
}

impl View for Label {
    fn id(&self) -> ViewId {
        self.id
    }

    fn view_style(&self) -> Option<Style> {
        None
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        format!("Label: {:?}", self.label).into()
    }

    fn event(&mut self, cx: &mut EventCx) -> EventPropagation {
        if let Some(layout) = LayoutChangedListener::extract(&cx.event) {
            self.layout_data
                .borrow_mut()
                .finalize_for_width(layout.new_content_box.width() as f32);
        }
        match &cx.event {
            Event::Focus(FocusEvent::Lost) => {
                self.selection_state = SelectionState::None;
                self.selection_range = None;
                cx.window_state.request_paint(self.id);
                return EventPropagation::Continue;
            }
            Event::Pointer(PointerEvent::Down(PointerButtonEvent { state, pointer, .. })) => {
                if self.label_props.text_selectable()
                    && state
                        .buttons
                        .contains(ui_events::pointer::PointerButton::Primary)
                {
                    self.selection_range = None;
                    self.selection_state = SelectionState::Ready(state.logical_point());
                    if let Some(pointer_id) = pointer.pointer_id {
                        cx.window_state.set_pointer_capture(pointer_id, self.id);
                    }
                    cx.window_state.request_paint(self.id);
                }
            }
            Event::Pointer(PointerEvent::Move(pu)) => {
                if !self.label_props.text_selectable() {
                    if self.selection_state != SelectionState::None {
                        self.selection_state = SelectionState::None;
                        self.selection_range = None;
                        cx.window_state.request_paint(self.id);
                    }
                } else {
                    let (SelectionState::Selecting(start, _) | SelectionState::Ready(start)) =
                        self.selection_state
                    else {
                        return EventPropagation::Continue;
                    };
                    // this check is here to make it so that text selection doesn't eat pointer events on very small move events
                    if start.distance(pu.current.logical_point()).abs() > 2.
                        && matches!(
                            self.selection_state,
                            SelectionState::Ready(_) | SelectionState::Selecting(_, _)
                        )
                    {
                        self.selection_state =
                            SelectionState::Selecting(start, pu.current.logical_point());
                        self.set_selection_range();
                        cx.window_state.request_paint(self.id);
                        self.id.request_focus();
                    }
                }
            }
            Event::Pointer(PointerEvent::Up { .. }) => {
                if let SelectionState::Selecting(start, end) = self.selection_state {
                    self.selection_state = SelectionState::Selected(start, end);
                } else {
                    self.selection_state = SelectionState::None;
                }
                cx.window_state.request_paint(self.id);
            }
            Event::Key(
                ke @ KeyboardEvent {
                    state: KeyState::Down,
                    ..
                },
            ) => {
                if cx.phase != Phase::Target || !cx.window_state.is_focused(self.id) {
                    return EventPropagation::Continue;
                }
                if self.handle_key_down(ke) {
                    return EventPropagation::Stop;
                }
            }
            _ => {}
        }
        EventPropagation::Continue
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.font_props.read(cx) | self.label_props.read(cx) {
            self.layout_data.borrow_mut().clear_overflow_state();
            self.set_text_layout();
            self.id.request_layout();
        }
        if self.selection_style.read(cx) {
            cx.window_state.request_paint(self.id);
        }
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) {
        if state.is::<String>()
            && let Ok(state) = state.downcast::<String>()
        {
            self.label = *state;
            self.layout_data.borrow_mut().clear_overflow_state();
            self.set_text_layout();
            cx.window_state.schedule_layout();
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if self.label.is_empty() {
            return;
        }

        let (Some(this_node), Some(text_node)) = (self.layout_node, self.text_node) else {
            return;
        };

        let text_loc = self.get_text_origin(this_node, text_node);

        self.with_effective_text_layout(|l| {
            l.draw(cx, text_loc);
            if cx.window_state.is_focused(self.id) {
                self.paint_selection(text_loc, cx);
            }
        });
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
        self = Self(self.0.set(SelectionColor, color));
        self
    }
}
impl Default for LabelCustomStyle {
    fn default() -> Self {
        Self::new()
    }
}
