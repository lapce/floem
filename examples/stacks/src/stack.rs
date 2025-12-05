use floem::prelude::*;

pub fn stack_view() -> impl IntoView {
    v_stack((
        text_input(RwSignal::new("this".to_string())),
        "Item 1",
        "Item 2",
        // The stack view using the postfix v_stack
        ("Item 3", "Item 4").v_stack().style(|s| s.gap(5)),
        // The vertical stack view which has flex_col() built in
        v_stack(("Item 5", "Item 6")).style(|s| s.gap(5)),
    ))
    .style(|s| s.gap(5).margin_top(10))
}
