use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ops::Range,
    rc::Rc,
};

use floem_editor_core::{
    buffer::{Buffer, InvalLines, rope_text::RopeText},
    command::EditCommand,
    cursor::Cursor,
    editor::{Action, EditConf, EditType},
    mode::{Mode, MotionMode},
    register::Register,
    selection::Selection,
    word::WordCursor,
};
use floem_reactive::{Effect, RwSignal, Scope, SignalGet, SignalTrack, SignalUpdate, SignalWith};
use lapce_xi_rope::{Rope, RopeDelta};
use smallvec::{SmallVec, smallvec};
use ui_events::keyboard::Modifiers;

use super::{
    Editor, EditorStyle,
    actions::{CommonAction, handle_command_default},
    command::{Command, CommandExecuted},
    id::EditorId,
    phantom_text::{PhantomText, PhantomTextKind, PhantomTextLine},
    text::{Document, DocumentPhantom, PreeditData, SystemClipboard},
    view::ScreenLines,
    visual_line::RVLine,
};
use crate::view::ViewId;

type PreCommandFn = Box<dyn Fn(PreCommand) -> CommandExecuted>;
#[derive(Clone, Copy)]
struct AttachedEditor {
    view_id: floem_reactive::RwSignal<Option<ViewId>>,
    screen_lines: floem_reactive::RwSignal<ScreenLines>,
}

