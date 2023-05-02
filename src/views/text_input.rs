#![allow(unused)]
use leptos_reactive::{
    create_effect, create_memo, create_rw_signal, create_signal, ReadSignal, RwSignal, SignalGet,
    SignalGetUntracked, SignalUpdate, SignalWith, SignalWithUntracked,
};
use taffy::{
    prelude::{Layout, Node},
    style::Dimension,
};

use floem_renderer::{
    cosmic_text::{Cursor, Edit, Editor, Style as FontStyle, Weight},
    Renderer,
};

use crate::{
    context::{LayoutCx, PaintCx},
    peniko::Color,
    style::Style,
    view::View,
    AppContext,
};

use std::{any::Any, ops::Range};

use crate::{
    cosmic_text::{Attrs, AttrsList, FamilyOwned, TextLayout, Wrap},
    style::ComputedStyle,
};
use glazier::{
    keyboard_types::Key,
    kurbo::{Line, Point, Rect, Size},
};

use crate::{
    context::{EventCx, UpdateCx},
    event::Event,
    id::Id,
    view::ChangeFlags,
};

use super::{label, stack, Decorators};

enum InputKind {
    SingleLine,
    MultiLine {
        //TODO:
        line_index: usize,
    },
}

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
    font_size: f32,
    width: f32,
    height: f32,
    font_family: Option<String>,
    font_weight: Option<Weight>,
    font_style: Option<FontStyle>,
    input_kind: InputKind,
    cursor_width: f64, // TODO: make this configurable
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

pub fn text_input(cx: AppContext, buffer: RwSignal<String>) -> TextInput {
    let id = cx.new_id();

    let text = create_effect(cx.scope, move |_| {
        let text: String = buffer.with(|buff| buff.to_string());
        AppContext::update_state(id, text, false);
    });

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
        input_kind: InputKind::SingleLine,
        clip_start_idx: 0,
        clip_offset_x: 0.0,
        clip_start_x: 0.0,
        cursor_width: 1.0,
        width: 0.0,
        height: 0.0,
    }
}

enum ClipDirection {
    None,
    Forward,
    Backward,
}

const DEFAULT_FONT_SIZE: f32 = 14.0;

