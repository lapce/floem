use floem::{
    action::set_window_scale,
    event::{Event, EventListener},
    prelude::*,
    reactive::Effect,
    style_class,
    unit::UnitExt,
};

style_class!(pub Button);

fn app_view() -> impl IntoView {
    let counter = RwSignal::new(0);
    let window_scale = RwSignal::new(1.0);
    Effect::new(move |_| {
        let window_scale = window_scale.get();
        set_window_scale(window_scale);
    });

    let value_label = label(move || format!("Value: {counter}")).style(|s| s.padding(10.0));

    let increment_button = "Increment".class(Button).action(move || {
        counter.update(|value| *value += 1);
    });
    let decrement_button = "Decrement"
        .class(Button)
        .action(move || {
            counter.update(|value| *value -= 1);
        })
        .style(|s| {
            s.margin_left(10.0)
                .hover(|s| s.background(Color::from_rgb8(244, 67, 54)))
                .active(|s| s.background(palette::css::RED))
        });
    let reset_to_zero_button = "Reset to 0"
        .class(Button)
        .action(move || {
            println!("Reset counter pressed"); // will not fire if button is disabled
            counter.update(|value| *value = 0);
        })
        .style(move |s| {
            s.margin_left(10.0)
                .background(palette::css::LIGHT_BLUE)
                .hover(|s| s.background(palette::css::LIGHT_YELLOW))
                .active(|s| s.background(palette::css::YELLOW_GREEN))
                .set_disabled(counter.get() == 0)
        });

    let counter_buttons = (increment_button, decrement_button, reset_to_zero_button).h_stack();

    let zoom_in_button = "Zoom In".class(Button).action(move || {
        window_scale.update(|scale| *scale *= 1.2);
    });
    let zoom_out_button = "Zoom Out".class(Button).action(move || {
        window_scale.update(|scale| *scale /= 1.2);
    });
    let zoom_reset_button = "Zoom Reset"
        .class(Button)
        .action(move || {
            window_scale.set(1.0);
        })
        .style(move |s| s.set_disabled(window_scale.get() == 1.0));

    let scale_buttons = (zoom_in_button, zoom_out_button, zoom_reset_button)
        .h_stack()
        .style(|s| {
            s.absolute()
                .inset_top(0)
                .inset_right(0)
                .gap(10)
                .padding_top(10)
                .padding_right(10)
        });

    (value_label, counter_buttons, scale_buttons)
        .v_stack()
        .style(|s| {
            s.size_full()
                .items_center()
                .justify_center()
                .class(Button, |s| {
                    s.border(1.0)
                        .border_radius(10.0)
                        .padding(10.0)
                        .focusable(true)
                        .focus_visible(|s| s.outline(2.).border_color(palette::css::BLUE))
                        .disabled(|s| s.background(palette::css::LIGHT_GRAY))
                        .hover(|s| s.background(palette::css::LIGHT_GREEN))
                        .active(|s| {
                            s.color(palette::css::WHITE)
                                .background(palette::css::DARK_GREEN)
                        })
                })
        })
        .on_event_stop(EventListener::KeyUp, move |v, e| {
            if let Event::Key(KeyboardEvent {
                state: KeyState::Up,
                key,
                ..
            }) = e
            {
                if *key == Key::Named(NamedKey::F11) {
                    v.id().inspect();
                }
            }
        })
}

fn main() {
    floem::launch(app_view);
}
