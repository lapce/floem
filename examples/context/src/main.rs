use floem::{
    peniko::{color::palette, Color},
    reactive::{Context, Scope},
    ui_events::keyboard::{Key, NamedKey},
    views::{Decorators, Empty, Label, Stack},
    IntoView, View,
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

    let view = Stack::vertical((
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
    });

    let id = view.id();
    view.on_key_up(
        Key::Named(NamedKey::F11),
        |m| m.is_empty(),
        move |_| id.inspect(),
    )
}

fn main() {
    floem::launch(app_view);
}
