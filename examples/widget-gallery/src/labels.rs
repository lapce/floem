use floem::{
    cosmic_text::{Style as FontStyle, Weight},
    peniko::Color,
    style::Style,
    view::View,
    views::{label, Decorators},
};

use crate::form::{form, form_item};

pub fn label_view() -> impl View {
    form(|| {
        (
            form_item("Simple Label:".to_string(), 120.0, || {
                label(move || "This is a simple label".to_owned())
            }),
            form_item("Styled Label:".to_string(), 120.0, || {
                label(move || "This is a styled label".to_owned()).style(|| {
                    Style::BASE
                        .background(Color::YELLOW)
                        .padding_px(10.0)
                        .color(Color::GREEN)
                        .font_weight(Weight::BOLD)
                        .font_style(FontStyle::Italic)
                        .font_size(24.0)
                })
            }),
        )
    })
}
