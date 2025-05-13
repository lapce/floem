use core::indent::IndentStyle;
use std::{
    cell::{Cell, RefCell},
    cmp::Ordering,
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use crate::{
    action::{TimerToken, exec_after},
    kurbo::{Point, Rect, Vec2},
    peniko::{Color, color::palette},
    prop, prop_extractor,
    reactive::{ReadSignal, RwSignal, Scope, batch, untrack},
    style::{CursorColor, StylePropValue, TextColor},
    text::{Attrs, AttrsList, LineHeightValue, TextLayout, Wrap},
    view::{IntoView, View},
    views::text,
};
use floem_editor_core::{
    buffer::rope_text::{RopeText, RopeTextVal},
    command::MoveCommand,
    cursor::{ColPosition, Cursor, CursorAffinity, CursorMode},
    mode::Mode,
    movement::Movement,
    register::Register,
    selection::Selection,
    soft_tab::{SnapDirection, snap_to_soft_tab_line_col},
};
use floem_reactive::{SignalGet, SignalTrack, SignalUpdate, SignalWith, Trigger};
use lapce_xi_rope::Rope;

pub mod actions;
pub mod color;
pub mod command;
pub mod gutter;
pub mod id;
pub mod keypress;
pub mod layout;
pub mod listener;
pub mod movement;
pub mod phantom_text;
pub mod text;
pub mod text_document;
pub mod view;
pub mod visual_line;

pub use floem_editor_core as core;
use peniko::Brush;
use ui_events::{keyboard::Modifiers, pointer::PointerState};

use self::{
    command::Command,
    id::EditorId,
    layout::TextLayoutLine,
    phantom_text::PhantomTextLine,
    text::{Document, Preedit, PreeditData, RenderWhitespace, Styling, WrapMethod},
    view::{LineInfo, ScreenLines, ScreenLinesBase},
    visual_line::{
        ConfigId, FontSizeCacheId, LayoutEvent, LineFontSizeProvider, Lines, RVLine, ResolvedWrap,
        TextLayoutProvider, VLine, VLineInfo, hit_position_aff,
    },
};

prop!(pub WrapProp: WrapMethod {} = WrapMethod::EditorWidth);
impl StylePropValue for WrapMethod {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(crate::views::text(self).into_any())
    }
}
prop!(pub CursorSurroundingLines: usize {} = 1);
prop!(pub ScrollBeyondLastLine: bool {} = false);
prop!(pub ShowIndentGuide: bool {} = false);
prop!(pub Modal: bool {} = false);
prop!(pub ModalRelativeLine: bool {} = false);
prop!(pub SmartTab: bool {} = false);
prop!(pub PhantomColor: Color {} = palette::css::DIM_GRAY);
prop!(pub PlaceholderColor: Color {} = palette::css::DIM_GRAY);
prop!(pub PreeditUnderlineColor: Color {} = palette::css::WHITE);
prop!(pub RenderWhitespaceProp: RenderWhitespace {} = RenderWhitespace::None);
impl StylePropValue for RenderWhitespace {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(crate::views::text(self).into_any())
    }
}
prop!(pub IndentStyleProp: IndentStyle {} = IndentStyle::Spaces(4));
impl StylePropValue for IndentStyle {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(text(self).into_any())
    }
}
prop!(pub DropdownShadow: Option<Color> {} = None);
prop!(pub Foreground: Color { inherited } = Color::from_rgb8(0x38, 0x3A, 0x42));
prop!(pub Focus: Option<Color> {} = None);
prop!(pub SelectionColor: Color {} = palette::css::BLACK.with_alpha(0.5));
prop!(pub CurrentLineColor: Option<Color> {  } = None);
prop!(pub Link: Option<Color> {} = None);
prop!(pub VisibleWhitespaceColor: Color {} = palette::css::TRANSPARENT);
prop!(pub IndentGuideColor: Color {} = palette::css::TRANSPARENT);
prop!(pub StickyHeaderBackground: Option<Color> {} = None);

prop_extractor! {
    pub EditorStyle {
        pub text_color: TextColor,
        pub phantom_color: PhantomColor,
        pub placeholder_color: PlaceholderColor,
        pub preedit_underline_color: PreeditUnderlineColor,
        pub show_indent_guide: ShowIndentGuide,
        pub modal: Modal,
        // Whether line numbers are relative in modal mode
        pub modal_relative_line: ModalRelativeLine,
        // Whether to insert the indent that is detected for the file when a tab character
        // is inputted.
        pub smart_tab: SmartTab,
        pub wrap_method: WrapProp,
        pub cursor_surrounding_lines: CursorSurroundingLines,
        pub render_whitespace: RenderWhitespaceProp,
        pub indent_style: IndentStyleProp,
        pub caret: CursorColor,
        pub selection: SelectionColor,
        pub current_line: CurrentLineColor,
        pub visible_whitespace: VisibleWhitespaceColor,
        pub indent_guide: IndentGuideColor,
        pub scroll_beyond_last_line: ScrollBeyondLastLine,
    }
}
impl EditorStyle {
    pub fn ed_text_color(&self) -> Color {
        self.text_color().unwrap_or(palette::css::BLACK)
    }
}
impl EditorStyle {
    pub fn ed_caret(&self) -> Brush {
        self.caret()
    }
}

pub(crate) const CHAR_WIDTH: f64 = 7.5;

/// The main structure for the editor view itself.  
/// This can be considered to be the data part of the `View`.
/// It holds an `Rc<dyn Document>` within as the document it is a view into.  
#[derive(Clone)]
pub struct Editor {
    pub cx: Cell<Scope>,
    effects_cx: Cell<Scope>,

    id: EditorId,

    pub active: RwSignal<bool>,

    /// Whether you can edit within this editor.
    pub read_only: RwSignal<bool>,

    pub(crate) doc: RwSignal<Rc<dyn Document>>,
    pub(crate) style: RwSignal<Rc<dyn Styling>>,

    pub cursor: RwSignal<Cursor>,

    pub window_origin: RwSignal<Point>,
    pub viewport: RwSignal<Rect>,
    pub parent_size: RwSignal<Rect>,

    pub editor_view_focused: Trigger,
    pub editor_view_focus_lost: Trigger,
    pub editor_view_id: RwSignal<Option<crate::id::ViewId>>,

    /// The current scroll position.
    pub scroll_delta: RwSignal<Vec2>,
    pub scroll_to: RwSignal<Option<Vec2>>,

    /// Holds the cache of the lines and provides many utility functions for them.
    lines: Rc<Lines>,
    pub screen_lines: RwSignal<ScreenLines>,

    /// Modal mode register
    pub register: RwSignal<Register>,
    /// Cursor rendering information, such as the cursor blinking state.
    pub cursor_info: CursorInfo,

    pub last_movement: RwSignal<Movement>,

    /// Whether ime input is allowed.  
    /// Should not be set manually outside of the specific handling for ime.
    pub ime_allowed: RwSignal<bool>,

