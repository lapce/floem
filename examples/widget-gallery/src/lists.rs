use floem::{
    reactive::{create_signal, SignalGet},
    style::Style,
    view::View,
    views::{label, scroll, virtual_list, Decorators, VirtualListDirection, VirtualListItemSize},
    AppContext,
};

use crate::form::{form, form_item};

pub fn virt_list_view() -> impl View {
    let cx = AppContext::get_current();
    let long_list: im::Vector<i32> = (0..100).collect();
    let (long_list, _set_long_list) = create_signal(cx.scope, long_list);

    form(move || {
        (form_item("List".to_string(), 30.0, move || {
            scroll(move || {
                virtual_list(
                    VirtualListDirection::Vertical,
                    VirtualListItemSize::Fixed(20.0),
                    move || long_list.get(),
                    move |item| *item,
                    move |item| {
                        label(move || item.to_string()).style(|| Style::BASE.height_px(24.0))
                    },
                )
                .style(|| Style::BASE.flex_col())
            })
            .style(|| Style::BASE.width_px(100.0).height_px(300.0).border(1.0))
        }),)
    })
}
