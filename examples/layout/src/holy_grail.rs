use floem::{
    event::EventListener,
    prelude::*,
    taffy::Position,
    ui_events::keyboard::{KeyState, KeyboardEvent},
};

const SIDEBAR_WIDTH: f64 = 140.0;
const TOPBAR_HEIGHT: f64 = 30.0;
const SIDEBAR_ITEM_HEIGHT: f64 = 21.0;

pub fn holy_grail_view() -> impl IntoView {
    let top_bar = label(|| String::from("Top bar"))
        .style(|s| s.padding(10.0).width_full().height(TOPBAR_HEIGHT));

    let side_bar_right = VirtualStack::with_view(
        || 0..100,
        |item| {
            label(move || item.to_string()).style(move |s| {
                s.padding(10.0)
                    .padding_top(3.0)
                    .padding_bottom(3.0)
                    .width(SIDEBAR_WIDTH)
                    .height(SIDEBAR_ITEM_HEIGHT)
                    .items_start()
                    .border_bottom(1.0)
                    .border_color(Color::from_rgb8(205, 205, 205))
            })
        },
    )
    .style(|s| s.flex_col().width(SIDEBAR_WIDTH - 1.0))
    .scroll()
    .style(|s| {
        s.width(SIDEBAR_WIDTH)
            .border_left(1.0)
            .border_top(1.0)
            .border_color(Color::from_rgb8(205, 205, 205))
    });

    let side_bar_left = VirtualStack::with_view(
        || 0..100,
        move |item| {
            label(move || item.to_string()).style(move |s| {
                s.padding(10.0)
                    .padding_top(3.0)
                    .padding_bottom(3.0)
                    .width(SIDEBAR_WIDTH)
                    .height(SIDEBAR_ITEM_HEIGHT)
                    .items_start()
                    .border_bottom(1.0)
                    .border_color(Color::from_rgb8(205, 205, 205))
            })
        },
    )
    .style(|s| s.flex_col().width(SIDEBAR_WIDTH - 1.0))
    .scroll()
    .style(|s| {
        s.width(SIDEBAR_WIDTH)
            .border_right(1.0)
            .border_top(1.0)
            .border_color(Color::from_rgb8(205, 205, 205))
    });

    let main_window = "Hello world"
        .style(|s| s.padding(10.0))
        .container()
        .style(|s| s.flex_col().items_start().padding_bottom(10.0))
        .scroll()
        .style(|s| s.flex_col().flex_basis(0).min_width(0).flex_grow(1.0))
        .style(|s| {
            s.border_top(1.0)
                .border_color(Color::from_rgb8(205, 205, 205))
                .width_full()
                .min_width(150.0)
        });

    let content = (side_bar_left, main_window, side_bar_right)
        .h_stack()
        .style(|s| {
            s.position(Position::Absolute)
                .inset_top(TOPBAR_HEIGHT)
                .inset_bottom(0.0)
                .width_full()
        });

    (top_bar, content)
        .v_stack()
        .style(|s| s.width_full().height_full())
        .on_event_stop(EventListener::KeyUp, move |e| {
            if let floem::event::Event::Key(KeyboardEvent {
                state: KeyState::Up,
                key,
                ..
            }) = e
            {
                if *key == floem::keyboard::Key::Named(floem::keyboard::NamedKey::F11) {
                    floem::action::inspect();
                }
            }
        })
}
