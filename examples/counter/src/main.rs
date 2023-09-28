use floem::{
    peniko::Color,
    reactive::create_signal,
    unit::UnitExt,
    view::View,
    views::{label, stack, text, Decorators},
};

fn app_view() -> impl View {
    let (counter, set_counter) = create_signal(0);
    stack((
        label(move || format!("Value: {}", counter.get())).style(|s| s.padding(10.0)),
        stack((
            text("Increment")
                .style(|s| {
                    s.border_radius(10.0)
                        .padding(10.0)
                        .background(Color::WHITE)
                        .box_shadow_blur(5.0)
                })
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
                .style(|s| {
                    s.box_shadow_blur(5.0)
                        .background(Color::WHITE)
                        .border_radius(10.0)
                        .padding(10.0)
                        .margin_left(10.0)
                })
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
                .style(|s| {
                    s.box_shadow_blur(5.0)
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
        )),
    ))
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
