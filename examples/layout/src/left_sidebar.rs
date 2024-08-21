use floem::{
    event::EventListener,
    peniko::Color,
    reactive::{create_rw_signal, SignalGet},
    style::Position,
    views::{
        container, h_stack, label, scroll, v_stack, virtual_stack, Decorators, VirtualDirection,
        VirtualItemSize,
    },
    IntoView, View,
};

const SIDEBAR_WIDTH: f64 = 140.0;
const TOPBAR_HEIGHT: f64 = 30.0;
const SIDEBAR_ITEM_HEIGHT: f64 = 21.0;

pub fn left_sidebar_view() -> impl IntoView {
    let long_list: im::Vector<u8> = (0..100).collect();
    let long_list = create_rw_signal(long_list);

    let top_bar = label(|| String::from("Top bar"))
        .style(|s| s.padding(10.0).width_full().height(TOPBAR_HEIGHT));

    let side_bar = scroll({
        virtual_stack(
            VirtualDirection::Vertical,
            VirtualItemSize::Fixed(Box::new(|| SIDEBAR_ITEM_HEIGHT)),
            move || long_list.get(),
            move |item| *item,
            move |item| {
                label(move || item.to_string()).style(move |s| {
                    s.padding(10.0)
                        .padding_top(3.0)
                        .padding_bottom(3.0)
                        .width(SIDEBAR_WIDTH)
                        .height(SIDEBAR_ITEM_HEIGHT)
                        .items_start()
                        .border_bottom(1.0)
                        .border_color(Color::rgb8(205, 205, 205))
                })
            },
        )
        .style(|s| s.flex_col().width(SIDEBAR_WIDTH - 1.0))
    })
    .style(|s| {
        s.width(SIDEBAR_WIDTH)
            .border_right(1.0)
            .border_top(1.0)
            .border_color(Color::rgb8(205, 205, 205))
    });

    let main_window = scroll(
        container(label(move || String::from("Hello world")).style(|s| s.padding(10.0)))
            .style(|s| s.flex_col().items_start().padding_bottom(10.0)),
    )
    .style(|s| {
        s.flex_col()
            .flex_basis(0)
            .min_width(0)
            .flex_grow(1.0)
            .border_top(1.0)
            .border_color(Color::rgb8(205, 205, 205))
    });

    let content = h_stack((side_bar, main_window)).style(|s| {
        s.position(Position::Absolute)
            .inset_top(TOPBAR_HEIGHT)
            .inset_bottom(0.0)
            .width_full()
    });

    let view = v_stack((top_bar, content)).style(|s| s.width_full().height_full());

    let id = view.id();
    view.on_event_stop(EventListener::KeyUp, move |e| {
        if let floem::event::Event::KeyUp(e) = e {
            if e.key.logical_key == floem::keyboard::Key::Named(floem::keyboard::NamedKey::F11) {
                id.inspect();
            }
        }
    })
}
