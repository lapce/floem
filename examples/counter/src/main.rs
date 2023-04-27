use floem::{
    peniko::Color,
    reactive::{create_signal, SignalGet, SignalUpdate},
    style::Style,
    view::View,
    views::{click, label, stack, Decorators},
    AppContext,
};

fn app_view(cx: AppContext) -> impl View {
    let (counter, set_counter) = create_signal(cx.scope, 0);
    stack(cx, |cx| {
        (
            label(cx, move || format!("Value: {}", counter.get()))
                .style(cx, || Style::BASE.padding(10.0)),
            stack(cx, |cx| {
                (
                    click(
                        cx,
                        |cx| label(cx, || "Increment".to_string()),
                        move || set_counter.update(|value| *value += 1),
                    )
                    .style(cx, || {
                        Style::BASE.border(1.0).border_radius(10.0).padding(10.0)
                    })
                    .hover_style(cx, || Style::BASE.background(Color::LIGHT_GREEN))
                    .active_style(cx, || {
                        Style::BASE
                            .color(Color::WHITE)
                            .background(Color::DARK_GREEN)
                    }),
                    click(
                        cx,
                        |cx| label(cx, || "Decrement".to_string()),
                        move || set_counter.update(|value| *value -= 1),
                    )
                    .style(cx, || {
                        Style::BASE
                            .border(1.0)
                            .border_radius(10.0)
                            .padding(10.0)
                            .margin_left(10.0)
                    })
                    .hover_style(cx, || Style::BASE.background(Color::rgb8(244, 67, 54)))
                    .active_style(cx, || {
                        Style::BASE.color(Color::WHITE).background(Color::RED)
                    }),
                    click(
                        cx,
                        |cx| label(cx, || "Reset to 0".to_string()),
                        move || {
                            println!("Reset counter pressed"); // will not fire if button is disabled
                            set_counter.update(|value| *value = 0);
                        },
                    )
                    .disabled(cx, move || counter.get() == 0)
                    .style(cx, || {
                        Style::BASE
                            .border(1.0)
                            .border_radius(10.0)
                            .padding(10.0)
                            .margin_left(10.0)
                            .background(Color::LIGHT_BLUE)
                    })
                    .disabled_style(cx, || Style::BASE.background(Color::LIGHT_GRAY))
                    .hover_style(cx, || Style::BASE.background(Color::LIGHT_YELLOW))
                    .active_style(cx, || {
                        Style::BASE
                            .color(Color::WHITE)
                            .background(Color::YELLOW_GREEN)
                    }),
                )
            }),
        )
    })
    .style(cx, || {
        Style::BASE
            .dimension_pct(1.0, 1.0)
            .flex_col()
            .items_center()
            .justify_center()
    })
}

fn main() {
    floem::launch(app_view);
}
