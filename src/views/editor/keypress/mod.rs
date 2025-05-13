pub mod key;
pub mod press;

use std::{collections::HashMap, str::FromStr};

use crate::reactive::RwSignal;
use floem_editor_core::{
    command::{EditCommand, MoveCommand, MultiSelectionCommand, ScrollCommand},
    mode::Mode,
};
use floem_reactive::{SignalGet, SignalWith};
use ui_events::keyboard::{Key, KeyboardEvent, Modifiers};

use super::{
    command::{Command, CommandExecuted},
    Editor,
};

/// The default keymap handler does not have modal-mode specific
/// keybindings.
#[derive(Clone)]
pub struct KeypressMap {
    pub keymaps: HashMap<KeyboardEvent, Command>,
}
impl KeypressMap {
    pub fn default_windows() -> Self {
        let mut keymaps = HashMap::new();
        add_default_common(&mut keymaps);
        add_default_windows(&mut keymaps);
        Self { keymaps }
    }

    pub fn default_macos() -> Self {
        let mut keymaps = HashMap::new();
        add_default_common(&mut keymaps);
        add_default_macos(&mut keymaps);
        Self { keymaps }
    }

    pub fn default_linux() -> Self {
        let mut keymaps = HashMap::new();
        add_default_common(&mut keymaps);
        add_default_linux(&mut keymaps);
        Self { keymaps }
    }
}
impl Default for KeypressMap {
    fn default() -> Self {
        match std::env::consts::OS {
            "macos" => Self::default_macos(),
            "windows" => Self::default_windows(),
            _ => Self::default_linux(),
        }
    }
}

fn key(s: &str, m: Modifiers) -> KeyboardEvent {
    KeyboardEvent {
        key: Key::from_str(s).unwrap(),
        modifiers: m,
        ..Default::default()
    }
}

fn key_d(s: &str) -> KeyboardEvent {
    key(s, Modifiers::default())
}

fn add_default_common(c: &mut HashMap<KeyboardEvent, Command>) {
    // Note: this should typically be kept in sync with Lapce's
    // `defaults/keymaps-common.toml`

    // --- Basic editing ---

    c.insert(
        key("up", Modifiers::ALT),
        Command::Edit(EditCommand::MoveLineUp),
    );
    c.insert(
        key("down", Modifiers::ALT),
        Command::Edit(EditCommand::MoveLineDown),
    );

    c.insert(key_d("delete"), Command::Edit(EditCommand::DeleteForward));
    c.insert(
        key_d("backspace"),
        Command::Edit(EditCommand::DeleteBackward),
    );
    c.insert(
        key("backspace", Modifiers::SHIFT),
        Command::Edit(EditCommand::DeleteForward),
    );

    c.insert(key_d("home"), Command::Move(MoveCommand::LineStartNonBlank));
    c.insert(key_d("end"), Command::Move(MoveCommand::LineEnd));

    c.insert(key_d("pageup"), Command::Scroll(ScrollCommand::PageUp));
    c.insert(key_d("pagedown"), Command::Scroll(ScrollCommand::PageDown));
    c.insert(
        key("pageup", Modifiers::CONTROL),
        Command::Scroll(ScrollCommand::ScrollUp),
    );
    c.insert(
        key("pagedown", Modifiers::CONTROL),
        Command::Scroll(ScrollCommand::ScrollDown),
    );

    // --- Multi cursor ---

    c.insert(
        key("i", Modifiers::ALT | Modifiers::SHIFT),
        Command::MultiSelection(MultiSelectionCommand::InsertCursorEndOfLine),
    );

    // TODO: should we have jump location backward/forward?

    // TODO: jump to snippet positions?

    // --- ---- ---
    c.insert(key_d("right"), Command::Move(MoveCommand::Right));
    c.insert(key_d("left"), Command::Move(MoveCommand::Left));
    c.insert(key_d("up"), Command::Move(MoveCommand::Up));
    c.insert(key_d("down"), Command::Move(MoveCommand::Down));

    c.insert(key_d("enter"), Command::Edit(EditCommand::InsertNewLine));

    c.insert(key_d("tab"), Command::Edit(EditCommand::InsertTab));

    c.insert(
        key("up", Modifiers::ALT | Modifiers::SHIFT),
        Command::Edit(EditCommand::DuplicateLineUp),
    );
    c.insert(
        key("down", Modifiers::ALT | Modifiers::SHIFT),
        Command::Edit(EditCommand::DuplicateLineDown),
    );
}

