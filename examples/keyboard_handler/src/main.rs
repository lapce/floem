use floem::{
    event::{Event, EventListener},
    keyboard::Key,
    ui_events::keyboard::{KeyState, KeyboardEvent},
    unit::UnitExt,
    views::{stack, text, Decorators},
    IntoView,
};

fn app_view() -> impl IntoView {
    let view =
        stack((text("Example: Keyboard event handler").style(|s| s.padding(10.0)),)).style(|s| {
            s.size(100.pct(), 100.pct())
                .flex_col()
                .items_center()
                .justify_center()
        });
    view.keyboard_navigable()
        .on_event_stop(EventListener::KeyDown, move |e| {
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
