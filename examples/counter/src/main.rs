use floem::{
    peniko::Color,
    reactive::create_signal,
    style::{box_shadow, Style},
    unit::Pct,
    view::View,
    views::{label, stack, text, Decorators},
};

fn app_view() -> impl View {
    let button_style = |s: Style| {
        s.p(10)
            .border_radius(8)
            .box_shadow(box_shadow(10, 10, 8, 2, Color::rgb(0.6, 0.6, 0.6)))
    };

    let (counter, set_counter) = create_signal(0);
    stack((
        label(move || format!("Value: {}", counter.get())).style(|s| s.padding(10)),
        stack((
            text("Increment")
                .style(move |s| button_style(s).background(Color::WHITE))
                .on_click({
                    move |_| {
                        set_counter.update(|value| *value += 1);
                        true
                    }
                })
                .hover_style(|s| s.background(Color::LIGHT_GREEN))
                .active_style(|s| s.color(Color::WHITE).background(Color::DARK_GREEN))
                .keyboard_navigatable()
                .focus_visible_style(|s| s.border_color(Color::BLUE).border(2.)),
            text("Decrement")
                .on_click({
                    move |_| {
                        set_counter.update(|value| *value -= 1);
                        true
                    }
                })
                .style(move |s| button_style(s).background(Color::WHITE).margin_left(16.0))
                .hover_style(|s| s.background(Color::rgb8(244, 67, 54)))
                .active_style(|s| s.color(Color::WHITE).background(Color::RED))
                .keyboard_navigatable()
                .focus_visible_style(|s| s.border_color(Color::BLUE).border(2.)),
            text("Reset to 0")
                .on_click(move |_| {
                    println!("Reset counter pressed"); // will not fire if button is disabled
                    set_counter.update(|value| *value = 0);
                    true
                })
                .disabled(move || counter.get() == 0)
                .style(move |s| {
                    button_style(s)
                        .margin_left(16)
                        .background(Color::LIGHT_BLUE)
                })
                .disabled_style(|s| s.background(Color::LIGHT_GRAY))
                .hover_style(|s| s.background(Color::LIGHT_YELLOW))
                .active_style(|s| s.color(Color::WHITE).background(Color::YELLOW_GREEN))
                .keyboard_navigatable()
                .focus_visible_style(|s| s.border_color(Color::BLUE).border(2.)),
        )),
    ))
    .style(|s| {
        s.size(Pct(100.0), Pct(100.0))
            .flex_col()
            .items_center()
            .justify_center()
    })
}

fn main() {
    floem::launch(app_view);
}
