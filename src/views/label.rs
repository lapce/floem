use std::{any::Any, cell::RefCell, fmt::Display, mem::swap, rc::Rc};

use crate::{
    Clipboard,
    context::{EventCx, PaintCx, UpdateCx},
    event::{Event, EventPropagation},
    id::ViewId,
    prop_extractor,
    style::{
        CursorColor, CustomStylable, CustomStyle, FontProps, LineHeight, Selectable,
        SelectionCornerRadius, SelectionStyle, Style, TextAlignProp, TextColor, TextOverflow,
        TextOverflowProp,
    },
    style_class,
    text::{Attrs, AttrsList, FamilyOwned, TextLayout},
    view::View,
    view_storage::{MeasureFunction, NodeContext},
};
use floem_reactive::UpdaterEffect;
use floem_renderer::{
    Renderer,
    text::{Align, Cursor},
};
use peniko::{
    Brush,
    color::palette,
    kurbo::{Point, Rect},
};
use ui_events::{
    keyboard::{Key, KeyState, KeyboardEvent},
    pointer::{PointerButtonEvent, PointerEvent},
};
use understory_responder::types::Phase;

use super::{Decorators, TextCommand};

/// A reusable struct containing all layout-related data for text rendering.
/// This struct can be wrapped in Rc<RefCell<>> and shared between the taffy layout
/// function and other text rendering operations without needing to roundtrip through update.
#[derive(Clone)]
pub struct TextLayoutData {
    /// The base text layout created from the original text.
    /// This is always created with no width constraint and represents
    /// the natural, unwrapped size of the text.
    text_layout: Option<TextLayout>,
    /// The original text string
    original_text: String,
    /// The truncated text string used for ellipsis overflow.
    available_text: Option<String>,
    /// The width that was available for text rendering when available_text_layout was computed.
    available_width: Option<f32>,
    /// The computed text layout used for rendering when text overflows.
    available_text_layout: Option<TextLayout>,
    /// Cached attributes list for creating new text layouts
    attrs_list: AttrsList,
    /// Text alignment for layout
    text_align: Option<Align>,
    /// Text overflow behavior
    text_overflow: TextOverflow,
}

impl TextLayoutData {
    pub fn new() -> Self {
        Self {
            text_layout: None,
            original_text: String::new(),
            available_text: None,
            available_width: None,
            available_text_layout: None,
            attrs_list: AttrsList::new(Attrs::new()),
            text_align: None,
            text_overflow: TextOverflow::Clip,
        }
    }

    pub fn set_text(&mut self, text: &str, attrs_list: AttrsList, text_align: Option<Align>) {
        self.original_text = text.to_string();
        self.attrs_list = attrs_list.clone();
        self.text_align = text_align;

        let mut text_layout = TextLayout::new();
        text_layout.set_text(text, attrs_list, text_align);
        self.text_layout = Some(text_layout);

        // Clear overflow layouts when base text changes
        self.available_text = None;
        self.available_width = None;
        self.available_text_layout = None;
    }

    pub fn set_text_overflow(&mut self, text_overflow: crate::style::TextOverflow) {
        if self.text_overflow != text_overflow {
            self.text_overflow = text_overflow;
            // Clear cached overflow layouts when overflow mode changes
            self.available_text = None;
            self.available_width = None;
            self.available_text_layout = None;
        }
    }

    pub fn get_effective_text_layout(&self) -> Option<&TextLayout> {
        self.available_text_layout
            .as_ref()
            .or(self.text_layout.as_ref())
    }

    pub fn with_effective_text_layout<O>(&self, with: impl FnOnce(&TextLayout) -> O) -> O {
        if let Some(layout) = self.available_text_layout.as_ref() {
            with(layout)
        } else {
            with(self.text_layout.as_ref().unwrap_or(&TextLayout::new()))
        }
    }

    pub fn clear_overflow_state(&mut self) {
        self.available_text = None;
        self.available_width = None;
        self.available_text_layout = None;
    }

