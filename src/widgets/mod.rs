//! # Floem widgets
//!
//! This module contains all of the built-in widgets of Floem.
//!

use crate::{
    style::{Background, Style, Transition},
    unit::UnitExt,
    views::scroll,
};
use peniko::Color;
use std::rc::Rc;

mod checkbox;
pub use checkbox::*;

mod toggle_button;
pub use toggle_button::*;

mod button;
pub use button::*;

mod text_input;
pub use text_input::*;

pub(crate) struct Theme {
    pub(crate) background: Color,
    pub(crate) style: Rc<Style>,
}

pub(crate) fn default_theme() -> Theme {
    let border = Color::rgb8(140, 140, 140);

    let padding = 5.0;
    let border_radius = 5.0;

    let hover_bg_color = Color::rgba8(228, 237, 216, 160);
    let focus_hover_bg_color = Color::rgb8(234, 230, 236);
    let active_bg_color = Color::rgb8(160, 160, 160);

    let light_hover_bg_color = Color::rgb8(250, 252, 248);
    let light_focus_hover_bg_color = Color::rgb8(250, 249, 251);

    let focus_applied_style = Style::new().border_color(Color::rgb8(114, 74, 140));

    let focus_visible_applied_style = Style::new().outline(3.0);

    let focus_style = Style::new()
        .outline_color(Color::rgba8(213, 208, 216, 150))
        .focus(|_| focus_applied_style.clone())
        .focus_visible(|_| focus_visible_applied_style.clone());

    let border_style = Style::new()
        .disabled(|s| s.border_color(Color::rgb8(131, 145, 123).with_alpha_factor(0.3)))
        .border(1.0)
        .border_color(border)
        .padding(padding)
        .border_radius(border_radius)
        .apply(focus_style.clone());

    let button_style = Style::new()
        .background(Color::rgb8(240, 240, 240))
        .disabled(|s| {
            s.background(Color::rgb8(180, 188, 175).with_alpha_factor(0.3))
                .border_color(Color::rgb8(131, 145, 123).with_alpha_factor(0.3))
                .color(Color::GRAY)
        })
        .active(|s| {
            s.background(active_bg_color)
                .color(Color::WHITE.with_alpha_factor(0.9))
        })
        .transition(Background, Transition::linear(0.04))
        .focus(|s| s.hover(|s| s.background(focus_hover_bg_color)))
        .hover(|s| s.background(hover_bg_color))
        .padding(padding)
        .justify_center()
        .items_center()
        .apply(focus_style.clone())
        .apply(border_style.clone())
        .color(Color::rgb8(40, 40, 40));

    let checkbox_style = Style::new()
        .width(20.)
        .height(20.)
        .background(Color::WHITE)
        .active(|s| s.background(active_bg_color))
        .transition(Background, Transition::linear(0.04))
        .hover(|s| s.background(hover_bg_color))
        .focus(|s| s.hover(|s| s.background(focus_hover_bg_color)))
        .apply(border_style.clone())
        .apply(focus_style.clone())
        .disabled(|s| {
            s.background(Color::rgb8(180, 188, 175).with_alpha_factor(0.3))
                .color(Color::GRAY)
        });

    let labeled_checkbox_style = Style::new()
        .gap(padding, 0.0)
        .hover(|s| s.background(hover_bg_color))
        .padding(padding)
        .transition(Background, Transition::linear(0.04))
        .border_radius(border_radius)
        .active(|s| s.class(CheckboxClass, |s| s.background(active_bg_color)))
        .focus(|s| {
            s.class(CheckboxClass, |_| focus_applied_style.clone())
                .hover(|s| s.background(focus_hover_bg_color))
        })
        .disabled(|s| {
            s.color(Color::GRAY).class(CheckboxClass, |s| {
                s.background(Color::rgb8(180, 188, 175).with_alpha_factor(0.3))
                    .color(Color::GRAY)
                    .hover(|s| s.background(Color::rgb8(180, 188, 175).with_alpha_factor(0.3)))
            })
        })
        .apply(focus_style.clone());

    const FONT_SIZE: f32 = 12.0;

    let input_style = Style::new()
        .background(Color::WHITE)
        .hover(|s| s.background(light_hover_bg_color))
        .focus(|s| s.hover(|s| s.background(light_focus_hover_bg_color)))
        .apply(border_style.clone())
        .apply(focus_style.clone())
        .padding_vert(8.0)
        .disabled(|s| {
            s.background(Color::rgb8(180, 188, 175).with_alpha_factor(0.3))
                .color(Color::GRAY)
        });

    let theme = Style::new()
        .class(FocusClass, |_| focus_style)
        .class(LabeledCheckboxClass, |_| labeled_checkbox_style)
        .class(CheckboxClass, |_| checkbox_style)
        .class(TextInputClass, |_| input_style)
        .class(ButtonClass, |_| button_style)
        .class(scroll::Handle, |s| {
            s.border_radius(4.0)
                .background(Color::rgba8(166, 166, 166, 140))
                .set(scroll::Thickness, 16.0)
                .set(scroll::Rounded, false)
                .active(|s| s.background(Color::rgb8(166, 166, 166)))
                .hover(|s| s.background(Color::rgb8(184, 184, 184)))
        })
        .class(scroll::Track, |s| {
            s.hover(|s| s.background(Color::rgba8(166, 166, 166, 30)))
        })
        .class(ToggleButtonClass, |s| {
            s.height(FONT_SIZE * 1.5)
                .aspect_ratio(2.)
                .border_radius(100.pct())
        })
        .font_size(FONT_SIZE)
        .color(Color::BLACK);

    Theme {
        background: Color::rgb8(248, 248, 248),
        style: Rc::new(theme),
    }
}
