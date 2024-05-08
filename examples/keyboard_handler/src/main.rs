use floem::{
    event::{Event, EventListener},
    keyboard::Key,
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
    view.keyboard_navigatable()
        .on_event_stop(EventListener::KeyDown, move |e| {
            if let Event::KeyDown(e) = e {
                if e.key.logical_key == Key::Character("q".into()) {
                    println!("Goodbye :)");
                    std::process::exit(0)
                }
                println!("Key pressed in KeyCode: {:?}", e.key.physical_key);
            }
        })
}

fn main() {
    floem::launch(app_view);
}
