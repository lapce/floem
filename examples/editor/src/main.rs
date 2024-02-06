use floem::{
    keyboard::{Key, ModifiersState, NamedKey},
    unit::UnitExt,
    view::View,
    views::{
        editor::{
            command::{Command, CommandExecuted},
            core::command::EditCommand,
            text::SimpleStyling,
        },
        stack, text_editor, Decorators,
    },
};

fn app_view() -> impl View {
    let editor_a = text_editor("Hello World!").styling(SimpleStyling::dark());
    let editor_b = editor_a
        .shared_editor()
        .pre_command(|ev| {
            if matches!(ev.cmd, Command::Edit(EditCommand::Undo)) {
                println!("Undo command executed on editor B, ignoring!");
                return CommandExecuted::Yes;
            }
            CommandExecuted::No
        })
        .update(|_| {
            // This hooks up to both editors!
            println!("Editor changed");
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
