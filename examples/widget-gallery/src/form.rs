use floem::{
    prelude::*,
    taffy::prelude::{auto, fr},
    text::Weight,
    view_tuple::ViewTupleFlat,
};

pub fn form<VTF: ViewTupleFlat + 'static>(children: VTF) -> impl IntoView {
    children
        .flatten()
        .style(|s| {
            s.grid()
                .grid_template_columns([auto(), fr(1.)])
                .justify_center()
                .items_center()
                .row_gap(20)
                .col_gap(10)
                .padding(30)
        })
        .debug_name("Form")
}

pub fn form_item<V: IntoView + 'static>(
    item_label: impl IntoView + 'static,
    view: V,
) -> Vec<Box<dyn View>> {
    let label_view = item_label.style(|s| s.font_weight(Weight::BOLD));

    (label_view, view).into_views()
}
