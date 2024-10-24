use floem::{
    prelude::*,
    text::{Style as FontStyle, Weight},
};

use crate::form::{form, form_item};

pub fn label_view() -> impl IntoView {
    form((
        form_item(
            "Simple Label:",
            tooltip("This is a simple label", || {
                static_label("This is a tooltip for the label.")
            }),
        ),
        form_item(
            "Styled Label:",
            "This is a styled label".style(|s| {
                s.background(palette::css::YELLOW)
                    .padding(10.0)
                    .color(palette::css::GREEN)
                    .font_weight(Weight::BOLD)
                    .font_style(FontStyle::Italic)
                    .font_size(24.0)
            }),
        ),
    ))
}
