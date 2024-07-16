use floem::{
    reactive::create_rw_signal,
    views::{dyn_stack, scroll, ButtonClass, Decorators},
    IntoView,
};

pub fn dyn_stack_view() -> impl IntoView {
    // With the dyn_stack you can change the stack at runtime by controlling
    // your stack with a signal.

    let long_list: im::Vector<i32> = (0..10).collect();
    let long_list = create_rw_signal(long_list);

    (
        "Add an item".class(ButtonClass).on_click_stop(move |_| {
            long_list.update(|list| list.push_front(list.len() as i32 + 1))
        }),
        scroll(
            dyn_stack(
                move || long_list.get(),
                move |item| *item,
                move |item| item.style(|s| s.height(20).justify_center()),
            )
            .style(|s| s.flex_col().width_full()),
        )
        .style(|s| s.width(100).height(200).border(1)),
    )
        .style(|s| s.flex_col().column_gap(5).margin_top(10))
}
