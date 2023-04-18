use floem::{
    app::AppContext,
    reactive::{create_signal, SignalGet, SignalUpdate},
    style::Style,
    view::View,
    views::{click, label, stack, Decorators}, peniko::Color,
};

fn app_logic(cx: AppContext) -> impl View {
    let (couter, set_counter) = create_signal(cx.scope, 0);
    stack(cx, |cx| {
        (
            label(cx, move || format!("Value: {}", couter.get()))
                .style(cx, || Style::default().padding(10.0)),
            stack(cx, |cx| {
                (
                    click(
                        cx,
                        |cx| label(cx, || "Increment".to_string()),
                        move || set_counter.update(|value| *value += 1),
                    )
                    .style(cx, || {
                        Style::default()
                            .border(1.0)
                            .border_radius(10.0)
                            .padding(10.0)
                    })
                    .hover_style(cx, || Style::default().background(Color::GREEN)),
                    click(
                        cx,
                        |cx| label(cx, || "Decrement".to_string()),
                        move || set_counter.update(|value| *value -= 1),
                    )
                    .style(cx, || {
                        Style::default()
                            .border(1.0)
                            .border_radius(10.0)
                            .padding(10.0)
                            .margin_left(10.0)
                    })
                    .hover_style(cx, || Style::default().background(Color::RED)),
                )
            }),
        )
    })
    .style(cx, || {
        Style::default()
            .dimension_pct(1.0, 1.0)
            .flex_col()
            .items_center()
            .justify_center()
    })
}

fn main() {
    floem::launch(app_logic);
}
