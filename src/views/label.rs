use std::{any::Any, cell::RefCell, fmt::Display, rc::Rc};

use crate::{
    Clipboard, ViewId,
    context::{EventCx, LayoutChangedListener, PaintCx, UpdateCx},
    event::{Event, EventPropagation, FocusEvent, Phase, listener},
    prelude::EventListenerTrait,
    prop_extractor,
    style::{
        ContextValue, CustomStylable, CustomStyle, ExprStyle, FontProps, LineHeight, Selectable,
        SelectionCornerRadius, SelectionStyle, Style, TextAlignProp, TextColor, TextOverflow,
        TextOverflowProp,
    },
    style_class,
    text::{
        Attrs, AttrsList, Cursor, FamilyOwned, TextLayout, TextLayoutState, TextSelection,
        WordBreakStrength,
    },
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
/// # use floem::text::TextOverflowChanged;
/// Label::derived(|| "Some long text that might overflow")
///     .style(|s| s.text_overflow(TextOverflow::NoWrap(NoWrapOverflow::Ellipsis)))
///     .on_event(TextOverflowChanged::listener(), |_cx, event| {
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
    Ready {
        origin: Point,
        selection: TextSelection,
    },
    Selecting(TextSelection),
    Selected(TextSelection),
}

/// A View that can display text from a [`String`]. See [`label`], [`text`], and [`static_label`].
pub struct Label {
    id: ViewId,
    label: String,
    /// Layout data containing text layouts and overflow handling logic
    layout_data: Rc<RefCell<TextLayoutState>>,
    selection_state: SelectionState,
    selection_style: SelectionStyle,
    font_props: FontProps,
    label_props: LabelProps,
    text_node: Option<taffy::NodeId>,
    layout_node: Option<taffy::NodeId>,
}

