pub mod key;
pub mod press;

use std::{collections::HashMap, str::FromStr};

use crate::reactive::RwSignal;
use floem_editor_core::{
    command::{EditCommand, MoveCommand, MultiSelectionCommand, ScrollCommand},
    mode::Mode,
};
use floem_reactive::{SignalGet, SignalWith};
use ui_events::keyboard::{Key, Modifiers};

use super::{
    Editor,
    command::{Command, CommandExecuted},
};

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct KeypressKey {
    pub key: Key,
    pub modifiers: Modifiers,
}

/// The default keymap handler does not have modal-mode specific
/// keybindings.
#[derive(Clone)]
pub struct KeypressMap {
    pub keymaps: HashMap<KeypressKey, Command>,
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

fn key(s: &str, m: Modifiers) -> KeypressKey {
    KeypressKey {
        key: Key::from_str(s).unwrap(),
        modifiers: m,
    }
}

fn key_d(s: &str) -> KeypressKey {
    key(s, Modifiers::default())
}

fn add_default_common(c: &mut HashMap<KeypressKey, Command>) {
    // Note: this should typically be kept in sync with Lapce's
    // `defaults/keymaps-common.toml`

    // --- Basic editing ---

    c.insert(
        key("ArrowUp", Modifiers::ALT),
        Command::Edit(EditCommand::MoveLineUp),
    );
    c.insert(
        key("ArrowDown", Modifiers::ALT),
        Command::Edit(EditCommand::MoveLineDown),
    );

    c.insert(key_d("Delete"), Command::Edit(EditCommand::DeleteForward));
    c.insert(
        key_d("Backspace"),
        Command::Edit(EditCommand::DeleteBackward),
    );
    c.insert(
        key("Backspace", Modifiers::SHIFT),
        Command::Edit(EditCommand::DeleteForward),
    );

    c.insert(key_d("Home"), Command::Move(MoveCommand::LineStartNonBlank));
    c.insert(key_d("End"), Command::Move(MoveCommand::LineEnd));

    c.insert(key_d("PageUp"), Command::Scroll(ScrollCommand::PageUp));
    c.insert(key_d("PageDown"), Command::Scroll(ScrollCommand::PageDown));
    c.insert(
        key("PageUp", Modifiers::CONTROL),
        Command::Scroll(ScrollCommand::ScrollUp),
    );
    c.insert(
        key("PageDown", Modifiers::CONTROL),
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
    c.insert(key_d("ArrowRight"), Command::Move(MoveCommand::Right));
    c.insert(key_d("ArrowLeft"), Command::Move(MoveCommand::Left));
    c.insert(key_d("ArrowUp"), Command::Move(MoveCommand::Up));
    c.insert(key_d("ArrowDown"), Command::Move(MoveCommand::Down));

    c.insert(key_d("Enter"), Command::Edit(EditCommand::InsertNewLine));

    c.insert(key_d("Tab"), Command::Edit(EditCommand::InsertTab));

    c.insert(
        key("ArrowUp", Modifiers::ALT | Modifiers::SHIFT),
        Command::Edit(EditCommand::DuplicateLineUp),
    );
    c.insert(
        key("ArrowDown", Modifiers::ALT | Modifiers::SHIFT),
        Command::Edit(EditCommand::DuplicateLineDown),
    );
}

fn add_default_windows(c: &mut HashMap<KeypressKey, Command>) {
    add_default_nonmacos(c);
}

fn add_default_macos(c: &mut HashMap<KeypressKey, Command>) {
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
        key("ArrowRight", Modifiers::ALT),
        Command::Move(MoveCommand::WordEndForward),
    );
    c.insert(
        key("ArrowLeft", Modifiers::ALT),
        Command::Move(MoveCommand::WordBackward),
    );
    c.insert(
        key("ArrowLeft", Modifiers::META),
        Command::Move(MoveCommand::LineStartNonBlank),
    );
    c.insert(
        key("ArrowRight", Modifiers::META),
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
        key("Backspace", Modifiers::ALT),
        Command::Edit(EditCommand::DeleteWordBackward),
    );
    c.insert(
        key("Backspace", Modifiers::META),
        Command::Edit(EditCommand::DeleteToBeginningOfLine),
    );
    c.insert(
        key("k", Modifiers::CONTROL),
        Command::Edit(EditCommand::DeleteToEndOfLine),
    );
    c.insert(
        key("Delete", Modifiers::ALT),
        Command::Edit(EditCommand::DeleteWordForward),
    );

    // TODO: match pairs?
    // TODO: indent/outdent line?

    c.insert(
        key("a", Modifiers::META),
        Command::MultiSelection(MultiSelectionCommand::SelectAll),
    );

    c.insert(
        key("Enter", Modifiers::META),
        Command::Edit(EditCommand::NewLineBelow),
    );
    c.insert(
        key("Enter", Modifiers::META | Modifiers::SHIFT),
        Command::Edit(EditCommand::NewLineAbove),
    );

    // --- Multi cursor ---
    c.insert(
        key("ArrowUp", Modifiers::ALT | Modifiers::META),
        Command::MultiSelection(MultiSelectionCommand::InsertCursorAbove),
    );
    c.insert(
        key("ArrowDown", Modifiers::ALT | Modifiers::META),
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
        key("ArrowUp", Modifiers::META),
        Command::Move(MoveCommand::DocumentStart),
    );
    c.insert(
        key("ArrowDown", Modifiers::META),
        Command::Move(MoveCommand::DocumentEnd),
    );
}

fn add_default_linux(c: &mut HashMap<KeypressKey, Command>) {
    add_default_nonmacos(c);
}

fn add_default_nonmacos(c: &mut HashMap<KeypressKey, Command>) {
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
        key("Delete", Modifiers::SHIFT),
        Command::Edit(EditCommand::ClipboardCut),
    );
    c.insert(
        key("c", Modifiers::CONTROL),
        Command::Edit(EditCommand::ClipboardCopy),
    );
    c.insert(
        key("Insert", Modifiers::CONTROL),
        Command::Edit(EditCommand::ClipboardCopy),
    );
    c.insert(
        key("v", Modifiers::CONTROL),
        Command::Edit(EditCommand::ClipboardPaste),
    );
    c.insert(
        key("Insert", Modifiers::SHIFT),
        Command::Edit(EditCommand::ClipboardPaste),
    );

