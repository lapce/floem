#![deny(missing_docs)]
use crate::action::{exec_after, set_ime_allowed, set_ime_cursor_area};
use crate::context::LayoutCx;
use crate::event::{EventListener, EventPropagation};
use crate::id::ViewId;
use crate::reactive::{Effect, RwSignal};
use crate::style::{FontFamily, FontProps, SelectionStyle, TextAlignProp};
use crate::style::{FontStyle, FontWeight, TextColor};
use crate::unit::PxPct;
use crate::{Clipboard, WindowState, prop_extractor, style_class};
use floem_reactive::{SignalGet, SignalUpdate, SignalWith};

use floem_renderer::Renderer;
use ui_events::keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey};
use ui_events::pointer::{PointerButton, PointerButtonEvent, PointerEvent};
use unicode_segmentation::UnicodeSegmentation;

use crate::{peniko::color::palette, view::View};

use std::{any::Any, ops::Range};

#[derive(Clone)]
/// An IME preedit
pub struct Preedit {
    /// preedit string
    pub text: String,
    /// where the cursor is for the preedit
    pub cursor: Option<(usize, usize)>,
    /// the offset
    pub offset: usize,
}

use crate::{
    style::TextOverflow,
    text::{Attrs, AttrsList, FamilyOwned},
    view_storage::NodeContext,
};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

use peniko::Brush;
use peniko::kurbo::{Point, Rect, Size};
use std::{cell::RefCell, rc::Rc};
use understory_responder::types::Phase;

use crate::{
    context::{EventCx, UpdateCx},
    event::Event,
};

use super::{Decorators, label::TextLayoutData};

style_class!(
    /// The style class that is applied to all `TextInput` views.
    pub TextInputClass
);
style_class!(
    /// The style class that is applied to the placeholder `TextInput` text.
    pub PlaceholderTextClass
);

prop_extractor! {
    Extractor {
        color: TextColor,
        text_align: TextAlignProp,
    }
}

prop_extractor! {
    PlaceholderStyle {
        pub color: TextColor,
        //TODO: pub font_size: FontSize,
        pub font_weight: FontWeight,
        pub font_style: FontStyle,
        pub font_family: FontFamily,
        pub text_align: TextAlignProp,
    }
}

/// Holds text buffer of InputText view.
struct BufferState {
    buffer: RwSignal<String>,
    last_buffer: String,
}

impl BufferState {
    fn update(&mut self, update: impl FnOnce(&mut String)) {
        self.buffer.update(|s| {
            update(s);
            self.last_buffer.clone_from(s);
        });
    }

    fn get_untracked(&self) -> String {
        self.buffer.get_untracked()
    }

    fn with_untracked<T>(&self, f: impl FnOnce(&String) -> T) -> T {
        self.buffer.with_untracked(f)
    }
}

/// Text Input View.
pub struct TextInput {
    id: ViewId,
    buffer: BufferState,
    /// Optional text shown when the text input buffer is empty.
    placeholder_text: Option<String>,
    on_enter: Option<Box<dyn Fn()>>,
    placeholder_style: PlaceholderStyle,
    selection_style: SelectionStyle,
    preedit: Option<Preedit>,
    // Index of where are we in the main buffer.
    cursor_glyph_idx: usize,
    // This can be retrieved from the glyph, but we store it for efficiency.
    cursor_x: f64,
    /// Layout data containing text layouts and overflow handling logic
    layout_data: Rc<RefCell<TextLayoutData>>,
    /// Horizontal scroll offset for text that overflows the visible area
    scroll_offset: f64,
    selection: Option<Range<usize>>,
    // Approx max size of a glyph, given the current font weight & size.
    glyph_max_size: Size,
    style: Extractor,
    font: FontProps,
    cursor_width: f64, // TODO: make this configurable
    is_focused: bool,
    last_pointer_down: Point,
    last_cursor_action_on: Instant,
    window_origin: Option<Point>,
    last_ime_cursor_area: Option<(Point, Size)>,
}

/// Type of cursor movement in navigation.
#[derive(Clone, Copy, Debug)]
pub enum Movement {
    /// Move by a glyph.
    Glyph,
    /// Move by a word.
    Word,
    /// Move by a line.
    Line,
}

/// Type of text direction in the file.
#[derive(Clone, Copy, Debug)]
pub enum TextDirection {
    /// Text direction from left to right.
    Left,
    /// Text direction from right to left.
    Right,
}

