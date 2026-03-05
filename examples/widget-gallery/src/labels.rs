use floem::{
    prelude::*,
    text::{Alignment, FontStyle, FontWeight},
    theme::StyleThemeExt,
};

use crate::form::{form, form_item};

pub fn label_view() -> impl IntoView {
    form((
        form_item(
            "Simple Label:",
            "This is a simple label with a tooltip.\n(hover over me)"
                .tooltip(|| "This is a tooltip for the label."),
        ),
        form_item(
            "Styled Label:",
            "This is a styled label".style(|s| {
                s.background(palette::css::YELLOW)
                    .padding(10.0)
                    .color(palette::css::GREEN)
                    .font_weight(FontWeight::BOLD)
                    .font_style(FontStyle::Italic)
                    .font_size(24.0)
            }),
        ),
        form_item(
            "Centered Label:",
            "This is a label\nthat is centered on two lines".style(|s| {
                s.with_theme(|s, t| s.background(t.bg_elevated()))
                    .padding(10.0)
                    .font_size(24.0)
                    .text_align(Alignment::Center)
            }),
        ),
    ))
}
