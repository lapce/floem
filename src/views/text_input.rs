use crate::action::exec_after;
use crate::keyboard::{self, KeyEvent};
use crate::reactive::{create_effect, RwSignal};
use crate::unit::PxPct;
use crate::{context::LayoutCx, style::CursorStyle};
use clipboard::{ClipboardContext, ClipboardProvider};
use taffy::prelude::{Layout, Node};

use floem_renderer::{
    cosmic_text::{Cursor, Style as FontStyle, Weight},
    Renderer,
};
use unicode_segmentation::UnicodeSegmentation;
use winit::keyboard::{Key, ModifiersState, SmolStr};

use crate::{peniko::Color, style::Style, view::View};

use std::{
    any::Any,
    ops::Range,
    time::{Duration, Instant},
};

use crate::{
    cosmic_text::{Attrs, AttrsList, FamilyOwned, TextLayout},
    style::ComputedStyle,
};
use kurbo::{Point, Rect};

use crate::{
    context::{EventCx, UpdateCx},
    event::Event,
    id::Id,
    view::ChangeFlags,
};

use super::Decorators;

enum InputKind {
    SingleLine,
    #[allow(unused)]
    MultiLine {
        //TODO:
        line_index: usize,
    },
}

/// Text Input View
pub struct TextInput {
    id: Id,
    buffer: RwSignal<String>,
    // Where are we in the main buffer
    cursor_glyph_idx: usize,
    // This can be retrieved from the glyph, but we store it for efficiency
    cursor_x: f64,
    text_buf: Option<TextLayout>,
    text_node: Option<Node>,
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
    color: Option<Color>,
    selection: Option<Range<usize>>,
    font_size: f32,
    width: f32,
    height: f32,
    font_family: Option<String>,
    font_weight: Option<Weight>,
    font_style: Option<FontStyle>,
    input_kind: InputKind,
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
    let id = Id::next();

    {
        create_effect(move |_| {
            let text = buffer.get();
            id.update_state(text, false);
        });
    }

    TextInput {
        id,
        cursor_glyph_idx: 0,
        buffer,
        text_buf: None,
        text_node: None,
        clipped_text: None,
        clip_txt_buf: None,
        color: None,
        font_size: DEFAULT_FONT_SIZE,
        font_family: None,
        font_weight: None,
        font_style: None,
        cursor_x: 0.0,
        selection: None,
        input_kind: InputKind::SingleLine,
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
}

#[derive(Copy, Clone, Debug)]
enum ClipDirection {
    None,
    Forward,
    Backward,
}

enum TextCommand {
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
            (ModifiersState::SUPER, "a") => Self::SelectAll,
            (ModifiersState::SUPER, "c") => Self::Copy,
            (ModifiersState::SUPER, "x") => Self::Cut,
            (ModifiersState::SUPER, "v") => Self::Paste,
            _ => {
                dbg!("Unhandled action", event.modifiers, ch);
                Self::None
            }
        }
        #[cfg(not(target_os = "macos"))]
        match (event.modifiers, ch.as_str()) {
            (ModifiersState::CONTROL, "a") => Self::SelectAll,
            (ModifiersState::CONTROL, "c") => Self::Copy,
            (ModifiersState::CONTROL, "x") => Self::Cut,
            (ModifiersState::CONTROL, "v") => Self::Paste,
            _ => {
                dbg!("Unhandled action", event.modifiers, ch);
                Self::None
            }
        }
    }
}

const DEFAULT_FONT_SIZE: f32 = 14.0;
const CURSOR_BLINK_INTERVAL_MS: u64 = 500;
// see https://developer.mozilla.org/en-US/docs/Web/HTML/Element/input/text#size
const APPROX_VISIBLE_CHARS: f32 = 10.0;

