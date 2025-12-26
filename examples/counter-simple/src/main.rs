use floem::prelude::*;

fn main() {
    floem::launch(counter_view);
}

fn counter_view() -> impl IntoView {
    let mut counter = RwSignal::new(0);

    Stack::horizontal((
        Button::new("Increment").action(move || counter += 1),
        Label::derived(move || format!("Value: {counter}")),
        Button::new("Decrement").action(move || counter -= 1),
    ))
    .style(|s| s.size_full().items_center().justify_center().gap(10))
}
