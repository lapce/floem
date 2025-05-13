use floem::{
    event::{Event, EventListener},
    prelude::*,
    unit::UnitExt,
    views::{ContainerExt, Decorators},
    IntoView,
};

fn app_view() -> impl IntoView {
    let view = "Example: Keyboard event handler"
        .style(|s| s.padding(10.0))
        .container()
        .style(|s| {
            s.size(100.pct(), 100.pct())
                .flex_col()
                .items_center()
                .justify_center()
                .focusable(true) // Focusable is needed for a view to be able to receive keyboard events
        });
    view.on_event_stop(EventListener::KeyDown, move |e| {
        if let Event::Key(KeyboardEvent {
            state: KeyState::Down,
            code,
            key,
            ..
        }) = e
        {
            if *key == Key::Character("q".into()) {
                println!("Goodbye :)");
                std::process::exit(0)
            }
            println!("Key pressed in KeyCode: {:?}", code);
        }
    })
}

fn main() {
    floem::launch(app_view);
}
