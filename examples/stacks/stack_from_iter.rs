use floem::{
    views::{stack_from_iter, Decorators},
    IntoView,
};

pub fn stack_from_iter_view() -> impl IntoView {
    // You can also use v_stack_from_iter and h_stack_from_iter for built in
    // flex direction.

    let collection: Vec<usize> = (0..10).collect();

    stack_from_iter(collection.iter().map(|val| format!("Item {}", val)))
        .style(|s| s.flex_col().column_gap(5).margin_top(10))
}
