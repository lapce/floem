use floem::{
    peniko::Color,
    reactive::create_rw_signal,
    style::SelectionCornerRadius,
    text::Weight,
    views::{text_input, Decorators, PlaceholderTextClass},
    IntoView,
};

use crate::form::{form, form_item};

pub fn text_input_view() -> impl IntoView {
    let text = create_rw_signal("".to_string());

    form({
        (
            form_item("Simple Input:".to_string(), 120.0, move || {
                text_input(text)
                    .placeholder("Placeholder text")
                    .keyboard_navigable()
            }),
            form_item("Styled Input:".to_string(), 120.0, move || {
                text_input(text)
                    .placeholder("Placeholder text")
                    .style(|s| {
                        s.border(1.5)
                            .width(250.0)
                            .background(Color::rgb8(224, 224, 224).multiply_alpha(0.1))
                            .border_radius(15.0)
                            .border_color(Color::rgb8(189, 189, 189))
                            .padding(10.0)
                            .hover(|s| {
                                s.background(Color::rgb8(224, 224, 224).multiply_alpha(0.2))
                                    .border_color(Color::rgb8(66, 66, 66))
                            })
                            .set(SelectionCornerRadius, 4.0)
                            .focus(|s| {
                                s.border_color(Color::LIGHT_SKY_BLUE.multiply_alpha(0.8))
                                    .hover(|s| s.border_color(Color::LIGHT_SKY_BLUE))
                            })
                            .class(PlaceholderTextClass, |s| {
                                s.color(Color::LIGHT_SKY_BLUE)
                                    .font_style(floem::text::Style::Italic)
                                    .font_weight(Weight::BOLD)
                            })
                            .font_family("monospace".to_owned())
                    })
                    .keyboard_navigable()
            }),
            form_item("Disabled Input:".to_string(), 120.0, move || {
                text_input(text).disabled(|| true)
            }),
        )
    })
}
