use floem::{
    cosmic_text::{Style as FontStyle, Weight},
    peniko::Color,
    style::Style,
    view::View,
    views::{label, Decorators},
    AppContext,
};

use crate::form::{form, form_item};

pub fn label_view(cx: AppContext) -> impl View {
    form(cx, |cx| {
        (
            form_item(cx, "Simple Label:".to_string(), 120.0, |cx| {
                label(cx, move || "This is a simple label".to_owned())
            }),
            form_item(cx, "Styled Label:".to_string(), 120.0, |cx| {
                label(cx, move || "This is a styled label".to_owned()).style(cx, || {
                    Style::BASE
                        .background(Color::YELLOW)
                        .padding(10.0)
                        .color(Color::GREEN)
                        .font_weight(Weight::BOLD)
                        .font_style(FontStyle::Italic)
                        .font_size(24.0)
                })
            }),
        )
    })
}
