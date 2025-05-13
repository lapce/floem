use floem::{
    event::EventListener,
    prelude::*,
    style::{CustomStylable, CustomStyle},
    ui_events::keyboard::{KeyState, KeyboardEvent},
};

pub fn draggable_sidebar_view() -> impl IntoView {
    let side_bar = VirtualStack::with_view(
        || 0..100,
        move |item| {
            label(move || format!("Item {item} with long lines")).style(move |s| {
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
    });

    let main_window = scroll(
        container(
            label(move || String::from("<-- drag me!\n \n(double click to return to default)"))
                .style(|s| s.padding(10.0)),
        )
        .style(|s| s.flex_col().items_start().padding_bottom(10.0)),
    )
    .style(|s| {
        s.flex_col()
            .flex_basis(0)
            .min_width(0)
            .flex_grow(1.0)
            .border_top(1.0)
            .border_color(Color::from_rgb8(205, 205, 205))
    });

    let dragger_color = Color::from_rgb8(205, 205, 205);
    let active_dragger_color = Color::from_rgb8(41, 98, 218);

    let view = resizable::resizable((side_bar, main_window))
        .style(|s| s.width_full().height_full())
        .custom_style(move |s| {
            s.handle_color(dragger_color)
                .active(|s| s.handle_color(active_dragger_color))
        });

    let id = view.id();
    view.on_event_stop(EventListener::KeyUp, move |e| {
        if let floem::event::Event::Key(KeyboardEvent {
            state: KeyState::Up,
            key,
            ..
        }) = e
        {
            if *key == floem::keyboard::Key::Named(floem::keyboard::NamedKey::F11) {
                id.inspect();
            }
        }
    })
}
