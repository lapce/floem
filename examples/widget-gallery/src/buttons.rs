use floem::{
    peniko::Color,
    style::{CursorStyle, Style},
    view::View,
    views::{label, Decorators},
    AppContext,
};

use crate::form::{form, form_item};

pub fn button_view(cx: AppContext) -> impl View {
    form(cx, |cx| {
        (
            form_item(cx, "Basic Button:".to_string(), 120.0, |cx| {
                label(cx, || "Click me".to_string())
                    .on_click(|_| {
                        println!("Button clicked");
                        true
                    })
                    .style(cx, || {
                        Style::BASE.border(1.0).border_radius(10.0).padding_px(10.0)
                    })
            }),
            form_item(cx, "Styled Button:".to_string(), 120.0, |cx| {
                label(cx, || "Click me".to_string())
                    .on_click(|_| {
                        println!("Button clicked");
                        true
                    })
                    .style(cx, || {
                        Style::BASE
                            .border(1.0)
                            .border_radius(10.0)
                            .padding_px(10.0)
                            .margin_left_px(10.0)
                            .background(Color::YELLOW_GREEN)
                            .color(Color::DARK_GREEN)
                            .cursor(CursorStyle::Pointer)
                    })
                    .hover_style(cx, || Style::BASE.background(Color::rgb8(244, 67, 54)))
                    .active_style(cx, || {
                        Style::BASE.color(Color::WHITE).background(Color::RED)
                    })
            }),
            form_item(cx, "Distabled Button:".to_string(), 120.0, |cx| {
                label(cx, || "Click me".to_string())
                    .disabled(cx, || true)
                    .on_click(|_| {
                        println!("Button clicked");
                        true
                    })
                    .style(cx, || {
                        Style::BASE
                            .border(1.0)
                            .border_radius(10.0)
                            .padding_px(10.0)
                            .color(Color::GRAY)
                    })
                    .hover_style(cx, || Style::BASE.background(Color::rgb8(224, 224, 224)))
            }),
        )
    })
}
