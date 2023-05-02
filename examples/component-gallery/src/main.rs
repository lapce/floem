use floem::{
    peniko::Color,
    reactive::create_rw_signal,
    style::{JustifyContent, Style},
    view::View,
    views::{click, label, stack, text_input, Decorators},
    AppContext,
};

fn app_view(cx: AppContext) -> impl View {
    let text = create_rw_signal(cx.scope, "".to_string());

    fn widget_cont_style() -> Style {
        Style::BASE
            .items_center()
            .margin_bottom(20.0)
            .justify_content(Some(JustifyContent::SpaceBetween))
    }

    stack(cx, |cx| {
        (stack(cx, |cx| {
            (
                stack(cx, |cx| {
                    (
                        label(cx, move || "Label:".to_owned())
                            .style(cx, || Style::BASE.margin_right(50.0)),
                        label(cx, move || "This is a label".to_owned()),
                    )
                })
                .style(cx, || widget_cont_style()),
                stack(cx, |cx| {
                    (
                        label(cx, move || "Button:".to_owned())
                            .style(cx, || Style::BASE.margin_right(50.0)),
                        click(
                            cx,
                            |cx| label(cx, || "Click me".to_string()),
                            move || println!("Button clicked"),
                        )
                        .style(cx, || {
                            Style::BASE.border(1.0).border_radius(10.0).padding(10.0)
                        }),
                    )
                })
                .style(cx, || widget_cont_style()),
                stack(cx, |cx| {
                    (
                        label(cx, move || "Text input:".to_owned())
                            .style(cx, || Style::BASE.margin_right(50.0)),
                        text_input(cx, text)
                            .style(cx, || {
                                Style::BASE
                                    .border(1.5)
                                    .background(Color::rgb8(224, 224, 224))
                                    .border_radius(15.0)
                                    .border_color(Color::rgb8(189, 189, 189))
                                    .padding(10.0)
                            })
                            .hover_style(cx, || Style::BASE.border_color(Color::rgb8(66, 66, 66)))
                            .focus_style(cx, || Style::BASE.border_color(Color::LIGHT_SKY_BLUE)),
                    )
                })
                .style(cx, || widget_cont_style()),
            )
        })
        .style(cx, || {
            Style::BASE
                .background(Color::WHITE_SMOKE)
                .padding_horiz(50.0)
                .padding_vert(20.0)
                .width_px(450.0)
                .flex_col()
                .justify_content(Some(JustifyContent::SpaceBetween))
        }),)
    })
    .style(cx, || {
        Style::BASE
            .dimension_pct(1.0, 1.0)
            .justify_center()
            .items_center()
    })
}

fn main() {
    floem::launch(app_view);
}
