use floem::{
    peniko::Color,
    reactive::{create_rw_signal, create_signal, SignalGet},
    style::{JustifyContent, Style},
    view::View,
    views::{
        label, scroll, stack, text_input, virtual_list, Decorators, VirtualListDirection,
        VirtualListItemSize,
    },
    AppContext,
};

fn widg_cont_style() -> Style {
    Style::BASE
        .items_center()
        .margin_bottom_px(25.0)
        .justify_content(Some(JustifyContent::SpaceBetween))
}

fn label_view(cx: AppContext) -> impl View {
    stack(cx, |cx| {
        (
            label(cx, move || "Label:".to_owned()).style(cx, || Style::BASE.margin_right_px(50.0)),
            label(cx, move || "This is a label".to_owned()),
        )
    })
    .style(cx, || widg_cont_style())
}

fn button_view(cx: AppContext) -> impl View {
    stack(cx, |cx| {
        (
            label(cx, move || "Button:".to_owned()).style(cx, || Style::BASE.margin_right_px(50.0)),
            label(cx, || "Click me".to_string())
                .on_click(|_| {
                    println!("Button clicked");
                    true
                })
                .style(cx, || {
                    Style::BASE.border(1.0).border_radius(10.0).padding_px(10.0)
                }),
        )
    })
    .style(cx, || widg_cont_style())
}

fn text_input_view(cx: AppContext) -> impl View {
    let text = create_rw_signal(cx.scope, "".to_string());

    stack(cx, |cx| {
        (
            label(cx, move || "Text input:".to_owned())
                .style(cx, || Style::BASE.margin_right_px(50.0)),
            text_input(cx, text)
                .style(cx, || {
                    Style::BASE
                        .border(1.5)
                        .background(Color::rgb8(224, 224, 224))
                        .border_radius(15.0)
                        .border_color(Color::rgb8(189, 189, 189))
                        .padding_px(10.0)
                })
                .hover_style(cx, || Style::BASE.border_color(Color::rgb8(66, 66, 66)))
                .focus_style(cx, || Style::BASE.border_color(Color::LIGHT_SKY_BLUE)),
        )
    })
    .style(cx, || widg_cont_style())
}

fn virt_list_view(cx: AppContext) -> impl View {
    let long_list: im::Vector<i32> = (0..1000000).into_iter().collect();
    let (long_list, _set_long_list) = create_signal(cx.scope, long_list);

    stack(cx, move |cx| {
        (
            label(cx, move || "List:".to_owned()).style(cx, || Style::BASE.margin_right_px(50.0)),
            scroll(cx, move |cx| {
                virtual_list(
                    cx,
                    VirtualListDirection::Vertical,
                    move || long_list.get(),
                    move |item| *item,
                    move |cx, item| {
                        label(cx, move || item.to_string())
                            .style(cx, || Style::BASE.height_px(20.0))
                    },
                    VirtualListItemSize::Fixed(20.0),
                )
                .style(cx, || Style::BASE.flex_col())
            })
            .style(cx, || {
                Style::BASE.width_px(100.0).height_px(100.0).border(1.0)
            }),
        )
    })
    .style(cx, || widg_cont_style())
}

fn app_view(cx: AppContext) -> impl View {
    stack(cx, |cx| {
        (stack(cx, |cx| {
            (
                label_view(cx),
                button_view(cx),
                text_input_view(cx),
                virt_list_view(cx),
            )
        })
        .style(cx, || {
            Style::BASE
                .background(Color::WHITE_SMOKE)
                .padding_horiz_px(50.0)
                .padding_vert_px(20.0)
                .width_px(450.0)
                .flex_col()
                .justify_content(Some(JustifyContent::SpaceBetween))
        }),)
    })
    .style(cx, || {
        Style::BASE
            .size_pct(100.0, 100.0)
            .justify_center()
            .items_center()
    })
}

fn main() {
    floem::launch(app_view);
}