/// Creates a [TextInput] view. This can be used for basic text input.
/// ### Examples
/// ```rust
/// # use floem::prelude::*;
/// # use floem::prelude::palette::css;
/// # use floem::text::Weight;
/// # use floem::style::SelectionCornerRadius;
/// // Create empty `String` as a text buffer in the read-write signal
/// let text = RwSignal::new(String::new());
/// // Create simple text imput from it
/// let simple = text_input(text)
///     // Optional placeholder text
///     .placeholder("Placeholder text")
///     // Width of the text widget
///     .style(|s| s
///         .width(250.)
///         // Enable keyboard navigation on the widget
///         .focusable(true)
///      );
///
/// // Stylized text example:
/// let stylized = text_input(text)
///     .placeholder("Placeholder text")
///     .style(|s| s
///         .border(1.5)
///         .width(250.0)
///         .background(css::LIGHT_GRAY)
///         .border_radius(15.0)
///         .border_color(css::DIM_GRAY)
///         .padding(10.0)
///         // Styles applied on widget pointer hover.
///         .hover(|s| s.background(css::LIGHT_GRAY.multiply_alpha(0.5)).border_color(css::DARK_GRAY))
///         .set(SelectionCornerRadius, 4.0)
///         // Styles applied when widget holds the focus.
///         .focus(|s| s
///             .border_color(css::SKY_BLUE)
///             // Styles applied on widget pointer hover when focused.
///             .hover(|s| s.border_color(css::SKY_BLUE))
///         )
///         // Apply class and override some of its styles.
///         .class(PlaceholderTextClass, |s| s
///             .color(css::SKY_BLUE)
///             .font_style(floem::text::Style::Italic)
///             .font_weight(Weight::BOLD)
///         )
///         .font_family("monospace".to_owned())
///         .focusable(true)
///     );
/// ```
/// ### Reactivity
/// The view is reactive and will track updates on buffer signal.
/// ### Info
/// For more advanced editing see [TextEditor](super::text_editor::TextEditor).
pub fn text_input(buffer: RwSignal<String>) -> TextInput {
    let id = ViewId::new();
    let is_focused = RwSignal::new(false);

    {
        Effect::new(move |_| {
            // subscribe to changes without cloning string
            buffer.with(|_| {});
            id.update_state(is_focused.get());
        });
    }

    let mut text_input = TextInput {
        id,
        cursor_glyph_idx: 0,
        placeholder_text: None,
        placeholder_style: Default::default(),
        selection_style: Default::default(),
        preedit: None,
        buffer: BufferState {
            buffer,
            last_buffer: buffer.get_untracked(),
        },
        layout_data: Rc::new(RefCell::new(TextLayoutData::new())),
        style: Default::default(),
        font: FontProps::default(),
        cursor_x: 0.0,
        selection: None,
        glyph_max_size: Size::ZERO,
        scroll_offset: 0.0,
        cursor_width: 1.0,
        is_focused: false,
        last_pointer_down: Point::ZERO,
        last_cursor_action_on: Instant::now(),
        on_enter: None,
        window_origin: None,
        last_ime_cursor_area: None,
    };

    text_input.update_text_layout();
    text_input.set_taffy_layout();
    text_input
        .on_event_stop(EventListener::FocusGained, move |_, _| {
            is_focused.set(true);
            set_ime_allowed(true);
        })
        .on_event_stop(EventListener::FocusLost, move |_, _| {
            is_focused.set(false);
            set_ime_allowed(false);
        })
        .class(TextInputClass)
}

pub(crate) enum TextCommand {
    SelectAll,
    Copy,
    Paste,
    Cut,
    None,
}
use ui_events::keyboard::Code;

impl From<&KeyboardEvent> for TextCommand {
    fn from(event: &KeyboardEvent) -> Self {
        #[cfg(target_os = "macos")]
        match (event.modifiers, event.code) {
            (Modifiers::META, Code::KeyA) => Self::SelectAll,
            (Modifiers::META, Code::KeyC) => Self::Copy,
            (Modifiers::META, Code::KeyX) => Self::Cut,
            (Modifiers::META, Code::KeyV) => Self::Paste,
            _ => Self::None,
        }
        #[cfg(not(target_os = "macos"))]
        match (event.modifiers, event.code) {
            (Modifiers::CONTROL, Code::KeyA) => Self::SelectAll,
            (Modifiers::CONTROL, Code::KeyC) => Self::Copy,
            (Modifiers::CONTROL, Code::KeyX) => Self::Cut,
            (Modifiers::CONTROL, Code::KeyV) => Self::Paste,
            _ => Self::None,
        }
    }
}

/// Determines if motion should be word based.
fn get_word_based_motion(event: &KeyboardEvent) -> Option<Movement> {
    #[cfg(not(target_os = "macos"))]
    return event
        .modifiers
        .contains(Modifiers::CONTROL)
        .then_some(Movement::Word);

    #[cfg(target_os = "macos")]
    return event
        .modifiers
        .contains(Modifiers::ALT)
        .then_some(Movement::Word)
        .or(event
            .modifiers
            .contains(Modifiers::META)
            .then_some(Movement::Line));
}

const DEFAULT_FONT_SIZE: f32 = 14.0;
const CURSOR_BLINK_INTERVAL_MS: u64 = 500;