    /// The Editor Style
    pub es: RwSignal<EditorStyle>,

    pub floem_style_id: RwSignal<u64>,
}
impl Editor {
    /// Create a new editor into the given document, using the styling.  
    /// `doc`: The backing [`Document`], such as [`TextDocument`](self::text_document::TextDocument)
    /// `style`: How the editor should be styled, such as [`SimpleStyling`](self::text::SimpleStyling)
    pub fn new(cx: Scope, doc: Rc<dyn Document>, style: Rc<dyn Styling>, modal: bool) -> Editor {
        let id = EditorId::next();
        Editor::new_id(cx, id, doc, style, modal)
    }

    /// Create a new editor into the given document, using the styling.  
    /// `id` should typically be constructed by [`EditorId::next`]  
    /// `doc`: The backing [`Document`], such as [`TextDocument`](self::text_document::TextDocument)
    /// `style`: How the editor should be styled, such as [`SimpleStyling`](self::text::SimpleStyling)
    pub fn new_id(
        cx: Scope,
        id: EditorId,
        doc: Rc<dyn Document>,
        style: Rc<dyn Styling>,
        modal: bool,
    ) -> Editor {
        let editor = Editor::new_direct(cx, id, doc, style, modal);
        editor.recreate_view_effects();

        editor
    }

    // TODO: shouldn't this accept an `RwSignal<Rc<dyn Document>>` so that it can listen for
    // changes in other editors?
    // TODO: should we really allow callers to arbitrarily specify the Id? That could open up
    // confusing behavior.

    /// Create a new editor into the given document, using the styling.  
    /// `id` should typically be constructed by [`EditorId::next`]  
    /// `doc`: The backing [`Document`], such as [`TextDocument`](self::text_document::TextDocument)
    /// `style`: How the editor should be styled, such as [`SimpleStyling`](self::text::SimpleStyling)
    /// This does *not* create the view effects. Use this if you're creating an editor and then
    /// replacing signals. Invoke [`Editor::recreate_view_effects`] when you are done.
    /// ```rust,ignore
    /// let shared_scroll_beyond_last_line = /* ... */;
    /// let editor = Editor::new_direct(cx, id, doc, style);
    /// editor.scroll_beyond_last_line.set(shared_scroll_beyond_last_line);
    /// ```
    pub fn new_direct(
        cx: Scope,
        id: EditorId,
        doc: Rc<dyn Document>,
        style: Rc<dyn Styling>,
        modal: bool,
    ) -> Editor {
        let cx = cx.create_child();

        let viewport = cx.create_rw_signal(Rect::ZERO);
        let cursor_mode = if modal {
            CursorMode::Normal(0)
        } else {
            CursorMode::Insert(Selection::caret(0))
        };
        let cursor = Cursor::new(cursor_mode, None, None);
        let cursor = cx.create_rw_signal(cursor);

        let doc = cx.create_rw_signal(doc);
        let style = cx.create_rw_signal(style);

        let font_sizes = RefCell::new(Rc::new(EditorFontSizes {
            id,
            style: style.read_only(),
            doc: doc.read_only(),
        }));
        let lines = Rc::new(Lines::new(cx, font_sizes));
        let screen_lines = cx.create_rw_signal(ScreenLines::new(cx, viewport.get_untracked()));

        let editor_style = cx.create_rw_signal(EditorStyle::default());

        let ed = Editor {
            cx: Cell::new(cx),
            effects_cx: Cell::new(cx.create_child()),
            id,
            active: cx.create_rw_signal(false),
            read_only: cx.create_rw_signal(false),
            doc,
            style,
            cursor,
            window_origin: cx.create_rw_signal(Point::ZERO),
            viewport,
            parent_size: cx.create_rw_signal(Rect::ZERO),
            scroll_delta: cx.create_rw_signal(Vec2::ZERO),
            scroll_to: cx.create_rw_signal(None),
            editor_view_focused: cx.create_trigger(),
            editor_view_focus_lost: cx.create_trigger(),
            editor_view_id: cx.create_rw_signal(None),
            lines,
            screen_lines,
            register: cx.create_rw_signal(Register::default()),
            cursor_info: CursorInfo::new(cx),
            last_movement: cx.create_rw_signal(Movement::Left),
            ime_allowed: cx.create_rw_signal(false),
            es: editor_style,
            floem_style_id: cx.create_rw_signal(0),
        };

        create_view_effects(ed.effects_cx.get(), &ed);

        ed
    }

    pub fn id(&self) -> EditorId {
        self.id
    }

    /// Get the document untracked
    pub fn doc(&self) -> Rc<dyn Document> {
        self.doc.get_untracked()
    }

    pub fn doc_track(&self) -> Rc<dyn Document> {
        self.doc.get()
    }

    // TODO: should this be `ReadSignal`? but read signal doesn't have .track
    pub fn doc_signal(&self) -> RwSignal<Rc<dyn Document>> {
        self.doc
    }

    pub fn config_id(&self) -> ConfigId {
        let style_id = self.style.with(|s| s.id());
        let floem_style_id = self.floem_style_id;
        ConfigId::new(style_id, floem_style_id.get_untracked())
    }

    pub fn recreate_view_effects(&self) {
        batch(|| {
            self.effects_cx.get().dispose();
            self.effects_cx.set(self.cx.get().create_child());
            create_view_effects(self.effects_cx.get(), self);
        });
    }

    /// Swap the underlying document out
    pub fn update_doc(&self, doc: Rc<dyn Document>, styling: Option<Rc<dyn Styling>>) {
        batch(|| {
            // Get rid of all the effects
            self.effects_cx.get().dispose();

            *self.lines.font_sizes.borrow_mut() = Rc::new(EditorFontSizes {
                id: self.id(),
                style: self.style.read_only(),
                doc: self.doc.read_only(),
            });
            self.lines.clear(0, None);
            self.doc.set(doc);
            if let Some(styling) = styling {
                self.style.set(styling);
            }
            self.screen_lines.update(|screen_lines| {
                screen_lines.clear(self.viewport.get_untracked());
            });

            // Recreate the effects
            self.effects_cx.set(self.cx.get().create_child());
            create_view_effects(self.effects_cx.get(), self);
        });
    }

    pub fn update_styling(&self, styling: Rc<dyn Styling>) {
        batch(|| {
            // Get rid of all the effects
            self.effects_cx.get().dispose();

            *self.lines.font_sizes.borrow_mut() = Rc::new(EditorFontSizes {
                id: self.id(),
                style: self.style.read_only(),
                doc: self.doc.read_only(),
            });
            self.lines.clear(0, None);

            self.style.set(styling);

            self.screen_lines.update(|screen_lines| {
                screen_lines.clear(self.viewport.get_untracked());
            });

            // Recreate the effects
            self.effects_cx.set(self.cx.get().create_child());
            create_view_effects(self.effects_cx.get(), self);
        });
    }

