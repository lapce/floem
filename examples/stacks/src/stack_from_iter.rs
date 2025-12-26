use floem::{
    views::{Decorators, Stack},
    IntoView,
};

pub fn stack_from_iter_view() -> impl IntoView {
    // You can also use Stack::vertical_from_iter and Stack::horizontal_from_iter for built in
    // flex direction.

    let collection: Vec<usize> = (0..10).collect();

    Stack::from_iter(collection.iter().map(|val| format!("Item {val}")))
        .style(|s| s.flex_col().row_gap(5).margin_top(10))
}
