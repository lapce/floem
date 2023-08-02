use floem::{
    peniko::Color,
    style::{CursorStyle, Style},
    view::View,
    views::{label, Decorators},
};

use crate::form::{form, form_item};

pub fn button_view() -> impl View {
    form(|| {
        (
            form_item("Basic Button:".to_string(), 120.0, || {
                label(|| "Click me".to_string())
                    .on_click(|_| {
                        println!("Button clicked");
                        true
                    })
                    .keyboard_navigatable()
                    .focus_visible_style(|| Style::BASE.border(2.).border_color(Color::BLUE))
                    .style(|| Style::BASE.border(1.0).border_radius(10.0).padding_px(10.0))
            }),
            form_item("Styled Button:".to_string(), 120.0, || {
                label(|| "Click me".to_string())
                    .on_click(|_| {
                        println!("Button clicked");
                        true
                    })
                    .keyboard_navigatable()
                    .focus_visible_style(|| Style::BASE.border(2.).border_color(Color::BLUE))
                    .style(|| {
                        Style::BASE
                            .border(1.0)
                            .border_radius(10.0)
                            .padding_px(10.0)
                            .margin_left_px(10.0)
                            .background(Color::YELLOW_GREEN)
                            .color(Color::DARK_GREEN)
                            .cursor(CursorStyle::Pointer)
                    })
                    .hover_style(|| Style::BASE.background(Color::rgb8(244, 67, 54)))
                    .active_style(|| Style::BASE.color(Color::WHITE).background(Color::RED))
            }),
            form_item("Distabled Button:".to_string(), 120.0, || {
                label(|| "Click me".to_string())
                    .disabled(|| true)
                    .on_click(|_| {
                        println!("Button clicked");
                        true
                    })
                    .keyboard_navigatable()
                    .focus_visible_style(|| Style::BASE.border(2.).border_color(Color::BLUE))
                    .style(|| {
                        Style::BASE
                            .border(1.0)
                            .border_radius(10.0)
                            .padding_px(10.0)
                            .color(Color::GRAY)
                    })
                    .hover_style(|| Style::BASE.background(Color::rgb8(224, 224, 224)))
            }),
            form_item("Secondary click button:".to_string(), 120.0, || {
                label(|| "Right click me".to_string())
                    .on_secondary_click(|_| {
                        println!("Secondary mouse button click.");
                        true
                    })
                    .keyboard_navigatable()
                    .focus_visible_style(|| Style::BASE.border(2.).border_color(Color::BLUE))
                    .style(|| Style::BASE.border(1.0).border_radius(10.0).padding_px(10.0))
            }),
        )
    })
}
