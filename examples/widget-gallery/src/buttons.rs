use floem::{
    peniko::Color,
    style::CursorStyle,
    view::View,
    views::{label, Decorators},
};

use crate::form::{form, form_item};

pub fn button_view() -> impl View {
    form({
        (
            form_item("Basic Button:".to_string(), 120.0, || {
                label(|| "Click me")
                    .on_click(|_| {
                        println!("Button clicked");
                        true
                    })
                    .keyboard_navigatable()
                    .style(|s| {
                        s.border(1.0)
                            .border_radius(10.0)
                            .padding(10.0)
                            .focus_visible(|s| s.border(2.).border_color(Color::BLUE))
                    })
            }),
            form_item("Styled Button:".to_string(), 120.0, || {
                label(|| "Click me")
                    .on_click(|_| {
                        println!("Button clicked");
                        true
                    })
                    .keyboard_navigatable()
                    .style(|s| {
                        s.border(1.0)
                            .border_radius(10.0)
                            .padding(10.0)
                            .margin_left(10.0)
                            .background(Color::YELLOW_GREEN)
                            .color(Color::DARK_GREEN)
                            .cursor(CursorStyle::Pointer)
                            .active(|s| s.color(Color::WHITE).background(Color::RED))
                            .hover(|s| s.background(Color::rgb8(244, 67, 54)))
                            .focus_visible(|s| s.border(2.).border_color(Color::BLUE))
                    })
            }),
            form_item("Distabled Button:".to_string(), 120.0, || {
                label(|| "Click me")
                    .disabled(|| true)
                    .on_click(|_| {
                        println!("Button clicked");
                        true
                    })
                    .keyboard_navigatable()
                    .style(|s| {
                        s.border(1.0)
                            .border_radius(10.0)
                            .padding(10.0)
                            .color(Color::GRAY)
                            .focus_visible(|s| s.border(2.).border_color(Color::BLUE))
                            .hover(|s| s.background(Color::rgb8(224, 224, 224)))
                    })
            }),
            form_item("Secondary click button:".to_string(), 120.0, || {
                label(|| "Right click me")
                    .on_secondary_click(|_| {
                        println!("Secondary mouse button click.");
                        true
                    })
                    .keyboard_navigatable()
                    .style(|s| {
                        s.border(1.0)
                            .border_radius(10.0)
                            .padding(10.0)
                            .focus_visible(|s| s.border(2.).border_color(Color::BLUE))
                    })
            }),
        )
    })
}
