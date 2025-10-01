use crate::{
    style::{Background, CursorStyle, CustomStyle, Foreground, Style, Transition},
    unit::{DurationUnitExt, UnitExt},
    views::{
        dropdown, scroll, slider, ButtonClass, CheckboxClass, LabelCustomStyle,
        LabeledCheckboxClass, LabeledRadioButtonClass, ListClass, ListItemClass,
        PlaceholderTextClass, RadioButtonClass, RadioButtonDotClass, TextInputClass,
        ToggleButtonCircleRad, ToggleButtonClass, ToggleButtonInset, TooltipClass,
    },
};
use peniko::color::palette::{self, css};
use peniko::{Brush, Color};
use std::rc::Rc;
use taffy::style::AlignItems;

pub(crate) struct Theme {
    pub(crate) background: Color,
    pub(crate) style: Rc<Style>,
}

pub(crate) fn default_theme() -> Theme {
    let border = Color::from_rgb8(140, 140, 140);

    let padding = 5.0;
    let border_radius = 5.0;

    let hover_bg_color = Color::from_rgba8(228, 237, 216, 160);
    let focus_hover_bg_color = Color::from_rgb8(234, 230, 236);
    let active_bg_color = Color::from_rgb8(160, 160, 160);

    let selected_bg_color = Color::from_rgb8(213, 208, 216);
    let selected_hover_bg_color = Color::from_rgb8(186, 180, 216);

    let selected_unfocused_bg_color = Color::from_rgb8(212, 212, 212);
    let selected_unfocused_hover_bg_color = Color::from_rgb8(197, 197, 197);

    let light_hover_bg_color = Color::from_rgb8(250, 252, 248);
    let light_focus_hover_bg_color = Color::from_rgb8(250, 249, 251);

    let focus_applied_style = Style::new().border_color(Color::from_rgb8(114, 74, 140));

    let focus_visible_applied_style = Style::new().outline(3.0);

    let focus_style = Style::new()
        .outline_color(Color::from_rgba8(213, 208, 216, 150))
        .focus(|_| focus_applied_style.clone())
        .focus_visible(|_| focus_visible_applied_style.clone());

    let border_style = Style::new()
        .disabled(|s| s.border_color(Color::from_rgb8(131, 145, 123).with_alpha(0.3)))
        .border(1.0)
        .border_color(border)
        .padding(padding)
        .border_radius(border_radius)
        .apply(focus_style.clone());

    let button_style = Style::new()
        .custom_style_class(|s: LabelCustomStyle| s.selectable(false))
        .background(Color::from_rgb8(240, 240, 240))
        .disabled(|s| {
            s.background(Color::from_rgb8(180, 188, 175).with_alpha(0.3))
                .border_color(Color::from_rgb8(131, 145, 123).with_alpha(0.3))
                .color(palette::css::GRAY)
        })
        .active(|s| {
            s.background(active_bg_color)
                .color(palette::css::WHITE.with_alpha(0.9))
        })
        .transition(Background, Transition::linear(100.millis()))
        .focus(|s| s.hover(|s| s.background(focus_hover_bg_color)))
        .hover(|s| s.background(hover_bg_color))
        .padding(padding)
        .justify_center()
        .items_center()
        .apply(focus_style.clone())
        .apply(border_style.clone())
        .color(Color::from_rgb8(40, 40, 40));

    let checkbox_style = Style::new()
        .width(20.)
        .height(20.)
        .background(palette::css::WHITE)
        .active(|s| s.background(active_bg_color))
        .transition(Background, Transition::linear(100.millis()))
        .hover(|s| s.background(hover_bg_color))
        .focus(|s| s.hover(|s| s.background(focus_hover_bg_color)))
        .apply(border_style.clone())
        .apply(focus_style.clone())
        .disabled(|s| {
            s.background(Color::from_rgb8(180, 188, 175).with_alpha(0.3))
                .color(palette::css::GRAY)
        });

    let labeled_checkbox_style = Style::new()
        .col_gap(padding)
        .hover(|s| s.background(hover_bg_color))
        .padding(padding)
        .transition(Background, Transition::linear(100.millis()))
        .border_radius(border_radius)
        .active(|s| s.class(CheckboxClass, |s| s.background(active_bg_color)))
        .focus(|s| {
            s.class(CheckboxClass, |_| focus_applied_style.clone())
                .hover(|s| s.background(focus_hover_bg_color))
        })
        .disabled(|s| {
            s.color(palette::css::GRAY).class(CheckboxClass, |s| {
                s.background(Color::from_rgb8(180, 188, 175).with_alpha(0.3))
                    .color(palette::css::GRAY)
                    .hover(|s| s.background(Color::from_rgb8(180, 188, 175).with_alpha(0.3)))
            })
        })
        .apply(focus_style.clone());

    let radio_button_style = Style::new()
        .width(20.)
        .height(20.)
        .align_items(AlignItems::Center)
        .justify_center()
        .background(palette::css::WHITE)
        .active(|s| s.background(active_bg_color))
        .transition(Background, Transition::linear(100.millis()))
        .hover(|s| s.background(hover_bg_color))
        .focus(|s| s.hover(|s| s.background(focus_hover_bg_color)))
        .apply(border_style.clone())
        .padding(0.)
        .border_radius(100.0)
        .apply(focus_style.clone())
        .disabled(|s| {
            s.background(Color::from_rgb8(180, 188, 175).with_alpha(0.3))
                .color(palette::css::GRAY)
        });

    let radio_button_dot_style = Style::new()
        .width(8.)
        .height(8.)
        .border_radius(100.0)
        .background(palette::css::BLACK)
        .disabled(|s| {
            s.background(Color::new([0.5, 0.5, 0.5, 1.0]))
                .hover(|s| s.background(Color::new([0.5, 0.5, 0.5, 1.0])))
        });

    let labeled_radio_button_style = Style::new()
        .col_gap(padding)
        .hover(|s| s.background(hover_bg_color))
        .padding(padding)
        .transition(Background, Transition::linear(100.millis()))
        .border_radius(border_radius)
        .active(|s| s.class(RadioButtonClass, |s| s.background(active_bg_color)))
        .focus(|s| {
            s.class(RadioButtonClass, |_| focus_applied_style.clone())
                .hover(|s| s.background(focus_hover_bg_color))
        })
        .disabled(|s| {
            s.color(palette::css::GRAY).class(RadioButtonClass, |s| {
                s.background(Color::from_rgb8(180, 188, 175).with_alpha(0.3))
                    .color(palette::css::GRAY)
                    .hover(|s| s.background(Color::from_rgb8(180, 188, 175).with_alpha(0.3)))
            })
        })
        .apply(focus_style.clone());

    let toggle_button_style = Style::new()
        .active(|s| {
            s.background(active_bg_color)
                .color(palette::css::WHITE.with_alpha(0.9))
                .set(
                    Foreground,
                    Brush::Solid(palette::css::WHITE.with_alpha(0.9)),
                )
        })
        .aspect_ratio(2.)
        .background(Color::from_rgb8(240, 240, 240))
        .border_radius(50.pct())
        .border(1.)
        .focus(|s| s.hover(|s| s.background(focus_hover_bg_color)))
        .height(FONT_SIZE * 1.75)
        .hover(|s| s.background(hover_bg_color))
        .padding(padding)
        .set(Foreground, Brush::Solid(palette::css::DARK_GRAY))
        .set(ToggleButtonCircleRad, 75.pct())
        .set(ToggleButtonInset, 10.pct())
        .apply(border_style.clone())
        .apply(focus_style.clone());

    const FONT_SIZE: f32 = 12.0;

    let input_style = Style::new()
        .background(palette::css::WHITE)
        .hover(|s| s.background(light_hover_bg_color))
        .focus(|s| s.hover(|s| s.background(light_focus_hover_bg_color)))
        .apply(border_style.clone())
        .apply(focus_style.clone())
        .cursor(CursorStyle::Text)
        .padding(padding)
        .disabled(|s| {
            s.background(Color::from_rgb8(180, 188, 175).with_alpha(0.3))
                .color(palette::css::GRAY)
        });

    let item_focused_style = Style::new().selected(|s| {
        s.background(selected_bg_color)
            .hover(|s| s.background(selected_hover_bg_color))
    });

    let item_unfocused_style = Style::new()
        .hover(|s| s.background(hover_bg_color))
        .selected(|s| {
            s.background(selected_unfocused_bg_color)
                .hover(|s| s.background(selected_unfocused_hover_bg_color))
        });

    let theme = Style::new()
        .class(ListClass, |s| {
            s.apply(focus_style)
                .focus(|s| s.class(ListItemClass, |_| item_focused_style))
                .class(ListItemClass, |_| item_unfocused_style)
        })
        .class(LabeledCheckboxClass, |_| labeled_checkbox_style)
        .class(CheckboxClass, |_| checkbox_style)
        .class(RadioButtonClass, |_| radio_button_style)
        .class(RadioButtonDotClass, |_| radio_button_dot_style)
        .class(LabeledRadioButtonClass, |_| labeled_radio_button_style)
        .class(TextInputClass, |_| input_style)
        .class(ButtonClass, |_| button_style)
        .custom_style_class(|s: scroll::ScrollCustomStyle| {
            s.handle_border_radius(4.0)
                .handle_background(Color::from_rgba8(166, 166, 166, 140))
                .handle_thickness(16.0)
                .handle_rounded(false)
        })
        .class(scroll::Handle, |s| {
            s.active(|s| s.background(Color::from_rgb8(166, 166, 166)))
                .hover(|s| s.background(Color::from_rgb8(184, 184, 184)))
        })
        .class(scroll::Track, |s| {
            s.hover(|s| s.background(Color::from_rgba8(166, 166, 166, 30)))
        })
        .class(ToggleButtonClass, |_| toggle_button_style)
        .custom_style_class(|s: slider::SliderCustomStyle| {
            s.bar_color(palette::css::BLACK)
                .bar_radius(100.pct())
                .accent_bar_color(palette::css::GREEN)
                .accent_bar_radius(100.pct())
                .handle_color(Brush::Solid(palette::css::DARK_GRAY))
                .handle_radius(100.pct())
                .edge_align(true)
                .style(|s| s.size(100, 15))
        })
        .class(PlaceholderTextClass, |s| {
            s.color(css::BLACK.with_alpha(0.3)).font_size(FONT_SIZE)
        })
        .class(TooltipClass, |s| {
            s.border(0.5)
                .border_color(Color::from_rgb8(140, 140, 140))
                .color(Color::from_rgb8(80, 80, 80))
                .border_radius(2.0)
                .padding(padding)
                .margin(10.0)
                .background(palette::css::WHITE_SMOKE)
                .box_shadow_blur(2.0)
                .box_shadow_h_offset(2.0)
                .box_shadow_v_offset(2.0)
                .box_shadow_color(palette::css::BLACK.with_alpha(0.2))
        })
        .class(dropdown::DropdownClass, |s| {
            s.width(75)
                .padding(3)
                .apply(border_style)
                .class(scroll::ScrollClass, |s| {
                    s.width_full()
                        .margin_top(3)
                        .padding_vert(3)
                        .background(palette::css::WHITE_SMOKE)
                        .box_shadow_blur(2.0)
                        .box_shadow_h_offset(2.0)
                        .box_shadow_v_offset(2.0)
                        .box_shadow_color(palette::css::BLACK.with_alpha(0.4))
                        .border_radius(5.pct())
                        .items_center()
                        .class(ListItemClass, |s| {
                            s.margin_horiz(3).padding(3).items_center()
                        })
                })
        })
        .font_size(FONT_SIZE)
        .color(palette::css::BLACK);

    Theme {
        background: Color::from_rgb8(248, 248, 248),
        style: Rc::new(theme),
    }
}
