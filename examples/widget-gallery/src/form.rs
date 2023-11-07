use floem::{
    cosmic_text::Weight,
    unit::UnitExt,
    view::View,
    view_tuple::ViewTuple,
    views::{container, label, stack, Decorators},
};

pub fn form<VT: ViewTuple + 'static>(children: VT) -> impl View {
    stack(children).style(|s| {
        s.flex_col()
            .items_start()
            .margin(10.0)
            .padding(10.0)
            .width(100.pct())
    })
}

pub fn form_item<V: View + 'static>(
    item_label: String,
    label_width: f32,
    view_fn: impl Fn() -> V,
) -> impl View {
    container(
        stack((
            container(label(move || item_label.clone()).style(|s| s.font_weight(Weight::BOLD)))
                .style(move |s| s.width(label_width).justify_end().margin_right(10.0)),
            view_fn(),
        ))
        .style(|s| s.flex_row().items_center()),
    )
    .style(|s| {
        s.flex_row()
            .items_center()
            .margin_bottom(10.0)
            .padding(10.0)
            .width(100.pct())
            .min_height(32.0)
    })
}
