use std::{borrow::Cow, fmt::Debug, ops::Range, rc::Rc};

use crate::{
    peniko::color::palette,
    peniko::Color,
    reactive::{RwSignal, Scope},
    text::{Attrs, AttrsList, FamilyOwned, Stretch, Weight},
    views::EditorCustomStyle,
};
use floem_editor_core::{
    buffer::rope_text::{RopeText, RopeTextVal},
    command::EditCommand,
    cursor::Cursor,
    editor::EditType,
    indent::IndentStyle,
    mode::MotionMode,
    register::{Clipboard, Register},
    selection::Selection,
    word::WordCursor,
};
use floem_reactive::SignalGet;
use lapce_xi_rope::Rope;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use ui_events::keyboard::Modifiers;

use super::{
    actions::CommonAction,
    command::{Command, CommandExecuted},
    gutter::GutterClass,
    id::EditorId,
    layout::TextLayoutLine,
    normal_compute_screen_lines,
    phantom_text::{PhantomText, PhantomTextKind, PhantomTextLine},
    view::{ScreenLines, ScreenLinesBase},
    Editor, EditorStyle,
};

// TODO(minor): Should we get rid of this now that this is in floem?
pub struct SystemClipboard;

impl Default for SystemClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemClipboard {
    pub fn new() -> Self {
        Self
    }

    #[cfg(windows)]
    pub fn get_file_list() -> Option<Vec<std::path::PathBuf>> {
        crate::Clipboard::get_file_list().ok()
    }
}

impl Clipboard for SystemClipboard {
    fn get_string(&mut self) -> Option<String> {
        crate::Clipboard::get_contents().ok()
    }

    fn put_string(&mut self, s: impl AsRef<str>) {
        let _ = crate::Clipboard::set_contents(s.as_ref().to_string());
    }
}

#[derive(Clone)]
pub struct Preedit {
    pub text: String,
    pub cursor: Option<(usize, usize)>,
    pub offset: usize,
}

/// IME Preedit  
/// This is used for IME input, and must be owned by the `Document`.  
#[derive(Debug, Clone)]
pub struct PreeditData {
    pub preedit: RwSignal<Option<Preedit>>,
}
impl PreeditData {
    pub fn new(cx: Scope) -> PreeditData {
        PreeditData {
            preedit: cx.create_rw_signal(None),
        }
    }
}

/// A document. This holds text.  
pub trait Document: DocumentPhantom + ::std::any::Any {
    /// Get the text of the document  
    /// Note: typically you should call [`Document::rope_text`] as that provides more checks and
    /// utility functions.
    fn text(&self) -> Rope;

    fn rope_text(&self) -> RopeTextVal {
        RopeTextVal::new(self.text())
    }

    fn cache_rev(&self) -> RwSignal<u64>;

    /// Find the next/previous offset of the match of the given character.  
    /// This is intended for use by the [`Movement::NextUnmatched`](floem_editor_core::movement::Movement::NextUnmatched) and
    /// [`Movement::PreviousUnmatched`](floem_editor_core::movement::Movement::PreviousUnmatched) commands.
    fn find_unmatched(&self, offset: usize, previous: bool, ch: char) -> usize {
        let text = self.text();
        let mut cursor = WordCursor::new(&text, offset);
        let new_offset = if previous {
            cursor.previous_unmatched(ch)
        } else {
            cursor.next_unmatched(ch)
        };

        new_offset.unwrap_or(offset)
    }

    /// Find the offset of the matching pair character.  
    /// This is intended for use by the [`Movement::MatchPairs`](floem_editor_core::movement::Movement::MatchPairs) command.
    fn find_matching_pair(&self, offset: usize) -> usize {
        WordCursor::new(&self.text(), offset)
            .match_pairs()
            .unwrap_or(offset)
    }

    fn preedit(&self) -> PreeditData;

    // TODO: I don't like passing `under_line` as a parameter but `Document` doesn't have styling
    // should we just move preedit + phantom text into `Styling`?
    fn preedit_phantom(&self, under_line: Option<Color>, line: usize) -> Option<PhantomText> {
        let preedit = self.preedit().preedit.get_untracked()?;

        let rope_text = self.rope_text();

        let (ime_line, col) = rope_text.offset_to_line_col(preedit.offset);

        if line != ime_line {
            return None;
        }

        Some(PhantomText {
            kind: PhantomTextKind::Ime,
            text: preedit.text,
            affinity: None,
            col,
            font_size: None,
            fg: None,
            bg: None,
            under_line,
        })
    }

