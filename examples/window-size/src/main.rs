use floem::{
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    kurbo::Size,
    prelude::{create_signal, SignalGet, SignalUpdate},
    ui_events::keyboard::{KeyState, KeyboardEvent},
    views::{label, v_stack, Decorators},
    window::WindowConfig,
    Application, IntoView, View,
};

fn app_view() -> impl IntoView {
    let (size, set_size) = create_signal(Size::default());

    let view = v_stack((label(move || format!("{}", size.get())).style(|s| s.font_size(30.0)),))
        .style(|s| {
            s.flex_col()
                .items_center()
                .justify_center()
                .width_full()
                .height_full()
                .row_gap(10.0)
        });

    let id = view.id();
    view.on_event_stop(EventListener::KeyUp, move |e| {
        if let Event::Key(KeyboardEvent {
            state: KeyState::Up,
            key,
            ..
        }) = e
        {
            if *key == Key::Named(NamedKey::F11) {
                id.inspect();
            }
        }
    })
    .on_resize(move |r| set_size.update(|value| *value = r.size()))
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
