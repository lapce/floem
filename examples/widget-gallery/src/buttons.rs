use floem::{
    peniko::Color,
    style::CursorStyle,
    view::View,
    views::Decorators,
    widgets::{self, button, toggle_button},
};

use crate::form::{form, form_item};

pub fn button_view() -> impl View {
    form({
        (
            form_item("Basic Button:".to_string(), 120.0, || {
                button(|| "Click me").on_click_stop(|_| {
                    println!("Button clicked");
                })
            }),
            form_item("Styled Button:".to_string(), 120.0, || {
                button(|| "Click me")
                    .on_click_stop(|_| {
                        println!("Button clicked");
                    })
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
            form_item("Disabled Button:".to_string(), 120.0, || {
                button(|| "Click me").disabled(|| true).on_click_stop(|_| {
                    println!("Button clicked");
                })
            }),
            form_item("Secondary click button:".to_string(), 120.0, || {
                button(|| "Right click me").on_secondary_click_stop(|_| {
                    println!("Secondary mouse button click.");
                })
            }),
            form_item("Toggle button - Switch:".to_string(), 120.0, || {
                toggle_button(|| true)
                    .on_toggle(|_| {
                        println!("Button Toggled");
                    })
                    .style(|s| {
                        s.set(
                            widgets::ToggleButtonBehavior,
                            widgets::ToggleButtonSwitch::Switch,
                        )
                    })
            }),
            form_item("Toggle button - Follow:".to_string(), 120.0, || {
                toggle_button(|| true)
                    .on_toggle(|_| {
                        println!("Button Toggled");
                    })
                    .style(|s| {
                        s.set(
                            widgets::ToggleButtonBehavior,
                            widgets::ToggleButtonSwitch::Follow,
                        )
                    })
            }),
        )
    })
}