    /// Compute the visible screen lines.  
    /// Note: you should typically *not* need to implement this, unless you have some custom
    /// behavior. Unfortunately this needs an `&self` to be a trait object. So don't call `.update`
    /// on `Self`
    fn compute_screen_lines(
        &self,
        editor: &Editor,
        base: RwSignal<ScreenLinesBase>,
    ) -> ScreenLines {
        normal_compute_screen_lines(editor, base)
    }

    /// Run a command on the document.  
    /// The `ed` will contain this document (at some level, if it was wrapped then it may not be
    /// directly `Rc<Self>`)
    fn run_command(
        &self,
        ed: &Editor,
        cmd: &Command,
        count: Option<usize>,
        modifiers: Modifiers,
    ) -> CommandExecuted;

    fn receive_char(&self, ed: &Editor, c: &str);

    /// Perform a single edit.  
    fn edit_single(&self, selection: Selection, content: &str, edit_type: EditType) {
        let mut iter = std::iter::once((selection, content));
        self.edit(&mut iter, edit_type);
    }

    /// Perform the edit(s) on this document.  
    /// This intentionally does not require an `Editor` as this is primarily intended for use by
    /// code that wants to modify the document from 'outside' the usual keybinding/command logic.  
    /// ```rust,ignore
    /// let editor: TextEditor = text_editor();
    /// let doc: Rc<dyn Document> = editor.doc();
    ///
    /// stack((
    ///     editor,
    ///     button(|| "Append 'Hello'").on_click_stop(move |_| {
    ///         let text = doc.text();
    ///         doc.edit_single(Selection::caret(text.len()), "Hello", EditType::InsertChars);
    ///     })
    /// ))
    /// ```
    fn edit(&self, iter: &mut dyn Iterator<Item = (Selection, &str)>, edit_type: EditType);
}

pub trait DocumentPhantom {
    fn phantom_text(&self, edid: EditorId, styling: &EditorStyle, line: usize) -> PhantomTextLine;

    /// Translate a column position into the position it would be before combining with
    /// the phantom text.
    fn before_phantom_col(
        &self,
        edid: EditorId,
        styling: &EditorStyle,
        line: usize,
        col: usize,
    ) -> usize {
        let phantom = self.phantom_text(edid, styling, line);
        phantom.before_col(col)
    }

    fn has_multiline_phantom(&self, _edid: EditorId, _styling: &EditorStyle) -> bool {
        true
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum WrapMethod {
    None,
    #[default]
    EditorWidth,
    WrapColumn {
        col: usize,
    },
    WrapWidth {
        width: f32,
    },
}
impl std::fmt::Display for WrapMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WrapMethod::None => f.write_str("None"),
            WrapMethod::EditorWidth => f.write_str("Editor Width"),
            WrapMethod::WrapColumn { col } => f.write_fmt(format_args!("Wrap at Column {col}")),
            WrapMethod::WrapWidth { width } => f.write_fmt(format_args!("Wrap Width {width}")),
        }
    }
}
impl WrapMethod {
    pub fn is_none(&self) -> bool {
        matches!(self, WrapMethod::None)
    }

    pub fn is_constant(&self) -> bool {
        matches!(
            self,
            WrapMethod::None | WrapMethod::WrapColumn { .. } | WrapMethod::WrapWidth { .. }
        )
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum RenderWhitespace {
    #[default]
    None,
    All,
    Boundary,
    Trailing,
}
impl std::fmt::Display for RenderWhitespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{self:?}"))
    }
}

/// There's currently three stages of styling text:  
/// - `Attrs`: This sets the default values for the text
///   - Default font size, font family, etc.
/// - `AttrsList`: This lets you set spans of text to have different styling
///   - Syntax highlighting, bolding specific words, etc.
///
/// Then once the text layout for the line is created from that, we have:
/// - `Layout Styles`: Where it may depend on the position of text in the line (after wrapping)
///   - Outline boxes
///
/// TODO: We could unify the first two steps if we expose a `.defaults_mut()` on `AttrsList`, and
/// then `Styling` mostly just applies whatever attributes it wants and defaults at the same time?
/// but that would complicate pieces of code that need the font size or line height independently.
pub trait Styling {
    // TODO: use a more granular system for invalidating styling, because it may simply be that
    // one line gets different styling.
    /// The id for caching the styling.
    fn id(&self) -> u64;

