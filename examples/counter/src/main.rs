use floem::{
    app::AppContext, button::button, label, reactive::create_signal, stack::stack, view::View,
};

fn app_logic(cx: AppContext) -> impl View {
    let (couter, set_counter) = create_signal(cx.scope, 0);
    stack(cx, move |cx| {
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
    })
}

fn main() {
    floem::launch(app_logic);
}
