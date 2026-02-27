use floem::{
    action::inspect,
    prelude::*,
    reactive::{Context, Scope},
};

fn colored_label(text: String) -> impl IntoView {
    let color: Color = Context::get().unwrap();
    Label::derived(move || text.clone()).style(move |s| s.color(color))
}

fn context_container<V: IntoView + 'static>(
    color: Color,
    name: String,
    view_fn: impl Fn() -> V,
) -> impl IntoView {
    // Create a child scope for this context container
    let scope = Scope::current().create_child();
    scope.enter(|| {
        Context::provide(color);

        Stack::vertical((colored_label(name), view_fn())).style(move |s| {
            s.padding(10)
                .border(1)
                .border_color(color)
                .row_gap(5)
                .items_center()
        })
    })
}

fn app_view() -> impl IntoView {
    Context::provide(palette::css::BLACK);

    Stack::vertical((
        colored_label(String::from("app_view")),
        context_container(
            palette::css::HOT_PINK,
            String::from("Nested context 1"),
            || {
                context_container(palette::css::BLUE, String::from("Nested context 2"), || {
                    context_container(
                        palette::css::GREEN,
                        String::from("Nested context 3"),
                        Empty::new,
                    )
                })
            },
        ),
    ))
    .style(|s| {
        s.width_full()
            .height_full()
            .items_center()
            .justify_center()
            .row_gap(5)
    })
    .on_event_stop(el::KeyUp, |_, KeyboardEvent { key, .. }| {
        if *key == Key::Named(NamedKey::F11) {
            inspect();
        }
    })
}

fn main() {
    floem::launch(app_view);
}