    fn font_size(&self, _edid: EditorId, _line: usize) -> usize {
        16
    }

    fn line_height(&self, edid: EditorId, line: usize) -> f32 {
        let font_size = self.font_size(edid, line) as f32;
        (1.5 * font_size).round().max(font_size)
    }

    fn font_family(&self, _edid: EditorId, _line: usize) -> Cow<'_, [FamilyOwned]> {
        Cow::Borrowed(&[FamilyOwned::SansSerif])
    }

    fn weight(&self, _edid: EditorId, _line: usize) -> Weight {
        Weight::NORMAL
    }

    // TODO(minor): better name?
    fn italic_style(&self, _edid: EditorId, _line: usize) -> crate::text::Style {
        crate::text::Style::Normal
    }

    fn stretch(&self, _edid: EditorId, _line: usize) -> Stretch {
        Stretch::Normal
    }

    /// Which line the indentation line should be based off of
    /// This is used for lining it up under a scope.
    fn indent_line(&self, _edid: EditorId, line: usize, _line_content: &str) -> usize {
        line
    }

    fn tab_width(&self, _edid: EditorId, _line: usize) -> usize {
        4
    }

    /// Whether the cursor should treat leading soft tabs as if they are hard tabs
    fn atomic_soft_tabs(&self, _edid: EditorId, _line: usize) -> bool {
        false
    }

    // TODO: get other style information based on EditorColor enum?
    // TODO: line_style equivalent?

    /// Apply custom attribute styles to the line  
    fn apply_attr_styles(
        &self,
        _edid: EditorId,
        _style: &EditorStyle,
        _line: usize,
        _default: Attrs,
        _attrs: &mut AttrsList,
    ) {
    }

    fn apply_layout_styles(
        &self,
        _edid: EditorId,
        _style: &EditorStyle,
        _line: usize,
        _layout_line: &mut TextLayoutLine,
    ) {
    }

    /// Whether it should draw the cursor caret on the given line.
    /// Note that these are extra conditions on top of the typical hide cursor &
    /// the editor being active conditions
    /// This is called whenever we paint the line.
    fn paint_caret(&self, _edid: EditorId, _line: usize) -> bool {
        true
    }
}

pub fn default_light_theme(mut style: EditorCustomStyle) -> EditorCustomStyle {
    let fg = Color::from_rgb8(0x38, 0x3A, 0x42);
    let bg = Color::from_rgb8(0xFA, 0xFA, 0xFA);
    let blue = Color::from_rgb8(0x40, 0x78, 0xF2);
    let grey = Color::from_rgb8(0xE5, 0xE5, 0xE6);
    let _scroll_bar = Color::from_rgba8(0xB4, 0xB4, 0xB4, 0xBB);
    let dim = Color::from_rgb8(0xA0, 0xA1, 0xA7);
    let cursor = Color::from_rgb8(0x52, 0x6F, 0xFF);
    let current_line = Color::from_rgb8(0xF2, 0xF2, 0xF2);
    let _dropdown_shadow = Color::from_rgb8(0xB4, 0xB4, 0xB4);
    let _link = blue;
    let _sticky_header_background = bg;

    style.0 = style
        .0
        .color(fg)
        .background(bg)
        .class(GutterClass, |s| s.background(bg));

    style
        .gutter_dim_color(dim)
        .cursor_color(cursor)
        .selection_color(grey)
        .current_line_color(current_line)
        .visible_whitespace(grey)
        .preedit_underline_color(fg)
        .indent_guide_color(grey)
        .gutter_current_color(current_line)
}

