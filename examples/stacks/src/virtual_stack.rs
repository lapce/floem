use floem::{
    reactive::create_rw_signal,
    views::{scroll, virtual_stack, ButtonClass, Decorators, VirtualDirection, VirtualItemSize},
    IntoView,
};

pub fn virtual_stack_view() -> impl IntoView {
    // A virtual list is optimized to only render the views that are visible
    // making it ideal for large lists with a lot of views.

    let long_list: im::Vector<i32> = (0..1000000).collect();
    let long_list = create_rw_signal(long_list);

    (
        "Add an item".class(ButtonClass).on_click_stop(move |_| {
            long_list.update(|list| list.push_front(list.len() as i32 + 1))
        }),
        scroll(
            virtual_stack(
                VirtualDirection::Vertical,
                VirtualItemSize::Fixed(Box::new(|| 20.0)),
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
