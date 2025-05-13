use crate::form::{form, form_item};
use floem::{
    dropped_file::FileDragEvent,
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    prelude::*,
};

pub fn dropped_file_view() -> impl IntoView {
    let filename = RwSignal::new("".to_string());

    let dropped_view = dyn_view(move || "dropped file".to_string())
        .style(|s| {
            s.size(200.0, 50.0)
                .background(palette::css::GRAY)
                .border(5.)
                .border_color(palette::css::BLACK)
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
            if let Event::FileDrag(e @ FileDragEvent::DragDropped { paths, .. }) = e {
                println!("DroppedFile {e:?}");
                filename.set(format!(
                    "{:?}",
                    paths.first().expect("at least one to start a drag")
                ));
            }
        });

    form((
        form_item("File:", label(move || filename.get())),
        form_item("", dropped_view),
    ))
}
