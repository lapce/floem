use floem::{
    peniko::Color,
    reactive::create_rw_signal,
    style::{CursorStyle, Style},
    view::View,
    views::{text_input, Decorators},
    AppContext,
};

use crate::form::{form, form_item};

pub fn text_input_view(cx: AppContext) -> impl View {
    let text = create_rw_signal(cx.scope, "".to_string());

    form(cx, move |cx| {
        (
            form_item(cx, "Simple Input:".to_string(), 120.0, move |cx| {
                text_input(cx, text)
                    .style(cx, || Style::BASE.border(1.0).height_px(32.0))
                    .keyboard_navigatable(cx)
            }),
            form_item(cx, "Styled Input:".to_string(), 120.0, move |cx| {
                text_input(cx, text)
                    .style(cx, || {
                        Style::BASE
                            .border(1.5)
                            .background(Color::rgb8(224, 224, 224))
                            .border_radius(15.0)
                            .border_color(Color::rgb8(189, 189, 189))
                            .padding_px(10.0)
                            .cursor(CursorStyle::Text)
                    })
                    .hover_style(cx, || Style::BASE.border_color(Color::rgb8(66, 66, 66)))
                    .focus_style(cx, || Style::BASE.border_color(Color::LIGHT_SKY_BLUE))
                    .keyboard_navigatable(cx)
            }),
        )
    })
}