#[derive(Debug, Clone)]
pub struct PreCommand<'a> {
    pub editor: &'a Editor,
    pub cmd: &'a Command,
    pub count: Option<usize>,
    pub mods: Modifiers,
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
///
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

    pub placeholders: RwSignal<HashMap<EditorId, String>>,

    // (cmd: &Command, count: Option<usize>, modifiers: ModifierState)
    /// Ran before a command is executed. If it says that it executed the command, then handlers
    /// after it will not be called.
    pre_command: Rc<RefCell<HashMap<EditorId, SmallVec<[PreCommandFn; 1]>>>>,

    on_updates: Rc<RefCell<SmallVec<[OnUpdateFn; 1]>>>,
    attached_editors: Rc<RefCell<HashMap<EditorId, AttachedEditor>>>,
}
impl TextDocument {
    pub fn new(cx: Scope, text: impl Into<Rope>) -> TextDocument {
        let text = text.into();
        let buffer = Buffer::new(text);
        let preedit = PreeditData {
            preedit: cx.create_rw_signal(None),
        };
        let cache_rev = cx.create_rw_signal(0);

        let placeholders = cx.create_rw_signal(HashMap::new());

        // Whenever the placeholders change, update the cache rev
        Effect::new(move |_| {
            placeholders.track();
            cache_rev.try_update(|cache_rev| {
                *cache_rev += 1;
            });
        });

        TextDocument {
            buffer: cx.create_rw_signal(buffer),
            cache_rev,
            preedit,
            keep_indent: Cell::new(true),
            auto_indent: Cell::new(false),
            placeholders,
            pre_command: Rc::new(RefCell::new(HashMap::new())),
            on_updates: Rc::new(RefCell::new(SmallVec::new())),
            attached_editors: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    fn update_cache_rev(&self) {
        self.cache_rev.try_update(|cache_rev| {
            *cache_rev += 1;
        });
    }

    fn on_update(&self, ed: Option<&Editor>, deltas: &[(Rope, RopeDelta, InvalLines)]) {
        self.request_shared_editor_paint(ed, deltas);

        let on_updates = self.on_updates.borrow();
        let data = OnUpdate { editor: ed, deltas };
        for on_update in on_updates.iter() {
            on_update(data.clone());
        }
    }

    pub fn register_editor(
        &self,
        editor_id: EditorId,
        view_id: floem_reactive::RwSignal<Option<ViewId>>,
        screen_lines: floem_reactive::RwSignal<ScreenLines>,
    ) {
        self.attached_editors.borrow_mut().insert(
            editor_id,
            AttachedEditor {
                view_id,
                screen_lines,
            },
        );
    }

    pub fn unregister_editor(&self, editor_id: EditorId) {
        self.attached_editors.borrow_mut().remove(&editor_id);
    }

    fn request_shared_editor_paint(
        &self,
        source: Option<&Editor>,
        deltas: &[(Rope, RopeDelta, InvalLines)],
    ) {
        let attached_editors = self.attached_editors.borrow();

        if let Some(source) = source {
            let Some(source_range) = source
                .screen_lines
                .with_untracked(|screen_lines| screen_lines.rvline_range())
            else {
                return;
            };

            for (editor_id, attached) in attached_editors.iter() {
                if *editor_id == source.id() {
                    continue;
                }

                let overlaps = attached
                    .screen_lines
                    .with_untracked(|screen_lines| screen_lines.rvline_range())
                    .is_some_and(|peer_range| rvline_ranges_overlap(source_range, peer_range));

                if overlaps && let Some(view_id) = attached.view_id.get_untracked() {
                    view_id.request_paint();
                }
            }

            return;
        }

        for attached in attached_editors.values() {
            let overlaps_change = attached
                .screen_lines
                .with_untracked(|screen_lines| screen_lines.rvline_range())
                .is_some_and(|visible_range| {
                    deltas
                        .iter()
                        .any(|(_, _, inval)| inval_overlaps_visible_lines(inval, visible_range))
                });

            if overlaps_change && let Some(view_id) = attached.view_id.get_untracked() {
                view_id.request_paint();
            }
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

    pub fn add_placeholder(&self, editor_id: EditorId, placeholder: String) {
        self.placeholders.update(|placeholders| {
            placeholders.insert(editor_id, placeholder);
        });
    }

    fn placeholder(&self, editor_id: EditorId) -> Option<String> {
        self.placeholders
            .with_untracked(|placeholders| placeholders.get(&editor_id).cloned())
    }
}

fn rvline_ranges_overlap(
    (a_start, a_end): (RVLine, RVLine),
    (b_start, b_end): (RVLine, RVLine),
) -> bool {
    a_start <= b_end && b_start <= a_end
}

fn inval_overlaps_visible_lines(inval: &InvalLines, visible_range: (RVLine, RVLine)) -> bool {
    let start_line = inval.start_line;
    let end_line = inval.start_line + usize::max(inval.inval_count, inval.new_count);
    let visible_start = visible_range.0.line;
    let visible_end = visible_range.1.line;

    start_line <= visible_end && visible_start <= end_line
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
        modifiers: Modifiers,
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
    fn phantom_text(&self, edid: EditorId, styling: &EditorStyle, line: usize) -> PhantomTextLine {
        let mut text = SmallVec::new();

        if self.buffer.with_untracked(Buffer::is_empty)
            && self.preedit.preedit.with_untracked(|p| p.is_none())
            && let Some(placeholder) = self.placeholder(edid)
        {
            text.push(PhantomText {
                kind: PhantomTextKind::Placeholder,
                col: 0,
                affinity: None,
                text: placeholder,
                font_size: None,
                fg: Some(styling.placeholder_color()),
                bg: None,
                under_line: None,
            });
        }

        if let Some(preedit) = self.preedit_phantom(Some(styling.preedit_underline_color()), line) {
            text.push(preedit);
        }

        PhantomTextLine { text }
    }

    fn has_multiline_phantom(&self, edid: EditorId, _styling: &EditorStyle) -> bool {
        if !self.buffer.with_untracked(Buffer::is_empty) {
            return false;
        }

        let placeholder_ml = self.placeholders.with_untracked(|placeholder| {
            let Some(placeholder) = placeholder.get(&edid) else {
                return false;
            };

            placeholder.lines().count() > 1
        });

        if placeholder_ml {
            return true;
        }

        self.preedit.preedit.with_untracked(|preedit| {
            let Some(preedit) = preedit else {
                return false;
            };

            preedit.text.lines().count() > 1
        })
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

            self.update_cache_rev();
            self.on_update(Some(ed), &deltas);
        }

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
