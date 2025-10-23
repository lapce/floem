use crate::form::{form, form_item};
use floem::{
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    prelude::*,
    theme::HoverTargetClass,
};

pub fn dropped_file_view() -> impl IntoView {
    let filename = RwSignal::new("".to_string());

    let dropped_view = "dropped file(s)"
        .class(HoverTargetClass)
        .style(|s| {
            s.size(200.0, 50.0)
                .flex_col()
                .items_center()
                .justify_center()
        })
        .on_key_up(
            Key::Named(NamedKey::F11),
            |m| m.is_empty(),
            move |_| floem::action::inspect(),
        )
        .on_event_stop(EventListener::DroppedFile, move |e| {
            if let Event::DroppedFiles(e) = e {
                println!("DroppedFile(s) {e:?}");
                filename.set(format!("{:?}", e.path));
            }
        });

    form((
        form_item("Files:", label(move || filename.get())),
        form_item("", dropped_view),
    ))
}
