use crate::form::{form, form_item};
use floem::{
    action::inspect,
    dropped_file::FileDragDropped,
    prelude::{palette::css, *},
    theme::HoverTargetClass,
    ui_events::keyboard::{Key, NamedKey},
};

pub fn dropped_file_view() -> impl IntoView {
    let filename = RwSignal::new("".to_string());

    let dropped_view = "dropped file(s)"
        .class(HoverTargetClass)
        .style(|s| {
            s.size(200.0, 50.0)
                .flex_col()
                .flex_shrink(1.)
                .hover(|s| s.color(css::PINK))
                .items_center()
                .justify_center()
        })
        .on_event_stop(el::KeyUp, |_, KeyboardEvent { key, .. }| {
            if *key == Key::Named(NamedKey::F11) {
                inspect();
            }
        })
        .on_event_stop(
            el::FileDragDrop,
            move |_cx, e @ FileDragDropped { paths, .. }| {
                println!("DroppedFile {e:?}");
                filename.set(format!(
                    "{:?}",
                    paths.first().expect("at least one to start a drag")
                ));
            },
        );

    form((
        form_item("Files:", Label::derived(move || filename.get())),
        form_item("", dropped_view),
    ))
}
