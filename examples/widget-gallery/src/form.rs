use floem::{
    cosmic_text::Weight,
    style::Style,
    view::View,
    view_tuple::ViewTuple,
    views::{container, label, stack, Decorators},
};

pub fn form<VT: ViewTuple + 'static>(children: impl FnOnce() -> VT) -> impl View {
    stack(children).style(|| {
        Style::BASE
            .flex_col()
            .items_start()
            .margin_px(10.0)
            .padding_px(10.0)
            .width_pct(100.0)
    })
}

pub fn form_item<V: View + 'static>(
    item_label: String,
    label_width: f32,
    view_fn: impl Fn() -> V,
) -> impl View {
    container(|| {
        stack(|| {
            (
                container(|| {
                    label(move || item_label.to_string())
                        .style(|| Style::BASE.font_weight(Weight::BOLD))
                })
                .style(move || {
                    Style::BASE
                        .width_px(label_width)
                        .justify_end()
                        .margin_right_px(10.0)
                }),
                view_fn(),
            )
        })
        .style(|| Style::BASE.flex_row().items_start())
    })
    .style(|| {
        Style::BASE
            .flex_row()
            .items_center()
            .margin_bottom_px(10.0)
            .padding_px(10.0)
            .width_pct(100.0)
            .min_height_px(32.0)
    })
}
