use floem::{
    peniko::Color, reactive::create_rw_signal, view::View, views::Decorators, widgets::text_input,
};

use crate::form::{form, form_item};

pub fn text_input_view() -> impl View {
    let text = create_rw_signal("".to_string());

    form({
        (
            form_item("Simple Input:".to_string(), 120.0, move || {
                text_input(text).keyboard_navigatable()
            }),
            form_item("Styled Input:".to_string(), 120.0, move || {
                text_input(text)
                    .style(|s| {
                        s.border(1.5)
                            .width(250.0)
                            .background(Color::rgb8(224, 224, 224).with_alpha_factor(0.1))
                            .border_radius(15.0)
                            .border_color(Color::rgb8(189, 189, 189))
                            .padding(10.0)
                            .hover(|s| {
                                s.background(Color::rgb8(224, 224, 224).with_alpha_factor(0.2))
                                    .border_color(Color::rgb8(66, 66, 66))
                            })
                            .focus(|s| {
                                s.border_color(Color::LIGHT_SKY_BLUE.with_alpha_factor(0.8))
                                    .hover(|s| s.border_color(Color::LIGHT_SKY_BLUE))
                            })
                    })
                    .keyboard_navigatable()
            }),
            form_item("Disabled Input:".to_string(), 120.0, move || {
                text_input(text).disabled(|| true)
            }),
        )
    })
}
