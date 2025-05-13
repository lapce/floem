use crate::form::{form, form_item};
use floem::{
    dropped_file::FileDragEvent,
    event::{Event, EventListener},
    prelude::*,
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
                .items_center()
                .justify_center()
        })
        .on_key_up(
            Key::Named(NamedKey::F11),
            |m| m.is_empty(),
            move |_| floem::action::inspect(),
        )
        .on_event_stop(EventListener::DroppedFiles, move |e| {
            if let Event::FileDrag(e @ FileDragEvent::DragDropped { paths, .. }) = e {
                println!("DroppedFile {e:?}");
                filename.set(format!(
                    "{:?}",
                    paths.first().expect("at least one to start a drag")
                ));
            }
        });

    form((
        form_item("Files:", label(move || filename.get())),
        form_item("", dropped_view),
    ))
}