    pub fn get_text_layout(&self) -> Option<&TextLayout> {
        self.text_layout.as_ref()
    }

    /// Compute what the overflow size would be without mutating visible state.
    /// Temporarily modifies text_layout size but restores it after.
    pub fn compute_overflow_size(
        &mut self,
        available_width: f32,
        text_overflow: TextOverflow,
    ) -> peniko::kurbo::Size {
        let Some(text_layout) = self.text_layout.as_mut() else {
            return peniko::kurbo::Size::new(0.0, 14.0);
        };

        match text_overflow {
            TextOverflow::Ellipsis => {
                let mut dots_text = TextLayout::new();
                dots_text.set_text("...", self.attrs_list.clone(), self.text_align);
                let dots_width = dots_text.size().width as f32;
                let width_left = available_width - dots_width;

                let hit_point = text_layout.hit_point(Point::new(width_left as f64, 0.0));
                let index = hit_point.index;

                let new_text = if index > 0 {
                    format!("{}...", &self.original_text[..index])
                } else {
                    "".to_string()
                };

                let mut temp_layout = TextLayout::new();
                temp_layout.set_text(&new_text, self.attrs_list.clone(), self.text_align);
                temp_layout.size()
            }
            TextOverflow::Wrap => {
                text_layout.set_size(available_width, f32::MAX);
                let size = text_layout.size();
                text_layout.clear_size(); // Reset
                size
            }
            _ => peniko::kurbo::Size::new(available_width as f64, text_layout.size().height),
        }
    }

    /// Finalize the text layout for the given width.
    /// Called after taffy layout is complete with the actual final dimensions.
    pub fn finalize_for_width(&mut self, final_width: f32) {
        let Some(text_layout) = self.text_layout.as_ref() else {
            return;
        };

        let natural_width = text_layout.size().width as f32;
        let overflows = natural_width > final_width + 0.5;

        if !overflows {
            self.clear_overflow_state();
            return;
        }

        if self.available_width == Some(final_width) {
            return; // Already finalized for this width
        }

        match self.text_overflow {
            TextOverflow::Ellipsis => {
                let mut dots_text = TextLayout::new();
                dots_text.set_text("...", self.attrs_list.clone(), self.text_align);
                let dots_width = dots_text.size().width as f32;
                let width_left = final_width - dots_width;

                let hit_point = text_layout.hit_point(Point::new(width_left as f64, 0.0));
                let index = hit_point.index;

                let new_text = if index > 0 {
                    format!("{}...", &self.original_text[..index])
                } else {
                    "".to_string()
                };

                // Only create a new layout if the text actually changed
                if self.available_text.as_ref() != Some(&new_text) {
                    let mut layout = TextLayout::new();
                    layout.set_text(&new_text, self.attrs_list.clone(), self.text_align);
                    self.available_text = Some(new_text);
                    self.available_text_layout = Some(layout);
                }
                self.available_width = Some(final_width);
            }
            TextOverflow::Wrap => {
                // Reuse existing available_text_layout if we have one, just update size
                if let Some(ref mut layout) = self.available_text_layout {
                    layout.set_size(final_width, f32::MAX);
                } else {
                    // First time - clone from base layout
                    let mut layout = text_layout.clone();
                    layout.set_size(final_width, f32::MAX);
                    self.available_text_layout = Some(layout);
                }
                self.available_width = Some(final_width);
            }
            _ => {
                self.clear_overflow_state();
            }
        }
    }

