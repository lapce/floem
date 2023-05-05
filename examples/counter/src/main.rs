use floem::{
    peniko::Color,
    reactive::{create_signal, SignalGet, SignalUpdate},
    style::Style,
    view::View,
    views::{label, stack, Decorators},
    AppContext,
};

fn app_view(cx: AppContext) -> impl View {
    let (counter, set_counter) = create_signal(cx.scope, 0);
    stack(cx, |cx| {
        (
            label(cx, move || format!("Value: {}", counter.get()))
                .style(cx, || Style::BASE.padding_px(10.0)),
            stack(cx, |cx| {
                (
                    label(cx, || "Increment".to_string())
                        .style(cx, || {
                            Style::BASE.border(1.0).border_radius(10.0).padding_px(10.0)
                        })
                        .on_click(move |_| {
                            set_counter.update(|value| *value += 1);
                            true
                        })
                        .hover_style(cx, || Style::BASE.background(Color::LIGHT_GREEN))
                        .active_style(cx, || {
                            Style::BASE
                                .color(Color::WHITE)
                                .background(Color::DARK_GREEN)
                        })
                        .keyboard_navigatable(cx)
                        .focus_visible_style(cx, || {
                            Style::BASE.border_color(Color::BLUE).border(2.)
                        }),
                    label(cx, || "Decrement".to_string())
                        .on_click(move |_| {
                            set_counter.update(|value| *value -= 1);
                            true
                        })
                        .style(cx, || {
                            Style::BASE
                                .border(1.0)
                                .border_radius(10.0)
                                .padding_px(10.0)
                                .margin_left_px(10.0)
                        })
                        .hover_style(cx, || Style::BASE.background(Color::rgb8(244, 67, 54)))
                        .active_style(cx, || {
                            Style::BASE.color(Color::WHITE).background(Color::RED)
                        })
                        .keyboard_navigatable(cx)
                        .focus_visible_style(cx, || {
                            Style::BASE.border_color(Color::BLUE).border(2.)
                        }),
                    label(cx, || "Reset to 0".to_string())
                        .on_click(move |_| {
                            println!("Reset counter pressed"); // will not fire if button is disabled
                            set_counter.update(|value| *value = 0);
                            true
                        })
                        .disabled(cx, move || counter.get() == 0)
                        .style(cx, || {
                            Style::BASE
                                .border(1.0)
                                .border_radius(10.0)
                                .padding_px(10.0)
                                .margin_left_px(10.0)
                                .background(Color::LIGHT_BLUE)
                        })
                        .disabled_style(cx, || Style::BASE.background(Color::LIGHT_GRAY))
                        .hover_style(cx, || Style::BASE.background(Color::LIGHT_YELLOW))
                        .active_style(cx, || {
                            Style::BASE
                                .color(Color::WHITE)
                                .background(Color::YELLOW_GREEN)
                        })
                        .keyboard_navigatable(cx)
                        .focus_visible_style(cx, || {
                            Style::BASE.border_color(Color::BLUE).border(2.)
                        }),
                )
            }),
        )
    })
    .style(cx, || {
        Style::BASE
            .size_pct(100.0, 100.0)
            .flex_col()
            .items_center()
            .justify_center()
    })
}

fn main() {
    floem::launch(app_view);
}
