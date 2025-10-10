use floem::{
    action::set_window_scale,
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    peniko::{color::palette, Color},
    prelude::ViewTuple,
    reactive::{create_effect, create_rw_signal, create_signal, SignalGet, SignalUpdate},
    style_class,
    ui_events::keyboard::{KeyState, KeyboardEvent},
    unit::UnitExt,
    views::{label, Decorators},
    IntoView, View,
};

style_class!(pub Button);

fn app_view() -> impl IntoView {
    let (counter, set_counter) = create_signal(0);
    let window_scale = create_rw_signal(1.0);
    create_effect(move |_| {
        let window_scale = window_scale.get();
        set_window_scale(window_scale);
    });

    let value_label = label(move || format!("Value: {}", counter.get())).style(|s| s.padding(10.0));

    let increment_button = "Increment"
        .class(Button)
        .on_click_stop(move |_| {
            set_counter.update(|value| *value += 1);
        })
        .keyboard_navigable();
    let decrement_button = "Decrement"
        .class(Button)
        .on_click_stop(move |_| {
            set_counter.update(|value| *value -= 1);
        })
        .style(|s| {
            s.margin_left(10.0)
                .hover(|s| s.background(Color::from_rgb8(244, 67, 54)))
                .active(|s| s.background(palette::css::RED))
        })
        .keyboard_navigable();
    let reset_to_zero_button = "Reset to 0"
        .class(Button)
        .on_click_stop(move |_| {
            println!("Reset counter pressed"); // will not fire if button is disabled
            set_counter.update(|value| *value = 0);
        })
        .disabled(move || counter.get() == 0)
        .style(|s| {
            s.margin_left(10.0)
                .background(palette::css::LIGHT_BLUE)
                .hover(|s| s.background(palette::css::LIGHT_YELLOW))
                .active(|s| s.background(palette::css::YELLOW_GREEN))
        })
        .keyboard_navigable();

    let counter_buttons = (increment_button, decrement_button, reset_to_zero_button).h_stack();

    let zoom_in_button = "Zoom In"
        .class(Button)
        .on_click_stop(move |_| {
            window_scale.update(|scale| *scale *= 1.2);
        })
        .style(|s| s.margin_top(10.0).margin_right(10.0));
    let zoom_out_button = "Zoom Out"
        .class(Button)
        .on_click_stop(move |_| {
            window_scale.update(|scale| *scale /= 1.2);
        })
        .style(|s| s.margin_top(10.0).margin_right(10.0));
    let zoom_reset_button = "Zoom Reset"
        .class(Button)
        .disabled(move || window_scale.get() == 1.0)
        .on_click_stop(move |_| {
            window_scale.set(1.0);
        })
        .style(|s| s.margin_top(10.0).margin_right(10.0));

    let scale_buttons = (zoom_in_button, zoom_out_button, zoom_reset_button)
        .h_stack()
        .style(|s| s.absolute().inset_top(0).inset_right(0));

    let view = (value_label, counter_buttons, scale_buttons)
        .v_stack()
        .style(|s| {
            s.size(100.pct(), 100.pct())
                .items_center()
                .justify_center()
                .class(Button, |s| {
                    s.border(1.0)
                        .border_radius(10.0)
                        .padding(10.0)
                        .focus_visible(|s| s.border(2.).border_color(palette::css::BLUE))
                        .disabled(|s| s.background(palette::css::LIGHT_GRAY))
                        .hover(|s| s.background(palette::css::LIGHT_GREEN))
                        .active(|s| {
                            s.color(palette::css::WHITE)
                                .background(palette::css::DARK_GREEN)
                        })
                })
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
    floem::launch(app_view);
}
