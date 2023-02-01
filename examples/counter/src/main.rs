use floem::{
    app::AppContext,
    button::button,
    height, hlist, label,
    reactive::create_signal,
    stack::{hstack, vstack},
    style::Dimension,
    view::View,
    vlist, width, Decorators,
};

fn app_logic(cx: AppContext) -> impl View {
    let (couter, set_counter) = create_signal(cx.scope, 0);
    let (a, set_a) = create_signal(cx.scope, "a".to_string());
    let (b, set_b) = create_signal(cx.scope, "b".to_string());
    let (c, set_c) = create_signal(cx.scope, "b".to_string());
    let (labels, set_labels) = create_signal(cx.scope, vec![a, b, c]);

    vstack(cx, move |cx| {
        (
            vlist(
                cx,
                move || labels.get(),
                move |item| item.get(),
                |cx, item| button(cx, move || item.get(), move || {}),
            ),
            width(cx, Dimension::Points(300.0), |cx| {
                height(cx, Dimension::Points(200.0), |cx| {
                    button(
                        cx,
                        move || couter.get().to_string(),
                        move || {
                            set_counter.update(|counter| *counter += 1);
                        },
                    )
                })
            }),
            width(cx, Dimension::Points(100.0), |cx| {
                height(cx, Dimension::Points(50.0), |cx| {
                    button(
                        cx,
                        move || couter.get().to_string(),
                        move || {
                            set_counter.update(|counter| *counter += 1);
                        },
                    )
                })
            }),
            // height(cx, Dimension::Points(300.0), |cx| {
            label(cx, move || couter.get().to_string()),
            // }),
            vstack(cx, move |cx| {
                (
                    button(
                        cx,
                        move || couter.get().to_string(),
                        move || {
                            set_counter.update(|counter| *counter += 1);
                        },
                    ),
                    label(cx, move || couter.get().to_string()),
                )
            }),
        )
    })
}

fn main() {
    floem::launch(app_logic);
}
