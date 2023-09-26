use floem::{
    peniko::Color,
    reactive::{create_rw_signal, create_signal},
    unit::UnitExt,
    view::View,
    views::{label, stack, Decorators},
};

fn app_view() -> impl View {
    let (counter, set_counter) = create_signal(0);
    let window_scale = create_rw_signal(1.0);
    stack((
        label(move || format!("Value: {}", counter.get())).style(|s| s.padding(10.0)),
        stack({
            (
                label(|| "Increment")
                    .style(|s| s.border(1.0).border_radius(10.0).padding(10.0))
                    .on_click(move |_| {
                        set_counter.update(|value| *value += 1);
                        true
                    })
                    .hover_style(|s| s.background(Color::LIGHT_GREEN))
                    .active_style(|s| s.color(Color::WHITE).background(Color::DARK_GREEN))
                    .keyboard_navigatable()
                    .focus_visible_style(|s| s.border_color(Color::BLUE).border(2.)),
                label(|| "Decrement")
                    .on_click(move |_| {
                        set_counter.update(|value| *value -= 1);
                        true
                    })
                    .style(|s| {
                        s.border(1.0)
                            .border_radius(10.0)
                            .padding(10.0)
                            .margin_left(10.0)
                    })
                    .hover_style(|s| s.background(Color::rgb8(244, 67, 54)))
                    .active_style(|s| s.color(Color::WHITE).background(Color::RED))
                    .keyboard_navigatable()
                    .focus_visible_style(|s| s.border_color(Color::BLUE).border(2.)),
                label(|| "Reset to 0")
                    .on_click(move |_| {
                        println!("Reset counter pressed"); // will not fire if button is disabled
                        set_counter.update(|value| *value = 0);
                        true
                    })
                    .disabled(move || counter.get() == 0)
                    .style(|s| {
                        s.border(1.0)
                            .border_radius(10.0)
                            .padding(10.0)
                            .margin_left(10.0)
                            .background(Color::LIGHT_BLUE)
                    })
                    .disabled_style(|s| s.background(Color::LIGHT_GRAY))
                    .hover_style(|s| s.background(Color::LIGHT_YELLOW))
                    .active_style(|s| s.color(Color::WHITE).background(Color::YELLOW_GREEN))
                    .keyboard_navigatable()
                    .focus_visible_style(|s| s.border_color(Color::BLUE).border(2.)),
            )
        }),
        stack({
            (
                label(|| "Zoom In")
                    .on_click(move |_| {
                        window_scale.update(|scale| *scale *= 1.2);
                        true
                    })
                    .style(|s| {
                        s.border(1.0)
                            .border_radius(10.0)
                            .margin_top(10.0)
                            .margin_right(10.0)
                            .padding(10.0)
                    })
                    .hover_style(|s| s.background(Color::LIGHT_GREEN)),
                label(|| "Zoom Out")
                    .on_click(move |_| {
                        window_scale.update(|scale| *scale /= 1.2);
                        true
                    })
                    .style(|s| {
                        s.border(1.0)
                            .border_radius(10.0)
                            .margin_top(10.0)
                            .margin_right(10.0)
                            .padding(10.0)
                    })
                    .hover_style(|s| s.background(Color::LIGHT_GREEN)),
                label(|| "Zoom Reset")
                    .disabled(move || window_scale.get() == 1.0)
                    .on_click(move |_| {
                        window_scale.set(1.0);
                        true
                    })
                    .style(|s| {
                        s.border(1.0)
                            .border_radius(10.0)
                            .margin_top(10.0)
                            .margin_right(10.0)
                            .padding(10.0)
                    })
                    .hover_style(|s| s.background(Color::LIGHT_GREEN))
                    .disabled_style(|s| s.background(Color::LIGHT_GRAY)),
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
    })
}

fn main() {
    floem::launch(app_view);
}
