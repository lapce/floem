// examples/custom_keymap/src/main.rs
//
// Demonstrates extending the default editor keymap with custom shortcuts.
//
// Ctrl+S prints "Save!" to the console.
// Ctrl+D duplicates the current line.
// All other keys are handled by the default keymap.

use floem::{
    prelude::{Key, Modifiers},
    views::{
        editor::{
            command::CommandExecuted,
            keypress::{KeypressKey, KeypressMap},
        },
        text_editor::text_editor_keys,
        Decorators,
    },
    IntoView,
};

use floem::views::editor::core::command::EditCommand;

fn app_view() -> impl IntoView {
    let mut keymap = KeypressMap::default();

    // Custom bindings can be inserted directly into the map.
    // This rebinds Ctrl+D to duplicate the current line down.
    keymap.keymaps.insert(
        KeypressKey {
            key: Key::Character("d".into()),
            modifiers: Modifiers::CONTROL,
        },
        floem::views::editor::command::Command::Edit(EditCommand::DuplicateLineDown),
    );

    let editor = text_editor_keys(
        "Try Ctrl+S to save, Ctrl+D to duplicate a line.\n\nAll other shortcuts work as usual.",
        move |editor_sig, keypress| {
            // Shortcuts that need custom logic go here.
            if keypress.modifiers == Modifiers::CONTROL
                && keypress.key == Key::Character("s".into())
            {
                println!("Save!");
                return CommandExecuted::Yes;
            }

            // Everything else: default keymap (plus our Ctrl+D addition above).
            keymap.handle_keypress(editor_sig, keypress)
        },
    )
    .style(|s| s.size_full());

    editor
}

fn main() {
    floem::launch(app_view);
}
