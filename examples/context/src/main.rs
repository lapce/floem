use floem::{
    keyboard::{Key, Modifiers, NamedKey},
    peniko::Color,
    reactive::{provide_context, use_context},
    view::View,
    views::{empty, label, v_stack, Decorators},
};

fn colored_label(text: String) -> impl View {
    let color: Color = use_context().unwrap();
    label(move || text.clone()).style(move |s| s.color(color))
}

fn context_container<V: View + 'static>(
    color: Color,
    name: String,
    view_fn: impl Fn() -> V,
) -> impl View {
    provide_context(color);

    v_stack((colored_label(name), view_fn())).style(move |s| {
        s.padding(10)
            .border(1)
            .border_color(color)
            .gap(0, 5)
            .items_center()
    })
}

fn app_view() -> impl View {
    provide_context(Color::BLACK);

    let view = v_stack((
        colored_label(String::from("app_view")),
        context_container(Color::HOT_PINK, String::from("Nested context 1"), || {
            context_container(Color::BLUE, String::from("Nested context 2"), || {
                context_container(Color::GREEN, String::from("Nested context 3"), empty)
            })
        }),
    ))
    .style(|s| {
        s.width_full()
            .height_full()
            .items_center()
            .justify_center()
            .gap(0, 5)
    });

    let id = view.id();
    view.on_key_up(Key::Named(NamedKey::F11), Modifiers::empty(), move |_| {
        id.inspect()
    })
}

fn main() {
    floem::launch(app_view);
}