pub fn default_dark_color(mut style: EditorCustomStyle) -> EditorCustomStyle {
    let fg = Color::from_rgb8(0xAB, 0xB2, 0xBF);
    let bg = Color::from_rgb8(0x28, 0x2C, 0x34);
    let blue = Color::from_rgb8(0x61, 0xAF, 0xEF);
    let grey = Color::from_rgb8(0x3E, 0x44, 0x51);
    let _scroll_bar = Color::from_rgba8(0x3E, 0x44, 0x51, 0xBB);
    let dim = Color::from_rgb8(0x5C, 0x63, 0x70);
    let cursor = Color::from_rgb8(0x52, 0x8B, 0xFF);
    let current_line = Color::from_rgb8(0x2C, 0x31, 0x3c);
    let _dropdown_shadow = palette::css::BLACK;
    let _link = blue;
    let _sticky_header_background = bg;

    style.0 = style
        .0
        .color(fg)
        .background(bg)
        .class(GutterClass, |s| s.background(bg));

    style
        .gutter_dim_color(dim)
        .cursor_color(cursor)
        .selection_color(grey)
        .current_line_color(current_line)
        .visible_whitespace(grey)
        .preedit_underline_color(fg)
        .indent_guide_color(grey)
        .gutter_current_color(current_line)
}

pub type DocumentRef = Rc<dyn Document>;

/// A document-wrapper for handling commands.  
pub struct ExtCmdDocument<D, F> {
    pub doc: D,
    /// Called whenever [`Document::run_command`] is called.  
    /// If `handler` returns [`CommandExecuted::Yes`] then the default handler on `doc: D` will not
    /// be called.
    pub handler: F,
}
impl<
        D: Document,
        F: Fn(&Editor, &Command, Option<usize>, Modifiers) -> CommandExecuted + 'static,
    > ExtCmdDocument<D, F>
{
    pub fn new(doc: D, handler: F) -> ExtCmdDocument<D, F> {
        ExtCmdDocument { doc, handler }
    }
}
// TODO: it'd be nice if there was some macro to wrap all of the `Document` methods
// but replace specific ones
impl<D, F> Document for ExtCmdDocument<D, F>
where
    D: Document,
    F: Fn(&Editor, &Command, Option<usize>, Modifiers) -> CommandExecuted + 'static,
{
    fn text(&self) -> Rope {
        self.doc.text()
    }

    fn rope_text(&self) -> RopeTextVal {
        self.doc.rope_text()
    }

    fn cache_rev(&self) -> RwSignal<u64> {
        self.doc.cache_rev()
    }

    fn find_unmatched(&self, offset: usize, previous: bool, ch: char) -> usize {
        self.doc.find_unmatched(offset, previous, ch)
    }

    fn find_matching_pair(&self, offset: usize) -> usize {
        self.doc.find_matching_pair(offset)
    }

    fn preedit(&self) -> PreeditData {
        self.doc.preedit()
    }

    fn preedit_phantom(&self, under_line: Option<Color>, line: usize) -> Option<PhantomText> {
        self.doc.preedit_phantom(under_line, line)
    }

    fn compute_screen_lines(
        &self,
        editor: &Editor,
        base: RwSignal<ScreenLinesBase>,
    ) -> ScreenLines {
        self.doc.compute_screen_lines(editor, base)
    }

    fn run_command(
        &self,
        ed: &Editor,
        cmd: &Command,
        count: Option<usize>,
        modifiers: Modifiers,
    ) -> CommandExecuted {
        if (self.handler)(ed, cmd, count, modifiers) == CommandExecuted::Yes {
            return CommandExecuted::Yes;
        }

        self.doc.run_command(ed, cmd, count, modifiers)
    }

    fn receive_char(&self, ed: &Editor, c: &str) {
        self.doc.receive_char(ed, c)
    }

    fn edit_single(&self, selection: Selection, content: &str, edit_type: EditType) {
        self.doc.edit_single(selection, content, edit_type)
    }

    fn edit(&self, iter: &mut dyn Iterator<Item = (Selection, &str)>, edit_type: EditType) {
        self.doc.edit(iter, edit_type)
    }
}
impl<D, F> DocumentPhantom for ExtCmdDocument<D, F>
where
    D: Document,
    F: Fn(&Editor, &Command, Option<usize>, Modifiers) -> CommandExecuted,
{
    fn phantom_text(&self, edid: EditorId, styling: &EditorStyle, line: usize) -> PhantomTextLine {
        self.doc.phantom_text(edid, styling, line)
    }

    fn has_multiline_phantom(&self, edid: EditorId, styling: &EditorStyle) -> bool {
        self.doc.has_multiline_phantom(edid, styling)
    }

    fn before_phantom_col(
        &self,
        edid: EditorId,
        styling: &EditorStyle,
        line: usize,
        col: usize,
    ) -> usize {
        self.doc.before_phantom_col(edid, styling, line, col)
    }
}
impl<D, F> CommonAction for ExtCmdDocument<D, F>
where
    D: Document + CommonAction,
    F: Fn(&Editor, &Command, Option<usize>, Modifiers) -> CommandExecuted,
{
    fn exec_motion_mode(
        &self,
        ed: &Editor,
        cursor: &mut Cursor,
        motion_mode: MotionMode,
        range: Range<usize>,
        is_vertical: bool,
        register: &mut Register,
    ) {
        self.doc
            .exec_motion_mode(ed, cursor, motion_mode, range, is_vertical, register)
    }

    fn do_edit(
        &self,
        ed: &Editor,
        cursor: &mut Cursor,
        cmd: &EditCommand,
        modal: bool,
        register: &mut Register,
        smart_tab: bool,
    ) -> bool {
        self.doc
            .do_edit(ed, cursor, cmd, modal, register, smart_tab)
    }
}

