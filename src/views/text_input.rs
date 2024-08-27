use crate::action::exec_after;
use crate::event::{EventListener, EventPropagation};
use crate::id::ViewId;
use crate::keyboard::{self, KeyEvent, Modifiers};
use crate::pointer::{PointerButton, PointerInputEvent};
use crate::reactive::{create_effect, RwSignal};
use crate::style::{FontProps, PaddingLeft, SelectionStyle};
use crate::style::{FontStyle, FontWeight, TextColor};
use crate::unit::{PxPct, PxPctAuto};
use crate::{prop_extractor, style_class, Clipboard};
use floem_reactive::{create_rw_signal, SignalGet, SignalUpdate, SignalWith};
use taffy::prelude::{Layout, NodeId};

use floem_renderer::{text::Cursor, Renderer};
use floem_winit::keyboard::{Key, NamedKey, SmolStr};
use unicode_segmentation::UnicodeSegmentation;

use crate::{peniko::Color, style::Style, view::View};

use std::{any::Any, ops::Range};

use crate::text::{Attrs, AttrsList, FamilyOwned, TextLayout};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

use peniko::kurbo::{Point, Rect, Size};

use crate::{
    context::{EventCx, UpdateCx},
    event::Event,
};

use super::Decorators;

style_class!(pub TextInputClass);
style_class!(pub PlaceholderTextClass);

prop_extractor! {
    Extractor {
        color: TextColor,
    }
}

prop_extractor! {
    PlaceholderStyle {
        pub color: TextColor,
        //TODO: pub font_size: FontSize,
        pub font_weight: FontWeight,
        pub font_style: FontStyle,
    }
}

struct BufferState {
    buffer: RwSignal<String>,
    last_buffer: String,
}

impl BufferState {
    fn update(&mut self, update: impl FnOnce(&mut String)) {
        self.buffer.update(|s| {
            update(s);
            self.last_buffer = s.clone();
        });
    }

    fn get_untracked(&self) -> String {
        self.buffer.get_untracked()
    }

    fn with_untracked<T>(&self, f: impl FnOnce(&String) -> T) -> T {
        self.buffer.with_untracked(f)
    }
}

/// Text Input View
pub struct TextInput {
    id: ViewId,
    buffer: BufferState,
    pub(crate) placeholder_text: Option<String>,
    placeholder_buff: Option<TextLayout>,
    placeholder_style: PlaceholderStyle,
    selection_style: SelectionStyle,
    // Where are we in the main buffer
    cursor_glyph_idx: usize,
    // This can be retrieved from the glyph, but we store it for efficiency
    cursor_x: f64,
    text_buf: Option<TextLayout>,
    text_node: Option<NodeId>,
    // Shown when the width exceeds node width for single line input
    clipped_text: Option<String>,
    // Glyph index from which we started clipping
    clip_start_idx: usize,
    // This can be retrieved from the clip start glyph, but we store it for efficiency
    clip_start_x: f64,
    clip_txt_buf: Option<TextLayout>,
    // When the visible range changes, we also may need to have a small offset depending on the direction we moved.
    // This makes sure character under the cursor is always fully visible and correctly aligned,
    // and may cause the last character in the opposite direction to be "cut"
    clip_offset_x: f64,
    selection: Option<Range<usize>>,
    width: f32,
    height: f32,
    // Approx max size of a glyph, given the current font weight & size.
    glyph_max_size: Size,
    style: Extractor,
    font: FontProps,
    cursor_width: f64, // TODO: make this configurable
    is_focused: bool,
    last_cursor_action_on: Instant,
}

#[derive(Clone, Copy, Debug)]
pub enum Movement {
    Glyph,
    Word,
    Line,
}

#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Left,
    Right,
}

