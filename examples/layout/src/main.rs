use floem::{
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    kurbo::Size,
    style::AlignContent,
    view::View,
    views::{container, h_stack, label, v_stack, Decorators},
    widgets::button,
    window::{new_window, WindowConfig},
};

pub mod holy_grail;
pub mod left_sidebar;
pub mod right_sidebar;

fn list_item<V: View + 'static>(name: String, view_fn: impl Fn() -> V) -> impl View {
    h_stack((
        label(move || name.clone()).style(|s| s),
        container(view_fn()).style(|s| s.width_full().justify_content(AlignContent::End)),
    ))
    .style(|s| s.width(200))
}

fn app_view() -> impl View {
    let view = v_stack((
        label(move || String::from("Layouts")).style(|s| s.font_size(30.0).margin_bottom(15.0)),
        list_item(String::from("Left sidebar"), move || {
            button(|| "Open").on_click_stop(|_| {
                new_window(
                    |_| left_sidebar::left_sidebar_view(),
                    Some(
                        WindowConfig::default()
                            .size(Size::new(700.0, 400.0))
                            .title("Left sidebar"),
                    ),
                );
            })
        }),
        list_item(String::from("Right sidebar"), move || {
            button(|| "Open").on_click_stop(|_| {
                new_window(
                    |_| right_sidebar::right_sidebar_view(),
                    Some(
                        WindowConfig::default()
                            .size(Size::new(700.0, 400.0))
                            .title("Right sidebar"),
                    ),
                );
            })
        }),
        list_item(String::from("Holy grail"), move || {
            button(|| "Open").on_click_stop(|_| {
                new_window(
                    |_| holy_grail::holy_grail_view(),
                    Some(
                        WindowConfig::default()
                            .size(Size::new(700.0, 400.0))
                            .title("Holy Grail"),
                    ),
                );
            })
        }),
    ))
    .style(|s| {
        s.flex_col()
            .width_full()
            .height_full()
            .padding(10.0)
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
    .window_title(|| String::from("Layout examples"))
}

fn main() {
    floem::launch(app_view);
}