impl TextInput {
    fn move_cursor(&mut self, move_kind: Movement, direction: Direction) -> bool {
        match (move_kind, direction) {
            (Movement::Glyph, Direction::Left) => {
                if self.cursor_glyph_idx >= 1 {
                    self.cursor_glyph_idx = self.cursor_glyph_idx - 1;
                    return true;
                }
                false
            }
            (Movement::Glyph, Direction::Right) => {
                if self.cursor_glyph_idx < self.buffer.get().len() {
                    self.cursor_glyph_idx = self.cursor_glyph_idx + 1;
                    return true;
                }
                false
            }
            (Movement::Line, Direction::Right) => {
                if self.cursor_glyph_idx < self.buffer.get().len() {
                    self.cursor_glyph_idx = self.buffer.get().len();
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
            InputKind::MultiLine { line_index } => todo!(),
        }
    }

    fn clip_text(&mut self, node_layout: &Layout) {
        let virt_text = self.text_buf.as_ref().unwrap();
        let node_width = node_layout.size.width as f64;
        let cursor_text_loc = Cursor::new(self.get_line_idx(), self.cursor_glyph_idx);
        let layout_cursor = virt_text.layout_cursor(&cursor_text_loc);
        let cursor_glyph_pos = virt_text.hit_position(layout_cursor.glyph);
        let cursor_x = cursor_glyph_pos.point.x;

        let location = node_layout.location;
        let text_start_point = Point::new(location.x as f64, location.y as f64);

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

        let vis_hit_point = virt_text.hit_point(Point::new(self.cursor_x, 0.0));

        let glyph_idx = vis_hit_point.index;

        let new_text = self
            .buffer
            .get()
            .chars()
            .skip(clip_start)
            .take(clip_end - clip_start)
            .collect();

        self.cursor_x = self.cursor_x - clip_start_x;
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

    fn update_text_layout(&mut self) {
        let mut text_layout = TextLayout::new();
        let attrs = self.get_text_attrs();

        let buff = self.buffer.get();
        text_layout.set_text(&buff, attrs.clone());

        self.width = 10.0 * self.font_size;
        self.height = self.font_size;

        // main buff should always get updated
        self.text_buf = Some(text_layout.clone());

        if let Some(cr_text) = self.clipped_text.clone().as_ref() {
            let mut clp_txt_lay = text_layout.clone();
            clp_txt_lay.set_text(&cr_text, attrs);

            self.clip_txt_buf = Some(clp_txt_lay);
        }
    }

    pub fn get_text_attrs(&self) -> AttrsList {
        let mut text_layout = TextLayout::new();
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

    fn handle_key_down(&mut self, cx: &EventCx, event: &glazier::KeyEvent) -> bool {
        match event.key {
            Key::Character(ref ch) => {
                self.buffer
                    .update(|buf| buf.insert_str(self.cursor_glyph_idx, &ch.clone()));
                self.move_cursor(Movement::Glyph, Direction::Right)
            }
            Key::Backspace => {
                if self.buffer.get_untracked().is_empty() {
                    return false;
                }
                self.buffer.update(|buf| {
                    if self.cursor_glyph_idx > 0 {
                        buf.remove(self.cursor_glyph_idx - 1);
                    }
                });
                self.move_cursor(Movement::Glyph, Direction::Left)
            }
            Key::End => self.move_cursor(Movement::Line, Direction::Right),
            Key::Home => self.move_cursor(Movement::Line, Direction::Left),
            Key::ArrowLeft => self.move_cursor(Movement::Glyph, Direction::Left),
            Key::ArrowRight => self.move_cursor(Movement::Glyph, Direction::Right),
            _ => {
                dbg!("Unhandled key");
                false
            }
        }
    }
}

impl View for TextInput {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&mut self, _id: Id) -> Option<&mut dyn View> {
        None
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) -> ChangeFlags {
        if let Ok(state) = state.downcast::<String>() {
            cx.request_layout(self.id());
            ChangeFlags::LAYOUT
        } else {
            dbg!("downcast failed");
            ChangeFlags::empty()
        }
    }

    fn event(&mut self, cx: &mut EventCx, id_path: Option<&[Id]>, event: Event) -> bool {
        let is_focused = cx.app_state.is_focused(&self.id);

        let is_handled = match &event {
            Event::MouseDown(event) if is_focused => {
                self.set_cursor_glyph_idx(self.buffer.get().len());
                true
            }
            Event::KeyDown(event) if is_focused => self.handle_key_down(cx, event),
            Event::MouseDown(event) if is_focused => {
                //TODO: move cursor to click pos
                false
            }
            _ => false,
        };

        if is_handled {
            cx.app_state.request_layout(self.id);
        }

        is_handled
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| {
            if self.text_layout_changed(cx) {
                self.font_size = cx.current_font_size().unwrap_or(DEFAULT_FONT_SIZE);
                self.font_family = cx.current_font_family().map(|s| s.to_string());
                self.font_weight = cx.font_weight;
                self.font_style = cx.font_style;
                self.update_text_layout();
            } else if self.text_buf.is_none() {
                self.update_text_layout();
            }
            let text_layout = self.text_buf.as_ref().unwrap();

            if self.text_node.is_none() {
                self.text_node = Some(
                    cx.app_state
                        .taffy
                        .new_leaf(taffy::style::Style::DEFAULT)
                        .unwrap(),
                );
            }
            let text_node = self.text_node.unwrap();

            let style = Style::BASE
                .width(Dimension::Points(self.width))
                .height(Dimension::Points(self.height))
                .compute(&ComputedStyle::default())
                .to_taffy_style();
            let _ = cx.app_state.taffy.set_style(text_node, style);

            let view = cx.app_state.view_state(self.id);
            let node = view.node;

            let mut main_layout = cx.app_state.taffy.layout(node).cloned().unwrap();

            vec![text_node]
        })
    }

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) {
        let text_node = self.text_node.unwrap();
        let node_layout = cx.app_state.taffy.layout(text_node).unwrap();

        self.update_text_layout();
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if !cx.app_state.is_focused(&self.id) && self.buffer.get().is_empty() {
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
        let buf_width = text_buf.size().width as f64;
        let node_layout = cx.app_state.taffy.layout(text_node).unwrap().clone();
        let node_width = node_layout.size.width as f64;

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

        if cx.app_state.is_focused(&self.id) {
            let cursor_rect = self.get_cursor_rect(&node_layout);
            cx.fill(&cursor_rect, Color::BLACK);
        }
    }
}