    pub fn duplicate(&self, editor_id: Option<EditorId>) -> Editor {
        let doc = self.doc();
        let style = self.style();
        let mut editor = Editor::new_direct(
            self.cx.get(),
            editor_id.unwrap_or_else(EditorId::next),
            doc,
            style,
            false,
        );

        batch(|| {
            editor.read_only.set(self.read_only.get_untracked());
            editor.es.set(self.es.get_untracked());
            editor
                .floem_style_id
                .set(self.floem_style_id.get_untracked());
            editor.cursor.set(self.cursor.get_untracked());
            editor.scroll_delta.set(self.scroll_delta.get_untracked());
            editor.scroll_to.set(self.scroll_to.get_untracked());
            editor.window_origin.set(self.window_origin.get_untracked());
            editor.viewport.set(self.viewport.get_untracked());
            editor.parent_size.set(self.parent_size.get_untracked());
            editor.register.set(self.register.get_untracked());
            editor.cursor_info = self.cursor_info.clone();
            editor.last_movement.set(self.last_movement.get_untracked());
            // ?
            // editor.ime_allowed.set(self.ime_allowed.get_untracked());
        });

        editor.recreate_view_effects();

        editor
    }

    /// Get the styling untracked
    pub fn style(&self) -> Rc<dyn Styling> {
        self.style.get_untracked()
    }

    /// Get the text of the document  
    /// You should typically prefer [`Self::rope_text`]
    pub fn text(&self) -> Rope {
        self.doc().text()
    }

    /// Get the [`RopeTextVal`] from `doc` untracked
    pub fn rope_text(&self) -> RopeTextVal {
        self.doc().rope_text()
    }

    pub fn lines(&self) -> &Lines {
        &self.lines
    }

    pub fn text_prov(&self) -> &Self {
        self
    }

    fn preedit(&self) -> PreeditData {
        self.doc.with_untracked(|doc| doc.preedit())
    }

    pub fn set_preedit(&self, text: String, cursor: Option<(usize, usize)>, offset: usize) {
        batch(|| {
            self.preedit().preedit.set(Some(Preedit {
                text,
                cursor,
                offset,
            }));

            self.doc().cache_rev().update(|cache_rev| {
                *cache_rev += 1;
            });
        });
    }

    pub fn clear_preedit(&self) {
        let preedit = self.preedit();
        if preedit.preedit.with_untracked(|preedit| preedit.is_none()) {
            return;
        }

        batch(|| {
            preedit.preedit.set(None);
            self.doc().cache_rev().update(|cache_rev| {
                *cache_rev += 1;
            });
        });
    }

    pub fn receive_char(&self, c: &str) {
        self.doc().receive_char(self, c)
    }

    fn compute_screen_lines(&self, base: RwSignal<ScreenLinesBase>) -> ScreenLines {
        // This function *cannot* access `ScreenLines` with how it is currently implemented.
        // This is being called from within an update to screen lines.

        self.doc().compute_screen_lines(self, base)
    }

    /// Default handler for `PointerDown` event
    pub fn pointer_down_primary(&self, state: &PointerState<Point>) {
        self.active.set(true);
        self.left_click(state);
    }

    pub fn left_click(&self, state: &PointerState<Point>) {
        match state.count {
            1 => {
                self.single_click(state);
            }
            2 => {
                self.double_click(state);
            }
            3 => {
                self.triple_click(state);
            }
            _ => {}
        }
    }

    pub fn single_click(&self, pointer_event: &PointerState<Point>) {
        let mode = self.cursor.with_untracked(|c| c.get_mode());
        let (new_offset, _) = self.offset_of_point(mode, pointer_event.position);
        self.cursor.update(|cursor| {
            cursor.set_offset(
                new_offset,
                pointer_event.modifiers.shift(),
                pointer_event.modifiers.alt(),
            )
        });
    }

    pub fn double_click(&self, pointer_event: &PointerState<Point>) {
        let mode = self.cursor.with_untracked(|c| c.get_mode());
        let (mouse_offset, _) = self.offset_of_point(mode, pointer_event.position);
        let (start, end) = self.select_word(mouse_offset);

        self.cursor.update(|cursor| {
            cursor.add_region(
                start,
                end,
                pointer_event.modifiers.shift(),
                pointer_event.modifiers.alt(),
            )
        });
    }

    pub fn triple_click(&self, pointer_event: &PointerState<Point>) {
        let mode = self.cursor.with_untracked(|c| c.get_mode());
        let (mouse_offset, _) = self.offset_of_point(mode, pointer_event.position);
        let line = self.line_of_offset(mouse_offset);
        let start = self.offset_of_line(line);
        let end = self.offset_of_line(line + 1);

        self.cursor.update(|cursor| {
            cursor.add_region(
                start,
                end,
                pointer_event.modifiers.shift(),
                pointer_event.modifiers.alt(),
            )
        });
    }

    pub fn pointer_move(&self, pointer_event: &PointerState<Point>) {
        let mode = self.cursor.with_untracked(|c| c.get_mode());
        let (offset, _is_inside) = self.offset_of_point(mode, pointer_event.position);
        if self.active.get_untracked() && self.cursor.with_untracked(|c| c.offset()) != offset {
            self.cursor
                .update(|cursor| cursor.set_offset(offset, true, pointer_event.modifiers.alt()));
        }
    }

    pub fn pointer_up(&self, _pointer_event: &PointerState<Point>) {
        self.active.set(false);
    }

    fn right_click(&self, pointer_event: &PointerState<Point>) {
        let mode = self.cursor.with_untracked(|c| c.get_mode());
        let (offset, _) = self.offset_of_point(mode, pointer_event.position);
        let doc = self.doc();
        let pointer_inside_selection = self
            .cursor
            .with_untracked(|c| c.edit_selection(&doc.rope_text()).contains(offset));
        if !pointer_inside_selection {
            // move cursor to pointer position if outside current selection
            self.single_click(pointer_event);
        }
    }

    // TODO: should this have modifiers state in its api
    pub fn page_move(&self, down: bool, mods: Modifiers) {
        let viewport = self.viewport.get_untracked();
        // TODO: don't assume line height is constant
        let line_height = f64::from(self.line_height(0));
        let lines = (viewport.height() / line_height / 2.0).round() as usize;
        let distance = (lines as f64) * line_height;
        self.scroll_delta
            .set(Vec2::new(0.0, if down { distance } else { -distance }));
        let cmd = if down {
            MoveCommand::Down
        } else {
            MoveCommand::Up
        };
        let cmd = Command::Move(cmd);
        self.doc().run_command(self, &cmd, Some(lines), mods);
    }

    pub fn center_window(&self) {
        let viewport = self.viewport.get_untracked();
        // TODO: don't assume line height is constant
        let line_height = f64::from(self.line_height(0));
        let offset = self.cursor.with_untracked(|cursor| cursor.offset());
        let (line, _col) = self.offset_to_line_col(offset);

        let viewport_center = viewport.height() / 2.0;

        let current_line_position = line as f64 * line_height;

        let desired_top = current_line_position - viewport_center + (line_height / 2.0);

        let scroll_delta = desired_top - viewport.y0;

        self.scroll_delta.set(Vec2::new(0.0, scroll_delta));
    }

