use floem::prelude::*;

fn main() {
    floem::launch(counter_view);
}

fn counter_view() -> impl IntoView {
    let mut counter = RwSignal::new(0);

    h_stack((
        button("Increment").action(move || counter += 1),
        label(move || format!("Value: {counter}")),
        button("Decrement").action(move || counter -= 1),
    ))
    .style(|s| s.size_full().items_center().justify_center().gap(10))
    .on_key_up(
        Key::Named(NamedKey::F11),
        |m| m.is_empty(),
        move |v, _| v.id().inspect(),
    )
}