fn add_default_windows(c: &mut HashMap<KeyboardEvent, Command>) {
    add_default_nonmacos(c);
}

fn add_default_macos(c: &mut HashMap<KeyboardEvent, Command>) {
    // Note: this should typically be kept in sync with Lapce's
    // `defaults/keymaps-macos.toml`

    // --- Basic editing ---
    c.insert(key("z", Modifiers::META), Command::Edit(EditCommand::Undo));
    c.insert(
        key("z", Modifiers::META | Modifiers::SHIFT),
        Command::Edit(EditCommand::Redo),
    );
    c.insert(key("y", Modifiers::META), Command::Edit(EditCommand::Redo));
    c.insert(
        key("x", Modifiers::META),
        Command::Edit(EditCommand::ClipboardCut),
    );
    c.insert(
        key("c", Modifiers::META),
        Command::Edit(EditCommand::ClipboardCopy),
    );
    c.insert(
        key("v", Modifiers::META),
        Command::Edit(EditCommand::ClipboardPaste),
    );

    c.insert(
        key("right", Modifiers::ALT),
        Command::Move(MoveCommand::WordEndForward),
    );
    c.insert(
        key("left", Modifiers::ALT),
        Command::Move(MoveCommand::WordBackward),
    );
    c.insert(
        key("left", Modifiers::META),
        Command::Move(MoveCommand::LineStartNonBlank),
    );
    c.insert(
        key("right", Modifiers::META),
        Command::Move(MoveCommand::LineEnd),
    );

    c.insert(
        key("a", Modifiers::CONTROL),
        Command::Move(MoveCommand::LineStartNonBlank),
    );
    c.insert(
        key("e", Modifiers::CONTROL),
        Command::Move(MoveCommand::LineEnd),
    );

    c.insert(
        key("k", Modifiers::META | Modifiers::SHIFT),
        Command::Edit(EditCommand::DeleteLine),
    );

    c.insert(
        key("backspace", Modifiers::ALT),
        Command::Edit(EditCommand::DeleteWordBackward),
    );
    c.insert(
        key("backspace", Modifiers::META),
        Command::Edit(EditCommand::DeleteToBeginningOfLine),
    );
    c.insert(
        key("k", Modifiers::CONTROL),
        Command::Edit(EditCommand::DeleteToEndOfLine),
    );
    c.insert(
        key("delete", Modifiers::ALT),
        Command::Edit(EditCommand::DeleteWordForward),
    );

    // TODO: match pairs?
    // TODO: indent/outdent line?

    c.insert(
        key("a", Modifiers::META),
        Command::MultiSelection(MultiSelectionCommand::SelectAll),
    );

    c.insert(
        key("enter", Modifiers::META),
        Command::Edit(EditCommand::NewLineBelow),
    );
    c.insert(
        key("enter", Modifiers::META | Modifiers::SHIFT),
        Command::Edit(EditCommand::NewLineAbove),
    );

    // --- Multi cursor ---
    c.insert(
        key("up", Modifiers::ALT | Modifiers::META),
        Command::MultiSelection(MultiSelectionCommand::InsertCursorAbove),
    );
    c.insert(
        key("down", Modifiers::ALT | Modifiers::META),
        Command::MultiSelection(MultiSelectionCommand::InsertCursorBelow),
    );

    c.insert(
        key("l", Modifiers::META),
        Command::MultiSelection(MultiSelectionCommand::SelectCurrentLine),
    );
    c.insert(
        key("l", Modifiers::META | Modifiers::SHIFT),
        Command::MultiSelection(MultiSelectionCommand::SelectAllCurrent),
    );

    c.insert(
        key("u", Modifiers::META),
        Command::MultiSelection(MultiSelectionCommand::SelectUndo),
    );

    // --- ---- ---
    c.insert(
        key("up", Modifiers::META),
        Command::Move(MoveCommand::DocumentStart),
    );
    c.insert(
        key("down", Modifiers::META),
        Command::Move(MoveCommand::DocumentEnd),
    );
}

fn add_default_linux(c: &mut HashMap<KeyboardEvent, Command>) {
    add_default_nonmacos(c);
}

