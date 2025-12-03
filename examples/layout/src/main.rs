use std::fmt::Display;

use floem::{
    kurbo::Size,
    prelude::*,
    style::AlignContent,
    window::{new_window, WindowConfig},
};

pub mod draggable_sidebar;
pub mod holy_grail;
pub mod left_sidebar;
pub mod right_sidebar;
pub mod tab_navigation;

fn list_item<V: IntoView + 'static>(name: impl Display, view_fn: impl Fn() -> V) -> impl IntoView {
    h_stack((
        text(name).style(|s| s),
        container(view_fn()).style(|s| s.width_full().justify_content(AlignContent::End)),
    ))
    .style(|s| s.width(200))
}

fn app_view() -> impl IntoView {
    v_stack((
        label(move || String::from("Static layouts"))
            .style(|s| s.font_size(30.0).margin_bottom(15.0)),
        list_item("Left sidebar", move || {
            "Open".button().action(|| {
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
        list_item("Right sidebar", move || {
            "Open".button().action(|| {
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
        list_item("Holy grail", move || {
            "Open".button().action(|| {
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
        "Interactive layouts".style(|s| s.font_size(30.0).margin_top(15.0).margin_bottom(15.0)),
        list_item("Tab navigation", move || {
            "Open".button().action(|| {
                new_window(
                    |_| tab_navigation::tab_navigation_view(),
                    Some(
                        WindowConfig::default()
                            .size(Size::new(400.0, 250.0))
                            .title("Tab navigation"),
                    ),
                );
            })
        }),
        list_item("Draggable sidebar", move || {
            "Open".button().action(|| {
                new_window(
                    |_| draggable_sidebar::draggable_sidebar_view(),
                    Some(
                        WindowConfig::default()
                            .size(Size::new(700.0, 400.0))
                            .title("Draggable sidebar"),
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
            .row_gap(10.0)
    })
    .on_key_up(
        Key::Named(NamedKey::F11),
        |_| true,
        move |_, _| {
            floem::action::inspect();
        },
    )
    .window_title(|| String::from("Layout examples"))
}

fn main() {
    floem::launch(app_view);
}
