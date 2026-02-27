use floem::{imbl, prelude::*};

pub fn dyn_stack_view() -> impl IntoView {
    // With the dyn_stack you can change the stack at runtime by controlling
    // your stack with a signal.

    let long_list: imbl::Vector<i32> = (0..10).collect();
    let long_list = RwSignal::new(long_list);

    let button = Button::new("Add an item")
        .action(move || long_list.update(|list| list.push_back(list.len() as i32 + 1)));

    let stack = dyn_stack(
        move || long_list.get(),
        move |item| *item,
        move |item| item.style(|s| s.height(20)),
    )
    .style(|s| s.flex_col())
    .scroll()
    .style(|s| s.flex_grow(1.).height(200).border(1));

    Stack::vertical((button, stack)).style(|s| s.row_gap(5).margin_top(10))
}
