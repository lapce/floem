use floem::{
    app::AppContext,
    button::button,
    reactive::create_signal,
    stack::stack,
    style::{Dimension, FlexDirection, Style},
    view::View,
    views::label,
    views::list,
    views::Decorators,
};

fn app_logic(cx: AppContext) -> impl View {
    let (couter, set_counter) = create_signal(cx.scope, 0);
    let (a, set_a) = create_signal(cx.scope, "a".to_string());
    let (b, set_b) = create_signal(cx.scope, "b".to_string());
    let (c, set_c) = create_signal(cx.scope, "b".to_string());
    let (labels, set_labels) = create_signal(cx.scope, vec![a, b, c]);

    stack(cx, move |cx| {
        (
            label(cx, move || couter.get().to_string()),
            list(
                cx,
                move || labels.get(),
                move |item| item.get(),
                move |cx, item| {
                    button(
                        cx,
                        move || item.get(),
                        move || {
                            set_counter.update(|counter| *counter += 1);
                        },
                    )
                    .style(cx, || Style {
                        width: Dimension::Points(50.0),
                        height: Dimension::Points(20.0),
                        ..Default::default()
                    })
                },
            ),
            stack(cx, move |cx| {
                (
                    label(cx, move || couter.get().to_string()),
                    button(
                        cx,
                        move || couter.get().to_string(),
                        move || {
                            set_counter.update(|counter| *counter += 1);
                        },
                    )
                    .style(cx, || Style {
                        width: Dimension::Points(50.0),
                        height: Dimension::Points(20.0),
                        ..Default::default()
                    }),
                )
            }),
            label(cx, move || couter.get().to_string()),
            button(
                cx,
                move || couter.get().to_string(),
                move || {
                    set_counter.update(|counter| *counter += 1);
                },
            )
            .style(cx, || Style {
                width: Dimension::Auto,
                height: Dimension::Auto,
                flex_grow: 1.0,
                ..Default::default()
            }),
            label(cx, move || "seprate".to_string()),
            button(
                cx,
                move || couter.get().to_string(),
                move || {
                    set_counter.update(|counter| *counter += 1);
                },
            )
            .style(cx, || Style {
                width: Dimension::Auto,
                height: Dimension::Auto,
                flex_grow: 2.0,
                ..Default::default()
            }),
            label(cx, move || couter.get().to_string()),
            stack(cx, move |cx| {
                (
                    label(cx, move || couter.get().to_string()),
                    label(cx, move || couter.get().to_string()),
                    label(cx, move || couter.get().to_string()),
                    button(
                        cx,
                        move || couter.get().to_string(),
                        move || {
                            set_counter.update(|counter| *counter += 1);
                        },
                    )
                    .style(cx, || Style {
                        width: Dimension::Points(50.0),
                        height: Dimension::Points(20.0),
                        ..Default::default()
                    }),
                    label(cx, move || couter.get().to_string()),
                )
            }),
        )
    })
    .style(cx, || Style {
        width: Dimension::Percent(1.0),
        height: Dimension::Percent(1.0),
        flex_direction: FlexDirection::Column,
        ..Default::default()
    })
}

fn main() {
    floem::launch(app_logic);
}
