use floem::{
    reactive::create_signal,
    unit::UnitExt,
    view::View,
    views::virtual_stack,
    views::Decorators,
    views::{container, label, scroll, VirtualStackDirection, VirtualStackItemSize},
};

fn app_view() -> impl View {
    let long_list: im::Vector<i32> = (0..1000000).collect();
    let (long_list, _set_long_list) = create_signal(long_list);

    container(
        scroll(
            virtual_stack(
                VirtualStackDirection::Vertical,
                VirtualStackItemSize::Fixed(Box::new(|| 20.0)),
                move || long_list.get(),
                move |item| *item,
                move |item| label(move || item.to_string()).style(|s| s.height(20.0)),
            )
            .style(|s| s.flex_col()),
        )
        .style(|s| s.width(100.0).height(100.pct()).border(1.0)),
    )
    .style(|s| {
        s.size(100.pct(), 100.pct())
            .padding_vert(20.0)
            .flex_col()
            .items_center()
    })
}

fn main() {
    floem::launch(app_view);
}