/// Text Input View
pub fn text_input(buffer: RwSignal<String>) -> TextInput {
    let id = ViewId::new();
    let is_focused = create_rw_signal(false);

    {
        create_effect(move |_| {
            let text = buffer.get();
            id.update_state((text, is_focused.get()));
        });
    }

    TextInput {
        id,
        cursor_glyph_idx: 0,
        placeholder_text: None,
        placeholder_buff: None,
        placeholder_style: Default::default(),
        selection_style: Default::default(),
        buffer: BufferState {
            buffer,
            last_buffer: buffer.get_untracked(),
        },
        text_buf: None,
        text_node: None,
        clipped_text: None,
        clip_txt_buf: None,
        style: Default::default(),
        font: FontProps::default(),
        cursor_x: 0.0,
        selection: None,
        glyph_max_size: Size::ZERO,
        clip_start_idx: 0,
        clip_offset_x: 0.0,
        clip_start_x: 0.0,
        cursor_width: 1.0,
        width: 0.0,
        height: 0.0,
        is_focused: false,
        last_cursor_action_on: Instant::now(),
    }
    .keyboard_navigatable()
    .on_event_stop(EventListener::FocusGained, move |_| {
        is_focused.set(true);
    })
    .on_event_stop(EventListener::FocusLost, move |_| {
        is_focused.set(false);
    })
    .class(TextInputClass)
}

#[derive(Copy, Clone, Debug)]
enum ClipDirection {
    None,
    Forward,
    Backward,
}

pub(crate) enum TextCommand {
    SelectAll,
    Copy,
    Paste,
    Cut,
    None,
}

impl From<(&KeyEvent, &SmolStr)> for TextCommand {
    fn from(val: (&keyboard::KeyEvent, &SmolStr)) -> Self {
        let (event, ch) = val;
        #[cfg(target_os = "macos")]
        match (event.modifiers, ch.as_str()) {
            (Modifiers::META, "a") => Self::SelectAll,
            (Modifiers::META, "c") => Self::Copy,
            (Modifiers::META, "x") => Self::Cut,
            (Modifiers::META, "v") => Self::Paste,
            _ => Self::None,
        }
        #[cfg(not(target_os = "macos"))]
        match (event.modifiers, ch.as_str()) {
            (Modifiers::CONTROL, "a") => Self::SelectAll,
            (Modifiers::CONTROL, "c") => Self::Copy,
            (Modifiers::CONTROL, "x") => Self::Cut,
            (Modifiers::CONTROL, "v") => Self::Paste,
            _ => Self::None,
        }
    }
}

fn get_word_based_motion(event: &KeyEvent) -> Option<Movement> {
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
/// Specifies approximately how many characters wide the input field should be
/// (i.e., how many characters can be seen at a time), when the width is not set in the styles.
/// Since character widths vary(depending on the font), may not be exact and should not be relied upon to be so.
/// See https://developer.mozilla.org/en-US/docs/Web/HTML/Element/input/text#size
// TODO: allow this to be set in the styles
const APPROX_VISIBLE_CHARS_TARGET: f32 = 10.0;

impl TextInput {
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder_text = Some(text.into());
        self
    }
}