    pub fn top_of_window(&self, scroll_off: usize) {
        let viewport = self.viewport.get_untracked();
        // TODO: don't assume line height is constant
        let line_height = f64::from(self.line_height(0));
        let offset = self.cursor.with_untracked(|cursor| cursor.offset());
        let (line, _col) = self.offset_to_line_col(offset);

        let desired_top = (line.saturating_sub(scroll_off)) as f64 * line_height;

        let scroll_delta = desired_top - viewport.y0;

        self.scroll_delta.set(Vec2::new(0.0, scroll_delta));
    }

    pub fn bottom_of_window(&self, scroll_off: usize) {
        let viewport = self.viewport.get_untracked();
        // TODO: don't assume line height is constant
        let line_height = f64::from(self.line_height(0));
        let offset = self.cursor.with_untracked(|cursor| cursor.offset());
        let (line, _col) = self.offset_to_line_col(offset);

        let desired_bottom = (line + scroll_off + 1) as f64 * line_height - viewport.height();

        let scroll_delta = desired_bottom - viewport.y0;

        self.scroll_delta.set(Vec2::new(0.0, scroll_delta));
    }

    pub fn scroll(&self, top_shift: f64, down: bool, count: usize, mods: Modifiers) {
        let viewport = self.viewport.get_untracked();
        // TODO: don't assume line height is constant
        let line_height = f64::from(self.line_height(0));
        let diff = line_height * count as f64;
        let diff = if down { diff } else { -diff };

        let offset = self.cursor.with_untracked(|cursor| cursor.offset());
        let (line, _col) = self.offset_to_line_col(offset);
        let top = viewport.y0 + diff + top_shift;
        let bottom = viewport.y0 + diff + viewport.height();

        let new_line = if (line + 1) as f64 * line_height + line_height > bottom {
            let line = (bottom / line_height).floor() as usize;
            line.saturating_sub(2)
        } else if line as f64 * line_height - line_height < top {
            let line = (top / line_height).ceil() as usize;
            line + 1
        } else {
            line
        };

        self.scroll_delta.set(Vec2::new(0.0, diff));

        let res = match new_line.cmp(&line) {
            Ordering::Greater => Some((MoveCommand::Down, new_line - line)),
            Ordering::Less => Some((MoveCommand::Up, line - new_line)),
            _ => None,
        };

        if let Some((cmd, count)) = res {
            let cmd = Command::Move(cmd);
            self.doc().run_command(self, &cmd, Some(count), mods);
        }
    }

    // === Information ===

    pub fn phantom_text(&self, line: usize) -> PhantomTextLine {
        self.doc()
            .phantom_text(self.id(), &self.es.get_untracked(), line)
    }

    pub fn line_height(&self, line: usize) -> f32 {
        self.style().line_height(self.id(), line)
    }

    // === Line Information ===

