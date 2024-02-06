use floem::{
    keyboard::{Key, ModifiersState, NamedKey},
    unit::UnitExt,
    view::View,
    views::{
        editor::{
            command::{Command, CommandExecuted},
            core::command::EditCommand,
            text::SimpleStyling,
            text_editor::text_editor,
        },
        stack, Decorators,
    },
};

fn app_view() -> impl View {
    let editor_a = text_editor("Hello World!").styling(SimpleStyling::dark());
    let editor_b = editor_a.shared_editor().pre_command(|_editor, cmd, _, _| {
        if matches!(cmd, Command::Edit(EditCommand::Undo)) {
            println!("Undo command executed on editor B, ignoring!");
            return CommandExecuted::Yes;
        }
        CommandExecuted::No
    });

    let view = stack((editor_a, editor_b)).style(|s| {
        s.size(100.pct(), 100.pct())
            .flex_col()
            .items_center()
            .justify_center()
    });

    let id = view.id();
    view.on_key_up(
        Key::Named(NamedKey::F11),
        ModifiersState::empty(),
        move |_| id.inspect(),
    )
}

fn main() {
    floem::launch(app_view)
}
