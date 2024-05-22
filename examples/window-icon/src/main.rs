use floem::{
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    kurbo::Size,
    views::Decorators,
    window::{Icon, WindowConfig},
    Application, IntoView, View,
};
use std::path::Path;

fn app_view() -> impl IntoView {
    let view = "Show Window Icon".style(|s| s.font_size(30.0)).style(|s| {
        s.flex_col()
            .items_center()
            .justify_center()
            .width_full()
            .height_full()
            .gap(0.0, 10.0)
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
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "./assets/icon.png");
    let icon = load_icon(Path::new(path));

    Application::new()
        .window(
            |_| app_view(),
            Some(
                WindowConfig::default()
                    .size(Size::new(800.0, 250.0))
                    .title("Window Size Example")
                    .set_window_icon(icon),
            ),
        )
        .run();
}

fn load_icon(path: &Path) -> Icon {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::open(path)
            .expect("Failed to open icon path")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    Icon::from_rgba(icon_rgba, icon_width, icon_height).expect("Failed to open icon")
}