    /// Iterate over the visual lines in the view, starting at the given line.
    pub fn iter_vlines(
        &self,
        backwards: bool,
        start: VLine,
    ) -> impl Iterator<Item = VLineInfo> + '_ {
        self.lines.iter_vlines(self.text_prov(), backwards, start)
    }

    /// Iterate over the visual lines in the view, starting at the given line and ending at the
    /// given line. `start_line..end_line`
    pub fn iter_vlines_over(
        &self,
        backwards: bool,
        start: VLine,
        end: VLine,
    ) -> impl Iterator<Item = VLineInfo> + '_ {
        self.lines
            .iter_vlines_over(self.text_prov(), backwards, start, end)
    }

    /// Iterator over *relative* [`VLineInfo`]s, starting at the buffer line, `start_line`.  
    /// The `visual_line`s provided by this will start at 0 from your `start_line`.  
    /// This is preferable over `iter_lines` if you do not need to absolute visual line value.
    pub fn iter_rvlines(
        &self,
        backwards: bool,
        start: RVLine,
    ) -> impl Iterator<Item = VLineInfo<()>> + '_ {
        self.lines.iter_rvlines(self.text_prov(), backwards, start)
    }

    /// Iterator over *relative* [`VLineInfo`]s, starting at the buffer line, `start_line` and
    /// ending at `end_line`.  
    /// `start_line..end_line`  
    /// This is preferable over `iter_lines` if you do not need to absolute visual line value.
    pub fn iter_rvlines_over(
        &self,
        backwards: bool,
        start: RVLine,
        end_line: usize,
    ) -> impl Iterator<Item = VLineInfo<()>> + '_ {
        self.lines
            .iter_rvlines_over(self.text_prov(), backwards, start, end_line)
    }

    // ==== Position Information ====

    pub fn first_rvline_info(&self) -> VLineInfo<()> {
        self.rvline_info(RVLine::default())
    }

    /// The number of lines in the document.
    pub fn num_lines(&self) -> usize {
        self.rope_text().num_lines()
    }

    /// The last allowed buffer line in the document.
    pub fn last_line(&self) -> usize {
        self.rope_text().last_line()
    }

    pub fn last_vline(&self) -> VLine {
        self.lines.last_vline(self.text_prov())
    }

    pub fn last_rvline(&self) -> RVLine {
        self.lines.last_rvline(self.text_prov())
    }

    pub fn last_rvline_info(&self) -> VLineInfo<()> {
        self.rvline_info(self.last_rvline())
    }

    // ==== Line/Column Positioning ====

    /// Convert an offset into the buffer into a line and idx.  
    pub fn offset_to_line_col(&self, offset: usize) -> (usize, usize) {
        self.rope_text().offset_to_line_col(offset)
    }

    pub fn offset_of_line(&self, offset: usize) -> usize {
        self.rope_text().offset_of_line(offset)
    }

    pub fn offset_of_line_col(&self, line: usize, col: usize) -> usize {
        self.rope_text().offset_of_line_col(line, col)
    }

    /// Get the buffer line of an offset
    pub fn line_of_offset(&self, offset: usize) -> usize {
        self.rope_text().line_of_offset(offset)
    }

    /// Returns the offset into the buffer of the first non blank character on the given line.
    pub fn first_non_blank_character_on_line(&self, line: usize) -> usize {
        self.rope_text().first_non_blank_character_on_line(line)
    }

    pub fn line_end_col(&self, line: usize, caret: bool) -> usize {
        self.rope_text().line_end_col(line, caret)
    }

    pub fn select_word(&self, offset: usize) -> (usize, usize) {
        self.rope_text().select_word(offset)
    }

    /// `affinity` decides whether an offset at a soft line break is considered to be on the
    /// previous line or the next line.  
    /// If `affinity` is `CursorAffinity::Forward` and is at the very end of the wrapped line, then
    /// the offset is considered to be on the next line.
    pub fn vline_of_offset(&self, offset: usize, affinity: CursorAffinity) -> VLine {
        self.lines
            .vline_of_offset(&self.text_prov(), offset, affinity)
    }

    pub fn vline_of_line(&self, line: usize) -> VLine {
        self.lines.vline_of_line(&self.text_prov(), line)
    }

    pub fn rvline_of_line(&self, line: usize) -> RVLine {
        self.lines.rvline_of_line(&self.text_prov(), line)
    }

    pub fn vline_of_rvline(&self, rvline: RVLine) -> VLine {
        self.lines.vline_of_rvline(&self.text_prov(), rvline)
    }

    /// Get the nearest offset to the start of the visual line.
    pub fn offset_of_vline(&self, vline: VLine) -> usize {
        self.lines.offset_of_vline(&self.text_prov(), vline)
    }

    /// Get the visual line and column of the given offset.  
    /// The column is before phantom text is applied.
    pub fn vline_col_of_offset(&self, offset: usize, affinity: CursorAffinity) -> (VLine, usize) {
        self.lines
            .vline_col_of_offset(&self.text_prov(), offset, affinity)
    }

    pub fn rvline_of_offset(&self, offset: usize, affinity: CursorAffinity) -> RVLine {
        self.lines
            .rvline_of_offset(&self.text_prov(), offset, affinity)
    }

    pub fn rvline_col_of_offset(&self, offset: usize, affinity: CursorAffinity) -> (RVLine, usize) {
        self.lines
            .rvline_col_of_offset(&self.text_prov(), offset, affinity)
    }

    pub fn offset_of_rvline(&self, rvline: RVLine) -> usize {
        self.lines.offset_of_rvline(&self.text_prov(), rvline)
    }

    pub fn vline_info(&self, vline: VLine) -> VLineInfo {
        let vline = vline.min(self.last_vline());
        self.iter_vlines(false, vline).next().unwrap()
    }

    pub fn screen_rvline_info_of_offset(
        &self,
        offset: usize,
        affinity: CursorAffinity,
    ) -> Option<VLineInfo<()>> {
        let rvline = self.rvline_of_offset(offset, affinity);
        self.screen_lines.with_untracked(|screen_lines| {
            screen_lines
                .iter_vline_info()
                .find(|vline_info| vline_info.rvline == rvline)
        })
    }

    pub fn rvline_info(&self, rvline: RVLine) -> VLineInfo<()> {
        let rvline = rvline.min(self.last_rvline());
        self.iter_rvlines(false, rvline).next().unwrap()
    }

    pub fn rvline_info_of_offset(&self, offset: usize, affinity: CursorAffinity) -> VLineInfo<()> {
        let rvline = self.rvline_of_offset(offset, affinity);
        self.rvline_info(rvline)
    }

    /// Get the first column of the overall line of the visual line
    pub fn first_col<T: std::fmt::Debug>(&self, info: VLineInfo<T>) -> usize {
        info.first_col(&self.text_prov())
    }

    /// Get the last column in the overall line of the visual line
    pub fn last_col<T: std::fmt::Debug>(&self, info: VLineInfo<T>, caret: bool) -> usize {
        info.last_col(&self.text_prov(), caret)
    }

    // ==== Points of locations ====

    pub fn max_line_width(&self) -> f64 {
        self.lines.max_width()
    }

    /// Returns the point into the text layout of the line at the given offset.
    /// `x` being the leading edge of the character, and `y` being the baseline.
    pub fn line_point_of_offset(&self, offset: usize, affinity: CursorAffinity) -> Point {
        let (line, col) = self.offset_to_line_col(offset);
        self.line_point_of_line_col(line, col, affinity, false)
    }

    /// Returns the point into the text layout of the line at the given line and col.
    /// `x` being the leading edge of the character, and `y` being the baseline.  
    pub fn line_point_of_line_col(
        &self,
        line: usize,
        col: usize,
        affinity: CursorAffinity,
        force_affinity: bool,
    ) -> Point {
        let text_layout = self.text_layout(line);
        let index = if force_affinity {
            text_layout
                .phantom_text
                .col_after_force(col, affinity == CursorAffinity::Forward)
        } else {
            text_layout
                .phantom_text
                .col_after(col, affinity == CursorAffinity::Forward)
        };
        hit_position_aff(
            &text_layout.text,
            index,
            affinity == CursorAffinity::Backward,
        )
        .point
    }

    /// Get the (point above, point below) of a particular offset within the editor.
    pub fn points_of_offset(&self, offset: usize, affinity: CursorAffinity) -> (Point, Point) {
        let line = self.line_of_offset(offset);
        let line_height = f64::from(self.style().line_height(self.id(), line));

        let info = self.screen_lines.with_untracked(|sl| {
            sl.iter_line_info().find(|info| {
                info.vline_info.interval.start <= offset && offset <= info.vline_info.interval.end
            })
        });
        let Some(info) = info else {
            // TODO: We could do a smarter method where we get the approximate y position
            // because, for example, this spot could be folded away, and so it would be better to
            // supply the *nearest* position on the screen.
            return (Point::new(0.0, 0.0), Point::new(0.0, 0.0));
        };

        let y = info.vline_y;

        let x = self.line_point_of_offset(offset, affinity).x;

        (Point::new(x, y), Point::new(x, y + line_height))
    }

    /// Get the offset of a particular point within the editor.
    /// The boolean indicates whether the point is inside the text or not
    /// Points outside of vertical bounds will return the last line.
    /// Points outside of horizontal bounds will return the last column on the line.
    pub fn offset_of_point(&self, mode: Mode, point: Point) -> (usize, bool) {
        let ((line, col), is_inside) = self.line_col_of_point(mode, point);
        (self.offset_of_line_col(line, col), is_inside)
    }

    /// Get the actual (line, col) of a particular point within the editor.
    pub fn line_col_of_point_with_phantom(&self, point: Point) -> (usize, usize) {
        let line_height = f64::from(self.style().line_height(self.id(), 0));
        let info = if point.y <= 0.0 {
            Some(self.first_rvline_info())
        } else {
            self.screen_lines
                .with_untracked(|sl| {
                    sl.iter_line_info().find(|info| {
                        info.vline_y <= point.y && info.vline_y + line_height >= point.y
                    })
                })
                .map(|info| info.vline_info)
        };
        let info = info.unwrap_or_else(|| {
            for (y_idx, info) in self.iter_rvlines(false, RVLine::default()).enumerate() {
                let vline_y = y_idx as f64 * line_height;
                if vline_y <= point.y && vline_y + line_height >= point.y {
                    return info;
                }
            }

            self.last_rvline_info()
        });

        let rvline = info.rvline;
        let line = rvline.line;
        let text_layout = self.text_layout(line);

        let y = text_layout.get_layout_y(rvline.line_index).unwrap_or(0.0);

        let hit_point = text_layout.text.hit_point(Point::new(point.x, y as f64));
        (line, hit_point.index)
    }

    /// Get the (line, col) of a particular point within the editor.
    /// The boolean indicates whether the point is within the text bounds.
    /// Points outside of vertical bounds will return the last line.
    /// Points outside of horizontal bounds will return the last column on the line.
    pub fn line_col_of_point(&self, mode: Mode, point: Point) -> ((usize, usize), bool) {
        // TODO: this assumes that line height is constant!
        let line_height = f64::from(self.style().line_height(self.id(), 0));
        let info = if point.y <= 0.0 {
            Some(self.first_rvline_info())
        } else {
            self.screen_lines
                .with_untracked(|sl| {
                    sl.iter_line_info().find(|info| {
                        info.vline_y <= point.y && info.vline_y + line_height >= point.y
                    })
                })
                .map(|info| info.vline_info)
        };
        let info = info.unwrap_or_else(|| {
            for (y_idx, info) in self.iter_rvlines(false, RVLine::default()).enumerate() {
                let vline_y = y_idx as f64 * line_height;
                if vline_y <= point.y && vline_y + line_height >= point.y {
                    return info;
                }
            }

            self.last_rvline_info()
        });

        let rvline = info.rvline;
        let line = rvline.line;
        let text_layout = self.text_layout(line);

        let y = text_layout.get_layout_y(rvline.line_index).unwrap_or(0.0);

        let hit_point = text_layout.text.hit_point(Point::new(point.x, y as f64));
        // We have to unapply the phantom text shifting in order to get back to the column in
        // the actual buffer
        let col = text_layout.phantom_text.before_col(hit_point.index);
        // Ensure that the column doesn't end up out of bounds, so things like clicking on the far
        // right end will just go to the end of the line.
        let max_col = self.line_end_col(line, mode != Mode::Normal);
        let mut col = col.min(max_col);

        // TODO: we need to handle affinity. Clicking at end of a wrapped line should give it a
        // backwards affinity, while being at the start of the next line should be a forwards aff

        // TODO: this is a hack to get around text layouts not including spaces at the end of
        // wrapped lines, but we want to be able to click on them
        if !hit_point.is_inside {
            // TODO(minor): this is probably wrong in some manners
            col = info.last_col(&self.text_prov(), true);
        }

        let tab_width = self.style().tab_width(self.id(), line);
        if self.style().atomic_soft_tabs(self.id(), line) && tab_width > 1 {
            col = snap_to_soft_tab_line_col(
                &self.text(),
                line,
                col,
                SnapDirection::Nearest,
                tab_width,
            );
        }

        ((line, col), hit_point.is_inside)
    }

    // TODO: colposition probably has issues with wrapping?
    pub fn line_horiz_col(&self, line: usize, horiz: &ColPosition, caret: bool) -> usize {
        match *horiz {
            ColPosition::Col(x) => {
                // TODO: won't this be incorrect with phantom text? Shouldn't this just use
                // line_col_of_point and get the col from that?
                let text_layout = self.text_layout(line);
                let hit_point = text_layout.text.hit_point(Point::new(x, 0.0));
                let n = hit_point.index;
                let col = text_layout.phantom_text.before_col(n);

                col.min(self.line_end_col(line, caret))
            }
            ColPosition::End => self.line_end_col(line, caret),
            ColPosition::Start => 0,
            ColPosition::FirstNonBlank => self.first_non_blank_character_on_line(line),
        }
    }

    /// Advance to the right in the manner of the given mode.
    /// Get the column from a horizontal at a specific line index (in a text layout)
    pub fn rvline_horiz_col(
        &self,
        RVLine { line, line_index }: RVLine,
        horiz: &ColPosition,
        caret: bool,
    ) -> usize {
        match *horiz {
            ColPosition::Col(x) => {
                let text_layout = self.text_layout(line);
                let y_pos = text_layout
                    .text
                    .layout_runs()
                    .nth(line_index)
                    .map(|run| run.line_y)
                    .or_else(|| text_layout.text.layout_runs().last().map(|run| run.line_y))
                    .unwrap_or(0.0);
                let hit_point = text_layout.text.hit_point(Point::new(x, y_pos as f64));
                let n = hit_point.index;
                let col = text_layout.phantom_text.before_col(n);

                col.min(self.line_end_col(line, caret))
            }
            // Otherwise it is the same as the other function
            _ => self.line_horiz_col(line, horiz, caret),
        }
    }

    /// Advance to the right in the manner of the given mode.  
    /// This is not the same as the [`Movement::Right`] command.
    pub fn move_right(&self, offset: usize, mode: Mode, count: usize) -> usize {
        self.rope_text().move_right(offset, mode, count)
    }

    /// Advance to the left in the manner of the given mode.
    /// This is not the same as the [`Movement::Left`] command.
    pub fn move_left(&self, offset: usize, mode: Mode, count: usize) -> usize {
        self.rope_text().move_left(offset, mode, count)
    }
}

