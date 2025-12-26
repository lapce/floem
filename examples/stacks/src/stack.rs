use floem::{
    views::{Decorators, Stack},
    IntoView,
};

#[rustfmt::skip]
pub fn stack_view() -> impl IntoView {
    // An example of the three different ways you can do a vertical stack

    // A stack just with a tuple as syntax sugar
    (
        "Item 1",
        "Item 2",

        // The stack view which takes a tuple as an argument
        Stack::new((
            "Item 3",
            "Item 4",
        )).style(|s| s.flex_col().row_gap(5)),

        // The vertical stack view which has flex_col() built in
        Stack::vertical((
            "Item 5",
            "Item 6",
        )).style(|s| s.row_gap(5)),

    )
    .style(|s| s.flex_col().gap( 5).margin_top(10))
}