pub const SCALE_OR_SIZE_LIMIT: f32 = 5.0;

#[derive(Debug, Clone)]
pub struct SimpleStyling {
    id: u64,
    font_size: usize,
    // TODO: should we really have this be a float? Shouldn't it just be a LineHeightValue?
    /// If less than 5.0, line height will be a multiple of the font size
    line_height: f32,
    font_family: Vec<FamilyOwned>,
    weight: Weight,
    italic_style: crate::text::Style,
    stretch: Stretch,
    tab_width: usize,
    atomic_soft_tabs: bool,
}
impl SimpleStyling {
    pub fn builder() -> SimpleStylingBuilder {
        SimpleStylingBuilder::default()
    }

    pub fn new() -> Self {
        Self::default()
    }
}
impl SimpleStyling {
    pub fn increment_id(&mut self) {
        self.id += 1;
    }

    pub fn set_font_size(&mut self, font_size: usize) {
        self.font_size = font_size;
        self.increment_id();
    }

    pub fn set_line_height(&mut self, line_height: f32) {
        self.line_height = line_height;
        self.increment_id();
    }

    pub fn set_font_family(&mut self, font_family: Vec<FamilyOwned>) {
        self.font_family = font_family;
        self.increment_id();
    }

    pub fn set_weight(&mut self, weight: Weight) {
        self.weight = weight;
        self.increment_id();
    }

    pub fn set_italic_style(&mut self, italic_style: crate::text::Style) {
        self.italic_style = italic_style;
        self.increment_id();
    }

    pub fn set_stretch(&mut self, stretch: Stretch) {
        self.stretch = stretch;
        self.increment_id();
    }

    pub fn set_tab_width(&mut self, tab_width: usize) {
        self.tab_width = tab_width;
        self.increment_id();
    }

    pub fn set_atomic_soft_tabs(&mut self, atomic_soft_tabs: bool) {
        self.atomic_soft_tabs = atomic_soft_tabs;
        self.increment_id();
    }
}
impl Default for SimpleStyling {
    fn default() -> Self {
        SimpleStyling {
            id: 0,
            font_size: 16,
            line_height: 1.5,
            font_family: vec![FamilyOwned::SansSerif],
            weight: Weight::NORMAL,
            italic_style: crate::text::Style::Normal,
            stretch: Stretch::Normal,
            tab_width: 4,
            atomic_soft_tabs: false,
        }
    }
}
impl Styling for SimpleStyling {
    fn id(&self) -> u64 {
        0
    }

    fn font_size(&self, _edid: EditorId, _line: usize) -> usize {
        self.font_size
    }

    fn line_height(&self, _edid: EditorId, _line: usize) -> f32 {
        let line_height = if self.line_height < SCALE_OR_SIZE_LIMIT {
            self.line_height * self.font_size as f32
        } else {
            self.line_height
        };

        // Prevent overlapping lines
        (line_height.round() as usize).max(self.font_size) as f32
    }