impl TextInput {
    fn move_cursor(&mut self, move_kind: Movement, direction: Direction) -> bool {
        if matches!(self.input_kind, InputKind::MultiLine { line_index: _ }) {
            todo!();
        }
        match (move_kind, direction) {
            (Movement::Glyph, Direction::Left) => {
                if self.cursor_glyph_idx >= 1 {
                    self.cursor_glyph_idx -= 1;
                    return true;
                }
                false
            }
            (Movement::Glyph, Direction::Right) => {
                if self.cursor_glyph_idx < self.buffer.with_untracked(|buff| buff.len()) {
                    self.cursor_glyph_idx += 1;
                    return true;
                }
                false
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
                self.buffer.with(|buff| {
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
            (movement, dir) => {
                dbg!(movement, dir);
                false
            }
        }
    }

    fn text_layout_changed(&self, cx: &LayoutCx) -> bool {
        self.font_size != cx.current_font_size().unwrap_or(DEFAULT_FONT_SIZE)
            || self.font_family.as_deref() != cx.current_font_family()
            || self.font_weight != cx.font_weight
            || self.font_style != cx.font_style
    }

    fn get_line_idx(&self) -> usize {
        match self.input_kind {
            InputKind::SingleLine => 0,
            InputKind::MultiLine { line_index: _ } => todo!(),
        }
    }

    fn clip_text(&mut self, node_layout: &Layout) {
        let virt_text = self.text_buf.as_ref().unwrap();
        let node_width = node_layout.size.width as f64;
        let cursor_text_loc = Cursor::new(self.get_line_idx(), self.cursor_glyph_idx);
        let layout_cursor = virt_text.layout_cursor(&cursor_text_loc);
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
            .get()
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
        let virtual_text = self.text_buf.as_ref().unwrap();
        let text_height = virtual_text.size().height;

        let node_location = node_layout.location;

        let cursor_start = Point::new(
            self.cursor_x + node_location.x as f64,
            node_location.y as f64,
        );

        Rect::from_points(
            cursor_start,
            Point::new(
                cursor_start.x + self.cursor_width,
                cursor_start.y + text_height,
            ),
        )
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

    fn update_text_layout(&mut self) {
        let mut text_layout = TextLayout::new();
        let attrs = self.get_text_attrs();

        self.buffer
            .with_untracked(|buff| text_layout.set_text(buff, attrs.clone()));

        self.width = APPROX_VISIBLE_CHARS * self.font_size;
        self.height = self.font_size;

        // main buff should always get updated
        self.text_buf = Some(text_layout.clone());

        if let Some(cr_text) = self.clipped_text.clone().as_ref() {
            let mut clp_txt_lay = text_layout;
            clp_txt_lay.set_text(cr_text, attrs);

            self.clip_txt_buf = Some(clp_txt_lay);
        }
    }

    pub fn get_text_attrs(&self) -> AttrsList {
        let mut attrs = Attrs::new().color(self.color.unwrap_or(Color::BLACK));

        attrs = attrs.font_size(self.font_size);

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
        AttrsList::new(attrs)
    }

    fn set_cursor_glyph_idx(&mut self, new_cursor_x: usize) {
        self.cursor_glyph_idx = new_cursor_x;
    }

    fn select_all(&mut self, cx: &mut EventCx) {
        let text_node = self.text_node.unwrap();
        let node_layout = *cx.app_state.taffy.layout(text_node).unwrap();
        let len = self.buffer.with(|val| val.len());
        self.cursor_glyph_idx = len;

        let text_buf = self.text_buf.as_ref().unwrap();
        let buf_width = text_buf.size().width;
        let node_width = node_layout.size.width as f64;

        if buf_width > node_width {
            self.clip_text(&node_layout);
        }

        self.selection = Some(0..len);
    }

    fn handle_modifier_cmd(
        &mut self,
        event: &KeyEvent,
        cx: &mut EventCx<'_>,
        character: &SmolStr,
    ) -> bool {
        if event.modifiers.is_empty() {
            return false;
        }

        let command = (event, character).into();

        match command {
            TextCommand::SelectAll => {
                self.select_all(cx);
                true
            }
            TextCommand::Copy => {
                if let Some(selection) = &self.selection {
                    let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
                    let selection_txt = self
                        .buffer
                        .get()
                        .chars()
                        .skip(selection.start)
                        .take(selection.end - selection.start)
                        .collect();
                    ctx.set_contents(selection_txt).unwrap();
                }
                true
            }
            TextCommand::Cut => {
                if let Some(selection) = &self.selection {
                    let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
                    let selection_txt = self
                        .buffer
                        .get()
                        .chars()
                        .skip(selection.start)
                        .take(selection.end - selection.start)
                        .collect();
                    ctx.set_contents(selection_txt).unwrap();

                    self.buffer
                        .update(|buf| replace_range(buf, selection.clone(), None));

                    self.cursor_glyph_idx = selection.start;
                    self.selection = None;
                }

                true
            }
            TextCommand::Paste => {
                let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
                let clipboard_content = ctx.get_contents().unwrap();
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
            Key::Character(ref ch) => {
                let handled_modifier_cmd = self.handle_modifier_cmd(event, cx, ch);
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
            Key::Space => {
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
            Key::Backspace => {
                let selection = self.selection.clone();
                if let Some(selection) = selection {
                    self.buffer
                        .update(|buf| replace_range(buf, selection, None));
                    self.cursor_glyph_idx = 0;
                    self.selection = None;
                    true
                } else {
                    let prev_cursor_idx = self.cursor_glyph_idx;

                    if event.modifiers.contains(ModifiersState::CONTROL) {
                        self.move_cursor(Movement::Word, Direction::Left);
                    } else {
                        self.move_cursor(Movement::Glyph, Direction::Left);
                    }
                    if self.cursor_glyph_idx == prev_cursor_idx {
                        return false;
                    }

                    self.buffer.update(|buf| {
                        replace_range(buf, self.cursor_glyph_idx..prev_cursor_idx, None);
                    });
                    true
                }
            }
            Key::Delete => {
                let prev_cursor_idx = self.cursor_glyph_idx;

                if event.modifiers.contains(ModifiersState::CONTROL) {
                    self.move_cursor(Movement::Word, Direction::Right);
                } else {
                    self.move_cursor(Movement::Glyph, Direction::Right);
                }

                if self.cursor_glyph_idx == prev_cursor_idx {
                    return false;
                }

                self.buffer.update(|buf| {
                    replace_range(buf, prev_cursor_idx..self.cursor_glyph_idx, None);
                });

                self.cursor_glyph_idx = prev_cursor_idx;
                true
            }
            Key::Escape => {
                cx.app_state.clear_focus();
                true
            }
            Key::End => self.move_cursor(Movement::Line, Direction::Right),
            Key::Home => self.move_cursor(Movement::Line, Direction::Left),
            Key::ArrowLeft => {
                let old_glyph_idx = self.cursor_glyph_idx;

                let cursor_moved = if event.modifiers.contains(ModifiersState::CONTROL) {
                    self.move_cursor(Movement::Word, Direction::Left)
                } else {
                    self.move_cursor(Movement::Glyph, Direction::Left)
                };

                if cursor_moved {
                    self.move_selection(
                        old_glyph_idx,
                        self.cursor_glyph_idx,
                        event.modifiers,
                        Direction::Left,
                    );
                } else if !event.modifiers.contains(ModifiersState::SHIFT)
                    && self.selection.is_some()
                {
                    self.selection = None;
                }

                cursor_moved
            }
            Key::ArrowRight => {
                let old_glyph_idx = self.cursor_glyph_idx;

                let cursor_moved = if event.modifiers.contains(ModifiersState::CONTROL) {
                    self.move_cursor(Movement::Word, Direction::Right)
                } else {
                    self.move_cursor(Movement::Glyph, Direction::Right)
                };

                if cursor_moved {
                    self.move_selection(
                        old_glyph_idx,
                        self.cursor_glyph_idx,
                        event.modifiers,
                        Direction::Right,
                    );
                } else if !event.modifiers.contains(ModifiersState::SHIFT)
                    && self.selection.is_some()
                {
                    self.selection = None;
                }

                cursor_moved
            }
            ref key => {
                dbg!("Unhandled key", key);
                false
            }
        }
    }

    fn move_selection(
        &mut self,
        old_glyph_idx: usize,
        curr_glyph_idx: usize,
        modifiers: ModifiersState,
        direction: Direction,
    ) {
        if !modifiers.contains(ModifiersState::SHIFT) {
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

impl View for TextInput {
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
        format!("TextInput: {:?}", self.buffer.get_untracked()).into()
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) -> ChangeFlags {
        if state.downcast::<String>().is_ok() {
            cx.request_layout(self.id());
            ChangeFlags::LAYOUT
        } else {
            dbg!("downcast failed");
            ChangeFlags::empty()
        }
    }

    fn event(&mut self, cx: &mut EventCx, _id_path: Option<&[Id]>, event: Event) -> bool {
        let is_handled = match &event {
            Event::PointerDown(event) => {
                if !self.is_focused {
                    // Just gained focus - move cursor to buff end
                    self.set_cursor_glyph_idx(self.buffer.with_untracked(|buff| buff.len()));
                } else {
                    // Already focused - move cursor to click pos
                    let layout = cx.get_layout(self.id()).unwrap();
                    let style = cx.app_state.get_computed_style(self.id);

                    let padding_left = match style.padding_left {
                        PxPct::Px(padding) => padding as f32,
                        PxPct::Pct(pct) => pct as f32 * layout.size.width,
                    };
                    let padding_top = match style.padding_top {
                        PxPct::Px(padding) => padding as f32,
                        PxPct::Pct(pct) => pct as f32 * layout.size.width,
                    };
                    self.cursor_glyph_idx = self
                        .text_buf
                        .as_ref()
                        .unwrap()
                        .hit_point(Point::new(
                            event.pos.x + self.clip_start_x - padding_left as f64,
                            // TODO: prevent cursor incorrectly going to end of buffer when clicking
                            // slightly below the text
                            event.pos.y - padding_top as f64,
                        ))
                        .index;
                }
                true
            }
            Event::KeyDown(event) => self.handle_key_down(cx, event),
            Event::PointerMove(_) => {
                if !matches!(cx.app_state.cursor, Some(CursorStyle::Text)) {
                    cx.app_state.cursor = Some(CursorStyle::Text);
                    return true;
                }
                false
            }
            _ => false,
        };

        if is_handled {
            cx.app_state.request_layout(self.id);
            self.last_cursor_action_on = Instant::now();
        }

        false
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| {
            self.is_focused = cx.app_state().is_focused(&self.id);
            if self.text_layout_changed(cx) {
                self.font_size = cx.current_font_size().unwrap_or(DEFAULT_FONT_SIZE);
                self.font_family = cx.current_font_family().map(|s| s.to_string());
                self.font_weight = cx.font_weight;
                self.font_style = cx.font_style;
                self.update_text_layout();
            } else if self.text_buf.is_none() {
                self.update_text_layout();
            }

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
                .width(self.width)
                .height(self.height)
                .compute(&ComputedStyle::default())
                .to_taffy_style();
            let _ = cx.app_state_mut().taffy.set_style(text_node, style);

            vec![text_node]
        })
    }

    fn compute_layout(&mut self, _cx: &mut crate::context::LayoutCx) -> Option<Rect> {
        self.update_text_layout();
        None
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if !cx.app_state.is_focused(&self.id) && self.buffer.with_untracked(|buff| buff.is_empty())
        {
            return;
        }

        if self.color != cx.color
            || self.font_size != cx.font_size.unwrap_or(DEFAULT_FONT_SIZE)
            || self.font_family.as_deref() != cx.font_family.as_deref()
            || self.font_weight != cx.font_weight
            || self.font_style != cx.font_style
        {
            self.color = cx.color;
            self.font_size = cx.font_size.unwrap_or(DEFAULT_FONT_SIZE);
            self.font_family = cx.font_family.clone();
            self.font_weight = cx.font_weight;
            self.font_style = cx.font_style;
            self.update_text_layout();
        }

        let text_node = self.text_node.unwrap();
        let text_buf = self.text_buf.as_ref().unwrap();
        let buf_width = text_buf.size().width;
        let node_layout = *cx.app_state.taffy.layout(text_node).unwrap();
        let node_width = node_layout.size.width as f64;
        let cursor_color = cx.app_state.get_computed_style(self.id).cursor_color;

        match self.input_kind {
            InputKind::SingleLine => {
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
            }
            InputKind::MultiLine { .. } => {
                todo!();
            }
        }

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

        let is_cursor_visible = cx.app_state.is_focused(&self.id)
            && (self.last_cursor_action_on.elapsed().as_millis()
                / CURSOR_BLINK_INTERVAL_MS as u128)
                % 2
                == 0;

        if is_cursor_visible {
            let cursor_rect = self.get_cursor_rect(&node_layout);
            cx.fill(&cursor_rect, cursor_color.unwrap_or(Color::BLACK), 0.0);
        }

        let style = cx.app_state.get_computed_style(self.id);

        let padding_left = match style.padding_left {
            PxPct::Px(padding) => padding as f32,
            PxPct::Pct(pct) => pct as f32 * node_layout.size.width,
        };

        if cx.app_state.is_focused(&self.id) {
            let selection_rect = self.get_selection_rect(&node_layout, padding_left as f64);
            cx.fill(
                &selection_rect,
                cursor_color.unwrap_or(Color::rgba8(0, 0, 0, 150)),
                0.0,
            );
        } else {
            self.selection = None;
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
}
