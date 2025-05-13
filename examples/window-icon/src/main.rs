use floem::{
    close_window,
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    kurbo::Size,
    new_window,
    ui_events::keyboard::{KeyState, KeyboardEvent},
    views::{button, label, v_stack, Decorators},
    window::{Icon, RgbaIcon, WindowConfig, WindowId},
    Application, IntoView, View,
};
use std::path::Path;

fn sub_window_view(id: WindowId) -> impl IntoView {
    v_stack((
        label(move || String::from("This window has an icon from an SVG file."))
            .style(|s| s.font_size(30.0)),
        button("Close this window").action(move || close_window(id)),
    ))
    .style(|s| {
        s.flex_col()
            .items_center()
            .justify_center()
            .width_full()
            .height_full()
            .row_gap(10.0)
    })
}

fn app_view() -> impl IntoView {
    let view = v_stack((
        label(move || String::from("This window has an icon from a PNG file"))
            .style(|s| s.font_size(30.0)),
        button("Open another window").action(|| {
            let svg_icon = load_svg_icon(include_str!("../assets/ferris.svg"));
            new_window(
                sub_window_view,
                Some(
                    WindowConfig::default()
                        .size(Size::new(600.0, 150.0))
                        .title("Window Icon Sub Example")
                        .window_icon(svg_icon),
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
}

fn main() {
    let png_icon_path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/ferris.png");
    let png_icon = load_png_icon(Path::new(png_icon_path));

    Application::new()
        .window(
            |_| app_view(),
            Some(
                WindowConfig::default()
                    .size(Size::new(800.0, 250.0))
                    .title("Window Icon Example")
                    .window_icon(png_icon),
            ),
        )
        .run();
}

fn load_png_icon(path: &Path) -> Icon {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::open(path)
            .expect("Failed to open icon path")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    RgbaIcon::new(icon_rgba, icon_width, icon_height)
        .expect("Failed to open icon")
        .into()
}

fn load_svg_icon(svg: &str) -> Icon {
    let svg = nsvg::parse_str(svg, nsvg::Units::Pixel, 96.0).unwrap();
    let (icon_width, icon_height, icon_rgba) = svg.rasterize_to_raw_rgba(1.0).unwrap();
    RgbaIcon::new(icon_rgba, icon_width, icon_height)
        .expect("Failed to open icon")
        .into()
}
