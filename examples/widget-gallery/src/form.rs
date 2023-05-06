use floem::{
    cosmic_text::Weight,
    style::Style,
    view::View,
    view_tuple::ViewTuple,
    views::{container, label, stack, Decorators},
    AppContext,
};

pub fn form<VT: ViewTuple + 'static>(
    cx: AppContext,
    children: impl FnOnce(AppContext) -> VT,
) -> impl View {
    stack(cx, children).style(cx, || {
        Style::BASE
            .flex_col()
            .items_start()
            .size_pct(100.0, 100.0)
            .margin_px(10.0)
            .padding_px(10.0)
    })
}

pub fn form_item<V: View + 'static>(
    cx: AppContext,
    item_label: String,
    label_width: f32,
    view_fn: impl Fn(AppContext) -> V,
) -> impl View {
    container(cx, |cx| {
        stack(cx, |cx| {
            (
                container(cx, |cx| {
                    label(cx, move || item_label.to_string())
                        .style(cx, || Style::BASE.font_weight(Weight::BOLD))
                })
                .style(cx, move || {
                    Style::BASE
                        .width_px(label_width)
                        .justify_end()
                        .margin_right_px(10.0)
                }),
                view_fn(cx),
            )
        })
        .style(cx, || {
            Style::BASE.flex_row().items_start().size_pct(100.0, 100.0)
        })
    })
    .style(cx, || {
        Style::BASE
            .flex_row()
            .items_center()
            .margin_bottom_px(10.0)
            .padding_px(10.0)
            .width_pct(100.0)
            .min_height_px(32.0)
    })
}
