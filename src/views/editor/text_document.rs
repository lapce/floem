use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ops::Range,
    rc::Rc,
};

use floem_editor_core::{
    buffer::{rope_text::RopeText, Buffer, InvalLines},
    command::EditCommand,
    cursor::Cursor,
    editor::{Action, EditConf, EditType},
    mode::{Mode, MotionMode},
    register::Register,
    selection::Selection,
    word::WordCursor,
};
use floem_reactive::{RwSignal, Scope};
use floem_winit::keyboard::ModifiersState;
use lapce_xi_rope::{Rope, RopeDelta};
use smallvec::{smallvec, SmallVec};

use super::{
    actions::{handle_command_default, CommonAction},
    command::{Command, CommandExecuted},
    id::EditorId,
    phantom_text::PhantomTextLine,
    text::{Document, DocumentPhantom, PreeditData, SystemClipboard},
    Editor,
};

type PreCommandFn = Box<dyn Fn(PreCommand) -> CommandExecuted>;
#[derive(Debug, Clone)]
pub struct PreCommand<'a> {
    pub editor: &'a Editor,
    pub cmd: &'a Command,
    pub count: Option<usize>,
    pub mods: ModifiersState,
}

type OnUpdateFn = Box<dyn Fn(OnUpdate)>;
#[derive(Debug, Clone)]
pub struct OnUpdate<'a> {
    /// Optional because the document can be edited from outside any editor views
    pub editor: Option<&'a Editor>,
    deltas: &'a [(Rope, RopeDelta, InvalLines)],
}
impl<'a> OnUpdate<'a> {
    pub fn deltas(&self) -> impl Iterator<Item = &'a RopeDelta> {
        self.deltas.iter().map(|(_, delta, _)| delta)
    }
}

/// A simple text document that holds content in a rope.  
/// This can be used as a base structure for common operations.
#[derive(Clone)]
pub struct TextDocument {
    buffer: RwSignal<Buffer>,
    cache_rev: RwSignal<u64>,
    preedit: PreeditData,

    /// Whether to keep the indent of the previous line when inserting a new line
    pub keep_indent: Cell<bool>,
    /// Whether to automatically indent the new line via heuristics
    pub auto_indent: Cell<bool>,

    // (cmd: &Command, count: Option<usize>, modifiers: ModifierState)
    /// Ran before a command is executed. If it says that it executed the command, then handlers
    /// after it will not be called.
    pre_command: Rc<RefCell<HashMap<EditorId, SmallVec<[PreCommandFn; 1]>>>>,

    on_updates: Rc<RefCell<SmallVec<[OnUpdateFn; 1]>>>,
}
impl TextDocument {
    pub fn new(cx: Scope, text: impl Into<Rope>) -> TextDocument {
        let text = text.into();
        let buffer = Buffer::new(text);
        let preedit = PreeditData {
            preedit: cx.create_rw_signal(None),
        };

        TextDocument {
            buffer: cx.create_rw_signal(buffer),
            cache_rev: cx.create_rw_signal(0),
            preedit,
            keep_indent: Cell::new(true),
            auto_indent: Cell::new(false),
            pre_command: Rc::new(RefCell::new(HashMap::new())),
            on_updates: Rc::new(RefCell::new(SmallVec::new())),
        }
    }

    fn update_cache_rev(&self) {
        self.cache_rev.try_update(|cache_rev| {
            *cache_rev += 1;
        });
    }

    fn on_update(&self, ed: Option<&Editor>, deltas: &[(Rope, RopeDelta, InvalLines)]) {
        let on_updates = self.on_updates.borrow();
        let data = OnUpdate { editor: ed, deltas };
        for on_update in on_updates.iter() {
            on_update(data.clone());
        }
    }

    pub fn add_pre_command(
        &self,
        id: EditorId,
        pre_command: impl Fn(PreCommand) -> CommandExecuted + 'static,
    ) {
        let pre_command: PreCommandFn = Box::new(pre_command);
        self.pre_command
            .borrow_mut()
            .insert(id, smallvec![pre_command]);
    }

    pub fn clear_pre_commands(&self) {
        self.pre_command.borrow_mut().clear();
    }

    pub fn add_on_update(&self, on_update: impl Fn(OnUpdate) + 'static) {
        self.on_updates.borrow_mut().push(Box::new(on_update));
    }

