use floem::{
    app::AppContext,
    reactive::{create_signal, SignalGet},
    style::{AlignItems, Dimension, FlexDirection, Style},
    view::View,
    views::virtual_list,
    views::Decorators,
    views::{container, label, scroll, VirtualListDirection, VirtualListItemSize},
};

fn app_logic(cx: AppContext) -> impl View {
    let long_list: im::Vector<String> = (0..1000000).into_iter().map(|i| i.to_string()).collect();
    let (long_list, _set_long_list) = create_signal(cx.scope, long_list);

    container(cx, move |cx| {
        scroll(cx, move |cx| {
            virtual_list(
                cx,
                VirtualListDirection::Vertical,
                move || long_list.get(),
                move |item| item.clone(),
                move |cx, item| {
                    label(cx, move || item.clone())
                        .style(cx, || Style::default().height(Dimension::Points(20.0)))
                },
                VirtualListItemSize::Fixed(20.0),
            )
            .style(cx, || {
                Style::default().flex_direction(FlexDirection::Column)
            })
        })
        .style(cx, || {
            Style::default().width_pt(100.0).flex_grow(1.0).border(1.0)
        })
    })
    .style(cx, || {
        Style::default()
            .width_pct(1.0)
            .height_pct(1.0)
            .flex_direction(FlexDirection::Column)
            .align_items(Some(AlignItems::Center))
    })
}

fn main() {
    floem::launch(app_logic);
}
