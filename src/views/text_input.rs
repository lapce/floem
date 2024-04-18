use std::rc::Rc;

use crate::{
    action::{set_ime_allowed, set_ime_cursor_area},
    context::EventCx,
    cosmic_text::{Attrs, AttrsList, FamilyOwned, TextLayout},
    event::{Event, EventListener},
    id::Id,
    keyboard::Modifiers,
    peniko::{
        kurbo::{Line, Point, Rect, Size, Vec2},
        Color,
    },
    prop, prop_extractor,
    reactive::{create_effect, create_rw_signal, RwSignal},
    style::{
        CursorColor, CursorStyle, FontFamily, FontSize, FontStyle, FontWeight, LineHeight,
        PaddingLeft, Style, TextColor,
    },
    taffy::prelude::NodeId,
    unit::PxPct,
    view::{AnyWidget, View, ViewData, Widget},
    views::Decorators,
    widgets::{PlaceholderTextClass, TextInputClass},
    EventPropagation, Renderer,
};
use floem_editor_core::{
    buffer::rope_text::RopeText,
    cursor::{Cursor, CursorMode},
    editor::EditType,
    selection::Selection,
};
use floem_winit::keyboard::{Key, NamedKey};

use super::{
    editor::{
        command::CommandExecuted,
        keypress::{default_key_handler, key::KeyInput, press::KeyPress},
        text::{Document, WrapMethod},
        Editor, WrapProp,
    },
    text_editor,
};

prop!(pub SelectionCornerRadius: f64 {} = 0.0);
prop!(pub SelectionColor: Color {} = Color::rgba8(0, 0, 0, 150));