    fn font_family(&self, _edid: EditorId, _line: usize) -> Cow<'_, [FamilyOwned]> {
        Cow::Borrowed(&self.font_family)
    }

    fn weight(&self, _edid: EditorId, _line: usize) -> Weight {
        self.weight
    }

    fn italic_style(&self, _edid: EditorId, _line: usize) -> crate::text::Style {
        self.italic_style
    }

    fn stretch(&self, _edid: EditorId, _line: usize) -> Stretch {
        self.stretch
    }

    fn tab_width(&self, _edid: EditorId, _line: usize) -> usize {
        self.tab_width
    }

    fn atomic_soft_tabs(&self, _edid: EditorId, _line: usize) -> bool {
        self.atomic_soft_tabs
    }

    fn apply_attr_styles(
        &self,
        _edid: EditorId,
        _style: &EditorStyle,
        _line: usize,
        _default: Attrs,
        _attrs: &mut AttrsList,
    ) {
    }

    fn apply_layout_styles(
        &self,
        _edid: EditorId,
        _style: &EditorStyle,
        _line: usize,
        _layout_line: &mut TextLayoutLine,
    ) {
    }
}

#[derive(Default, Clone)]
pub struct SimpleStylingBuilder {
    font_size: Option<usize>,
    line_height: Option<f32>,
    font_family: Option<Vec<FamilyOwned>>,
    weight: Option<Weight>,
    italic_style: Option<crate::text::Style>,
    stretch: Option<Stretch>,
    indent_style: Option<IndentStyle>,
    tab_width: Option<usize>,
    atomic_soft_tabs: Option<bool>,
    wrap: Option<WrapMethod>,
}
impl SimpleStylingBuilder {
    /// Set the font size
    /// Default: 16
    pub fn font_size(&mut self, font_size: usize) -> &mut Self {
        self.font_size = Some(font_size);
        self
    }

    /// Set the line height
    /// Default: 1.5
    pub fn line_height(&mut self, line_height: f32) -> &mut Self {
        self.line_height = Some(line_height);
        self
    }

    /// Set the font families used
    /// Default: `[FamilyOwned::SansSerif]`
    pub fn font_family(&mut self, font_family: Vec<FamilyOwned>) -> &mut Self {
        self.font_family = Some(font_family);
        self
    }

    /// Set the font weight (such as boldness or thinness)
    /// Default: `Weight::NORMAL`
    pub fn weight(&mut self, weight: Weight) -> &mut Self {
        self.weight = Some(weight);
        self
    }

    /// Set the italic style
    /// Default: `Style::Normal`
    pub fn italic_style(&mut self, italic_style: crate::text::Style) -> &mut Self {
        self.italic_style = Some(italic_style);
        self
    }

    /// Set the font stretch
    /// Default: `Stretch::Normal`
    pub fn stretch(&mut self, stretch: Stretch) -> &mut Self {
        self.stretch = Some(stretch);
        self
    }

    /// Set the indent style
    /// Default: `IndentStyle::Spaces(4)`
    pub fn indent_style(&mut self, indent_style: IndentStyle) -> &mut Self {
        self.indent_style = Some(indent_style);
        self
    }

    /// Set the tab width
    /// Default: 4
    pub fn tab_width(&mut self, tab_width: usize) -> &mut Self {
        self.tab_width = Some(tab_width);
        self
    }

    /// Set whether the cursor should treat leading soft tabs as if they are hard tabs
    /// Default: false
    pub fn atomic_soft_tabs(&mut self, atomic_soft_tabs: bool) -> &mut Self {
        self.atomic_soft_tabs = Some(atomic_soft_tabs);
        self
    }

    /// Set the wrapping method
    /// Default: `WrapMethod::EditorWidth`
    pub fn wrap(&mut self, wrap: WrapMethod) -> &mut Self {
        self.wrap = Some(wrap);
        self
    }

    /// Build the styling with the given color scheme
    pub fn build(&self) -> SimpleStyling {
        let default = SimpleStyling::new();
        SimpleStyling {
            id: 0,
            font_size: self.font_size.unwrap_or(default.font_size),
            line_height: self.line_height.unwrap_or(default.line_height),
            font_family: self.font_family.clone().unwrap_or(default.font_family),
            weight: self.weight.unwrap_or(default.weight),
            italic_style: self.italic_style.unwrap_or(default.italic_style),
            stretch: self.stretch.unwrap_or(default.stretch),
            tab_width: self.tab_width.unwrap_or(default.tab_width),
            atomic_soft_tabs: self.atomic_soft_tabs.unwrap_or(default.atomic_soft_tabs),
        }
    }
}
