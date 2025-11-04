use std::rc::Rc;

use floem_editor_core::{buffer::rope_text::RopeTextVal, indent::IndentStyle};
use floem_reactive::{RwSignal, Scope, SignalUpdate, SignalWith, create_updater, with_scope};
use peniko::Color;

use lapce_xi_rope::Rope;

use crate::{
    id::ViewId,
    style::{CursorColor, Style},
    view::{IntoView, View},
    views::editor::{
        Editor,
        command::CommandExecuted,
        id::EditorId,
        keypress::{KeypressKey, default_key_handler},
        text::{Document, SimpleStyling, Styling},
        text_document::{OnUpdate, PreCommand, TextDocument},
        view::editor_container_view,
    },
};

use super::editor::{
    CurrentLineColor, CursorSurroundingLines, IndentGuideColor, IndentStyleProp, Modal,
    ModalRelativeLine, PhantomColor, PlaceholderColor, PreeditUnderlineColor, RenderWhitespaceProp,
    ScrollBeyondLastLine, SelectionColor, ShowIndentGuide, SmartTab, VisibleWhitespaceColor,
    WrapProp,
    gutter::{DimColor, GutterClass, LeftOfCenterPadding, RightOfCenterPadding},
    text::{RenderWhitespace, WrapMethod},
    view::EditorViewClass,
};

/// A text editor view built on top of [Editor](super::editor::Editor). See [`text_editor`].
///
/// Note: this requires that the document underlying it is a [`TextDocument`] for the use of some
/// logic.
pub struct TextEditor {
    id: ViewId,
    child: ViewId,
    // /// The scope this view was created in, used for creating the final view
    cx: Scope,
    editor: Editor,
}

// Note: this should typically be kept in sync with Lapce's
// `defaults/keymaps-common.toml`
//
/// A text editor view built on top of [Editor](super::editor::Editor). This is the main editor view used in the
/// [Lapce](https://lap.dev/lapce/) code editor. The default keymap includes the standard editing keys, for using your
/// own keymap use [`text_editor_keys`].
///
/// ## Default Keymaps
/// ### Basic Editing
/// Up + ALT => Move Line Up
/// Down + ALT => Move Line Down
///
/// Delete => Delete Forward
/// Backspace => Delete Backward
/// Backspace + Shift => Delete Forward
///
/// Home => Move to the start of the file
/// End => Move to the end of the file
///
/// PageUp => Scroll up by a page
/// PageDown => Scroll down by a page
///
/// PageUp + CTRL => Scroll up
/// PageDown + CTRL => Scroll down
///
/// Enter => Insert New Line
/// Tab => Insert Tab
///
/// Up + ALT, Up + SHIFT => Duplicate line up
/// Down + ALT, Down + SHIFT => Duplicate line down
///
/// ### Multi Cursor
/// i + ALT, i + SHIFT => Insert Cursor at the end of the line
pub fn text_editor(text: impl Into<Rope>) -> TextEditor {
    let id = ViewId::new();
    let cx = Scope::current();

    let doc = Rc::new(TextDocument::new(cx, text));
    let style = Rc::new(SimpleStyling::new());
    let editor = Editor::new(cx, doc, style, false);

    let editor_sig = cx.create_rw_signal(editor.clone());
    let child = with_scope(cx, || {
        editor_container_view(editor_sig, |_| true, default_key_handler(editor_sig))
    })
    .into_view();

    let child_id = child.id();
    id.set_children([child]);

    TextEditor {
        id,
        child: child_id,
        cx,
        editor,
    }
}

/// A text editor view built on top of [Editor](super::editor::Editor) that allows providing your own keymap callback.
///
/// See [`text_editor`] for a list of the default keymaps that you will need to handle yourself if using this function.
pub fn text_editor_keys(
    text: impl Into<Rope>,
    handle_key_event: impl Fn(RwSignal<Editor>, &KeypressKey) -> CommandExecuted + 'static,
) -> TextEditor {
    let id = ViewId::new();
    let cx = Scope::current();

    let doc = Rc::new(TextDocument::new(cx, text));
    let style = Rc::new(SimpleStyling::new());
    let editor = Editor::new(cx, doc, style, false);

    let editor_sig = cx.create_rw_signal(editor.clone());
    let child = with_scope(cx, || {
        editor_container_view(
            editor_sig,
            |_| true,
            move |kp| handle_key_event(editor_sig, &kp),
        )
    })
    .into_view();

    let child_id = child.id();
    id.set_children([child]);

    TextEditor {
        id,
        cx,
        editor,
        child: child_id,
    }
}

