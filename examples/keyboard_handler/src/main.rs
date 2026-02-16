use floem::{
    prelude::*,
    unit::UnitExt,
    views::{ContainerExt, Decorators},
    IntoView,
};

fn app_view() -> impl IntoView {
    "Example: Keyboard event handler"
        .style(|s| s.padding(10.0))
        .container()
        .style(|s| {
            s.size(100.pct(), 100.pct())
                .flex_col()
                .items_center()
                .justify_center()
                .keyboard_navigable()
        })
        .on_event_stop(
            listener::KeyDown,
            move |_cx, KeyboardEvent { code, key, .. }| {
                if *key == Key::Character("q".into()) {
                    println!("Goodbye :)");
                    std::process::exit(0)
                }
                println!("Key pressed in KeyCode: {:?}", code);
            },
        )
}

fn main() {
    floem::launch(app_view);
}
