use floem::{
    reactive::{create_signal, SignalGet},
    style::Style,
    view::View,
    views::virtual_list,
    views::Decorators,
    views::{container, label, scroll, VirtualListDirection, VirtualListItemSize},
    AppContext,
};

fn app_view() -> impl View {
    let cx = AppContext::get_current();

    let long_list: im::Vector<i32> = (0..1000000).into_iter().collect();
    let (long_list, _set_long_list) = create_signal(cx.scope, long_list);

    container(move || {
        scroll(move || {
            virtual_list(
                VirtualListDirection::Vertical,
                VirtualListItemSize::Fixed(Box::new(|| 20.0)),
                move || long_list.get(),
                move |item| *item,
                move |item| label(move || item.to_string()).style(|| Style::BASE.height_px(20.0)),
            )
            .style(|| Style::BASE.flex_col())
        })
        .style(|| Style::BASE.width_px(100.0).height_pct(100.0).border(1.0))
    })
    .style(|| {
        Style::BASE
            .size_pct(100.0, 100.0)
            .padding_vert_px(20.0)
            .flex_col()
            .items_center()
    })
}

fn main() {
    floem::launch(app_view);
}
