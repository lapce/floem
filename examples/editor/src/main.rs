use floem::{
    keyboard::{Key, NamedKey},
    reactive::{RwSignal, SignalGet, SignalUpdate},
    views::{
        button,
        editor::{
            command::{Command, CommandExecuted},
            core::{
                command::EditCommand, cursor::CursorAffinity, editor::EditType,
                selection::Selection,
            },
            text::{default_dark_color, SimpleStyling},
        },
        stack, text_editor, Decorators,
    },
    IntoView, View,
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

    let view = stack((
        editor_a,
        editor_b,
        stack((
            button("Clear").action(move || {
                doc.edit_single(
                    Selection::region(0, doc.text().len(), CursorAffinity::Backward),
                    "",
                    EditType::DeleteSelection,
                );
            }),
            button("Flip Gutter").action(move || {
                hide_gutter_a.update(|hide| *hide = !*hide);
                hide_gutter_b.update(|hide| *hide = !*hide);
            }),
        ))
        .style(|s| s.width_full().flex_row().items_center().justify_center()),
    ))
    .style(|s| s.size_full().flex_col().items_center().justify_center());

    let id = view.id();
    view.on_key_up(
        Key::Named(NamedKey::F11),
        |m| m.is_empty(),
        move |_| id.inspect(),
    )
}

fn main() {
    floem::launch(app_view)
}