impl View for TextEditor {
    fn id(&self) -> ViewId {
        self.id
    }

    fn view_style(&self) -> Option<Style> {
        Some(Style::new().min_width(25).min_height(10))
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Text Editor".into()
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.save();
        let size = self
            .id
            .get_layout()
            .map(|layout| {
                peniko::kurbo::Size::new(layout.size.width as f64, layout.size.height as f64)
            })
            .unwrap_or_default();
        let border_radii =
            crate::view::border_to_radii(&self.id.state().borrow().combined_style, size);

        if crate::view::radii_max(border_radii) > 0.0 {
            let rect = size.to_rect().to_rounded_rect(border_radii);
            cx.clip(&rect);
        } else {
            cx.clip(&size.to_rect());
        }
        cx.paint_view(self.child);
        cx.restore();
    }
}

/// The custom style elements that are specific to an [Editor].
pub struct EditorCustomStyle(pub(crate) Style);

impl EditorCustomStyle {
    /// Sets whether the gutter should be hidden.
    pub fn hide_gutter(mut self, hide: bool) -> Self {
        self.0 = self
            .0
            .class(GutterClass, |s| s.apply_if(hide, |s| s.hide()));
        self
    }

    /// Sets the text accent color of the gutter.
    ///
    /// This is the color of the line number for the current line.
    /// It will default to the current Text Color
    pub fn gutter_accent_color(mut self, color: Color) -> Self {
        self.0 = self.0.class(GutterClass, |s| s.color(color));
        self
    }

    /// Sets the text dim color of the gutter.
    ///
    /// This is the color of the line number for all lines except the current line.
    /// If this is not specified it will default to the gutter accent color.
    pub fn gutter_dim_color(mut self, color: Color) -> Self {
        self.0 = self.0.class(GutterClass, |s| s.set(DimColor, color));
        self
    }

    /// Sets the padding to the left of the numbers in the gutter.
    pub fn gutter_left_padding(mut self, padding: f64) -> Self {
        self.0 = self
            .0
            .class(GutterClass, |s| s.set(LeftOfCenterPadding, padding));
        self
    }

    /// Sets the padding to the right of the numbers in the gutter.
    pub fn gutter_right_padding(mut self, padding: f64) -> Self {
        self.0 = self
            .0
            .class(GutterClass, |s| s.set(RightOfCenterPadding, padding));
        self
    }

    /// Sets the background color of the current line in the gutter
    pub fn gutter_current_color(mut self, color: Color) -> Self {
        self.0 = self
            .0
            .class(GutterClass, |s| s.set(CurrentLineColor, color));
        self
    }

    /// Sets the background color to be applied around selected text.
    pub fn selection_color(mut self, color: Color) -> Self {
        self.0 = self
            .0
            .class(EditorViewClass, |s| s.set(SelectionColor, color));
        self
    }

    /// Sets the indent style.
    pub fn indent_style(mut self, indent_style: IndentStyle) -> Self {
        self.0 = self
            .0
            .class(EditorViewClass, |s| s.set(IndentStyleProp, indent_style));
        self
    }

    /// Sets the color of the indent guide.
    pub fn indent_guide_color(mut self, color: Color) -> Self {
        self.0 = self
            .0
            .class(EditorViewClass, |s| s.set(IndentGuideColor, color));
        self
    }

    /// Sets the method for wrapping lines.
    pub fn wrap_method(mut self, wrap: WrapMethod) -> Self {
        self.0 = self.0.class(EditorViewClass, |s| s.set(WrapProp, wrap));
        self
    }

    /// Sets the color of the cursor.
    pub fn cursor_color(mut self, cursor: Color) -> Self {
        self.0 = self
            .0
            .class(EditorViewClass, |s| s.set(CursorColor, cursor));
        self
    }

    /// Allow scrolling beyond the last line of the document.
    pub fn scroll_beyond_last_line(mut self, scroll_beyond: bool) -> Self {
        self.0 = self.0.class(EditorViewClass, |s| {
            s.set(ScrollBeyondLastLine, scroll_beyond)
        });
        self
    }

    /// Sets the background color of the current line.
    pub fn current_line_color(mut self, color: Color) -> Self {
        self.0 = self
            .0
            .class(EditorViewClass, |s| s.set(CurrentLineColor, color));
        self
    }

    /// Sets the color of visible whitespace characters.
    pub fn visible_whitespace(mut self, color: Color) -> Self {
        self.0 = self
            .0
            .class(EditorViewClass, |s| s.set(VisibleWhitespaceColor, color));
        self
    }