impl std::fmt::Debug for Editor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Editor").field(&self.id).finish()
    }
}

// Text layout creation
impl Editor {
    // Get the text layout for a document line, creating it if needed.
    pub fn text_layout(&self, line: usize) -> Arc<TextLayoutLine> {
        self.text_layout_trigger(line, true)
    }

    pub fn text_layout_trigger(&self, line: usize, trigger: bool) -> Arc<TextLayoutLine> {
        let cache_rev = self.doc().cache_rev().get_untracked();
        self.lines
            .get_init_text_layout(cache_rev, self.config_id(), self, line, trigger)
    }

    fn try_get_text_layout(&self, line: usize) -> Option<Arc<TextLayoutLine>> {
        let cache_rev = self.doc().cache_rev().get_untracked();
        self.lines
            .try_get_text_layout(cache_rev, self.config_id(), line)
    }

    /// Create rendable whitespace layout by creating a new text layout
    /// with invisible spaces and special utf8 characters that display
    /// the different white space characters.
    fn new_whitespace_layout(
        line_content: &str,
        text_layout: &TextLayout,
        phantom: &PhantomTextLine,
        render_whitespace: RenderWhitespace,
    ) -> Option<Vec<(char, (f64, f64))>> {
        let mut render_leading = false;
        let mut render_boundary = false;
        let mut render_between = false;

        // TODO: render whitespaces only on highlighted text
        match render_whitespace {
            RenderWhitespace::All => {
                render_leading = true;
                render_boundary = true;
                render_between = true;
            }
            RenderWhitespace::Boundary => {
                render_leading = true;
                render_boundary = true;
            }
            RenderWhitespace::Trailing => {} // All configs include rendering trailing whitespace
            RenderWhitespace::None => return None,
        }

        let mut whitespace_buffer = Vec::new();
        let mut rendered_whitespaces: Vec<(char, (f64, f64))> = Vec::new();
        let mut char_found = false;
        let mut col = 0;
        for c in line_content.chars() {
            match c {
                '\t' => {
                    let col_left = phantom.col_after(col, true);
                    let col_right = phantom.col_after(col + 1, false);
                    let x0 = text_layout.hit_position(col_left).point.x;
                    let x1 = text_layout.hit_position(col_right).point.x;
                    whitespace_buffer.push(('\t', (x0, x1)));
                }
                ' ' => {
                    let col_left = phantom.col_after(col, true);
                    let col_right = phantom.col_after(col + 1, false);
                    let x0 = text_layout.hit_position(col_left).point.x;
                    let x1 = text_layout.hit_position(col_right).point.x;
                    whitespace_buffer.push((' ', (x0, x1)));
                }
                _ => {
                    if (char_found && render_between)
                        || (char_found && render_boundary && whitespace_buffer.len() > 1)
                        || (!char_found && render_leading)
                    {
                        rendered_whitespaces.extend(whitespace_buffer.iter());
                    }

                    char_found = true;
                    whitespace_buffer.clear();
                }
            }
            col += c.len_utf8();
        }
        rendered_whitespaces.extend(whitespace_buffer.iter());

        Some(rendered_whitespaces)
    }
}
impl TextLayoutProvider for Editor {
    // TODO: should this just return a `Rope`?
    fn text(&self) -> Rope {
        Editor::text(self)
    }

