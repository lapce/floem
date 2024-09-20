use floem::{
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    kurbo::Size,
    views::{button, label, v_stack, Decorators},
    window::{close_window, new_window, WindowConfig, WindowId},
    Application, IntoView, View,
};

fn sub_window_view(id: WindowId) -> impl IntoView {
    v_stack((
        label(move || String::from("Hello world")).style(|s| s.font_size(30.0)),
        button("Close this window").action(move || close_window(id)),
    ))
    .style(|s| {
        s.flex_col()
            .items_center()
            .justify_center()
            .width_full()
            .height_full()
            .column_gap(10.0)
    })
}

fn app_view() -> impl IntoView {
    let view = v_stack((
        label(move || String::from("Hello world")).style(|s| s.font_size(30.0)),
        button("Open another window").action(|| {
            new_window(
                sub_window_view,
                Some(
                    WindowConfig::default()
                        .size(Size::new(600.0, 150.0))
                        .title("Window Size Sub Example"),
                ),
            );
        }),
    ))
    .style(|s| {
        s.flex_col()
            .items_center()
            .justify_center()
            .width_full()
            .height_full()
            .column_gap(10.0)
    });

    let id = view.id();
    view.on_event_stop(EventListener::KeyUp, move |e| {
        if let Event::KeyUp(e) = e {
            if e.key.logical_key == Key::Named(NamedKey::F11) {
                id.inspect();
            }
        }
    })
}

fn main() {
    Application::new()
        .window(
            |_| app_view(),
            Some(
                WindowConfig::default()
                    .size(Size::new(800.0, 250.0))
                    .title("Window Size Example"),
            ),
        )
        .run();
}
