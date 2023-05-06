use floem::{
    reactive::{create_signal, SignalGet},
    style::Style,
    view::View,
    views::{
        label, scroll, virtual_list, Decorators, VirtualListDirection, VirtualListItemSize,
    },
    AppContext,
};

use crate::form::{form, form_item};

pub fn virt_list_view(cx: AppContext) -> impl View {
    let long_list: im::Vector<i32> = (0..100).collect();
    let (long_list, _set_long_list) = create_signal(cx.scope, long_list);

    form(cx, move |cx| {
        (form_item(cx, "List".to_string(), 30.0, move |cx| {
            scroll(cx, move |cx| {
                virtual_list(
                    cx,
                    VirtualListDirection::Vertical,
                    VirtualListItemSize::Fixed(20.0),
                    move || long_list.get(),
                    move |item| *item,
                    move |cx, item| {
                        label(cx, move || item.to_string())
                            .style(cx, || Style::BASE.height_px(24.0))
                    },
                )
                .style(cx, || Style::BASE.flex_col())
            })
            .style(cx, || {
                Style::BASE.width_px(100.0).height_px(300.0).border(1.0)
            })
        }),)
    })
}
