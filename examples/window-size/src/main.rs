use floem::{
    action::inspect,
    event::{Event, EventListener},
    kurbo::Size,
    prelude::*,
    window::WindowConfig,
    Application,
};

fn app_view() -> impl IntoView {
    let size = RwSignal::new(Size::default());

    label(move || format!("{}", size.get()))
        .style(|s| s.font_size(30.0))
        .container()
        .style(|s| {
            s.flex_col()
                .items_center()
                .justify_center()
                .width_full()
                .height_full()
                .row_gap(10.0)
        })
        .on_event_stop(EventListener::KeyUp, move |_, e| {
            if let Event::Key(KeyboardEvent {
                state: KeyState::Up,
                key,
                ..
            }) = e
            {
                if *key == Key::Named(NamedKey::F11) {
                    inspect();
                }
            }
        })
        .on_resize(move |r| size.set(r.size()))
}

fn main() {
    let app = Application::new().window(
        |_| app_view(),
        Some(
            WindowConfig::default()
                .size(Size::new(800.0, 600.0))
                .min_size(Size::new(400.0, 300.0))
                .max_size(Size::new(1200.0, 900.0))
                .title("Window Size Example"),
        ),
    );
    app.run();
}
