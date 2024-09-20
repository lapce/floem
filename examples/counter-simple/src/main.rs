use floem::{
    reactive::create_signal,
    views::{button, label, Decorators},
    IntoView,
};

fn app_view() -> impl IntoView {
    // Create a reactive signal with a counter value, defaulting to 0
    let (counter, mut set_counter) = create_signal(0);

    // Create a vertical layout
    (
        // The counter value updates automatically, thanks to reactivity
        label(move || format!("Value: {counter}")),
        // Create a horizontal layout
        (
            button("Increment").action(move || set_counter += 1),
            button("Decrement").action(move || set_counter -= 1),
        ),
    )
        .style(|s| s.flex_col())
}

fn main() {
    floem::launch(app_view);
}
