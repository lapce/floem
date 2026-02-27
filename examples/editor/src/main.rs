use floem::{
    prelude::*,
    views::editor::{
        command::{Command, CommandExecuted},
        core::{
            command::EditCommand, cursor::CursorAffinity, editor::EditType, selection::Selection,
        },
        text::{default_dark_color, SimpleStyling},
    },
};

fn app_view() -> impl IntoView {
    let text = std::env::args()
        .nth(1)
        .map(|s| std::fs::read_to_string(s).unwrap());
    let text = text.as_deref().unwrap_or("Hello world");

    let hide_gutter_a = RwSignal::new(false);
    let hide_gutter_b = RwSignal::new(true);

    let editor_a = text_editor(text)
        .styling(SimpleStyling::new())
        .style(|s| s.size_full())
        .editor_style(default_dark_color)
        .editor_style(move |s| s.hide_gutter(hide_gutter_a.get()));
    let editor_b = editor_a
        .shared_editor()
        .editor_style(default_dark_color)
        .editor_style(move |s| s.hide_gutter(hide_gutter_b.get()))
        .style(|s| s.size_full())
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
        })
        .placeholder("Some placeholder text");
    let doc = editor_a.doc();

    Stack::new((
        editor_a,
        editor_b,
        Stack::new((
            Button::new("Clear").action(move || {
                doc.edit_single(
                    Selection::region(0, doc.text().len(), CursorAffinity::Backward),
                    "",
                    EditType::DeleteSelection,
                );
            }),
            Button::new("Flip Gutter").action(move || {
                hide_gutter_a.update(|hide| *hide = !*hide);
                hide_gutter_b.update(|hide| *hide = !*hide);
            }),
        ))
        .style(|s| s.width_full().flex_row().items_center().justify_center()),
    ))
    .style(|s| s.size_full().flex_col().items_center().justify_center())
    .on_event_stop(el::KeyUp, move |_cx, KeyboardEvent { key, .. }| {
        if let Key::Named(NamedKey::F11) = key {
            floem::action::inspect();
        }
    })
}

fn main() {
    floem::launch(app_view)
}