    fn new_text_layout(
        &self,
        line: usize,
        _font_size: usize,
        _wrap: ResolvedWrap,
    ) -> Arc<TextLayoutLine> {
        // TODO: we could share text layouts between different editor views given some knowledge of
        // their wrapping
        let edid = self.id();
        let text = self.rope_text();
        let style = self.style();
        let doc = self.doc();

        let line_content_original = text.line_content(line);

        let font_size = style.font_size(edid, line);

        // Get the line content with newline characters replaced with spaces
        // and the content without the newline characters
        // TODO: cache or add some way that text layout is created to auto insert the spaces instead
        // though we immediately combine with phantom text so that's a thing.
        let line_content = if let Some(s) = line_content_original.strip_suffix("\r\n") {
            format!("{s}  ")
        } else if let Some(s) = line_content_original.strip_suffix('\n') {
            format!("{s} ",)
        } else {
            line_content_original.to_string()
        };
        // Combine the phantom text with the line content
        let phantom_text = doc.phantom_text(edid, &self.es.get_untracked(), line);
        let line_content = phantom_text.combine_with_text(&line_content);

        let family = style.font_family(edid, line);
        let attrs = Attrs::new()
            .color(self.es.with(|s| s.ed_text_color()))
            .family(&family)
            .font_size(font_size as f32)
            .line_height(LineHeightValue::Px(style.line_height(edid, line)));
        let mut attrs_list = AttrsList::new(attrs.clone());

        self.es.with_untracked(|es| {
            style.apply_attr_styles(edid, es, line, attrs.clone(), &mut attrs_list);
        });

        // Apply phantom text specific styling
        for (offset, size, col, phantom) in phantom_text.offset_size_iter() {
            let start = col + offset;
            let end = start + size;

            let mut attrs = attrs.clone();
            if let Some(fg) = phantom.fg {
                attrs = attrs.color(fg);
            } else {
                attrs = attrs.color(self.es.with(|es| es.phantom_color()))
            }
            if let Some(phantom_font_size) = phantom.font_size {
                attrs = attrs.font_size(phantom_font_size.min(font_size) as f32);
            }
            attrs_list.add_span(start..end, attrs);
            // if let Some(font_family) = phantom.font_family.clone() {
            //     layout_builder = layout_builder.range_attribute(
            //         start..end,
            //         TextAttribute::FontFamily(font_family),
            //     );
            // }
        }

        let mut text_layout = TextLayout::new();
        // TODO: we could move tab width setting to be done by the document
        text_layout.set_tab_width(style.tab_width(edid, line));
        text_layout.set_text(&line_content, attrs_list, None);

        // dbg!(self.editor_style.with(|s| s.wrap_method()));
        match self.es.with(|s| s.wrap_method()) {
            WrapMethod::None => {}
            WrapMethod::EditorWidth => {
                let width = self.viewport.get_untracked().width();
                text_layout.set_wrap(Wrap::WordOrGlyph);
                text_layout.set_size(width as f32, f32::MAX);
            }
            WrapMethod::WrapWidth { width } => {
                text_layout.set_wrap(Wrap::WordOrGlyph);
                text_layout.set_size(width, f32::MAX);
            }
            // TODO:
            WrapMethod::WrapColumn { .. } => {}
        }

        let whitespaces = Self::new_whitespace_layout(
            &line_content_original,
            &text_layout,
            &phantom_text,
            self.es.with(|s| s.render_whitespace()),
        );

        let indent_line = style.indent_line(edid, line, &line_content_original);

        let indent = if indent_line != line {
            // TODO: This creates the layout if it isn't already cached, but it doesn't cache the
            // result because the current method of managing the cache is not very smart.
            let layout = self.try_get_text_layout(indent_line).unwrap_or_else(|| {
                self.new_text_layout(
                    indent_line,
                    style.font_size(edid, indent_line),
                    self.lines.wrap(),
                )
            });
            layout.indent + 1.0
        } else {
            let offset = text.first_non_blank_character_on_line(indent_line);
            let (_, col) = text.offset_to_line_col(offset);
            text_layout.hit_position(col).point.x
        };

        let mut layout_line = TextLayoutLine {
            text: text_layout,
            extra_style: Vec::new(),
            whitespaces,
            indent,
            phantom_text,
        };
        self.es.with_untracked(|es| {
            style.apply_layout_styles(edid, es, line, &mut layout_line);
        });

        Arc::new(layout_line)
    }

    fn before_phantom_col(&self, line: usize, col: usize) -> usize {
        self.doc()
            .before_phantom_col(self.id(), &self.es.get_untracked(), line, col)
    }

    fn has_multiline_phantom(&self) -> bool {
        self.doc()
            .has_multiline_phantom(self.id(), &self.es.get_untracked())
    }
}

struct EditorFontSizes {
    id: EditorId,
    style: ReadSignal<Rc<dyn Styling>>,
    doc: ReadSignal<Rc<dyn Document>>,
}
impl LineFontSizeProvider for EditorFontSizes {
    fn font_size(&self, line: usize) -> usize {
        self.style
            .with_untracked(|style| style.font_size(self.id, line))
    }

    fn cache_id(&self) -> FontSizeCacheId {
        let mut hasher = DefaultHasher::new();

        // TODO: is this actually good enough for comparing cache state?
        // We could just have it return an arbitrary type that impl's Eq?
        self.style
            .with_untracked(|style| style.id().hash(&mut hasher));
        self.doc
            .with_untracked(|doc| doc.cache_rev().get_untracked().hash(&mut hasher));

        hasher.finish()
    }
}

/// Minimum width that we'll allow the view to be wrapped at.
const MIN_WRAPPED_WIDTH: f32 = 100.0;