impl Label {
    fn new_internal(id: ViewId, label: String) -> Self {
        id.register_listener(LayoutChangedListener::listener_key());
        let layout_data = Rc::new(RefCell::new(TextLayoutState::new(Some(id))));
        let mut label = Label {
            id,
            label,
            layout_data,
            text_node: None,
            layout_node: None,
            selection_state: SelectionState::None,
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
    fn mark_text_measure_dirty(&self) {
        if let Some(text_node) = self.text_node {
            let _ = self.id.taffy().borrow_mut().mark_dirty(text_node);
        }
        let _ = self.id.mark_view_layout_dirty();
    }

    fn get_attrs_list(&self) -> AttrsList {
        let mut attrs = Attrs::new().color(self.label_props.color().unwrap_or(palette::css::BLACK));
        let font_size = self.font_props.size();
        attrs = attrs.font_size(font_size as f32);

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
        attrs = attrs.line_height(self.label_props.line_height());
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

        self.mark_text_measure_dirty();
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

    fn update_drag_selection(&mut self, focus_point: Point) {
        let Some(focus) = self.get_hit_point(focus_point) else {
            return;
        };

        match self.selection_state {
            SelectionState::Ready { selection, .. } => {
                let next_selection = self
                    .layout_data
                    .borrow()
                    .get_effective_text_layout()
                    .map(|layout| layout.begin_selection(selection.anchor(), focus))
                    .expect("label text layout should be available while selecting");
                self.selection_state = SelectionState::Selecting(next_selection);
            }
            SelectionState::Selecting(selection) | SelectionState::Selected(selection) => {
                let selection = self
                    .layout_data
                    .borrow()
                    .get_effective_text_layout()
                    .map(|layout| layout.selection(selection.anchor(), focus))
                    .expect("label text layout should be available while selecting");
                self.selection_state = SelectionState::Selecting(selection);
            }
            SelectionState::None => {}
        }
    }

    fn selection(&self) -> Option<TextSelection> {
        match self.selection_state {
            SelectionState::Selecting(selection) | SelectionState::Selected(selection)
                if !selection.is_collapsed() =>
            {
                Some(selection)
            }
            SelectionState::Ready { .. } | SelectionState::None => None,
            SelectionState::Selecting(_) | SelectionState::Selected(_) => None,
        }
    }

    fn handle_modifier_cmd(&mut self, command: &TextCommand) -> bool {
        match command {
            TextCommand::Copy => {
                if let Some(selection) = self.selection() {
                    let layout_data = self.layout_data.borrow();
                    if let Some(text_layout) = layout_data.get_effective_text_layout() {
                        let range = text_layout.selection_text_range(&selection);
                        let selection_txt = self.label[range].into();
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
        if let Some(selection) = self.selection() {
            let selection_color = self.selection_style.selection_color();
            self.layout_data
                .borrow()
                .selection_rects_for_selection(&selection, text_loc, |rect| {
                    paint_cx.fill(&rect, &selection_color, 0.0);
                });
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

        let layout_fn = TextLayoutState::create_taffy_layout_fn(self.layout_data.clone());
        let finalize_fn = TextLayoutState::create_finalize_fn(self.layout_data.clone());
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
                cx.window_state.request_paint(self.id);
                return EventPropagation::Continue;
            }
            Event::Pointer(PointerEvent::Down(PointerButtonEvent { state, pointer, .. })) => {
                if self.label_props.text_selectable()
                    && state
                        .buttons
                        .contains(ui_events::pointer::PointerButton::Primary)
                {
                    self.selection_state = self
                        .get_hit_point(state.logical_point())
                        .map(|cursor| SelectionState::Ready {
                            origin: state.logical_point(),
                            selection: self
                                .layout_data
                                .borrow()
                                .get_effective_text_layout()
                                .map(|layout| layout.collapsed_selection(cursor))
                                .expect("label text layout should be available on pointer down"),
                        })
                        .unwrap_or(SelectionState::None);
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
                        cx.window_state.request_paint(self.id);
                    }
                } else {
                    match self.selection_state {
                        SelectionState::Ready { origin, .. } => {
                            // This check prevents text selection from eating tiny pointer moves.
                            if origin.distance(pu.current.logical_point()).abs() > 2. {
                                self.update_drag_selection(pu.current.logical_point());
                                cx.window_state.request_paint(self.id);
                                self.id.request_focus();
                            }
                        }
                        SelectionState::Selecting(_) => {
                            self.update_drag_selection(pu.current.logical_point());
                            cx.window_state.request_paint(self.id);
                        }
                        SelectionState::Selected(_) => return EventPropagation::Continue,
                        SelectionState::None => return EventPropagation::Continue,
                    }
                }
            }
            Event::Pointer(PointerEvent::Up { .. }) => {
                self.selection_state = match self.selection_state {
                    SelectionState::Selecting(selection) if !selection.is_collapsed() => {
                        SelectionState::Selected(selection)
                    }
                    _ => SelectionState::None,
                };
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
            self.id.request_paint();
        }
        if self.selection_style.read(cx) {
            cx.window_state.request_paint(self.id.into());
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
        self = Self(self.0.set(SelectionColor, color.into()));
        self
    }
}
impl Default for LabelCustomStyle {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Default)]
pub struct LabelCustomExprStyle(Style);
impl From<LabelCustomExprStyle> for Style {
    fn from(value: LabelCustomExprStyle) -> Self {
        value.0
    }
}
impl From<Style> for LabelCustomExprStyle {
    fn from(value: Style) -> Self {
        Self(value)
    }
}
impl LabelCustomExprStyle {
    pub fn new() -> Self {
        Self(Style::new())
    }

    pub fn selectable<T>(mut self, selectable: ContextValue<T>) -> Self
    where
        T: Into<bool> + 'static,
    {
        self = Self(
            ExprStyle::from(self.0)
                .set_context(Selectable, selectable.map(Into::into))
                .into(),
        );
        self
    }

    pub fn selection_corner_radius<T>(mut self, corner_radius: ContextValue<T>) -> Self
    where
        T: Into<f64> + 'static,
    {
        self = Self(
            ExprStyle::from(self.0)
                .set_context(SelectionCornerRadius, corner_radius.map(Into::into))
                .into(),
        );
        self
    }

    pub fn selection_color<T>(mut self, color: ContextValue<T>) -> Self
    where
        T: Into<Brush> + 'static,
    {
        self = Self(
            ExprStyle::from(self.0)
                .set_context(SelectionColor, color.map(Into::into))
                .into(),
        );
        self
    }
}
