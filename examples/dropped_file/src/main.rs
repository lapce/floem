use floem::{
    event::EventListener,
    keyboard::{Key, NamedKey},
    unit::UnitExt,
    views::{dyn_view, Decorators},
    IntoView, View,
};

fn app_view() -> impl IntoView {
    let view = dyn_view(move || "dropped file".to_string()).style(|s| {
        s.size(100.pct(), 100.pct())
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
    .on_event_stop(EventListener::PointerMove, |x| {
        println!("PointerMove {:?}", x.point());
    })
    .on_event_stop(EventListener::DroppedFile, |x| {
        println!("DroppedFile {:?}", x);
    })
}

fn main() {
    floem::launch(app_view);
}