/// Create various reactive effects to update the screen lines whenever relevant parts of the view,
/// doc, text layouts, viewport, etc. change.
/// This tries to be smart to a degree.
fn create_view_effects(cx: Scope, ed: &Editor) {
    // Cloning is fun.
    let ed2 = ed.clone();
    let ed3 = ed.clone();
    let ed4 = ed.clone();

    // Reset cursor blinking whenever the cursor changes
    {
        let cursor_info = ed.cursor_info.clone();
        let cursor = ed.cursor;
        cx.create_effect(move |_| {
            cursor.track();
            cursor_info.reset();
        });
    }

    let update_screen_lines = |ed: &Editor| {
        // This function should not depend on the viewport signal directly.

        // This is wrapped in an update to make any updates-while-updating very obvious
        // which they wouldn't be if we computed and then `set`.
        ed.screen_lines.update(|screen_lines| {
            let new_screen_lines = ed.compute_screen_lines(screen_lines.base);

            *screen_lines = new_screen_lines;
        });
    };

    // Listen for layout events, currently only when a layout is created, and update screen
    // lines based on that
    ed3.lines.layout_event.listen_with(cx, move |val| {
        let ed = &ed2;
        // TODO: Move this logic onto screen lines somehow, perhaps just an auxiliary
        // function, to avoid getting confused about what is relevant where.

        match val {
            LayoutEvent::CreatedLayout { line, .. } => {
                let sl = ed.screen_lines.get_untracked();

                // Intelligently update screen lines, avoiding recalculation if possible
                let should_update = sl.on_created_layout(ed, line);

                if should_update {
                    untrack(|| {
                        update_screen_lines(ed);
                    });

                    // Ensure that it is created even after the base/viewport signals have been
                    // updated.
                    // But we have to trigger an event since it could alter the screenlines
                    // TODO: this has some risk for infinite looping if we're unlucky.
                    ed2.text_layout_trigger(line, true);
                }
            }
        }
    });

    // TODO: should we have some debouncing for editor width? Ideally we'll be fast enough to not
    // even need it, though we might not want to use a bunch of cpu whilst resizing anyway.

    let viewport_changed_trigger = cx.create_trigger();

    // Watch for changes to the viewport so that we can alter the wrapping
    // As well as updating the screen lines base
    cx.create_effect(move |_| {
        let ed = &ed3;

        let viewport = ed.viewport.get();

        let wrap = match ed.es.with(|s| s.wrap_method()) {
            WrapMethod::None => ResolvedWrap::None,
            WrapMethod::EditorWidth => {
                ResolvedWrap::Width((viewport.width() as f32).max(MIN_WRAPPED_WIDTH))
            }
            WrapMethod::WrapColumn { .. } => todo!(),
            WrapMethod::WrapWidth { width } => ResolvedWrap::Width(width),
        };

        ed.lines.set_wrap(wrap);

        // Update the base
        let base = ed.screen_lines.with_untracked(|sl| sl.base);

        // TODO: should this be a with or with_untracked?
        if viewport != base.with_untracked(|base| base.active_viewport) {
            batch(|| {
                base.update(|base| {
                    base.active_viewport = viewport;
                });
                // TODO: Can I get rid of this and just call update screen lines with an
                // untrack around it?
                viewport_changed_trigger.notify();
            });
        }
    });
    // Watch for when the viewport as changed in a relevant manner
    // and for anything that `update_screen_lines` tracks.
    cx.create_effect(move |_| {
        viewport_changed_trigger.track();

        update_screen_lines(&ed4);
    });
}

pub fn normal_compute_screen_lines(
    editor: &Editor,
    base: RwSignal<ScreenLinesBase>,
) -> ScreenLines {
    let lines = &editor.lines;
    let style = editor.style.get();
    // TODO: don't assume universal line height!
    let line_height = style.line_height(editor.id(), 0);

    let (y0, y1) = base.with_untracked(|base| (base.active_viewport.y0, base.active_viewport.y1));
    // Get the start and end (visual) lines that are visible in the viewport
    let min_vline = VLine((y0 / line_height as f64).floor() as usize);
    let max_vline = VLine((y1 / line_height as f64).ceil() as usize);

    let cache_rev = editor.doc.get().cache_rev().get();
    editor.lines.check_cache_rev(cache_rev);

    let min_info = editor.iter_vlines(false, min_vline).next();

    let mut rvlines = Vec::new();
    let mut info = HashMap::new();

    let Some(min_info) = min_info else {
        return ScreenLines {
            lines: Rc::new(rvlines),
            info: Rc::new(info),
            diff_sections: None,
            base,
        };
    };

    // TODO: the original was min_line..max_line + 1, are we iterating too little now?
    // the iterator is from min_vline..max_vline
    let count = max_vline.get() - min_vline.get();
    let iter = lines
        .iter_rvlines_init(
            editor.text_prov(),
            cache_rev,
            editor.config_id(),
            min_info.rvline,
            false,
        )
        .take(count);

    for (i, vline_info) in iter.enumerate() {
        rvlines.push(vline_info.rvline);

        let line_height = f64::from(style.line_height(editor.id(), vline_info.rvline.line));

        let y_idx = min_vline.get() + i;
        let vline_y = y_idx as f64 * line_height;
        let line_y = vline_y - vline_info.rvline.line_index as f64 * line_height;

        // Add the information to make it cheap to get in the future.
        // This y positions are shifted by the baseline y0
        info.insert(
            vline_info.rvline,
            LineInfo {
                y: line_y - y0,
                vline_y: vline_y - y0,
                vline_info,
            },
        );
    }

    ScreenLines {
        lines: Rc::new(rvlines),
        info: Rc::new(info),
        diff_sections: None,
        base,
    }
}

// TODO: should we put `cursor` on this structure?
/// Cursor rendering information
#[derive(Clone)]
pub struct CursorInfo {
    pub hidden: RwSignal<bool>,

    pub blink_timer: RwSignal<TimerToken>,
    // TODO: should these just be rwsignals?
    pub should_blink: Rc<dyn Fn() -> bool + 'static>,
    pub blink_interval: Rc<dyn Fn() -> u64 + 'static>,
}

impl CursorInfo {
    pub fn new(cx: Scope) -> CursorInfo {
        CursorInfo {
            hidden: cx.create_rw_signal(false),

            blink_timer: cx.create_rw_signal(TimerToken::INVALID),
            should_blink: Rc::new(|| true),
            blink_interval: Rc::new(|| 500),
        }
    }

    pub fn blink(&self) {
        let info = self.clone();
        let blink_interval = (info.blink_interval)();
        if blink_interval > 0 && (info.should_blink)() {
            let blink_timer = info.blink_timer;
            let timer_token =
                exec_after(Duration::from_millis(blink_interval), move |timer_token| {
                    if info.blink_timer.try_get_untracked() == Some(timer_token) {
                        info.hidden.update(|hide| {
                            *hide = !*hide;
                        });
                        info.blink();
                    }
                });
            blink_timer.set(timer_token);
        }
    }

    pub fn reset(&self) {
        if self.hidden.get_untracked() {
            self.hidden.set(false);
        }

        self.blink_timer.set(TimerToken::INVALID);

        self.blink();
    }
}