impl TextInput {
    fn move_cursor(&mut self, move_kind: Movement, direction: Direction) -> bool {
        match (move_kind, direction) {
            (Movement::Glyph, Direction::Left) => {
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
            (Movement::Glyph, Direction::Right) => {
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
            (Movement::Line, Direction::Right) => {
                if self.cursor_glyph_idx < self.buffer.with_untracked(|buff| buff.len()) {
                    self.cursor_glyph_idx = self.buffer.with_untracked(|buff| buff.len());
                    return true;
                }
                false
            }
            (Movement::Line, Direction::Left) => {
                if self.cursor_glyph_idx > 0 {
                    self.cursor_glyph_idx = 0;
                    return true;
                }
                false
            }
            (Movement::Word, Direction::Right) => self.buffer.with_untracked(|buff| {
                for (idx, word) in buff.unicode_word_indices() {
                    let word_end_idx = idx + word.len();
                    if word_end_idx > self.cursor_glyph_idx {
                        self.cursor_glyph_idx = word_end_idx;
                        return true;
                    }
                }
                false
            }),
            (Movement::Word, Direction::Left) if self.cursor_glyph_idx > 0 => {
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

    fn clip_text(&mut self, node_layout: &Layout) {
        let virt_text = self.text_buf.as_mut().unwrap();
        let node_width = node_layout.size.width as f64;
        let cursor_text_loc = Cursor::new(0, self.cursor_glyph_idx);
        let layout_cursor = virt_text.layout_cursor(cursor_text_loc);
        let cursor_glyph_pos = virt_text.hit_position(layout_cursor.glyph);
        let cursor_x = cursor_glyph_pos.point.x;

        let mut clip_start_x = self.clip_start_x;

        let visible_range = clip_start_x..=clip_start_x + node_width;

        let mut clip_dir = ClipDirection::None;
        if !visible_range.contains(&cursor_glyph_pos.point.x) {
            if cursor_x < *visible_range.start() {
                clip_start_x = cursor_x;
                clip_dir = ClipDirection::Backward;
            } else {
                clip_dir = ClipDirection::Forward;
                clip_start_x = cursor_x - node_width;
            }
        }
        self.cursor_x = cursor_x;

        let clip_start = virt_text.hit_point(Point::new(clip_start_x, 0.0)).index;
        let clip_end = virt_text
            .hit_point(Point::new(clip_start_x + node_width, 0.0))
            .index;

        let new_text = self
            .buffer
            .get_untracked()
            .chars()
            .skip(clip_start)
            .take(clip_end - clip_start)
            .collect();

        self.cursor_x -= clip_start_x;
        self.clip_start_idx = clip_start;
        self.clip_start_x = clip_start_x;
        self.clipped_text = Some(new_text);

        self.update_text_layout();
        match clip_dir {
            ClipDirection::None => {}
            ClipDirection::Forward => {
                self.clip_offset_x = self.clip_txt_buf.as_ref().unwrap().size().width - node_width
            }
            ClipDirection::Backward => self.clip_offset_x = 0.0,
        }
    }

    fn get_cursor_rect(&self, node_layout: &Layout) -> Rect {
        let node_location = node_layout.location;

        let text_height = self.height;

        let cursor_start = Point::new(
            self.cursor_x + node_location.x as f64,
            node_location.y as f64,
        );

        Rect::from_points(
            cursor_start,
            Point::new(
                cursor_start.x + self.cursor_width,
                cursor_start.y + text_height as f64,
            ),
        )
    }

    fn handle_double_click(&mut self, pos_x: f64, pos_y: f64) {
        let clicked_glyph_idx = self.get_box_position(pos_x, pos_y);

        self.buffer.with_untracked(|buff| {
            let selection = get_dbl_click_selection(clicked_glyph_idx, buff);
            self.cursor_glyph_idx = selection.end;
            self.selection = Some(selection);
        })
    }

    fn get_box_position(&self, pos_x: f64, pos_y: f64) -> usize {
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
        self.text_buf
            .as_ref()
            .unwrap()
            .hit_point(Point::new(
                pos_x + self.clip_start_x - padding_left as f64,
                // TODO: prevent cursor incorrectly going to end of buffer when clicking
                // slightly below the text
                pos_y - padding_top as f64,
            ))
            .index
    }

    fn get_selection_rect(&self, node_layout: &Layout, left_padding: f64) -> Rect {
        let selection = if let Some(curr_selection) = &self.selection {
            curr_selection
        } else {
            return Rect::ZERO;
        };

        let virtual_text = self.text_buf.as_ref().unwrap();
        let text_height = virtual_text.size().height;

        let selection_start_x =
            virtual_text.hit_position(selection.start).point.x - self.clip_start_x;
        let selection_start_x = selection_start_x.max(node_layout.location.x as f64 - left_padding);

        let selection_end_x =
            virtual_text.hit_position(selection.end).point.x + left_padding - self.clip_start_x;
        let selection_end_x =
            selection_end_x.min(selection_start_x + self.width as f64 + left_padding);

        let node_location = node_layout.location;

        let selection_start = Point::new(
            selection_start_x + node_location.x as f64,
            node_location.y as f64,
        );

        Rect::from_points(
            selection_start,
            Point::new(selection_end_x, selection_start.y + text_height),
        )
    }

    /// Determine approximate max size of a single glyph, given the current font weight & size
    fn get_font_glyph_max_size(&self) -> Size {
        let mut tmp = TextLayout::new();
        let attrs_list = self.get_text_attrs();
        tmp.set_text("W", attrs_list);
        tmp.size()
    }

    fn update_selection(&mut self, selection_start: usize, selection_stop: usize) {
        if selection_stop < selection_start {
            self.selection = Some(Range {
                start: selection_stop,
                end: selection_start,
            });
        } else {
            self.selection = Some(Range {
                start: selection_start,
                end: selection_stop,
            });
        }
    }

    fn update_text_layout(&mut self) {
        let mut text_layout = TextLayout::new();
        let attrs_list = self.get_text_attrs();

        self.buffer
            .with_untracked(|buff| text_layout.set_text(buff, attrs_list.clone()));

        let glyph_max_size = self.get_font_glyph_max_size();
        self.height = glyph_max_size.height as f32;
        self.glyph_max_size = glyph_max_size;

        // main buff should always get updated
        self.text_buf = Some(text_layout.clone());

        if let Some(cr_text) = self.clipped_text.clone().as_ref() {
            let mut clp_txt_lay = text_layout;
            clp_txt_lay.set_text(cr_text, attrs_list);

            self.clip_txt_buf = Some(clp_txt_lay);
        }
    }

    fn font_size(&self) -> f32 {
        self.font.size().unwrap_or(DEFAULT_FONT_SIZE)
    }

    pub fn get_placeholder_text_attrs(&self) -> AttrsList {
        let mut attrs = Attrs::new().color(self.placeholder_style.color().unwrap_or(Color::BLACK));

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
        AttrsList::new(attrs)
    }

    pub fn get_text_attrs(&self) -> AttrsList {
        let mut attrs = Attrs::new().color(self.style.color().unwrap_or(Color::BLACK));

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

    fn select_all(&mut self) {
        let text_node = self.text_node.unwrap();
        let node_layout = self
            .id
            .taffy()
            .borrow()
            .layout(text_node)
            .cloned()
            .unwrap_or_default();
        let len = self.buffer.with_untracked(|val| val.len());
        self.cursor_glyph_idx = len;

        let text_buf = self.text_buf.as_ref().unwrap();
        let buf_width = text_buf.size().width;
        let node_width = node_layout.size.width as f64;

        if buf_width > node_width {
            self.clip_text(&node_layout);
        }

        self.selection = Some(0..len);
    }

    fn handle_modifier_cmd(&mut self, event: &KeyEvent, character: &SmolStr) -> bool {
        if event.modifiers.is_empty() {
            return false;
        }

        let command = (event, character).into();

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
                let clipboard_content = match Clipboard::get_contents() {
                    Ok(content) => content,
                    Err(_) => return false,
                };
                if clipboard_content.is_empty() {
                    return false;
                }

                if let Some(selection) = &self.selection {
                    self.buffer.update(|buf| {
                        replace_range(buf, selection.clone(), Some(&clipboard_content))
                    });

                    self.cursor_glyph_idx +=
                        clipboard_content.len() - selection.len().min(clipboard_content.len());
                    self.selection = None;
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

    fn handle_key_down(&mut self, cx: &mut EventCx, event: &KeyEvent) -> bool {
        match event.key.logical_key {
            Key::Character(ref ch) => self.insert_text(event, ch),
            Key::Unidentified(_) => event
                .key
                .text
                .as_ref()
                .map_or(false, |ch| self.insert_text(event, ch)),
            Key::Named(NamedKey::Space) => {
                if let Some(selection) = &self.selection {
                    self.buffer
                        .update(|buf| replace_range(buf, selection.clone(), None));
                    self.cursor_glyph_idx = selection.start;
                    self.selection = None;
                } else {
                    self.buffer
                        .update(|buf| buf.insert(self.cursor_glyph_idx, ' '));
                }
                self.move_cursor(Movement::Glyph, Direction::Right)
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
                        Direction::Left,
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
                    Direction::Right,
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
                cx.app_state.clear_focus();
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
                self.move_cursor(Movement::Line, Direction::Right)
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
                self.move_cursor(Movement::Line, Direction::Left)
            }
            Key::Named(NamedKey::ArrowLeft) => {
                let old_glyph_idx = self.cursor_glyph_idx;

                let cursor_moved = self.move_cursor(
                    get_word_based_motion(event).unwrap_or(Movement::Glyph),
                    Direction::Left,
                );

                if cursor_moved {
                    self.move_selection(
                        old_glyph_idx,
                        self.cursor_glyph_idx,
                        event.modifiers,
                        Direction::Left,
                    );
                } else if !event.modifiers.contains(Modifiers::SHIFT) && self.selection.is_some() {
                    self.selection = None;
                }

                cursor_moved
            }
            Key::Named(NamedKey::ArrowRight) => {
                let old_glyph_idx = self.cursor_glyph_idx;

                let cursor_moved = self.move_cursor(
                    get_word_based_motion(event).unwrap_or(Movement::Glyph),
                    Direction::Right,
                );

                if cursor_moved {
                    self.move_selection(
                        old_glyph_idx,
                        self.cursor_glyph_idx,
                        event.modifiers,
                        Direction::Right,
                    );
                } else if !event.modifiers.contains(Modifiers::SHIFT) && self.selection.is_some() {
                    self.selection = None;
                }

                cursor_moved
            }
            _ => false,
        }
    }

    fn insert_text(&mut self, event: &KeyEvent, ch: &SmolStr) -> bool {
        let handled_modifier_cmd = self.handle_modifier_cmd(event, ch);
        if handled_modifier_cmd {
            return true;
        }

        let selection = self.selection.clone();
        if let Some(selection) = selection {
            self.buffer
                .update(|buf| replace_range(buf, selection.clone(), None));
            self.cursor_glyph_idx = selection.start;
            self.selection = None;
        }

        self.buffer
            .update(|buf| buf.insert_str(self.cursor_glyph_idx, &ch.clone()));
        self.move_cursor(Movement::Glyph, Direction::Right)
    }

    fn move_selection(
        &mut self,
        old_glyph_idx: usize,
        curr_glyph_idx: usize,
        modifiers: Modifiers,
        direction: Direction,
    ) {
        if !modifiers.contains(Modifiers::SHIFT) {
            if self.selection.is_some() {
                self.selection = None;
            }
            return;
        }

        let new_selection = if let Some(selection) = &self.selection {
            match (direction, selection.contains(&curr_glyph_idx)) {
                (Direction::Left, true) | (Direction::Right, false) => {
                    selection.start..curr_glyph_idx
                }
                (Direction::Right, true) | (Direction::Left, false) => {
                    curr_glyph_idx..selection.end
                }
            }
        } else {
            match direction {
                Direction::Left => curr_glyph_idx..old_glyph_idx,
                Direction::Right => old_glyph_idx..curr_glyph_idx,
            }
        };
        // when we move in the opposite direction and end up in the same selection range,
        // the selection should be cancelled out
        if self
            .selection
            .as_ref()
            .is_some_and(|sel| sel == &new_selection)
        {
            self.selection = None;
        } else {
            self.selection = Some(new_selection);
        }
    }

    fn paint_placeholder_text(
        &self,
        placeholder_buff: &TextLayout,
        cx: &mut crate::context::PaintCx,
    ) {
        let text_node = self.text_node.unwrap();
        let layout = self
            .id
            .taffy()
            .borrow()
            .layout(text_node)
            .cloned()
            .unwrap_or_default();
        let node_location = layout.location;
        let text_start_point = Point::new(node_location.x as f64, node_location.y as f64);
        cx.draw_text(placeholder_buff, text_start_point);
    }

    fn paint_selection_rect(&self, &node_layout: &Layout, cx: &mut crate::context::PaintCx<'_>) {
        let view_state = self.id.state();
        let view_state = view_state.borrow();
        let style = &view_state.combined_style;

        let cursor_color = self.selection_style.selection_color();

        let padding_left = match style.get(PaddingLeft) {
            PxPct::Px(padding) => padding,
            PxPct::Pct(pct) => pct / 100.0 * node_layout.size.width as f64,
        };

        let border_radius = self.selection_style.corner_radius();
        let selection_rect = self
            .get_selection_rect(&node_layout, padding_left)
            .inflate(1., 0.)
            .to_rounded_rect(border_radius);
        cx.fill(&selection_rect, &cursor_color, 0.0);
    }
}

fn replace_range(buff: &mut String, del_range: Range<usize>, replacement: Option<&str>) {
    assert!(del_range.start <= del_range.end);
    if !buff.is_char_boundary(del_range.end) {
        eprintln!(
            "[Floem] Tried to delete range with invalid end: {:?}",
            del_range
        );
        return;
    }

    if !buff.is_char_boundary(del_range.start) {
        eprintln!(
            "[Floem] Tried to delete range with invalid start: {:?}",
            del_range
        );
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

fn get_dbl_click_selection(glyph_idx: usize, buffer: &String) -> Range<usize> {
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

    fn update(&mut self, _cx: &mut UpdateCx, state: Box<dyn Any>) {
        if let Ok(state) = state.downcast::<(String, bool)>() {
            let (value, is_focused) = *state;

            // Only update recomputation if the state has actually changed
            if self.is_focused != is_focused || value != self.buffer.last_buffer {
                if is_focused {
                    self.cursor_glyph_idx = self.buffer.with_untracked(|buf| buf.len());
                }
                self.is_focused = is_focused;
                self.id.request_layout();
            }
        } else {
            eprintln!("downcast failed");
        }
    }

    fn event_before_children(&mut self, cx: &mut EventCx, event: &Event) -> EventPropagation {
        let buff_len = self.buffer.with_untracked(|buff| buff.len());
        // Workaround for cursor going out of bounds when text buffer is modified externally
        // TODO: find a better way to handle this
        if self.cursor_glyph_idx > buff_len {
            self.cursor_glyph_idx = buff_len;
        }

        let is_handled = match &event {
            // match on pointer primary button press
            Event::PointerDown(
                event @ PointerInputEvent {
                    button: PointerButton::Primary,
                    ..
                },
            ) => {
                cx.update_active(self.id);
                self.id.request_layout();

                if event.count == 2 {
                    self.handle_double_click(event.pos.x, event.pos.y);
                } else {
                    self.cursor_glyph_idx = self.get_box_position(event.pos.x, event.pos.y);
                    self.selection = None;
                }
                true
            }
            Event::PointerMove(event) => {
                self.id.request_layout();
                if cx.is_active(self.id) {
                    let selection_stop = self.get_box_position(event.pos.x, event.pos.y);
                    self.update_selection(self.cursor_glyph_idx, selection_stop);
                }
                false
            }
            Event::KeyDown(event) => self.handle_key_down(cx, event),
            _ => false,
        };

        if is_handled {
            self.id.request_layout();
            self.last_cursor_action_on = Instant::now();
        }

        EventPropagation::Continue
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        let style = cx.style();
        if self.font.read(cx) || self.text_buf.is_none() {
            self.update_text_layout();
            self.id.request_layout();
        }
        if self.style.read(cx) {
            cx.app_state_mut().request_paint(self.id);
        }

        self.selection_style.read_style(cx, &style);

        let placeholder_style = style.clone().apply_class(PlaceholderTextClass);
        self.placeholder_style.read_style(cx, &placeholder_style);
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::tree::NodeId {
        cx.layout_node(self.id(), true, |cx| {
            let was_focused = self.is_focused;
            self.is_focused = cx.app_state().is_focused(&self.id);

            if was_focused && !self.is_focused {
                self.selection = None;
            }

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

            // FIXME: This layout is undefined.
            let layout = self.id.get_layout().unwrap_or_default();
            let view_state = self.id.state();
            let view_state = view_state.borrow();
            let style = view_state.combined_style.builtin();
            let node_width = layout.size.width;

            if self.placeholder_buff.is_none() {
                if let Some(placeholder_text) = &self.placeholder_text {
                    let mut placeholder_buff = TextLayout::new();
                    let attrs_list = self.get_placeholder_text_attrs();
                    placeholder_buff.set_text(placeholder_text, attrs_list);
                    self.placeholder_buff = Some(placeholder_buff);
                }
            }

            let style_width = style.width();
            let width_px = match style_width {
                crate::unit::PxPctAuto::Px(px) => px as f32,
                // the percent is already applied to the view, so we don't need to
                // apply it to the inner text node as well
                crate::unit::PxPctAuto::Pct(_) => node_width,
                crate::unit::PxPctAuto::Auto => {
                    APPROX_VISIBLE_CHARS_TARGET * self.glyph_max_size.width as f32
                }
            };

            let is_auto_width = matches!(style_width, PxPctAuto::Auto);
            self.width = if is_auto_width {
                width_px
            } else {
                let padding_left = match style.padding_left() {
                    PxPct::Px(padding) => padding as f32,
                    PxPct::Pct(pct) => pct as f32 / 100.0 * node_width,
                };
                let padding_right = match style.padding_right() {
                    PxPct::Px(padding) => padding as f32,
                    PxPct::Pct(pct) => pct as f32 / 100.0 * node_width,
                };
                let padding = padding_left + padding_right;
                f32::max(width_px - padding, 1.0)
            };

            let taffy_node_width = match style_width {
                PxPctAuto::Px(_) | PxPctAuto::Auto => PxPctAuto::Px(self.width as f64),
                // the pct is already applied to the text input view, so text node should be 100% of parent
                PxPctAuto::Pct(_) => PxPctAuto::Pct(100.),
            };

            let style = Style::new()
                .width(taffy_node_width)
                .height(self.height)
                .to_taffy_style();
            let _ = self.id.taffy().borrow_mut().set_style(text_node, style);

            vec![text_node]
        })
    }

    fn compute_layout(&mut self, _cx: &mut crate::context::ComputeLayoutCx) -> Option<Rect> {
        self.update_text_layout();

        let text_buf = self.text_buf.as_ref().unwrap();
        let buf_width = text_buf.size().width;
        let text_node = self.text_node.unwrap();
        let node_layout = self
            .id
            .taffy()
            .borrow()
            .layout(text_node)
            .cloned()
            .unwrap_or_default();
        let node_width = node_layout.size.width as f64;

        if buf_width > node_width {
            self.clip_text(&node_layout);
        } else {
            self.clip_txt_buf = None;
            self.clip_start_idx = 0;
            self.clip_start_x = 0.0;
            let hit_pos = self
                .text_buf
                .as_ref()
                .unwrap()
                .hit_position(self.cursor_glyph_idx);
            self.cursor_x = hit_pos.point.x;
        }

        None
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if !cx.app_state.is_focused(&self.id())
            && self.buffer.with_untracked(|buff| buff.is_empty())
        {
            if let Some(placeholder_buff) = &self.placeholder_buff {
                self.paint_placeholder_text(placeholder_buff, cx);
            }
            return;
        }

        let text_node = self.text_node.unwrap();
        let node_layout = self
            .id
            .taffy()
            .borrow()
            .layout(text_node)
            .cloned()
            .unwrap_or_default();

        let location = node_layout.location;
        let text_start_point = Point::new(location.x as f64, location.y as f64);

        if let Some(clip_txt) = self.clip_txt_buf.as_mut() {
            cx.draw_text(
                clip_txt,
                Point::new(text_start_point.x - self.clip_offset_x, text_start_point.y),
            );
        } else {
            cx.draw_text(self.text_buf.as_ref().unwrap(), text_start_point);
        }

        let is_cursor_visible = cx.app_state.is_focused(&self.id())
            && self.selection.is_none()
            && (self.last_cursor_action_on.elapsed().as_millis()
                / CURSOR_BLINK_INTERVAL_MS as u128)
                % 2
                == 0;

        if is_cursor_visible {
            let cursor_color = self
                .id
                .state()
                .borrow()
                .combined_style
                .builtin()
                .cursor_color();
            let cursor_rect = self.get_cursor_rect(&node_layout);
            cx.fill(&cursor_rect, &cursor_color, 0.0);
        }

        if cx.app_state.is_focused(&self.id()) && self.selection.is_some() {
            self.paint_selection_rect(&node_layout, cx);
        }

        let id = self.id();
        exec_after(
            Duration::from_millis(CURSOR_BLINK_INTERVAL_MS),
            Box::new(move |_| {
                id.request_paint();
            }),
        );
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