    c.insert(
        key("ArrowRight", Modifiers::CONTROL),
        Command::Move(MoveCommand::WordEndForward),
    );
    c.insert(
        key("ArrowLeft", Modifiers::CONTROL),
        Command::Move(MoveCommand::WordBackward),
    );

    c.insert(
        key("Backspace", Modifiers::CONTROL),
        Command::Edit(EditCommand::DeleteWordBackward),
    );
    c.insert(
        key("Delete", Modifiers::CONTROL),
        Command::Edit(EditCommand::DeleteWordForward),
    );

    // TODO: match pairs?

    // TODO: indent/outdent line?

    c.insert(
        key("a", Modifiers::CONTROL),
        Command::MultiSelection(MultiSelectionCommand::SelectAll),
    );

    c.insert(
        key("Enter", Modifiers::CONTROL),
        Command::Edit(EditCommand::NewLineAbove),
    );

    // --- Multi cursor ---
    c.insert(
        key("ArrowUp", Modifiers::CONTROL | Modifiers::ALT),
        Command::MultiSelection(MultiSelectionCommand::InsertCursorAbove),
    );
    c.insert(
        key("ArrowDown", Modifiers::CONTROL | Modifiers::ALT),
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
        key("Home", Modifiers::CONTROL),
        Command::Move(MoveCommand::DocumentStart),
    );
    c.insert(
        key("End", Modifiers::CONTROL),
        Command::Move(MoveCommand::DocumentEnd),
    );
}

pub fn default_key_handler(
    editor: RwSignal<Editor>,
) -> impl Fn(KeypressKey) -> CommandExecuted + 'static {
    let keypress_map = KeypressMap::default();
    move |keypress| {
        let command = keypress_map.keymaps.get(&keypress).or_else(|| {
            let mode = editor.get_untracked().cursor.get_untracked().get_mode();
            if mode == Mode::Insert {
                let mut modified_modifiers = keypress.modifiers;
                modified_modifiers.set(Modifiers::SHIFT, false);
                keypress_map.keymaps.get(&KeypressKey {
                    key: keypress.key,
                    modifiers: modified_modifiers,
                })
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
                .run_command(editor, command, Some(1), keypress.modifiers)
        })
    }
}
