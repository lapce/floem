use floem::{
    reactive::create_signal,
    views::{label, ButtonClass, Decorators},
    IntoView,
};

fn app_view() -> impl IntoView {
    // Create a reactive signal with a counter value, defaulting to 0
    let (counter, set_counter) = create_signal(0);

    // Create a vertical layout
    (
        // The counter value updates automatically, thanks to reactivity
        label(move || format!("Value: {}", counter.get())),
        // Create a horizontal layout
        (
            "Increment".class(ButtonClass).on_click_stop(move |_| {
                set_counter.update(|value| *value += 1);
            }),
            "Decrement".class(ButtonClass).on_click_stop(move |_| {
                set_counter.update(|value| *value -= 1);
            }),
        ),
    )
        .style(|s| s.flex_col())
}

fn main() {
    floem::launch(app_view);
}