    /// Sets which white space characters should be rendered.
    pub fn render_white_space(mut self, render_white_space: RenderWhitespace) -> Self {
        self.0 = self.0.class(EditorViewClass, |s| {
            s.set(RenderWhitespaceProp, render_white_space)
        });
        self
    }

    /// Set the number of lines to keep visible above and below the cursor.
    /// Default: `1`
    pub fn cursor_surrounding_lines(mut self, lines: usize) -> Self {
        self.0 = self
            .0
            .class(EditorViewClass, |s| s.set(CursorSurroundingLines, lines));
        self
    }

    /// Sets whether the indent guides should be displayed.
    pub fn indent_guide(mut self, show: bool) -> Self {
        self.0 = self
            .0
            .class(EditorViewClass, |s| s.set(ShowIndentGuide, show));
        self
    }

    /// Sets the editor's mode to modal or non-modal.
    pub fn modal(mut self, modal: bool) -> Self {
        self.0 = self.0.class(EditorViewClass, |s| s.set(Modal, modal));
        self
    }

    /// Determines if line numbers are relative in modal mode.
    pub fn modal_relative_line(mut self, modal_relative_line: bool) -> Self {
        self.0 = self.0.class(EditorViewClass, |s| {
            s.set(ModalRelativeLine, modal_relative_line)
        });
        self
    }

    /// Enables or disables smart tab behavior, which inserts the indent style detected in the file when the tab key is pressed.
    pub fn smart_tab(mut self, smart_tab: bool) -> Self {
        self.0 = self
            .0
            .class(EditorViewClass, |s| s.set(SmartTab, smart_tab));
        self
    }

    /// Sets the color of phantom text
    pub fn phantom_color(mut self, color: Color) -> Self {
        self.0 = self
            .0
            .class(EditorViewClass, |s| s.set(PhantomColor, color));
        self
    }

    /// Sets the color of the placeholder text.
    pub fn placeholder_color(mut self, color: Color) -> Self {
        self.0 = self
            .0
            .class(EditorViewClass, |s| s.set(PlaceholderColor, color));
        self
    }

    /// Sets the color of the underline for preedit text.
    pub fn preedit_underline_color(mut self, color: Color) -> Self {
        self.0 = self
            .0
            .class(EditorViewClass, |s| s.set(PreeditUnderlineColor, color));
        self
    }
}

impl TextEditor {
    /// Sets the custom style properties of the `TextEditor`.
    pub fn editor_style(
        self,
        style: impl Fn(EditorCustomStyle) -> EditorCustomStyle + 'static,
    ) -> Self {
        let id = self.id();
        let view_state = id.state();
        let offset = view_state.borrow_mut().style.next_offset();
        let style = create_updater(
            move || style(EditorCustomStyle(Style::new())),
            move |style| id.update_style(offset, style.0),
        );
        view_state.borrow_mut().style.push(style.0);
        self
    }

    /// Return a reference to the underlying [Editor].
    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    /// Allows for creation of a [TextEditor] with an existing [Editor].
    pub fn with_editor(self, f: impl FnOnce(&Editor)) -> Self {
        f(&self.editor);
        self
    }

    /// Allows for creation of a [TextEditor] with an existing mutable [Editor].
    pub fn with_editor_mut(mut self, f: impl FnOnce(&mut Editor)) -> Self {
        f(&mut self.editor);
        self
    }

    /// Returns the [EditorId] of the underlying [Editor].
    pub fn editor_id(&self) -> EditorId {
        self.editor.id()
    }

    /// Opens the `TextEditor` with the provided [`Document`].
    /// You should usually not swap this out without good reason.
    pub fn with_doc(self, f: impl FnOnce(&dyn Document)) -> Self {
        self.editor.doc.with_untracked(|doc| {
            f(doc.as_ref());
        });
        self
    }

    /// Returns a reference to the underlying [Document]. This should usually be a [TextDocument].
    pub fn doc(&self) -> Rc<dyn Document> {
        self.editor.doc()
    }

    /// Try downcasting the document to a [`TextDocument`].
    /// Returns `None` if the document is not a [`TextDocument`].
    fn text_doc(&self) -> Option<Rc<TextDocument>> {
        (self.doc() as Rc<dyn ::std::any::Any>).downcast().ok()
    }

    // TODO(minor): should this be named `text`? Ideally most users should use the rope text version
    pub fn rope_text(&self) -> RopeTextVal {
        self.editor.rope_text()
    }

    /// Use a different document in the text editor
    pub fn use_doc(self, doc: Rc<dyn Document>) -> Self {
        self.editor.update_doc(doc, None);
        self
    }