prop_extractor! {
    InputStyle {
        color: TextColor,
        font_size: FontSize,
        font_family: FontFamily,
        font_weight: FontWeight,
        font_style: FontStyle,
        line_height: LineHeight,
        selection_corner_radius: SelectionCornerRadius,
        selection_color: SelectionColor,
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

//TODO add to styles
static IME_FOREGROUND: Color = Color::BLACK;

pub fn text_input(text: impl Fn() -> String + 'static) -> TextInput {
    let id = Id::next();

    let text_editor = text_editor(text());
    let editor = create_rw_signal(text_editor.editor().to_owned());

    let doc = editor.with_untracked(|e| e.doc_signal());
    let cursor = editor.with_untracked(|e| e.cursor);
    let window_origin = create_rw_signal(Point::ZERO);
    let cursor_line = create_rw_signal(Line::new(Point::ZERO, Point::ZERO));
    let is_focused = create_rw_signal(false);
    let hide_cursor = editor.with_untracked(|e| e.cursor_info.hidden);
    let skip_content_sync = create_rw_signal(true);
    let pointer_focus = create_rw_signal(false);

    // Syncs with external changes to the text. This should *only* happen if the closure
    // re-runs due to an external update.
    // For example the closure is subscribed to a signal and the signal is updated by another view.
    // NOTE: The updates are dispatched in the same order they occur, so this is safe to do
    create_effect(move |_| {
        let text = text();
        // The update came from this instance itself(or this is the initial run), so we are already up to date
        if skip_content_sync.get_untracked() {
            skip_content_sync.set(false);
            return;
        }

        // External update
        doc.with_untracked(|d| {
            d.edit_single(
                Selection::region(0, d.text().len()),
                &text,
                EditType::InsertChars,
            );
        });

        id.update_state(TextInputState::ContentExternal(text));
    });

    {
        create_effect(move |_| {
            let doc = doc.get();
            let offset = cursor.with(|c| c.offset());
            let (content, offset, preedit_range) = {
                let content = String::from(doc.rope_text().text());
                if let Some(preedit) = doc.preedit().preedit.get().as_ref() {
                    let mut new_content = String::new();
                    new_content.push_str(&content[..offset]);
                    new_content.push_str(&preedit.text);
                    new_content.push_str(&content[offset..]);
                    let range = (offset, offset + preedit.text.len());
                    let offset = preedit
                        .cursor
                        .as_ref()
                        .map(|(_, end)| offset + *end)
                        .unwrap_or(offset);
                    (new_content, offset, Some(range))
                } else {
                    (content, offset, None)
                }
            };
            id.update_state(TextInputState::Content {
                text: content.clone(),
                offset,
                preedit_range,
            });
        });
    }

    {
        create_effect(move |_| {
            let focus = is_focused.get();

            if focus {
                // Reset blink interval every time we gain focus
                editor.with_untracked(|e| e.cursor_info.reset());
            }
            id.update_state(TextInputState::Focus(focus));
        });

        // Syncs with the blinking state of the cursor.
        // This runs every 500 milliseconds when the input is focused
        create_effect(move |_| {
            let focus = is_focused.get();
            if focus {
                let hidden = editor.with_untracked(|e| e.cursor_info.hidden);
                hidden.track();
                id.request_paint();
            }
        });

        let editor = editor.get();
        let ime_allowed = editor.ime_allowed;
        create_effect(move |_| {
            let focus = is_focused.get();
            if focus {
                if !ime_allowed.get_untracked() {
                    ime_allowed.set(true);
                    set_ime_allowed(true);
                }
                let cursor_line = cursor_line.get();

                let window_origin = window_origin.get();
                let viewport = editor.viewport.get();
                let origin = window_origin
                    + Vec2::new(
                        cursor_line.p1.x - viewport.x0,
                        cursor_line.p1.y - viewport.y0,
                    );
                set_ime_cursor_area(origin, Size::new(800.0, 600.0));
            }
        });
    }

    TextInput {
        id,
        data: ViewData::new(id),
        editor,
        offset: 0,
        preedit_range: None,
        layout_rect: Rect::ZERO,
        content: "".to_string(),
        focused: false,
        text_node: None,
        text_layout: create_rw_signal(None),
        text_rect: Rect::ZERO,
        text_viewport: Rect::ZERO,
        placeholder: "".to_string(),
        placeholder_style: Default::default(),
        placeholder_text_layout: None,
        cursor,
        doc,
        cursor_pos: Point::ZERO,
        pointer_focus,
        hide_cursor,
        style: Default::default(),
        on_update_fn: None,
    }
    .style(|s| {
        s.cursor(CursorStyle::Text)
            .padding_horiz(10.0)
            .padding_vert(6.0)
            .disabled(|s| s.cursor(CursorStyle::Default))
            .min_width(100.0)
            // Otherwise moving to buff start/end will not work properly.
            .set(WrapProp, WrapMethod::None)
    })
    .on_move(move |pos| {
        window_origin.set(pos);
    })
    .on_event_stop(EventListener::FocusGained, move |_| {
        is_focused.set(true);
    })
    .on_event_stop(EventListener::FocusLost, move |_| {
        is_focused.set(false);
        pointer_focus.set(false);
    })
    .on_event(EventListener::KeyDown, move |event| {
        on_keydown(event, editor, cursor, doc, id, skip_content_sync)
    })
    .on_event(EventListener::ImePreedit, move |event| {
        if !is_focused.get_untracked() {
            return EventPropagation::Continue;
        }

        if let Event::ImePreedit {
            text,
            cursor: ime_cursor,
        } = event
        {
            if text.is_empty() {
                editor.get().clear_preedit();
            } else {
                let offset = cursor.with_untracked(|c| c.offset());
                editor
                    .get()
                    .set_preedit(text.clone(), ime_cursor.to_owned(), offset);
            }
        }
        EventPropagation::Stop
    })
    .on_event(EventListener::ImeCommit, move |event| {
        if !is_focused.get_untracked() {
            return EventPropagation::Continue;
        }

        if let Event::ImeCommit(text) = event {
            editor.get().clear_preedit();
            editor.get().receive_char(text.as_str());
            skip_content_sync.set(true);
        }
        EventPropagation::Stop
    })
    .class(TextInputClass)
}

fn on_keydown(
    event: &Event,
    editor: RwSignal<Editor>,
    cursor: RwSignal<Cursor>,
    doc: RwSignal<Rc<dyn Document>>,
    id: Id,
    skip_content_sync: RwSignal<bool>,
) -> EventPropagation {
    let Event::KeyDown(key_event) = event else {
        debug_assert!(false, "Expected keydown event");
        return EventPropagation::Stop;
    };
    let Ok(keypress) = KeyPress::try_from(key_event) else {
        return EventPropagation::Stop;
    };
    if let KeyInput::Keyboard(Key::Named(NamedKey::Enter), _) = keypress.key {
        return EventPropagation::Continue;
    }
    if let KeyInput::Keyboard(Key::Named(NamedKey::Tab), _) = keypress.key {
        cursor.update(|cursor| {
            let len = doc.with_untracked(|d| d.text().len());
            cursor.set_insert(Selection::caret(len));
        });
        return EventPropagation::Continue;
    }
    let old_revision = doc.with_untracked(|d| d.cache_rev().get_untracked());
    let event_handler_fn = default_key_handler(editor);
    let cmd_executed = event_handler_fn(&keypress, key_event.modifiers);

    if cmd_executed == CommandExecuted::Yes {
        // Reset blink interval
        editor.with_untracked(|e| e.cursor_info.reset());
    }

    let mut mods = key_event.modifiers;
    mods.set(Modifiers::SHIFT, false);
    #[cfg(target_os = "macos")]
    mods.set(Modifiers::ALT, false);

    if mods.is_empty() {
        if let KeyInput::Keyboard(Key::Character(c), _) = keypress.key {
            editor.with_untracked(|e| e.receive_char(&c));
        } else if let KeyInput::Keyboard(Key::Named(NamedKey::Space), _) = keypress.key {
            editor.with_untracked(|e| e.receive_char(" "));
        }
    }

    if old_revision != doc.with_untracked(|e| e.cache_rev().get_untracked()) {
        skip_content_sync.set(true);
        id.update_state(TextInputState::DocumentUpdated)
    }
    EventPropagation::Stop
}

#[derive(Debug)]
enum TextInputState {
    Content {
        text: String,
        offset: usize,
        preedit_range: Option<(usize, usize)>,
    },
    /// The closure we depend on was updated externally
    ContentExternal(String),
    DocumentUpdated,
    Focus(bool),
    Placeholder(String),
}

pub struct TextInput {
    id: Id,
    data: ViewData,
    editor: RwSignal<Editor>,
    content: String,
    offset: usize,
    preedit_range: Option<(usize, usize)>,
    doc: RwSignal<Rc<dyn Document>>,
    cursor: RwSignal<Cursor>,
    focused: bool,
    text_node: Option<NodeId>,
    text_layout: RwSignal<Option<TextLayout>>,
    text_rect: Rect,
    text_viewport: Rect,
    layout_rect: Rect,
    placeholder: String,
    placeholder_text_layout: Option<TextLayout>,
    cursor_pos: Point,
    hide_cursor: RwSignal<bool>,
    /// Pointer moves cursor to the clicked position, while keyboard(or programatic) focus moves it to the end.
    pointer_focus: RwSignal<bool>,
    on_update_fn: Option<Box<dyn Fn(String)>>,
    style: InputStyle,
    placeholder_style: PlaceholderStyle,
}

impl TextInput {
    pub fn placeholder(self, placeholder: impl Fn() -> String + 'static) -> Self {
        let id = self.id;
        create_effect(move |_| {
            let placeholder = placeholder();
            id.update_state(TextInputState::Placeholder(placeholder));
        });
        self
    }

    pub fn static_placeholder(self, text: impl Into<String>) -> Self {
        self.id
            .update_state(TextInputState::Placeholder(text.into()));
        self
    }

    pub fn on_update(mut self, action: impl Fn(String) + 'static) -> Self {
        self.on_update_fn = Some(Box::new(action));
        self
    }

    fn set_text_layout(&mut self) {
        let mut text_layout = TextLayout::new();
        let mut attrs = Attrs::new().color(self.style.color().unwrap_or(Color::BLACK));
        if let Some(font_size) = self.style.font_size() {
            attrs = attrs.font_size(font_size);
        }
        if let Some(font_style) = self.style.font_style() {
            attrs = attrs.style(font_style);
        }
        let font_family = self.style.font_family().as_ref().map(|font_family| {
            let family: Vec<FamilyOwned> = FamilyOwned::parse_list(font_family).collect();
            family
        });
        if let Some(font_family) = font_family.as_ref() {
            attrs = attrs.family(font_family);
        }
        if let Some(font_weight) = self.style.font_weight() {
            attrs = attrs.weight(font_weight);
        }
        if let Some(line_height) = self.style.line_height() {
            attrs = attrs.line_height(line_height);
        }
        text_layout.set_text(
            if self.content.is_empty() {
                " "
            } else {
                self.content.as_str()
            },
            AttrsList::new(attrs),
        );
        self.text_layout.set(Some(text_layout));

        let mut placeholder_text_layout = TextLayout::new();
        attrs = attrs.color(
            self.placeholder_style
                .color()
                .unwrap_or(Color::BLACK.with_alpha_factor(0.5)),
        );

        if let Some(font_style) = self.placeholder_style.font_style() {
            attrs = attrs.style(font_style);
        }
        if let Some(font_weight) = self.placeholder_style.font_weight() {
            attrs = attrs.weight(font_weight);
        }
        placeholder_text_layout.set_text(&self.placeholder, AttrsList::new(attrs));
        self.placeholder_text_layout = Some(placeholder_text_layout);
    }

    fn hit_index(&self, cx: &mut EventCx, point: Point) -> usize {
        self.text_layout.with_untracked(|text_layout| {
            if let Some(text_layout) = text_layout.as_ref() {
                let padding_left = cx
                    .get_computed_style(self.id)
                    .map(|s| match s.get(PaddingLeft) {
                        PxPct::Px(v) => v,
                        PxPct::Pct(pct) => {
                            let layout = cx.get_layout(self.id()).unwrap();
                            pct * layout.size.width as f64
                        }
                    })
                    .unwrap_or(0.0);
                let hit = text_layout.hit_point(Point::new(point.x - padding_left, 0.0));
                hit.index.min(self.content.len())
            } else {
                0
            }
        })
    }

    fn clamp_text_viewport(&mut self, text_viewport: Rect) {
        let text_rect = self.text_rect;
        let actual_size = text_rect.size();
        let width = text_rect.width();
        let height = text_rect.height();
        let child_size = self
            .text_layout
            .with_untracked(|text_layout| text_layout.as_ref().unwrap().size());

        let mut text_viewport = text_viewport;
        if width >= child_size.width {
            text_viewport.x0 = 0.0;
        } else if text_viewport.x0 > child_size.width - width {
            text_viewport.x0 = child_size.width - width;
        } else if text_viewport.x0 < 0.0 {
            text_viewport.x0 = 0.0;
        }

        if height >= child_size.height {
            text_viewport.y0 = 0.0;
        } else if text_viewport.y0 > child_size.height - height {
            text_viewport.y0 = child_size.height - height;
        } else if text_viewport.y0 < 0.0 {
            text_viewport.y0 = 0.0;
        }

        let text_viewport = text_viewport.with_size(actual_size);
        if text_viewport != self.text_viewport {
            self.text_viewport = text_viewport;
            self.id.request_paint();
        }
    }

    fn ensure_cursor_visible(&mut self) {
        fn closest_on_axis(val: f64, min: f64, max: f64) -> f64 {
            assert!(min <= max);
            if val > min && val < max {
                0.0
            } else if val <= min {
                val - min
            } else {
                val - max
            }
        }

        let rect = Rect::ZERO.with_origin(self.cursor_pos).inflate(10.0, 0.0);
        // Clamp the target region size to our own size.
        // This means we will show the portion of the target region that includes the origin.
        let target_size = Size::new(
            rect.width().min(self.text_viewport.width()),
            rect.height().min(self.text_viewport.height()),
        );
        let rect = rect.with_size(target_size);

        let x0 = closest_on_axis(
            rect.min_x(),
            self.text_viewport.min_x(),
            self.text_viewport.max_x(),
        );
        let x1 = closest_on_axis(
            rect.max_x(),
            self.text_viewport.min_x(),
            self.text_viewport.max_x(),
        );
        let y0 = closest_on_axis(
            rect.min_y(),
            self.text_viewport.min_y(),
            self.text_viewport.max_y(),
        );
        let y1 = closest_on_axis(
            rect.max_y(),
            self.text_viewport.min_y(),
            self.text_viewport.max_y(),
        );

        let delta_x = if x0.abs() > x1.abs() { x0 } else { x1 };
        let delta_y = if y0.abs() > y1.abs() { y0 } else { y1 };
        let new_origin = self.text_viewport.origin() + Vec2::new(delta_x, delta_y);
        self.clamp_text_viewport(self.text_viewport.with_origin(new_origin));
    }

    fn paint_cursor(&self, cx: &mut crate::context::PaintCx<'_>, text_layout: &TextLayout) {
        if !self.hide_cursor.get_untracked() && (self.focused || cx.is_focused(self.id)) {
            cx.clip(&self.text_rect.inflate(2.0, 2.0));

            let hit_position = text_layout.hit_position(self.offset);
            let cursor_point = hit_position.point + self.layout_rect.origin().to_vec2()
                - self.text_viewport.origin().to_vec2();

            let line = Line::new(
                Point::new(cursor_point.x, cursor_point.y - hit_position.glyph_ascent),
                Point::new(cursor_point.x, cursor_point.y + hit_position.glyph_descent),
            );
            let style = cx.app_state.get_computed_style(self.id());
            let cursor_color = style.get(CursorColor).unwrap_or(Color::BLACK);
            cx.stroke(&line, cursor_color, 2.0);
        }
    }

    fn paint_selection(
        &self,
        cx: &mut crate::context::PaintCx<'_>,
        text_layout: &TextLayout,
        point: Point,
    ) {
        if !cx.is_focused(self.id) {
            return;
        }
        let selection_color = self.style.selection_color();
        let sel_corner_radius = self.style.selection_corner_radius();

        let height = text_layout.size().height;
        let cursor = self.cursor.get_untracked();

        if let CursorMode::Insert(selection) = &cursor.mode {
            for region in selection.regions() {
                if !region.is_caret() {
                    let min = text_layout.hit_position(region.min()).point.x;
                    let max = text_layout.hit_position(region.max()).point.x;
                    cx.fill(
                        &Rect::ZERO
                            .with_size(Size::new(max - min, height))
                            .with_origin(Point::new(min + point.x, point.y))
                            .to_rounded_rect(sel_corner_radius),
                        selection_color,
                        0.0,
                    );
                }
            }
        }
    }

    fn paint_preedit(&self, cx: &mut crate::context::PaintCx<'_>, text_layout: &TextLayout) {
        if let Some((start, end)) = self.preedit_range {
            let start_position = text_layout.hit_position(start);
            let start_point = start_position.point + self.layout_rect.origin().to_vec2()
                - self.text_viewport.origin().to_vec2();
            let end_position = text_layout.hit_position(end);
            let end_point = end_position.point + self.layout_rect.origin().to_vec2()
                - self.text_viewport.origin().to_vec2();

            let line = Line::new(
                Point::new(start_point.x, start_point.y + start_position.glyph_descent),
                Point::new(end_point.x, end_point.y + end_position.glyph_descent),
            );
            cx.stroke(&line, IME_FOREGROUND, 1.0);
        }
    }
}

impl View for TextInput {
    fn id(&self) -> Id {
        self.id
    }

    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn build(self) -> AnyWidget {
        Box::new(self)
    }
}
impl Widget for TextInput {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast() {
            match *state {
                TextInputState::Content {
                    text,
                    offset,
                    preedit_range,
                } => {
                    self.content = text;
                    self.offset = offset;
                    self.preedit_range = preedit_range;
                    self.text_layout.set(None);
                }
                TextInputState::ContentExternal(text) => {
                    self.content = text;
                    self.text_layout.set(None);
                }
                TextInputState::Focus(focus) => {
                    self.focused = focus;
                    // NOTE: Moving cursor for pointer focus is handled in the event method, since we need to know
                    // the click pos
                    if !self.pointer_focus.get_untracked() {
                        self.cursor.update(|cursor| {
                            cursor.set_insert(Selection::caret(self.content.len()));
                        });
                    }
                }
                TextInputState::DocumentUpdated => {
                    if let Some(func) = self.on_update_fn.as_ref() {
                        func(self.content.clone())
                    }
                }
                TextInputState::Placeholder(placeholder) => {
                    self.placeholder = placeholder;
                    self.placeholder_text_layout = None;
                }
            }
            cx.request_layout(self.id);
        }
    }

    fn style(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        let style = cx.style();
        let id = self.id();
        let placeholder_style = style.clone().apply_class(PlaceholderTextClass);
        if self.placeholder_style.read_style(cx, &placeholder_style) {
            cx.app_state_mut().request_paint(id);
        }

        if self.style.read(cx) {
            self.set_text_layout();
            cx.app_state_mut().request_layout(id);
        }

        // To set wrap method to `None`
        self.editor.with_untracked(|ed| {
            ed.es.update(|s| {
                if s.read(cx) {
                    ed.floem_style_id.update(|val| *val += 1);
                    cx.app_state_mut().request_paint(id);
                }
            })
        });
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> crate::taffy::prelude::NodeId {
        cx.layout_node(self.id, true, |cx| {
            if self
                .text_layout
                .with_untracked(|text_layout| text_layout.is_none())
                || self.placeholder_text_layout.is_none()
            {
                self.set_text_layout();
            }

            let text_layout = self.text_layout;
            text_layout.with_untracked(|text_layout| {
                let text_layout = text_layout.as_ref().unwrap();

                let offset = self.cursor.get_untracked().offset();
                let cursor_point = text_layout.hit_position(offset).point;
                if cursor_point != self.cursor_pos {
                    self.cursor_pos = cursor_point;
                    self.ensure_cursor_visible();
                }

                let size = text_layout.size();
                let height = size.height as f32;

                if self.text_node.is_none() {
                    self.text_node = Some(cx.new_node());
                }

                let text_node = self.text_node.unwrap();

                let style = Style::new().height(height).to_taffy_style();
                cx.set_style(text_node, style);
            });

            vec![self.text_node.unwrap()]
        })
    }

    fn compute_layout(&mut self, cx: &mut crate::context::ComputeLayoutCx) -> Option<Rect> {
        let layout = cx.get_layout(self.id).unwrap();

        let style = cx.app_state_mut().get_builtin_style(self.id);
        let padding_left = match style.padding_left() {
            PxPct::Px(padding) => padding,
            PxPct::Pct(pct) => pct * layout.size.width as f64,
        };
        let padding_right = match style.padding_right() {
            PxPct::Px(padding) => padding,
            PxPct::Pct(pct) => pct * layout.size.width as f64,
        };

        let size = Size::new(layout.size.width as f64, layout.size.height as f64);
        let mut text_rect = size.to_rect();
        text_rect.x0 += padding_left;
        text_rect.x1 -= padding_right;
        self.text_rect = text_rect;

        self.clamp_text_viewport(self.text_viewport);

        let text_node = self.text_node.unwrap();
        let location = cx.layout(text_node).unwrap().location;
        self.layout_rect = size
            .to_rect()
            .with_origin(Point::new(location.x as f64, location.y as f64));

        None
    }

    fn event(
        &mut self,
        cx: &mut crate::context::EventCx,
        _id_path: Option<&[Id]>,
        event: crate::event::Event,
    ) -> EventPropagation {
        let text_offset = self.text_viewport.origin();
        let event = event.offset((-text_offset.x, -text_offset.y));
        match event {
            Event::PointerDown(pointer) => {
                let offset = self.hit_index(cx, pointer.pos);

                if !self.focused {
                    self.pointer_focus.set(true);
                }

                self.cursor.update(|cursor| {
                    cursor.set_insert(Selection::caret(offset));
                });
                if pointer.button.is_primary() && pointer.count == 2 {
                    let offset = self.hit_index(cx, pointer.pos);
                    let rope = self.doc.with_untracked(|d| d.rope_text());

                    let (start, end) = rope.select_word(offset);
                    self.cursor.update(|cursor| {
                        cursor.set_insert(Selection::region(start, end));
                    });
                } else if pointer.button.is_primary() && pointer.count == 3 {
                    self.cursor.update(|cursor| {
                        cursor.set_insert(Selection::region(0, self.content.len()));
                    });
                }
                cx.update_active(self.id);
            }
            Event::PointerMove(pointer) => {
                if cx.is_active(self.id) {
                    let offset = self.hit_index(cx, pointer.pos);
                    self.cursor.update(|cursor| {
                        cursor.set_offset(offset, true, false);
                    });
                }
            }
            Event::PointerWheel(pointer_event) => {
                let delta = pointer_event.delta;
                let delta = if delta.x == 0.0 && delta.y != 0.0 {
                    Vec2::new(delta.y, delta.x)
                } else {
                    delta
                };
                self.clamp_text_viewport(self.text_viewport + delta);
                return EventPropagation::Continue;
            }
            _ => {}
        }
        EventPropagation::Continue
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.save();
        cx.clip(&self.text_rect.inflate(1.0, 0.0));

        let text_node = self.text_node.unwrap();
        let location = cx.layout(text_node).unwrap().location;
        let point = Point::new(location.x as f64, location.y as f64)
            - self.text_viewport.origin().to_vec2();

        self.text_layout.with_untracked(|text_layout| {
            let text_layout = text_layout.as_ref().unwrap();

            self.paint_selection(cx, text_layout, point);

            if !self.content.is_empty() {
                cx.draw_text(text_layout, point);
            } else if !self.placeholder.is_empty() {
                cx.draw_text(self.placeholder_text_layout.as_ref().unwrap(), point);
            }

            self.paint_preedit(cx, text_layout);

            self.paint_cursor(cx, text_layout);

            cx.restore();
        });
    }
}