    /// Create a taffy layout function that can be used with NodeContext::custom
    /// This function handles all ellipsis and wrap logic internally without requiring updates
    pub fn create_taffy_layout_fn(layout_data: Rc<RefCell<Self>>) -> Box<MeasureFunction> {
        Box::new(
            move |known_dimensions, available_space, node_id, _style, measure_ctx| {
                use taffy::*;

                // Mark for finalization - don't mutate here
                measure_ctx.needs_finalization(node_id);

                // Get text layout info
                let (has_text_layout, natural_size, text_overflow) = {
                    let layout_data = layout_data.borrow();
                    let has_text = layout_data.text_layout.is_some();
                    let size = layout_data
                        .text_layout
                        .as_ref()
                        .map(|tl| tl.size())
                        .unwrap_or_else(|| peniko::kurbo::Size::new(0.0, 14.0));
                    (has_text, size, layout_data.text_overflow)
                };

                if !has_text_layout {
                    return Size {
                        width: known_dimensions.width.unwrap_or(0.0),
                        height: known_dimensions.height.unwrap_or(14.0),
                    };
                }

                let natural_width = natural_size.width as f32;

                // Determine the effective width for layout
                let effective_width: Option<f32> = if let Some(w) = known_dimensions.width {
                    if w == 0.0 {
                        match available_space.height {
                            AvailableSpace::MinContent | AvailableSpace::MaxContent => None,
                            AvailableSpace::Definite(_) => Some(w),
                        }
                    } else {
                        Some(w)
                    }
                } else {
                    match available_space.width {
                        AvailableSpace::Definite(w) => Some(w),
                        AvailableSpace::MinContent => match text_overflow {
                            crate::style::TextOverflow::Wrap => None,
                            _ => None,
                        },
                        AvailableSpace::MaxContent => None,
                    }
                };

                // Calculate the actual text size based on effective width
                let text_size = if let Some(width) = effective_width {
                    let overflows = natural_width > width + 0.5;

                    if overflows {
                        // Just compute what the size would be
                        match text_overflow {
                            crate::style::TextOverflow::Ellipsis
                            | crate::style::TextOverflow::Wrap => {
                                let mut layout_data = layout_data.borrow_mut();
                                layout_data.compute_overflow_size(width, text_overflow)
                            }
                            _ => {
                                // Clip mode
                                peniko::kurbo::Size::new(width as f64, natural_size.height)
                            }
                        }
                    } else {
                        natural_size
                    }
                } else {
                    natural_size
                };

                Size {
                    width: known_dimensions.width.unwrap_or(text_size.width as f32),
                    height: known_dimensions.height.unwrap_or(text_size.height as f32),
                }
            },
        )
    }

    pub fn create_finalize_fn(
        layout_data: Rc<RefCell<Self>>,
    ) -> Box<dyn Fn(taffy::NodeId, taffy::Size<f32>)> {
        Box::new(move |_node_id, final_size| {
            let mut layout_data = layout_data.borrow_mut();
            layout_data.finalize_for_width(final_size.width);
        })
    }
}

impl Default for TextLayoutData {
    fn default() -> Self {
        Self::new()
    }
}

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
    /// Layout data containing text layouts and overflow handling logic
    layout_data: Rc<RefCell<TextLayoutData>>,
    text_overflow_listener: Option<TextOverflowListener>,
    selection_state: SelectionState,
    selection_range: Option<(Cursor, Cursor)>,
    selection_style: SelectionStyle,
    font: FontProps,
    style: Extractor,
}