    /// Use the same document as another text editor view.
    ///
    /// ```rust,ignore
    /// let primary = text_editor();
    /// let secondary = text_editor().share_document(&primary);
    ///
    /// stack((
    ///     primary,
    ///     secondary,
    /// ))
    /// ```
    ///
    /// If you wish for it to also share the styling, consider using [`TextEditor::shared_editor`]
    /// instead.
    pub fn share_doc(self, other: &TextEditor) -> Self {
        self.use_doc(other.editor.doc())
    }

    /// Create a new [`TextEditor`] instance from this instance, sharing the document and styling.
    ///
    /// ```rust,ignore
    /// let primary = text_editor();
    /// let secondary = primary.shared_editor();
    /// ```
    ///
    /// Also see the [Editor example](https://github.com/lapce/floem/tree/main/examples/editor).
    pub fn shared_editor(&self) -> TextEditor {
        let id = ViewId::new();

        let doc = self.editor.doc();
        let style = self.editor.style();
        let editor = Editor::new(self.cx, doc, style, false);

        let editor_sig = self.cx.create_rw_signal(editor.clone());
        let child = with_scope(self.cx, || {
            editor_container_view(editor_sig, |_| true, default_key_handler(editor_sig))
        })
        .into_view();

        let child_id = child.id();
        id.set_children([child]);

        TextEditor {
            id,
            cx: self.cx,
            editor,
            child: child_id,
        }
    }

    /// Change the [`Styling`] used for the editor.
    ///
    /// ```rust,ignore
    /// let styling = SimpleStyling::builder()
    ///     .font_size(12)
    ///     .weight(Weight::BOLD);
    /// text_editor().styling(styling);
    /// ```
    pub fn styling(self, styling: impl Styling + 'static) -> Self {
        self.styling_rc(Rc::new(styling))
    }

    /// Use an `Rc<dyn Styling>` to share between different editors.
    pub fn styling_rc(self, styling: Rc<dyn Styling>) -> Self {
        self.editor.update_styling(styling);
        self
    }

    /// Set the text editor to read only.
    /// Equivalent to setting [`Editor::read_only`]
    /// Default: `false`
    pub fn read_only(self) -> Self {
        self.editor.read_only.set(true);
        self
    }

    /// Set the placeholder text that is displayed when the document is empty.
    /// Can span multiple lines.
    /// This is per-editor, not per-document.
    /// Equivalent to calling [`TextDocument::add_placeholder`]
    /// Default: `None`
    ///
    /// Note: only works for the default backing [`TextDocument`] doc
    pub fn placeholder(self, text: impl Into<String>) -> Self {
        if let Some(doc) = self.text_doc() {
            doc.add_placeholder(self.editor_id(), text.into());
        }

        self
    }

    /// When commands are run on the document, this function is called.
    /// If it returns [`CommandExecuted::Yes`] then further handlers after it, including the
    /// default handler, are not executed.
    ///
    /// ```rust
    /// use floem::views::editor::command::{Command, CommandExecuted};
    /// use floem::views::text_editor::text_editor;
    /// use floem_editor_core::command::EditCommand;
    /// text_editor("Hello")
    ///     .pre_command(|ev| {
    ///         if matches!(ev.cmd, Command::Edit(EditCommand::Undo)) {
    ///             // Sorry, no undoing allowed
    ///             CommandExecuted::Yes
    ///         } else {
    ///             CommandExecuted::No
    ///         }
    ///     })
    ///     .pre_command(|_| {
    ///         // This will never be called if command was an undo
    ///         CommandExecuted::Yes
    ///     })
    ///     .pre_command(|_| {
    ///         // This will never be called
    ///         CommandExecuted::No
    ///     });
    /// ```
    ///
    /// Note that these are specific to each text editor view.
    ///
    /// Note: only works for the default backing [`TextDocument`] doc
    pub fn pre_command(self, f: impl Fn(PreCommand) -> CommandExecuted + 'static) -> Self {
        if let Some(doc) = self.text_doc() {
            doc.add_pre_command(self.editor.id(), f);
        }
        self
    }

    /// Listen for deltas applied to the editor.
    ///
    /// Useful for anything that has positions based in the editor that can be updated after
    /// typing, such as syntax highlighting.
    ///
    /// Note: only works for the default backing [`TextDocument`] doc
    pub fn update(self, f: impl Fn(OnUpdate) + 'static) -> Self {
        if let Some(doc) = self.text_doc() {
            doc.add_on_update(f);
        }
        self
    }
}