impl TextInput {
    /// Add placeholder text visible when buffer is empty.
    /// ```
    /// # use floem::views::text_input;
    /// # use floem_reactive::RwSignal;
    /// let text = RwSignal::new(String::new());
    /// let simple = text_input(text)
    ///     // Optional placeholder text
    ///     .placeholder("Placeholder text");
    /// ```
    /// ### Reactivity
    /// This method is not reactive.
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder_text = Some(text.into());
        self
    }

    /// Add action that will run on `Enter` key press.
    ///
    /// Useful for submitting forms using a keyboard.
    /// ```
    /// # use floem::views::text_input;
    /// # use floem_reactive::RwSignal;
    /// # use floem_reactive::SignalGet;
    /// let form = RwSignal::new(String::new());
    /// text_input(form)
    ///     .placeholder("fill the form")
    ///     .on_enter(move || { format!("Form {} submitted!", form.get_untracked()); });
    /// ``````
    /// ### Reactivity
    /// This method is not reactive, but will always run provided function
    /// when pressed `Enter`.
    pub fn on_enter(mut self, action: impl Fn() + 'static) -> Self {
        self.on_enter = Some(Box::new(action));
        self
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

impl TextInput {
    fn move_cursor(&mut self, move_kind: Movement, direction: TextDirection) -> bool {
        match (move_kind, direction) {
            (Movement::Glyph, TextDirection::Left) => {
                let untracked_buffer = self.buffer.get_untracked();
                let mut grapheme_iter = untracked_buffer[..self.cursor_glyph_idx].graphemes(true);
                match grapheme_iter.next_back() {
                    None => false,
                    Some(prev_character) => {
                        self.cursor_glyph_idx -= prev_character.len();
                        true
                    }
                }
            }
            (Movement::Glyph, TextDirection::Right) => {
                let untracked_buffer = self.buffer.get_untracked();
                let mut grapheme_iter = untracked_buffer[self.cursor_glyph_idx..].graphemes(true);
                match grapheme_iter.next() {
                    None => false,
                    Some(next_character) => {
                        self.cursor_glyph_idx += next_character.len();
                        true
                    }
                }
            }
            (Movement::Line, TextDirection::Right) => {
                if self.cursor_glyph_idx < self.buffer.with_untracked(|buff| buff.len()) {
                    self.cursor_glyph_idx = self.buffer.with_untracked(|buff| buff.len());
                    return true;
                }
                false
            }
            (Movement::Line, TextDirection::Left) => {
                if self.cursor_glyph_idx > 0 {
                    self.cursor_glyph_idx = 0;
                    return true;
                }
                false
            }
            (Movement::Word, TextDirection::Right) => self.buffer.with_untracked(|buff| {
                for (idx, word) in buff.unicode_word_indices() {
                    let word_end_idx = idx + word.len();
                    if word_end_idx > self.cursor_glyph_idx {
                        self.cursor_glyph_idx = word_end_idx;
                        return true;
                    }
                }
                false
            }),
            (Movement::Word, TextDirection::Left) if self.cursor_glyph_idx > 0 => {
                self.buffer.with_untracked(|buff| {
                    let mut prev_word_idx = 0;
                    for (idx, _) in buff.unicode_word_indices() {
                        if idx < self.cursor_glyph_idx {
                            prev_word_idx = idx;
                        } else {
                            break;
                        }
                    }
                    self.cursor_glyph_idx = prev_word_idx;
                    true
                })
            }
            (_movement, _dir) => false,
        }
    }

    fn cursor_visual_idx(&self) -> usize {
        let Some(preedit) = &self.preedit else {
            return self.cursor_glyph_idx;
        };

        let Some(cursor) = preedit.cursor else {
            return self.cursor_glyph_idx + preedit.text.len();
        };

        self.cursor_glyph_idx + cursor.0
    }

    fn calculate_scroll_offset(&mut self, lcx: &mut LayoutCx) {
        // Just call the unified update method
        self.update_cursor_position_and_scroll(lcx);
    }

    fn get_cursor_rect(&self, lcx: &mut LayoutCx) -> Rect {
        let node_location = lcx.content_rect_local().origin();
        let text_height = self.glyph_max_size.height;

        let cursor_start = Point::new(
            self.cursor_x - self.scroll_offset + node_location.x,
            node_location.y,
        );

        Rect::from_points(
            cursor_start,
            Point::new(
                cursor_start.x + self.cursor_width,
                node_location.y + text_height,
            ),
        )
    }

    fn scroll(&mut self, offset: f64, visible_width: f64) {
        self.scroll_offset += offset;

        // Get text width from layout_data
        let text_width = self
            .layout_data
            .borrow()
            .with_effective_text_layout(|layout| layout.size().width);

        let max_scroll = (text_width - visible_width).max(0.0);
        self.scroll_offset = self.scroll_offset.max(0.0).min(max_scroll);
    }

    fn handle_double_click(&mut self, pos_x: f64) {
        let clicked_glyph_idx = self.get_box_position(pos_x);

        self.buffer.with_untracked(|buff| {
            let selection = get_dbl_click_selection(clicked_glyph_idx, buff);
            if selection.start != selection.end {
                self.cursor_glyph_idx = selection.end;
                self.selection = Some(selection);
            }
        })
    }

    fn handle_triple_click(&mut self) {
        self.select_all();
    }

    fn get_box_position(&self, pos_x: f64) -> usize {
        let layout = self.id.layout().unwrap_or_default();
        let view_state = self.id.state();
        let view_state = view_state.borrow();
        let style = view_state.combined_style.builtin();

        let padding_left = match style.padding_left() {
            PxPct::Px(padding) => padding as f32,
            PxPct::Pct(pct) => pct as f32 * layout.size.width,
        };

        self.layout_data
            .borrow()
            .with_effective_text_layout(|layout| {
                layout
                    .hit_point(Point::new(
                        pos_x + self.scroll_offset - padding_left as f64,
                        0.0,
                    ))
                    .index
            })
    }

    fn get_selection_rect(&self, lcx: &mut LayoutCx, left_padding: f64) -> Rect {
        let selection = if let Some(curr_selection) = &self.selection {
            curr_selection.clone()
        } else if let Some(cursor) = self.preedit.as_ref().and_then(|p| p.cursor) {
            self.cursor_glyph_idx + cursor.0..self.cursor_glyph_idx + cursor.1
        } else {
            return Rect::ZERO;
        };

        let text_height = self.glyph_max_size.height;
        let node_rect = lcx.content_rect_local();
        let visible_width = node_rect.size().width;

        let (selection_start_x, selection_end_x) = self
            .layout_data
            .borrow()
            .with_effective_text_layout(|layout| {
                let start_x = layout.hit_position(selection.start).point.x - self.scroll_offset;
                let end_x =
                    layout.hit_position(selection.end).point.x + left_padding - self.scroll_offset;
                (start_x, end_x)
            });

        let selection_start_x = selection_start_x.max(node_rect.origin().x - left_padding);
        let selection_end_x = selection_end_x.min(selection_start_x + visible_width + left_padding);

        let selection_start = Point::new(
            selection_start_x + node_rect.origin().x,
            node_rect.origin().y,
        );

        Rect::from_points(
            selection_start,
            Point::new(selection_end_x, selection_start.y + text_height),
        )
    }

    /// Determine approximate max size of a single glyph, given the current font weight & size
    fn get_font_glyph_max_size(&self) -> Size {
        let mut tmp = TextLayoutData::new();
        let attrs_list = self.get_text_attrs();
        let align = self.style.text_align();
        tmp.set_text("W", attrs_list, align);
        if let Some(text_layout) = tmp.get_text_layout() {
            text_layout.size() + Size::new(0., text_layout.hit_position(0).glyph_descent)
        } else {
            Size::new(8.0, DEFAULT_FONT_SIZE as f64) // fallback size
        }
    }

    fn update_selection(&mut self, selection_start: usize, selection_end: usize) {
        self.selection = match selection_start.cmp(&selection_end) {
            std::cmp::Ordering::Less => Some(Range {
                start: selection_start,
                end: selection_end,
            }),
            std::cmp::Ordering::Greater => Some(Range {
                start: selection_end,
                end: selection_start,
            }),
            std::cmp::Ordering::Equal => None,
        };
    }

    fn update_ime_cursor_area(&mut self) {
        if !self.is_focused {
            return;
        }

        let (Some(layout), Some(origin)) = (self.id.layout(), self.window_origin) else {
            return;
        };

        let left_padding = layout.border.left + layout.padding.left;
        let top_padding = layout.border.top + layout.padding.top;

        let pos = Point::new(
            origin.x + self.cursor_x - self.scroll_offset + left_padding as f64,
            origin.y + top_padding as f64,
        );

        let width = self
            .preedit
            .as_ref()
            .map(|preedit| {
                let start_idx = preedit.offset;
                let end_idx = start_idx + preedit.text.len();

                self.layout_data
                    .borrow()
                    .with_effective_text_layout(|text_layout| {
                        let start_x = text_layout.hit_position(start_idx).point.x;
                        let end_x = text_layout.hit_position(end_idx).point.x;
                        (end_x - start_x).abs()
                    })
            })
            .unwrap_or_default();

        let size = Size::new(width, layout.content_box_height() as f64);

        if self.last_ime_cursor_area != Some((pos, size)) {
            set_ime_cursor_area(pos, size);
            self.last_ime_cursor_area = Some((pos, size));
        }
    }

    fn commit_preedit(&mut self) -> bool {
        if let Some(preedit) = self.preedit.take() {
            self.buffer
                .update(|buf| buf.insert_str(self.cursor_glyph_idx, &preedit.text));

            if self.is_focused {
                // toggle IME to flush external preedit state
                set_ime_allowed(false);
                set_ime_allowed(true);
                // ime area will be set in compute_layout
            }

            self.update_text_layout();
            true
        } else {
            false
        }
    }

    fn update_text_layout(&mut self) {
        let glyph_max_size = self.get_font_glyph_max_size();
        self.glyph_max_size = glyph_max_size;

        let buffer_is_empty = self.buffer.with_untracked(|buff| {
            buff.is_empty() && self.preedit.as_ref().is_none_or(|p| p.text.is_empty())
        });

        if let (Some(placeholder_text), true) = (&self.placeholder_text, buffer_is_empty) {
            let attrs_list = self.get_placeholder_text_attrs();
            let align = self.placeholder_style.text_align();

            let mut layout_data = self.layout_data.borrow_mut();
            layout_data.set_text(placeholder_text, attrs_list, align);
            layout_data.set_text_overflow(TextOverflow::Clip);
        } else {
            let attrs_list = self.get_text_attrs();
            let align = self.style.text_align();
            self.buffer.with_untracked(|buff| {
                let preedited;
                let display_text = if let Some(preedit) = &self.preedit {
                    let preedit_offset = self.cursor_glyph_idx.min(buff.len());

                    preedited = [
                        &buff[..preedit_offset],
                        &preedit.text,
                        &buff[preedit_offset..],
                    ]
                    .concat();

                    &preedited
                } else {
                    buff
                };

                let mut layout_data = self.layout_data.borrow_mut();
                layout_data.set_text(display_text, attrs_list, align);
                layout_data.set_text_overflow(TextOverflow::Clip);
            });
        }
    }

    fn update_cursor_position_and_scroll(&mut self, lcx: &mut LayoutCx) {
        // Update cursor position from the text layout
        let cursor_glyph_pos = self
            .layout_data
            .borrow()
            .with_effective_text_layout(|layout| layout.hit_position(self.cursor_visual_idx()));
        self.cursor_x = cursor_glyph_pos.point.x;

        // Update scroll offset using layout context
        let visible_width = lcx.content_rect_local().size().width;
        const SCROLL_PADDING: f64 = 10.0;

        let mut scroll_offset = self.scroll_offset;

        // If cursor is too far left, scroll left
        if self.cursor_x < scroll_offset + SCROLL_PADDING {
            scroll_offset = (self.cursor_x - SCROLL_PADDING).max(0.0);
        }
        // If cursor is too far right, scroll right
        else if self.cursor_x > scroll_offset + visible_width - SCROLL_PADDING {
            scroll_offset = self.cursor_x - visible_width + SCROLL_PADDING;
        }

        // Constrain scroll to valid range
        let text_width = self
            .layout_data
            .borrow()
            .with_effective_text_layout(|layout| layout.size().width);
        let max_scroll = (text_width - visible_width).max(0.0);
        self.scroll_offset = scroll_offset.max(0.0).min(max_scroll);
    }

    fn font_size(&self) -> f32 {
        self.font.size().unwrap_or(DEFAULT_FONT_SIZE)
    }

    /// Retrieve attributes for the placeholder text.
    pub fn get_placeholder_text_attrs(&self) -> AttrsList {
        let mut attrs = Attrs::new().color(
            self.placeholder_style
                .color()
                .unwrap_or(palette::css::BLACK),
        );

        //TODO:
        // self.placeholder_style
        //     .font_size()
        //     .unwrap_or(self.font_size())
        attrs = attrs.font_size(self.font_size());

        if let Some(font_style) = self.placeholder_style.font_style() {
            attrs = attrs.style(font_style);
        } else if let Some(font_style) = self.font.style() {
            attrs = attrs.style(font_style);
        }

        if let Some(font_weight) = self.placeholder_style.font_weight() {
            attrs = attrs.weight(font_weight);
        } else if let Some(font_weight) = self.font.weight() {
            attrs = attrs.weight(font_weight);
        }

        let input_font_family = self.font.family().as_ref().map(|font_family| {
            let family: Vec<FamilyOwned> = FamilyOwned::parse_list(font_family).collect();
            family
        });

        let placeholder_font_family =
            self.placeholder_style
                .font_family()
                .as_ref()
                .map(|font_family| {
                    let family: Vec<FamilyOwned> = FamilyOwned::parse_list(font_family).collect();
                    family
                });

        // Inherit the font family of the text input unless overridden by the placeholder
        if let Some(font_family) = placeholder_font_family.as_ref() {
            attrs = attrs.family(font_family);
        } else if let Some(font_family) = input_font_family.as_ref() {
            attrs = attrs.family(font_family);
        }

        AttrsList::new(attrs)
    }

    /// Retrieve attributes for the text.
    pub fn get_text_attrs(&self) -> AttrsList {
        let mut attrs = Attrs::new().color(self.style.color().unwrap_or(palette::css::BLACK));

        attrs = attrs.font_size(self.font_size());

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
        AttrsList::new(attrs)
    }

    /// Select all text in the buffer.
    fn select_all(&mut self) {
        let len = self.buffer.with_untracked(|val| val.len());

        if len == 0 {
            return;
        }

        self.cursor_glyph_idx = len;

        // Update cursor position and scroll
        let mut lcx = LayoutCx::new(self.id);
        self.update_cursor_position_and_scroll(&mut lcx);

        self.selection = Some(0..len);
    }

    fn handle_modifier_cmd(&mut self, event: &KeyboardEvent) -> bool {
        if event.modifiers.is_empty() || event.modifiers == Modifiers::SHIFT {
            return false;
        }

        let command = event.into();

        match command {
            TextCommand::SelectAll => {
                self.select_all();
                true
            }
            TextCommand::Copy => {
                if let Some(selection) = &self.selection {
                    let selection_txt = self
                        .buffer
                        .get_untracked()
                        .chars()
                        .skip(selection.start)
                        .take(selection.end - selection.start)
                        .collect();
                    let _ = Clipboard::set_contents(selection_txt);
                }
                true
            }
            TextCommand::Cut => {
                if let Some(selection) = &self.selection {
                    let selection_txt = self
                        .buffer
                        .get_untracked()
                        .chars()
                        .skip(selection.start)
                        .take(selection.end - selection.start)
                        .collect();
                    let _ = Clipboard::set_contents(selection_txt);

                    self.buffer
                        .update(|buf| replace_range(buf, selection.clone(), None));

                    self.cursor_glyph_idx = selection.start;
                    self.selection = None;
                }

                true
            }
            TextCommand::Paste => {
                let mut clipboard_content = match Clipboard::get_contents() {
                    Ok(content) => content,
                    Err(_) => return false,
                };

                clipboard_content.retain(|c| c != '\r' && c != '\n');

                if clipboard_content.is_empty() {
                    return false;
                }

                if let Some(selection) = self.selection.take() {
                    self.buffer.update(|buf| {
                        replace_range(buf, selection.clone(), Some(&clipboard_content))
                    });

                    self.cursor_glyph_idx = selection.start + clipboard_content.len();
                } else {
                    self.buffer
                        .update(|buf| buf.insert_str(self.cursor_glyph_idx, &clipboard_content));
                    self.cursor_glyph_idx += clipboard_content.len();
                }

                true
            }
            TextCommand::None => {
                self.selection = None;
                false
            }
        }
    }

    fn handle_key_down(&mut self, window_state: &mut WindowState, event: &KeyboardEvent) -> bool {
        let handled = match event.key {
            Key::Character(ref c) if c == " " => {
                if let Some(selection) = &self.selection {
                    self.buffer
                        .update(|buf| replace_range(buf, selection.clone(), None));
                    self.cursor_glyph_idx = selection.start;
                    self.selection = None;
                } else {
                    self.buffer
                        .update(|buf| buf.insert(self.cursor_glyph_idx, ' '));
                }
                self.move_cursor(Movement::Glyph, TextDirection::Right)
            }
            Key::Named(NamedKey::Backspace) => {
                let selection = self.selection.clone();
                if let Some(selection) = selection {
                    self.cursor_glyph_idx = selection.start;
                    self.buffer
                        .update(|buf| replace_range(buf, selection, None));
                    self.selection = None;
                    true
                } else {
                    let prev_cursor_idx = self.cursor_glyph_idx;

                    self.move_cursor(
                        get_word_based_motion(event).unwrap_or(Movement::Glyph),
                        TextDirection::Left,
                    );

                    if self.cursor_glyph_idx == prev_cursor_idx {
                        return false;
                    }

                    self.buffer.update(|buf| {
                        replace_range(buf, self.cursor_glyph_idx..prev_cursor_idx, None);
                    });
                    true
                }
            }
            Key::Named(NamedKey::Delete) => {
                let selection = self.selection.clone();
                if let Some(selection) = selection {
                    self.cursor_glyph_idx = selection.start;
                    self.buffer
                        .update(|buf| replace_range(buf, selection, None));
                    self.selection = None;
                    return true;
                }

                let prev_cursor_idx = self.cursor_glyph_idx;

                self.move_cursor(
                    get_word_based_motion(event).unwrap_or(Movement::Glyph),
                    TextDirection::Right,
                );

                if self.cursor_glyph_idx == prev_cursor_idx {
                    return false;
                }

                self.buffer.update(|buf| {
                    replace_range(buf, prev_cursor_idx..self.cursor_glyph_idx, None);
                });

                self.cursor_glyph_idx = prev_cursor_idx;
                true
            }
            Key::Named(NamedKey::Escape) => {
                window_state.clear_focus();
                true
            }
            Key::Named(NamedKey::Enter) => {
                if let Some(action) = &self.on_enter {
                    action();
                }
                true
            }
            Key::Named(NamedKey::End) => {
                if event.modifiers.contains(Modifiers::SHIFT) {
                    match &self.selection {
                        Some(selection_value) => self.update_selection(
                            selection_value.start,
                            self.buffer.get_untracked().len(),
                        ),
                        None => self.update_selection(
                            self.cursor_glyph_idx,
                            self.buffer.get_untracked().len(),
                        ),
                    }
                } else {
                    self.selection = None;
                }
                self.move_cursor(Movement::Line, TextDirection::Right)
            }
            Key::Named(NamedKey::Home) => {
                if event.modifiers.contains(Modifiers::SHIFT) {
                    match &self.selection {
                        Some(selection_value) => self.update_selection(0, selection_value.end),
                        None => self.update_selection(0, self.cursor_glyph_idx),
                    }
                } else {
                    self.selection = None;
                }
                self.move_cursor(Movement::Line, TextDirection::Left)
            }
            Key::Named(NamedKey::ArrowLeft) => {
                let old_glyph_idx = self.cursor_glyph_idx;

                let move_kind = get_word_based_motion(event).unwrap_or(Movement::Glyph);
                let cursor_moved = self.move_cursor(move_kind, TextDirection::Left);

                if event.modifiers.contains(Modifiers::SHIFT) {
                    self.move_selection(old_glyph_idx, self.cursor_glyph_idx);
                } else if let Some(selection) = self.selection.take() {
                    // clear and jump to the start of the selection
                    if matches!(move_kind, Movement::Glyph) {
                        self.cursor_glyph_idx = selection.start;
                        window_state.request_paint(self.id);
                    }
                }

                cursor_moved
            }
            Key::Named(NamedKey::ArrowRight) => {
                let old_glyph_idx = self.cursor_glyph_idx;

                let move_kind = get_word_based_motion(event).unwrap_or(Movement::Glyph);
                let cursor_moved = self.move_cursor(move_kind, TextDirection::Right);

                if event.modifiers.contains(Modifiers::SHIFT) {
                    self.move_selection(old_glyph_idx, self.cursor_glyph_idx);
                } else if let Some(selection) = self.selection.take() {
                    // clear and jump to the end of the selection
                    if matches!(move_kind, Movement::Glyph) {
                        self.cursor_glyph_idx = selection.end;
                        window_state.request_paint(self.id);
                    }
                }

                cursor_moved
            }
            _ => false,
        };
        if handled {
            return true;
        }

        match event.key {
            Key::Character(ref ch) => {
                let handled_modifier_cmd = self.handle_modifier_cmd(event);
                if handled_modifier_cmd {
                    return true;
                }
                let non_shift_mask = Modifiers::all().difference(Modifiers::SHIFT);
                if event.modifiers.intersects(non_shift_mask) {
                    return false;
                }
                self.insert_text(ch)
            }
            _ => false,
        }
    }

    fn insert_text(&mut self, ch: &str) -> bool {
        let selection = self.selection.clone();
        if let Some(selection) = selection {
            self.buffer
                .update(|buf| replace_range(buf, selection.clone(), None));
            self.cursor_glyph_idx = selection.start;
            self.selection = None;
        }

        self.buffer
            .update(|buf| buf.insert_str(self.cursor_glyph_idx, ch));
        self.move_cursor(Movement::Glyph, TextDirection::Right)
    }

    fn move_selection(&mut self, old_glyph_idx: usize, curr_glyph_idx: usize) {
        let new_selection = if let Some(selection) = &self.selection {
            // we're making an assumption that the caret is at the selection's edge
            // the opposite edge will be our anchor
            let anchor = if selection.start == old_glyph_idx {
                selection.end
            } else {
                selection.start
            };

            if anchor < curr_glyph_idx {
                anchor..curr_glyph_idx
            } else {
                curr_glyph_idx..anchor
            }
        } else if old_glyph_idx < curr_glyph_idx {
            old_glyph_idx..curr_glyph_idx
        } else {
            curr_glyph_idx..old_glyph_idx
        };

        // avoid empty selection
        self.selection = if new_selection.is_empty() {
            None
        } else {
            Some(new_selection)
        };
    }

    fn paint_selection_rect(&self, lcx: &mut LayoutCx, cx: &mut crate::context::PaintCx<'_>) {
        let view_state = self.id.state();
        let view_state = view_state.borrow();
        let style = &view_state.combined_style;

        let cursor_color = self.selection_style.selection_color();

        let content_rect = lcx.content_rect_local();
        let padding_left = match style.builtin().padding_left() {
            PxPct::Px(padding) => padding,
            PxPct::Pct(pct) => pct / 100.0 * content_rect.size().width,
        };

        let border_radius = self.selection_style.corner_radius();
        let selection_rect = self
            .get_selection_rect(lcx, padding_left)
            .to_rounded_rect(border_radius);
        cx.save();
        cx.clip(&self.id.layout_rect_local());
        cx.fill(&selection_rect, &cursor_color, 0.0);
        cx.restore();
    }
}

fn replace_range(buff: &mut String, del_range: Range<usize>, replacement: Option<&str>) {
    assert!(del_range.start <= del_range.end);
    if !buff.is_char_boundary(del_range.end) {
        eprintln!("[Floem] Tried to delete range with invalid end: {del_range:?}");
        return;
    }

    if !buff.is_char_boundary(del_range.start) {
        eprintln!("[Floem] Tried to delete range with invalid start: {del_range:?}");
        return;
    }

    // Get text after range to delete
    let after_del_range = buff.split_off(del_range.end);

    // Truncate up to range's start to delete it
    buff.truncate(del_range.start);

    if let Some(repl) = replacement {
        buff.push_str(repl);
    }

    buff.push_str(&after_del_range);
}

fn get_dbl_click_selection(glyph_idx: usize, buffer: &str) -> Range<usize> {
    let mut selectable_ranges: Vec<Range<usize>> = Vec::new();
    let glyph_idx = usize::min(glyph_idx, buffer.len().saturating_sub(1));

    for (idx, word) in buffer.unicode_word_indices() {
        let word_range = idx..idx + word.len();

        if let Some(prev) = selectable_ranges.last() {
            if prev.end != idx {
                // non-alphanumeric char sequence between previous word and current word
                selectable_ranges.push(prev.end..idx);
            }
        } else if idx > 0 {
            // non-alphanumeric char sequence at the beginning of the buffer(before the first word)
            selectable_ranges.push(0..idx);
        }

        selectable_ranges.push(word_range);
    }

    // left-over non-alphanumeric char sequence at the end of the buffer(after the last word)
    if let Some(last) = selectable_ranges.last() {
        if last.end != buffer.len() {
            selectable_ranges.push(last.end..buffer.len());
        }
    }

    for range in selectable_ranges {
        if range.contains(&glyph_idx) {
            return range;
        }
    }

    // should reach here only if buffer does not contain any words(only non-alphanumeric characters)
    0..buffer.len()
}

impl View for TextInput {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        format!("TextInput: {:?}", self.buffer.get_untracked()).into()
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) {
        if let Ok(state) = state.downcast::<bool>() {
            let is_focused = *state;

            if self.is_focused != is_focused {
                self.is_focused = is_focused;
                self.last_ime_cursor_area = None;

                self.commit_preedit();
                self.update_ime_cursor_area();

                if is_focused && !cx.window_state.is_active(self.id) {
                    self.selection = None;
                    self.cursor_glyph_idx = self.buffer.with_untracked(|buf| buf.len());
                }
            }

            // Only update recomputation if the state has actually changed
            let text_updated = self.buffer.buffer.with_untracked(|buf| {
                let updated = *buf != self.buffer.last_buffer;

                if updated {
                    self.buffer.last_buffer.clone_from(buf);
                }

                updated
            });

            if text_updated {
                self.update_text_layout();
                cx.window_state.request_layout();
            }
        } else {
            eprintln!("downcast failed");
        }
    }

    fn event(&mut self, cx: &mut EventCx) -> EventPropagation {
        if cx.phase != Phase::Target {
            return EventPropagation::Continue;
        }

        let buff_len = self.buffer.with_untracked(|buff| buff.len());
        // Workaround for cursor going out of bounds when text buffer is modified externally
        // TODO: find a better way to handle this
        if self.cursor_glyph_idx > buff_len {
            self.cursor_glyph_idx = buff_len;
        }

        let is_handled = match &cx.event {
            // match on pointer primary button press
            Event::Pointer(PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            })) => {
                cx.window_state.update_active(self.id);
                let point = state.logical_point();
                self.last_pointer_down = point;

                self.commit_preedit();

                if self.buffer.with_untracked(|buff| !buff.is_empty()) {
                    if state.count == 2 {
                        self.handle_double_click(point.x);
                    } else if state.count == 3 {
                        self.handle_triple_click();
                    } else {
                        self.cursor_glyph_idx = self.get_box_position(point.x);
                        self.selection = None;
                    }
                }
                true
            }
            Event::Pointer(PointerEvent::Move(pu)) => {
                if cx.window_state.is_active(self.id)
                    && self.buffer.with_untracked(|buff| !buff.is_empty())
                {
                    if self.commit_preedit() {
                        cx.window_state.request_layout();
                    }
                    let pos = pu.current.logical_point();

                    // Get visible width for scrolling using LayoutCx
                    let mut lcx = LayoutCx::new(self.id);
                    let content_rect = lcx.content_rect_local();
                    let visible_width = content_rect.size().width;

                    if pos.x < content_rect.x0 && pos.x < self.last_pointer_down.x {
                        self.scroll(pos.x, visible_width);
                    } else if pos.x > content_rect.x1 && pos.x > self.last_pointer_down.x {
                        self.scroll(pos.x - visible_width, visible_width);
                    }

                    let selection_stop = self.get_box_position(pos.x);
                    self.update_selection(self.cursor_glyph_idx, selection_stop);

                    cx.window_state.request_paint(self.id);
                }
                false
            }
            Event::Key(
                ke @ KeyboardEvent {
                    state: KeyState::Down,
                    ..
                },
            ) => self.handle_key_down(cx.window_state, ke),
            Event::ImePreedit { text, cursor } => {
                if self.is_focused && !text.is_empty() {
                    if let Some(selection) = self.selection.take() {
                        self.cursor_glyph_idx = selection.start;
                        self.buffer
                            .update(|buf| replace_range(buf, selection.clone(), None));
                    }

                    let mut preedit = self.preedit.take().unwrap_or_else(|| Preedit {
                        text: Default::default(),
                        cursor: None,
                        offset: 0,
                    });
                    preedit.text.clone_from(text);
                    preedit.cursor = *cursor;
                    self.preedit = Some(preedit);

                    true
                } else {
                    // clear preedit and queue UI update
                    self.preedit.take().is_some()
                }
            }
            Event::ImeCommit(text) => {
                if self.is_focused {
                    self.buffer
                        .update(|buf| buf.insert_str(self.cursor_glyph_idx, text));
                    self.cursor_glyph_idx += text.len();
                    self.preedit = None;

                    true
                } else {
                    false
                }
            }
            _ => false,
        };

        if is_handled {
            self.update_text_layout();
            // Update cursor position and scroll after text changes
            let mut lcx = LayoutCx::new(self.id);
            self.update_cursor_position_and_scroll(&mut lcx);
            self.id.request_layout();
            self.last_cursor_action_on = Instant::now();
        }

        if is_handled {
            EventPropagation::Stop
        } else {
            EventPropagation::Continue
        }
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        let style = cx.style();

        let placeholder_style = style.clone().apply_class(PlaceholderTextClass);
        self.placeholder_style.read_style(cx, &placeholder_style);

        if self.font.read(cx) {
            self.layout_data.borrow_mut().clear_overflow_state();
            self.update_text_layout();
            self.id.request_layout();
        }
        if self.style.read(cx) {
            cx.window_state.request_paint(self.id);

            // necessary to update the text layout attrs
            self.layout_data.borrow_mut().clear_overflow_state();
            self.update_text_layout();
        }

        self.selection_style.read_style(cx, &style);
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let mut lcx = LayoutCx::new(self.id);
        let text_start_point = lcx.content_rect_local().origin();

        cx.save();
        cx.clip(&self.id.content_rect_local());

        // Draw text using layout_data
        self.layout_data
            .borrow()
            .with_effective_text_layout(|text_layout| {
                cx.draw_text(
                    text_layout,
                    Point::new(text_start_point.x - self.scroll_offset, text_start_point.y),
                );
            });

        // underline the preedit text
        if let Some(preedit) = &self.preedit {
            let start_idx = self.cursor_glyph_idx;
            let end_idx = start_idx + preedit.text.len();

            self.layout_data
                .borrow()
                .with_effective_text_layout(|text_layout| {
                    let start_hit = text_layout.hit_position(start_idx);
                    let start_x = text_start_point.x + start_hit.point.x - self.scroll_offset;
                    let end_x = text_start_point.x + text_layout.hit_position(end_idx).point.x
                        - self.scroll_offset;

                    let color = self.style.color().unwrap_or(palette::css::BLACK);
                    let y = text_start_point.y + start_hit.glyph_ascent;

                    cx.fill(
                        &Rect::new(start_x, y, end_x, y + 1.0),
                        &Brush::Solid(color),
                        0.0,
                    );
                });
        }

        cx.restore();

        // skip rendering selection / cursor if we don't have focus
        if !cx.window_state.is_focused(self.id()) {
            return;
        }

        // see if we have a selection range
        let has_selection = self.selection.is_some()
            || self
                .preedit
                .as_ref()
                .is_some_and(|p| p.cursor.is_some_and(|c| c.0 != c.1));

        if has_selection {
            self.paint_selection_rect(&mut lcx, cx);
            return;
        }

        // see if we should render the cursor
        let is_cursor_visible = (self.last_cursor_action_on.elapsed().as_millis()
            / CURSOR_BLINK_INTERVAL_MS as u128)
            .is_multiple_of(2);

        if is_cursor_visible {
            let cursor_color = self
                .id
                .state()
                .borrow()
                .combined_style
                .builtin()
                .cursor_color();
            let cursor_rect = self.get_cursor_rect(&mut lcx);
            cx.fill(&cursor_rect, &cursor_color, 0.0);
        }

        // request paint either way if we're attempting draw a cursor
        let id = self.id();
        exec_after(Duration::from_millis(CURSOR_BLINK_INTERVAL_MS), move |_| {
            id.request_paint();
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::views::text_input::get_dbl_click_selection;

    use super::replace_range;

    #[test]
    fn replace_range_start() {
        let mut s = "Sample text".to_owned();
        replace_range(&mut s, 0..7, Some("Replaced___"));
        assert_eq!("Replaced___text", s);
    }

    #[test]
    fn delete_range_start() {
        let mut s = "Sample text".to_owned();
        replace_range(&mut s, 0..7, None);
        assert_eq!("text", s);
    }

    #[test]
    fn replace_range_end() {
        let mut s = "Sample text".to_owned();
        let len = s.len();
        replace_range(&mut s, 6..len, Some("++Replaced"));
        assert_eq!("Sample++Replaced", s);
    }

    #[test]
    fn delete_range_full() {
        let mut s = "Sample text".to_owned();
        let len = s.len();
        replace_range(&mut s, 0..len, None);
        assert_eq!("", s);
    }

    #[test]
    fn replace_range_full() {
        let mut s = "Sample text".to_owned();
        let len = s.len();
        replace_range(&mut s, 0..len, Some("Hello world"));
        assert_eq!("Hello world", s);
    }

    #[test]
    fn delete_range_end() {
        let mut s = "Sample text".to_owned();
        let len = s.len();
        replace_range(&mut s, 6..len, None);
        assert_eq!("Sample", s);
    }

    #[test]
    fn dbl_click_whitespace_before_word() {
        let s = "  select  ".to_owned();

        let range = get_dbl_click_selection(0, &s);
        assert_eq!(range, 0..2);

        let range = get_dbl_click_selection(1, &s);
        assert_eq!(range, 0..2);
    }

    #[test]
    fn dbl_click_word_surrounded_by_whitespace() {
        let s = "  select  ".to_owned();

        let range = get_dbl_click_selection(2, &s);
        assert_eq!(range, 2..8);

        let range = get_dbl_click_selection(6, &s);
        assert_eq!(range, 2..8);
    }

    #[test]
    fn dbl_click_whitespace_bween_words() {
        let s = "select   select".to_owned();

        let range = get_dbl_click_selection(6, &s);
        assert_eq!(range, 6..9);

        let range = get_dbl_click_selection(7, &s);
        assert_eq!(range, 6..9);

        let range = get_dbl_click_selection(8, &s);
        assert_eq!(range, 6..9);
    }

    #[test]
    fn dbl_click_whitespace_after_word() {
        let s = "  select  ".to_owned();

        let range = get_dbl_click_selection(8, &s);
        assert_eq!(range, 8..10);

        let range = get_dbl_click_selection(9, &s);
        assert_eq!(range, 8..10);
    }

    #[test]
    fn dbl_click_letter_after_whitespace() {
        let s = "     s".to_owned();
        let range = get_dbl_click_selection(5, &s);

        assert_eq!(range, 5..6);
    }

    #[test]
    fn dbl_click_single_letter() {
        let s = "s".to_owned();
        let range = get_dbl_click_selection(0, &s);

        assert_eq!(range, 0..1);
    }

    #[test]
    fn dbl_click_outside_boundaries_selects_all() {
        let s = "     ".to_owned();
        let range = get_dbl_click_selection(100, &s);

        assert_eq!(range, 0..5);
    }

    #[test]
    fn dbl_click_letters_with_whitespace() {
        let s = " s  s  ".to_owned();
        let range = get_dbl_click_selection(1, &s);
        assert_eq!(range, 1..2);

        let range = get_dbl_click_selection(4, &s);
        assert_eq!(range, 4..5);
    }

    #[test]
    fn dbl_click_single_word() {
        let s = "123testttttttttttttttttttt123".to_owned();
        let range = get_dbl_click_selection(1, &s);
        let len = s.len();
        assert_eq!(range, 0..len);

        let range = get_dbl_click_selection(5, &s);
        assert_eq!(range, 0..len);

        let range = get_dbl_click_selection(len - 1, &s);
        assert_eq!(range, 0..len);
    }

    #[test]
    fn dbl_click_two_words_and_whitespace() {
        let s = "  word1  word2 ".to_owned();

        let range = get_dbl_click_selection(2, &s);
        assert_eq!(range, 2..7);

        let range = get_dbl_click_selection(6, &s);
        assert_eq!(range, 2..7);
    }

    #[test]
    fn dbl_click_empty_string() {
        let s = "".to_owned();

        let range = get_dbl_click_selection(0, &s);
        assert_eq!(range, 0..0);

        let range = get_dbl_click_selection(1, &s);
        assert_eq!(range, 0..0);
    }

    #[test]
    fn dbl_click_whitespace_only() {
        let s = "       ".to_owned();
        let range = get_dbl_click_selection(2, &s);

        assert_eq!(range, 0..s.len());
    }
}
