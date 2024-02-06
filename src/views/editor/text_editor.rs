use std::rc::Rc;

use floem_editor_core::buffer::{rope_text::RopeTextVal, InvalLines};
use floem_reactive::{with_scope, Scope};
use floem_winit::keyboard::ModifiersState;
use lapce_xi_rope::{Rope, RopeDelta};

use crate::{
    id::Id,
    view::{View, ViewData, Widget},
};

use super::{
    command::{Command, CommandExecuted},
    editor::Editor,
    id::EditorId,
    keypress::default_key_handler,
    text::{Document, SimpleStyling, Styling, TextDocument},
    view::editor_container_view,
};

/// A text editor view.  
/// Note: this requires that the document underlying it is a [`TextDocument`] for the use of some
/// logic.
pub struct TextEditor {
    data: ViewData,
    /// The scope this view was created in, used for creating the final view
    cx: Scope,

    editor: Editor,
}

pub fn text_editor(text: impl Into<Rope>) -> TextEditor {
    let id = Id::next();
    let cx = Scope::current();
    // TODO: state update?

    let doc = Rc::new(TextDocument::new(cx, text));
    let style = Rc::new(SimpleStyling::light());
    let editor = Editor::new(cx, doc, style);

    TextEditor {
        data: ViewData::new(id),
        cx,
        editor,
    }
}

impl View for TextEditor {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn build(self) -> Box<dyn Widget> {
        let cx = self.cx;

        let editor = cx.create_rw_signal(self.editor);
        // TODO: better is_active
        let view = with_scope(self.cx, || {
            editor_container_view(editor, |_| true, default_key_handler(editor))
        });
        view.build()
    }
}

impl TextEditor {
    /// Note: this requires that the document underlying it is a [`TextDocument`] for the use of
    /// some logic. You should usually not swap this out without good reason.
    pub fn with_editor(self, f: impl FnOnce(&Editor)) -> Self {
        f(&self.editor);
        self
    }

    /// Note: this requires that the document underlying it is a [`TextDocument`] for the use of
    /// some logic. You should usually not swap this out without good reason.
    pub fn with_editor_mut(mut self, f: impl FnOnce(&mut Editor)) -> Self {
        f(&mut self.editor);
        self
    }

    pub fn editor_id(&self) -> EditorId {
        self.editor.id()
    }

    pub fn with_document(self, f: impl FnOnce(&dyn Document)) -> Self {
        self.editor.doc.with_untracked(|doc| {
            f(doc.as_ref());
        });
        self
    }

    pub fn document(&self) -> Rc<dyn Document> {
        self.editor.doc()
    }

    // TODO(minor): should this be named `text`? Ideally most users should use the rope text version
    pub fn rope_text(&self) -> RopeTextVal {
        self.editor.rope_text()
    }

    /// Use a different document in the text editor  
    pub fn use_document(self, doc: Rc<dyn Document>) -> Self {
        self.editor.update_doc(doc, None);
        self
    }

    /// Use the same document as another text editor view.  
    /// ```rust,ignore
    /// let primary = text_editor();
    /// let secondary = text_editor().share_document(&primary);
    ///
    /// stack((
    ///     primary,
    ///     secondary,
    /// ))
    /// ```  
    /// If you wish for it to also share the styling, consider using [`TextEditor::shared_editor`]
    /// instead.
    pub fn share_document(self, other: &TextEditor) -> Self {
        self.use_document(other.editor.doc())
    }

    /// Create a new [`TextEditor`] instance from this instance, sharing the document and styling.
    /// ```rust,ignore
    /// let primary = text_editor();
    /// let secondary = primary.shared_editor();
    /// ```
    pub fn shared_editor(&self) -> TextEditor {
        let id = Id::next();

        let doc = self.editor.doc();
        let style = self.editor.style();
        let editor = Editor::new(self.cx, doc, style);

        TextEditor {
            data: ViewData::new(id),
            cx: self.cx,
            editor,
        }
    }

    /// Change the [`Styling`] used for the editor.  
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

    /// When commands are run on the document, this function is called.  
    /// If it returns [`CommandExecuted::Yes`] then further handlers after it, including the
    /// default handler, are not executed.  
    /// ```rust,ignore
    /// text_editor("Hello")
    ///     .pre_command(|_editor, cmd, _count, _modifiers| {
    ///         if matches!(cmd, Command::Edit(EditCommand::Undo)) {
    ///             // Sorry, no undoing allowed   
    ///             CommandExecuted::Yes
    ///         } else {
    ///             CommandExecuted::No
    ///         }
    ///     })
    ///     .pre_command(|_editor, _cmd, _, _| {
    ///         // This will never be called if command was an undo
    ///         CommandExecuted::Yes
    ///     }))
    ///     .pre_command(|_editor, _cmd, _, _| {
    ///         // This will never be called
    ///         CommandExecuted::No
    ///     })
    /// ```
    /// Note that these are specific to each text editor view.
    pub fn pre_command(
        self,
        f: impl Fn(&Editor, &Command, Option<usize>, ModifiersState) -> CommandExecuted + 'static,
    ) -> Self {
        let doc: Result<Rc<TextDocument>, _> = self.editor.doc().downcast_rc();
        if let Ok(doc) = doc {
            doc.add_pre_command(self.editor.id(), f);
        }
        self
    }

    pub fn update(self, f: impl Fn(&Editor, &[(Rope, RopeDelta, InvalLines)]) + 'static) -> Self {
        let doc: Result<Rc<TextDocument>, _> = self.editor.doc().downcast_rc();
        if let Ok(doc) = doc {
            doc.add_on_update(f);
        }
        self
    }
}
