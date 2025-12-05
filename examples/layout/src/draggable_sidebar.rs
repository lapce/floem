use floem::{
    action::inspect,
    event::{Event, EventListener},
    prelude::{palette::css, *},
    style::{CustomStylable, CustomStyle},
};

pub fn draggable_sidebar_view() -> impl IntoView {
    let side_bar = VirtualStack::with_view(
        || 0..100,
        move |item| {
            text(format!("Item {item} with long lines")).style(move |s| {
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
            .size_full()
    });

    let main_window = "<-- drag me!\n \n(double click to return to default)"
        .style(|s| s.padding(10.0).width_full());

    let dragger_color = Color::from_rgb8(205, 205, 205);
    let active_dragger_color = Color::from_rgb8(41, 98, 218);

    resizable::resizable((
        side_bar,
        main_window,
        "this is a test".style(|s| {
            s.width_full()
                .padding_left(15)
                .border(1.)
                .border_color(css::WHITE)
        }),
        "this is another test \n with a new line \n and anotehr"
            .style(|s| s.width_full().border(1.).border_color(css::WHITE)),
    ))
    .style(|s| s.size_full().justify_between())
    .custom_style(move |s| {
        s.handle_color(dragger_color)
            .active(|s| s.handle_color(active_dragger_color))
    })
    .on_event_stop(EventListener::KeyUp, move |_, cx| {
        if let Event::Key(KeyboardEvent {
            key: Key::Named(NamedKey::F11),
            ..
        }) = &cx.event
        {
            inspect();
        }
    })
}