impl Label {
    fn new(id: ViewId, label: String) -> Self {
        let layout_data = Rc::new(RefCell::new(TextLayoutData::new()));
        let mut label = Label {
            id,
            label,
            layout_data,
            text_overflow_listener: None,
            selection_state: SelectionState::None,
            selection_range: None,
            selection_style: Default::default(),
            font: FontProps::default(),
            style: Default::default(),
        };
        label.set_text_layout();
        label.set_taffy_layout();
        label.class(LabelClass)
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
    let initial_label = UpdaterEffect::new(
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
        let attrs_list = self.get_attrs_list();
        let align = self.style.text_align();
        let text_overflow = self.style.text_overflow();

        let mut layout_data = self.layout_data.borrow_mut();
        layout_data.set_text(&self.label, attrs_list, align);
        layout_data.set_text_overflow(text_overflow);

        let _ = self.id.mark_view_layout_dirty();
    }

    fn get_hit_point(&self, point: Point) -> Option<Cursor> {
        let location = self.id.content_rect_local().origin();
        self.with_effective_text_layout(|l| {
            l.hit(
                point.x as f32 - location.x as f32,
                // TODO: prevent cursor incorrectly going to end of buffer when clicking
                // slightly below the text
                point.y as f32 - location.y as f32,
            )
        })
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
                if let Some((start_c, end_c)) = &self.selection_range {
                    let layout_data = self.layout_data.borrow();
                    if let Some(text_layout) = layout_data.get_text_layout() {
                        let start_line_idx = text_layout.lines_range()[start_c.line].start;
                        let end_line_idx = text_layout.lines_range()[end_c.line].start;
                        let start_idx = start_line_idx + start_c.index;
                        let end_idx = end_line_idx + end_c.index;
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

    fn paint_selection(&self, text_layout: &TextLayout, paint_cx: &mut PaintCx) {
        if let Some((start_c, end_c)) = &self.selection_range {
            let location = self.id.content_rect_local().origin();
            let ss = &self.selection_style;
            let selection_color = ss.selection_color();

            for run in text_layout.layout_runs() {
                if let Some((mut start_x, width)) = run.highlight(*start_c, *end_c) {
                    start_x += location.x as f32;
                    let end_x = width + start_x;
                    let start_y = location.y + run.line_top as f64;
                    let end_y = start_y + run.line_height as f64;
                    let rect = Rect::new(start_x.into(), start_y, end_x.into(), end_y)
                        .to_rounded_rect(ss.corner_radius());
                    paint_cx.fill(&rect, &selection_color, 0.0);
                }
            }
        }
    }

    fn set_taffy_layout(&mut self) {
        let taffy = self.id.taffy();
        let taffy_node = self.id.taffy_node();
        let mut taffy = taffy.borrow_mut();

        let layout_fn = TextLayoutData::create_taffy_layout_fn(self.layout_data.clone());
        let finalize_fn = TextLayoutData::create_finalize_fn(self.layout_data.clone());

        let _ = taffy.set_node_context(
            taffy_node,
            Some(NodeContext::Custom {
                measure: layout_fn,
                finalize: Some(finalize_fn),
            }),
        );
    }
}

impl View for Label {
    fn id(&self) -> ViewId {
        self.id
    }

    fn view_style(&self) -> Option<Style> {
        Some(Style::new().min_width(0.))
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        format!("Label: {:?}", self.label).into()
    }

    fn event(&mut self, _cx: &mut EventCx, event: &Event, phase: Phase) -> EventPropagation {
        if phase != Phase::Target {
            return EventPropagation::Continue;
        }

        match event {
            Event::Pointer(PointerEvent::Down(PointerButtonEvent { state, .. })) => {
                if self.style.text_selectable() {
                    self.selection_range = None;
                    self.selection_state = SelectionState::Ready(state.logical_point());
                    self.id.request_paint();
                }
            }
            Event::Pointer(PointerEvent::Move(pu)) => {
                if !self.style.text_selectable() {
                    if self.selection_range.is_some() {
                        self.selection_state = SelectionState::None;
                        self.selection_range = None;
                        self.id.request_paint();
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
                        self.set_selection_range();
                        self.id.request_active();
                        self.id.request_paint();
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
                self.id.clear_active();
                self.id.request_paint();
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
            self.layout_data.borrow_mut().clear_overflow_state();
            self.set_text_layout();
            cx.window_state.schedule_layout();
        }
        if self.selection_style.read(cx) {
            self.id.request_paint();
        }
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) {
        if state.is::<String>() {
            if let Ok(state) = state.downcast::<String>() {
                self.label = *state;
                self.layout_data.borrow_mut().clear_overflow_state();
                self.set_text_layout();
                cx.window_state.schedule_layout();
            }
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if self.label.is_empty() {
            return;
        }

        let text_loc = self.id.content_rect_local().origin();

        self.with_effective_text_layout(|l| {
            cx.draw_text(l, text_loc);
            if cx.window_state.is_focused(&self.id()) {
                self.paint_selection(l, cx);
            }
        });
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
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