fn add_default_nonmacos(c: &mut HashMap<KeyboardEvent, Command>) {
    // Note: this should typically be kept in sync with Lapce's
    // `defaults/keymaps-nonmacos.toml`

    // --- Basic editing ---
    c.insert(
        key("z", Modifiers::CONTROL),
        Command::Edit(EditCommand::Undo),
    );
    c.insert(
        key("z", Modifiers::CONTROL | Modifiers::SHIFT),
        Command::Edit(EditCommand::Redo),
    );
    c.insert(
        key("y", Modifiers::CONTROL),
        Command::Edit(EditCommand::Redo),
    );
    c.insert(
        key("x", Modifiers::CONTROL),
        Command::Edit(EditCommand::ClipboardCut),
    );
    c.insert(
        key("delete", Modifiers::SHIFT),
        Command::Edit(EditCommand::ClipboardCut),
    );
    c.insert(
        key("c", Modifiers::CONTROL),
        Command::Edit(EditCommand::ClipboardCopy),
    );
    c.insert(
        key("insert", Modifiers::CONTROL),
        Command::Edit(EditCommand::ClipboardCopy),
    );
    c.insert(
        key("v", Modifiers::CONTROL),
        Command::Edit(EditCommand::ClipboardPaste),
    );
    c.insert(
        key("insert", Modifiers::SHIFT),
        Command::Edit(EditCommand::ClipboardPaste),
    );

    c.insert(
        key("right", Modifiers::CONTROL),
        Command::Move(MoveCommand::WordEndForward),
    );
    c.insert(
        key("left", Modifiers::CONTROL),
        Command::Move(MoveCommand::WordBackward),
    );

    c.insert(
        key("backspace", Modifiers::CONTROL),
        Command::Edit(EditCommand::DeleteWordBackward),
    );
    c.insert(
        key("delete", Modifiers::CONTROL),
        Command::Edit(EditCommand::DeleteWordForward),
    );

    // TODO: match pairs?

    // TODO: indent/outdent line?

    c.insert(
        key("a", Modifiers::CONTROL),
        Command::MultiSelection(MultiSelectionCommand::SelectAll),
    );

    c.insert(
        key("enter", Modifiers::CONTROL),
        Command::Edit(EditCommand::NewLineAbove),
    );

    // --- Multi cursor ---
    c.insert(
        key("up", Modifiers::CONTROL | Modifiers::ALT),
        Command::MultiSelection(MultiSelectionCommand::InsertCursorAbove),
    );
    c.insert(
        key("down", Modifiers::CONTROL | Modifiers::ALT),
        Command::MultiSelection(MultiSelectionCommand::InsertCursorBelow),
    );

    c.insert(
        key("l", Modifiers::CONTROL),
        Command::MultiSelection(MultiSelectionCommand::SelectCurrentLine),
    );
    c.insert(
        key("l", Modifiers::CONTROL | Modifiers::SHIFT),
        Command::MultiSelection(MultiSelectionCommand::SelectAllCurrent),
    );

    c.insert(
        key("u", Modifiers::CONTROL),
        Command::MultiSelection(MultiSelectionCommand::SelectUndo),
    );

    // --- Navigation ---
    c.insert(
        key("home", Modifiers::CONTROL),
        Command::Move(MoveCommand::DocumentStart),
    );
    c.insert(
        key("end", Modifiers::CONTROL),
        Command::Move(MoveCommand::DocumentEnd),
    );
}

pub fn default_key_handler(
    editor: RwSignal<Editor>,
) -> impl Fn(&KeyboardEvent, Modifiers) -> CommandExecuted + 'static {
    let keypress_map = KeypressMap::default();
    move |keypress, modifiers| {
        let command = keypress_map.keymaps.get(keypress).or_else(|| {
            let mode = editor.get_untracked().cursor.get_untracked().get_mode();
            if mode == Mode::Insert {
                let mut keypress = keypress.clone();
                keypress.modifiers.set(Modifiers::SHIFT, false);
                keypress_map.keymaps.get(&keypress)
            } else {
                None
            }
        });

        let Some(command) = command else {
            return CommandExecuted::No;
        };

        editor.with_untracked(|editor| {
            editor
                .doc()
                .run_command(editor, command, Some(1), modifiers)
        })
    }
}
