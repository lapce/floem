use floem::{
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    peniko::Color,
    reactive::{create_rw_signal, SignalGet, SignalUpdate},
    views::{dyn_view, label, Decorators},
    IntoView, View,
};

use crate::form::{form, form_item};

pub fn dropped_file_view() -> impl IntoView {
    let filename = create_rw_signal("".to_string());

    form({
        (
            form_item("File:".to_string(), 80.0, move || {
                label(move || filename.get())
            }),
            form_item("".to_string(), 80.0, move || {
                let view = dyn_view(move || "dropped file".to_string()).style(|s| {
                    s.size(200.0, 50.0)
                        .background(Color::GRAY)
                        .border(5.)
                        .border_color(Color::BLACK)
                        .flex_col()
                        .items_center()
                        .justify_center()
                });
                let id = view.id();
                view.on_key_up(
                    Key::Named(NamedKey::F11),
                    |m| m.is_empty(),
                    move |_| id.inspect(),
                )
                .on_event_stop(EventListener::DroppedFile, move |e| {
                    if let Event::DroppedFile(e) = e {
                        println!("DroppedFile {:?}", e);
                        filename.set(format!("{:?}", e.path));
                    }
                })
            }),
        )
    })
}
