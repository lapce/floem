use floem::{im_rc, prelude::*};

pub fn virtual_stack_view() -> impl IntoView {
    // A virtual list is optimized to only render the views that are visible
    // making it ideal for large lists with a lot of views.

    let long_list: im_rc::Vector<i32> = (0..1000000).collect();
    let long_list = RwSignal::new(long_list);

    let button = button("Add an item")
        .action(move || long_list.update(|list| list.push_back(list.len() as i32 + 1)));

    let virtual_stack = VirtualStack::new(move || long_list.get())
        .style(|s| s.flex_col().width_full())
        .scroll()
        .style(|s| s.width(100).height(200).border(1));

    (button, virtual_stack)
        .h_stack()
        .style(|s| s.flex_col().row_gap(5).margin_top(10))
}
