use std::time::Duration;

use floem::{
    peniko::Color,
    style::{Style, TextColor, Transition},
    style_class,
    views::{Button, Decorators, button},
};

style_class!(pub InputBtn);
style_class!(pub AppWindow);
style_class!(pub InputButtonIsland);
style_class!(pub OutputTxt);

pub fn default_theme_buttons_style() -> Style {
    Style::new()
        .height_full()
        .flex()
        .items_center()
        .justify_center()
        .width(50f64)
}

pub fn render_input_button(text: &'static str) -> Button {
    button(text).class(InputBtn).style(|s| {
        s.flex()
            .items_center()
            .justify_center()
            .border_top_left_radius(50)
            .border_bottom_left_radius(50)
            .border_bottom_right_radius(50)
            .border_top_right_radius(50)
            .font_bold()
            .font_size(24f64)
    })
}

fn dark_input_button() -> Style {
    Style::new()
        .background(Color::from_rgb8(223, 208, 184))
        .color(Color::from_rgb8(148, 137, 121))
        .transition(
            TextColor,
            Transition::ease_in_out(Duration::from_millis(5000)),
        )
}

fn light_input_button() -> Style {
    Style::new()
        .background(Color::from_rgb8(255, 239, 239))
        .color(Color::from_rgb8(243, 208, 215))
        .transition(
            TextColor,
            Transition::ease_in_out(Duration::from_millis(1000)),
        )
}

pub fn dark_theme() -> Style {
    Style::new()
        .background(Color::from_rgb8(34, 40, 49))
        .class(InputBtn, move |_| dark_input_button())
        .class(InputButtonIsland, move |s| {
            s.background(Color::from_rgb8(57, 62, 70))
        })
        .class(OutputTxt, move |s| s.color(Color::from_rgb8(148, 137, 121)))
}

pub fn light_theme() -> Style {
    Style::new()
        .background(Color::from_rgb8(246, 245, 242))
        .class(InputBtn, move |_| light_input_button())
        .class(InputButtonIsland, move |s| {
            s.background(Color::from_rgb8(240, 235, 227))
        })
        .class(OutputTxt, move |s| s.color(Color::from_rgb8(243, 208, 215)))
}
