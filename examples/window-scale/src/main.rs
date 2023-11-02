use floem::{
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    peniko::Color,
    reactive::{create_rw_signal, create_signal},
    style_class,
    unit::UnitExt,
    view::View,
    views::{label, stack, Decorators},
};

style_class!(pub Button);

fn app_view() -> impl View {
    let (counter, set_counter) = create_signal(0);
    let window_scale = create_rw_signal(1.0);
    let view = stack((
        label(move || format!("Value: {}", counter.get())).style(|s| s.padding(10.0)),
        stack({
            (
                label(|| "Increment")
                    .class(Button)
                    .on_click(move |_| {
                        set_counter.update(|value| *value += 1);
                        true
                    })
                    .keyboard_navigatable(),
                label(|| "Decrement")
                    .class(Button)
                    .on_click(move |_| {
                        set_counter.update(|value| *value -= 1);
                        true
                    })
                    .style(|s| {
                        s.margin_left(10.0)
                            .hover(|s| s.background(Color::rgb8(244, 67, 54)))
                            .active(|s| s.background(Color::RED))
                    })
                    .keyboard_navigatable(),
                label(|| "Reset to 0")
                    .class(Button)
                    .on_click(move |_| {
                        println!("Reset counter pressed"); // will not fire if button is disabled
                        set_counter.update(|value| *value = 0);
                        true
                    })
                    .disabled(move || counter.get() == 0)
                    .style(|s| {
                        s.margin_left(10.0)
                            .background(Color::LIGHT_BLUE)
                            .hover(|s| s.background(Color::LIGHT_YELLOW))
                            .active(|s| s.background(Color::YELLOW_GREEN))
                    })
                    .keyboard_navigatable(),
            )
        }),
        stack({
            (
                label(|| "Zoom In")
                    .class(Button)
                    .on_click(move |_| {
                        window_scale.update(|scale| *scale *= 1.2);
                        true
                    })
                    .style(|s| s.margin_top(10.0).margin_right(10.0)),
                label(|| "Zoom Out")
                    .class(Button)
                    .on_click(move |_| {
                        window_scale.update(|scale| *scale /= 1.2);
                        true
                    })
                    .style(|s| s.margin_top(10.0).margin_right(10.0)),
                label(|| "Zoom Reset")
                    .class(Button)
                    .disabled(move || window_scale.get() == 1.0)
                    .on_click(move |_| {
                        window_scale.set(1.0);
                        true
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
    view.on_event(EventListener::KeyUp, move |e| {
        if let Event::KeyUp(e) = e {
            if e.key.logical_key == Key::Named(NamedKey::F11) {
                id.inspect();
            }
        }
        true
    })
}

fn main() {
    floem::launch(app_view);
}
