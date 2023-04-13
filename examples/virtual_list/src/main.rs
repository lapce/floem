use floem::{
    app::AppContext,
    reactive::{create_signal, SignalGet},
    style::Style,
    view::View,
    views::virtual_list,
    views::Decorators,
    views::{container, label, scroll, VirtualListDirection, VirtualListItemSize},
};

fn app_logic(cx: AppContext) -> impl View {
    let long_list: im::Vector<i32> = (0..1000000).into_iter().collect();
    let (long_list, _set_long_list) = create_signal(cx.scope, long_list);

    container(cx, move |cx| {
        scroll(cx, move |cx| {
            virtual_list(
                cx,
                VirtualListDirection::Vertical,
                move || long_list.get(),
                move |item| *item,
                move |cx, item| {
                    label(cx, move || item.to_string())
                        .style(cx, || Style::default().height_pt(20.0))
                },
                VirtualListItemSize::Fixed(20.0),
            )
            .style(cx, || Style::default().flex_col())
        })
        .style(cx, || {
            Style::default().width_pt(100.0).height_pct(1.0).border(1.0)
        })
    })
    .style(cx, || {
        Style::default()
            .dimension_pct(1.0, 1.0)
            .padding_vert(20.0)
            .flex_col()
            .items_center()
    })
}

fn main() {
    floem::launch(app_logic);
}
