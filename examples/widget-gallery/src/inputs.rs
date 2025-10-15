use floem::{prelude::*, style::SelectionCornerRadius, text::Weight};

use crate::form::{form, form_item};

pub fn text_input_view() -> impl IntoView {
    let text = RwSignal::new(String::new());

    const LIGHT_GRAY_224: Color = Color::from_rgb8(224, 224, 224);
    const MEDIUM_GRAY_189: Color = Color::from_rgb8(189, 189, 189);
    const DARK_GRAY_66: Color = Color::from_rgb8(66, 66, 66);
    const SKY_BLUE: Color = palette::css::LIGHT_SKY_BLUE;

    const LIGHT_GRAY_BG: Color = LIGHT_GRAY_224.with_alpha(0.1);
    const LIGHT_GRAY_BG_HOVER: Color = LIGHT_GRAY_224.with_alpha(0.2);
    const SKY_BLUE_FOCUS: Color = SKY_BLUE.with_alpha(0.8);

    form((
        form_item(
            "Simple Input:",
            text_input(text)
                .placeholder("Placeholder text")
                .style(|s| s.width(250.))
                .keyboard_navigable(),
        ),
        form_item(
            "Styled Input:",
            text_input(text)
                .placeholder("Placeholder text")
                .style(|s| {
                    s.border(1.5)
                        .width(250.0)
                        .background(LIGHT_GRAY_BG)
                        .border_radius(15.0)
                        .border_color(MEDIUM_GRAY_189)
                        .padding(10.0)
                        .hover(|s| s.background(LIGHT_GRAY_BG_HOVER).border_color(DARK_GRAY_66))
                        .set(SelectionCornerRadius, 4.0)
                        .focus(|s| {
                            s.border_color(SKY_BLUE_FOCUS)
                                .hover(|s| s.border_color(SKY_BLUE))
                        })
                        .class(PlaceholderTextClass, |s| {
                            s.color(SKY_BLUE)
                                .font_style(floem::text::Style::Italic)
                                .font_weight(Weight::BOLD)
                        })
                        .font_family("monospace".to_owned())
                })
                .keyboard_navigable(),
        ),
        form_item("Disabled Input:", text_input(text).disabled(|| true)),
    ))
}
