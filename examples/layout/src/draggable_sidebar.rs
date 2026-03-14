use floem::{
    peniko::Brush,
    prelude::*,
    style::{CustomStylable, CustomStyle},
    views::resizable::Resizable,
};

pub fn draggable_sidebar_view() -> impl IntoView {
    let side_bar = VirtualStack::with_view(
        || 0..100,
        move |item| {
            Label::derived(move || format!("Item {item} with long lines")).style(move |s| {
                s.text_ellipsis()
                    .height(22)
                    .padding(10.0)
                    .padding_top(3.0)
                    .padding_bottom(3.0)
                    .width_full()
                    .items_start()
                    .border_bottom(1.0)
                    .border_color(Color::from_rgb8(205, 205, 205))
            })
        },
    )
    .style(move |s| s.flex_col().width_full())
    .scroll()
    .style(move |s| {
        s.border_right(1.0)
            .border_top(1.0)
            .border_color(Color::from_rgb8(205, 205, 205))
            .flex_grow(1.)
    });

    let main_window = Scroll::new(
        Container::new(
            Label::derived(move || {
                String::from("<-- drag me!\n \n(double click to return to default)")
            })
            .style(|s| s.padding(10.0)),
        )
        .style(|s| s.flex_col().items_start().padding_bottom(10.0)),
    )
    .style(|s| {
        s.flex_col()
            .width_full()
            .border_top(1.0)
            .border_color(Color::from_rgb8(205, 205, 205))
    });

    let dragger_color = Color::from_rgb8(205, 205, 205);
    let active_dragger_color = Color::from_rgb8(41, 98, 218);

    Resizable::new((side_bar, main_window))
        .style(|s| s.width_full().height_full())
        .custom_style(move |s| {
            s.handle_color(Brush::Solid(dragger_color))
                .active(|s| s.handle_color(Brush::Solid(active_dragger_color)))
        })
        .on_event_stop(el::KeyUp, move |_cx, KeyboardEvent { key, .. }| {
            if let Key::Named(NamedKey::F11) = key {
                floem::action::inspect();
            }
        })
}