    pub fn clear_on_updates(&self) {
        self.on_updates.borrow_mut().clear();
    }
}
impl Document for TextDocument {
    fn text(&self) -> Rope {
        self.buffer.with_untracked(|buffer| buffer.text().clone())
    }

    fn cache_rev(&self) -> RwSignal<u64> {
        self.cache_rev
    }

    fn preedit(&self) -> PreeditData {
        self.preedit.clone()
    }

    fn run_command(
        &self,
        ed: &Editor,
        cmd: &Command,
        count: Option<usize>,
        modifiers: ModifiersState,
    ) -> CommandExecuted {
        let pre_commands = self.pre_command.borrow();
        let pre_commands = pre_commands.get(&ed.id());
        let pre_commands = pre_commands.iter().flat_map(|c| c.iter());
        let data = PreCommand {
            editor: ed,
            cmd,
            count,
            mods: modifiers,
        };

        for pre_command in pre_commands {
            if pre_command(data.clone()) == CommandExecuted::Yes {
                return CommandExecuted::Yes;
            }
        }

        handle_command_default(ed, self, cmd, count, modifiers)
    }

    fn receive_char(&self, ed: &Editor, c: &str) {
        if ed.read_only.get_untracked() {
            return;
        }

        let mode = ed.cursor.with_untracked(|c| c.get_mode());
        if mode == Mode::Insert {
            let mut cursor = ed.cursor.get_untracked();
            {
                let old_cursor_mode = cursor.mode.clone();
                let deltas = self
                    .buffer
                    .try_update(|buffer| {
                        Action::insert(
                            &mut cursor,
                            buffer,
                            c,
                            &|_, c, offset| {
                                WordCursor::new(&self.text(), offset).previous_unmatched(c)
                            },
                            // TODO: ?
                            false,
                            false,
                        )
                    })
                    .unwrap();
                self.buffer.update(|buffer| {
                    buffer.set_cursor_before(old_cursor_mode);
                    buffer.set_cursor_after(cursor.mode.clone());
                });
                // TODO: line specific invalidation
                self.update_cache_rev();
                self.on_update(Some(ed), &deltas);
            }
            ed.cursor.set(cursor);
        }
    }

    fn edit(&self, iter: &mut dyn Iterator<Item = (Selection, &str)>, edit_type: EditType) {
        let deltas = self
            .buffer
            .try_update(|buffer| buffer.edit(iter, edit_type));
        let deltas = deltas.map(|x| [x]);
        let deltas = deltas.as_ref().map(|x| x as &[_]).unwrap_or(&[]);

        self.update_cache_rev();
        self.on_update(None, deltas);
    }
}
impl DocumentPhantom for TextDocument {
    fn phantom_text(&self, _line: usize) -> PhantomTextLine {
        PhantomTextLine::default()
    }

    fn has_multiline_phantom(&self) -> bool {
        false
    }
}
impl CommonAction for TextDocument {
    fn exec_motion_mode(
        &self,
        _ed: &Editor,
        cursor: &mut Cursor,
        motion_mode: MotionMode,
        range: Range<usize>,
        is_vertical: bool,
        register: &mut Register,
    ) {
        self.buffer.try_update(move |buffer| {
            Action::execute_motion_mode(cursor, buffer, motion_mode, range, is_vertical, register)
        });
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
        if ed.read_only.get_untracked() && !cmd.not_changing_buffer() {
            return false;
        }

        let mut clipboard = SystemClipboard::new();
        let old_cursor = cursor.mode.clone();
        // TODO: configurable comment token
        let deltas = self
            .buffer
            .try_update(|buffer| {
                Action::do_edit(
                    cursor,
                    buffer,
                    cmd,
                    &mut clipboard,
                    register,
                    EditConf {
                        modal,
                        comment_token: "",
                        smart_tab,
                        keep_indent: self.keep_indent.get(),
                        auto_indent: self.auto_indent.get(),
                    },
                )
            })
            .unwrap();

        if !deltas.is_empty() {
            self.buffer.update(|buffer| {
                buffer.set_cursor_before(old_cursor);
                buffer.set_cursor_after(cursor.mode.clone());
            });
        }

        self.update_cache_rev();
        self.on_update(Some(ed), &deltas);

        !deltas.is_empty()
    }
}

impl std::fmt::Debug for TextDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("TextDocument");
        s.field("text", &self.text());
        s.finish()
    }
}
