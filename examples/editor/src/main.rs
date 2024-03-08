use floem::{
    keyboard::{Key, ModifiersState, NamedKey},
    view::View,
    views::{
        editor::{
            command::{Command, CommandExecuted},
            core::{command::EditCommand, editor::EditType, selection::Selection},
            text::SimpleStyling,
        },
        stack, text_editor, Decorators,
    },
    widgets::button,
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
        .gutter(false)
        .update(|_| {
            // This hooks up to both editors!
            println!("Editor changed");
        })
        .placeholder("Some placeholder text");
    let doc = editor_a.doc();
    let gutter_a = editor_a.editor().gutter;
    let gutter_b = editor_b.editor().gutter;

    let view = stack((
        editor_a,
        editor_b,
        stack((
            button(|| "Clear").on_click_stop(move |_| {
                doc.edit_single(
                    Selection::region(0, doc.text().len()),
                    "",
                    EditType::DeleteSelection,
                );
            }),
            button(|| "Flip Gutter").on_click_stop(move |_| {
                let a = !gutter_a.get_untracked();
                let b = !gutter_b.get_untracked();
                gutter_a.set(a);
                gutter_b.set(b);
            }),
        ))
        .style(|s| s.width_full().flex_row().items_center().justify_center()),
    ))
    .style(|s| s.size_full().flex_col().items_center().justify_center());

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
