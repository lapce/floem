use floem::{
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    peniko::Color,
    reactive::{create_rw_signal, create_signal, SignalGet, SignalUpdate},
    style_class,
    unit::UnitExt,
    views::{label, stack, Decorators},
    IntoView, View,
};

style_class!(pub Button);

fn app_view() -> impl IntoView {
    let (counter, set_counter) = create_signal(0);
    let window_scale = create_rw_signal(1.0);
    let view = stack((
        label(move || format!("Value: {}", counter.get())).style(|s| s.padding(10.0)),
        stack({
            (
                label(|| "Increment")
                    .class(Button)
                    .on_click_stop(move |_| {
                        set_counter.update(|value| *value += 1);
                    })
                    .keyboard_navigable(),
                label(|| "Decrement")
                    .class(Button)
                    .on_click_stop(move |_| {
                        set_counter.update(|value| *value -= 1);
                    })
                    .style(|s| {
                        s.margin_left(10.0)
                            .hover(|s| s.background(Color::rgb8(244, 67, 54)))
                            .active(|s| s.background(Color::RED))
                    })
                    .keyboard_navigable(),
                label(|| "Reset to 0")
                    .class(Button)
                    .on_click_stop(move |_| {
                        println!("Reset counter pressed"); // will not fire if button is disabled
                        set_counter.update(|value| *value = 0);
                    })
                    .disabled(move || counter.get() == 0)
                    .style(|s| {
                        s.margin_left(10.0)
                            .background(Color::LIGHT_BLUE)
                            .hover(|s| s.background(Color::LIGHT_YELLOW))
                            .active(|s| s.background(Color::YELLOW_GREEN))
                    })
                    .keyboard_navigable(),
            )
        }),
        stack({
            (
                label(|| "Zoom In")
                    .class(Button)
                    .on_click_stop(move |_| {
                        window_scale.update(|scale| *scale *= 1.2);
                    })
                    .style(|s| s.margin_top(10.0).margin_right(10.0)),
                label(|| "Zoom Out")
                    .class(Button)
                    .on_click_stop(move |_| {
                        window_scale.update(|scale| *scale /= 1.2);
                    })
                    .style(|s| s.margin_top(10.0).margin_right(10.0)),
                label(|| "Zoom Reset")
                    .class(Button)
                    .disabled(move || window_scale.get() == 1.0)
                    .on_click_stop(move |_| {
                        window_scale.set(1.0);
                    })
                    .style(|s| s.margin_top(10.0).margin_right(10.0)),
            )
        })
        .style(|s| {
            s.absolute()
                .size(100.pct(), 100.pct())
                .items_start()
                .justify_end()
        }),
    ))
    .window_scale(move || window_scale.get())
    .style(|s| {
        s.size(100.pct(), 100.pct())
            .flex_col()
            .items_center()
            .justify_center()
            .class(Button, |s| {
                s.border(1.0)
                    .border_radius(10.0)
                    .padding(10.0)
                    .focus_visible(|s| s.border(2.).border_color(Color::BLUE))
                    .disabled(|s| s.background(Color::LIGHT_GRAY))
                    .hover(|s| s.background(Color::LIGHT_GREEN))
                    .active(|s| s.color(Color::WHITE).background(Color::DARK_GREEN))
            })
    });

    let id = view.id();
    view.on_event_stop(EventListener::KeyUp, move |e| {
        if let Event::KeyUp(e) = e {
            if e.key.logical_key == Key::Named(NamedKey::F11) {
                id.inspect();
            }
        }
    })
}

fn main() {
    floem::launch(app_view);
}
