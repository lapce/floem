use std::time::Duration;

use crate::{
    prop,
    style::{
        Background, CursorStyle, CustomStyle, FontSize, Foreground, Style, StylePropValue,
        Transition,
    },
    unit::{DurationUnitExt, UnitExt},
    views::{
        dropdown,
        resizable::{ResizableClass, ResizableCustomStyle},
        scroll,
        slider::{SliderClass, SliderCustomStyle},
        ButtonClass, CheckboxClass, LabelClass, LabelCustomStyle, LabeledCheckboxClass,
        LabeledRadioButtonClass, ListClass, ListItemClass, PlaceholderTextClass, RadioButtonClass,
        RadioButtonDotClass, SvgClass, TabSelectorClass, TextInputClass, ToggleButtonCircleRad,
        ToggleButtonClass, ToggleButtonInset, TooltipClass,
    },
    AnyView,
};
use floem_renderer::text::Weight;
use peniko::{color::palette::css, Brush, Color};
use taffy::Overflow;

#[derive(Debug, Clone, PartialEq)]
pub struct DesignSystem {
    pub bg_base: Color,
    pub text_base: Color,
    pub text_lightness: f32,
    pub primary_base: Color,
    pub success_base: Color,
    pub warning_base: Color,
    pub danger_base: Color,
    pub is_dark: bool,
    pub padding: f32,
    pub border_radius: f32,
    pub font_size: f32,
}
// const BORDER_RADIUS: f32 = 5.0;
// const FONT_SIZE: f32 = 12.0;

impl DesignSystem {
    /// Create a light mode design system.
    pub fn light() -> Self {
        Self {
            bg_base: Color::from_rgb8(248, 248, 248),
            text_base: Color::from_rgb8(0, 0, 0),
            text_lightness: 0.05,
            primary_base: Color::from_rgb8(0x18, 0x96, 0xC2),
            success_base: Color::from_rgb8(0x2D, 0x9D, 0x67),
            warning_base: Color::from_rgb8(0xE5, 0xA2, 0x23),
            danger_base: Color::from_rgb8(0xD7, 0x37, 0x45),
            padding: 5.,
            border_radius: 5.,
            font_size: 14.,
            is_dark: false,
        }
    }

    /// Create a dark mode design system.
    pub fn dark() -> Self {
        Self {
            bg_base: Color::from_rgb8(0x24, 0x24, 0x24),
            text_base: Color::from_rgb8(255, 255, 255),
            text_lightness: 0.95,
            primary_base: Color::from_rgb8(0x3A, 0xAA, 0xD8),
            success_base: Color::from_rgb8(0x4A, 0xBE, 0x8A),
            warning_base: Color::from_rgb8(0xF5, 0xB8, 0x4E),
            danger_base: Color::from_rgb8(0xF0, 0x56, 0x54),
            padding: 5.,
            border_radius: 5.,
            font_size: 14.,
            is_dark: true,
        }
    }

    // Background levels

    pub fn bg_base(&self) -> Color {
        self.bg_base
    }

    pub fn bg_elevated(&self) -> Color {
        let adjustment = 0.05;
        self.bg_base.map_lightness(|l| l + adjustment)
    }

    pub fn bg_overlay(&self) -> Color {
        let adjustment = 0.10;
        self.bg_base.map_lightness(|l| l + adjustment)
    }

    pub fn bg_disabled(&self) -> Color {
        let adjustment = if self.is_dark { -0.05 } else { -0.1 };
        self.bg_base.map_lightness(|l| l + adjustment)
    }

    // Border

    pub fn border(&self) -> Color {
        let adjustment = if self.is_dark { 0.15 } else { -0.15 };
        self.bg_base.map_lightness(|l| l + adjustment)
    }

    pub fn border_muted(&self) -> Color {
        let adjustment = if self.is_dark { 0.15 } else { -0.15 };
        self.border()
            .map_lightness(|l| l + adjustment)
            .with_alpha(0.8)
    }

    // Text

    pub fn text(&self) -> Color {
        self.text_base.map_lightness(|_| self.text_lightness)
    }

    pub fn text_muted(&self) -> Color {
        let adjustment = if self.is_dark { -0.25 } else { 0.25 };
        self.text_base
            .map_lightness(|l| l + adjustment)
            .with_alpha(0.5)
    }

    // Primary

    pub fn primary(&self) -> Color {
        self.primary_base
    }

    pub fn primary_muted(&self) -> Color {
        self.primary_base.map_lightness(|l| l - 0.05)
    }

    // Semantic colors

    pub fn success(&self) -> Color {
        self.success_base
    }

    pub fn warning(&self) -> Color {
        self.warning_base
    }

    pub fn danger(&self) -> Color {
        self.danger_base
    }

    pub fn info(&self) -> Color {
        self.primary_base
    }

    pub fn padding(&self) -> f32 {
        self.padding
    }

    pub fn border_radius(&self) -> f32 {
        self.border_radius
    }

    pub fn font_size(&self) -> f32 {
        self.font_size
    }
}

impl StylePropValue for DesignSystem {
    fn debug_view(&self) -> Option<AnyView> {
        use crate::prelude::*;

        let design_system = self.clone();
        let is_expanded = RwSignal::new(false);

        let color_swatch = |label: &str, color: Color| {
            stack((
                label.to_string().style(|s| s.width(120.0).font_size(12.0)),
                color.debug_view().unwrap(),
            ))
            .style(|s| s.flex_row().items_center().gap(8.0).padding_vert(2.0))
        };

        let scalar_field = |label: &str, value: f32| {
            stack((
                label.to_string().style(|s| s.width(120.0).font_size(12.0)),
                format!("{:.2}", value).style(|s| s.font_size(12.0)),
            ))
            .style(|s| s.flex_row().items_center().gap(8.0).padding_vert(2.0))
        };

        let chevron = move || {
            if is_expanded.get() {
                svg(
                    r#"<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M4.427 6.427l3.396 3.396a.25.25 0 00.354 0l3.396-3.396A.25.25 0 0011.396 6H4.604a.25.25 0 00-.177.427z"/></svg>"#,
                )
            } else {
                svg(
                    r#"<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M6.427 4.427l3.396 3.396a.25.25 0 010 .354l-3.396 3.396A.25.25 0 016 11.396V4.604a.25.25 0 01.427-.177z"/></svg>"#,
                )
            }.style(|s| s.size_full().with_theme(|s, t| s.color(t.text())))
        };

        let header = stack((
            dyn_view(chevron)
                .class(ButtonClass)
                .style(|s| s.size(16.0, 16.0).padding(0.)),
            "Design System"
                .to_string()
                .style(|s| s.font_size(14.0).font_weight(Weight::SEMIBOLD)),
        ))
        .on_click_stop(move |_| {
            is_expanded.update(|v| *v = !*v);
        })
        .style(|s| {
            s.flex_row()
                .items_center()
                .gap(8.0)
                .cursor(CursorStyle::Pointer)
        });

        let content = stack((
            header,
            stack((
                color_swatch("bg_base", design_system.bg_base),
                color_swatch("text_base", design_system.text_base),
                color_swatch("primary_base", design_system.primary_base),
                color_swatch("success_base", design_system.success_base),
                color_swatch("warning_base", design_system.warning_base),
                color_swatch("danger_base", design_system.danger_base),
                scalar_field("text_lightness", design_system.text_lightness),
                scalar_field("padding", design_system.padding),
                scalar_field("border_radius", design_system.border_radius),
                scalar_field("font_size", design_system.font_size),
                format!("is_dark: {}", design_system.is_dark).style(|s| s.font_size(12.0)),
            ))
            .style(move |s| s.flex_col().gap(4.0))
            .clip()
            .style(move |s| {
                s.height_pct(100.)
                    .apply_if(!is_expanded.get(), |s| s.height_pct(0.))
                    .transition_height(Transition::ease_in_out(Duration::from_millis(200)))
            }),
        ))
        .style(|s| {
            // this view here should be getting set to have a height of just the two children combined
            // I think this is a bug in taffy
            s.flex_col()
                .padding(8.0)
                .border(1.)
                .border_color(palette::css::WHITE.with_alpha(0.3))
                .border_radius(6.0)
                .min_width(280.0)
                .min_height_pct(0.)
                .flex_grow(0.)
                .flex_shrink(1.)
        });

        Some(content.into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        use peniko::color::HueDirection;
        let t = value as f32;
        let inv_t = 1.0 - t;

        Some(DesignSystem {
            bg_base: self.bg_base.lerp(other.bg_base, t, HueDirection::default()),
            text_base: self
                .text_base
                .lerp(other.text_base, t, HueDirection::default()),
            text_lightness: self.text_lightness * inv_t + other.text_lightness * t,
            primary_base: self
                .primary_base
                .lerp(other.primary_base, t, HueDirection::default()),
            success_base: self
                .success_base
                .lerp(other.success_base, t, HueDirection::default()),
            warning_base: self
                .warning_base
                .lerp(other.warning_base, t, HueDirection::default()),
            danger_base: self
                .danger_base
                .lerp(other.danger_base, t, HueDirection::default()),
            is_dark: if t < 0.5 { self.is_dark } else { other.is_dark },
            padding: self.padding * inv_t + other.padding * t,
            border_radius: self.border_radius * inv_t + other.border_radius * t,
            font_size: self.font_size * inv_t + other.font_size * t,
        })
    }
}

prop!(
    pub Theme: DesignSystem { inherited } = DesignSystem::light()
);
pub trait StyleThemeExt {
    fn theme(self, theme: DesignSystem) -> Self;
    fn with_theme(self, f: impl Fn(Self, &DesignSystem) -> Self + 'static) -> Self
    where
        Self: std::marker::Sized;
}

impl StyleThemeExt for Style {
    fn theme(self, theme: DesignSystem) -> Self {
        self.set(Theme, theme)
    }
    fn with_theme(self, f: impl Fn(Self, &DesignSystem) -> Self + 'static) -> Self {
        self.with_context::<Theme>(f)
    }
}

pub fn brighten_bg() -> Style {
    Style::new().with_context::<Background>(|s, bg| {
        s.apply_opt(
            bg.clone().and_then(|bg| {
                if let Brush::Solid(bg) = bg {
                    Some(bg)
                } else {
                    None
                }
            }),
            |s, bg| {
                s.background(bg.map_lightness(|l| l - 0.1))
                    .dark_mode(|s| s.background(bg.map_lightness(|l| l + 0.1)))
            },
        )
    })
}

pub fn hover_style() -> Style {
    Style::new().hover(|s| s.apply(brighten_bg()))
}

pub fn focus_applied_style() -> Style {
    Style::new().with_theme(|s, t| s.border_color(t.primary()))
}

pub fn focus_style() -> Style {
    let focus_visible_applied_style = Style::new().outline(3.0);

    Style::new()
        .with_theme(|s, t| s.outline_color(t.primary().with_alpha(0.5)))
        .focus(|_| focus_applied_style())
        .focus_visible(|_| focus_visible_applied_style.clone())
}

pub fn border_style(with_radius: bool) -> Style {
    Style::new()
        .with_theme(move |s, t| {
            s.border_color(t.border())
                .disabled(|s| s.border_color(t.border()))
                .padding(t.padding())
                .apply_if(with_radius, |s| s.border_radius(t.border_radius()))
        })
        .border(1.0)
        .apply(focus_style())
}

pub fn item_selected_style() -> Style {
    Style::new().selected(|s| {
        s.with_theme(|s, t| {
            s.background(t.primary())
                .color(t.bg_base)
                .hover(|s| s.background(t.primary_muted()))
        })
    })
}

pub(crate) fn default_theme(os_theme: winit::window::Theme) -> Style {
    let hover_style = hover_style();

    let button_style = Style::new()
        .custom_style_class(|s: LabelCustomStyle| s.selectable(false))
        .with_theme(|s, t| {
            s.background(t.bg_elevated())
                .padding(t.padding())
                .disabled(|s| s.background(t.bg_disabled()).color(t.text_muted()))
                .active(|s| s.background(t.primary()).color(t.bg_base()))
        })
        .transition(Background, Transition::linear(100.millis()))
        .focus(|s| s.with_theme(|s, t| s.hover(|s| s.background(t.bg_overlay()))))
        .justify_center()
        .items_center()
        .apply(focus_style())
        .apply(hover_style.clone())
        .apply(border_style(true));

    let checkbox_style = Style::new()
        .size(20, 20)
        .with_theme(|s, t| {
            s.background(t.bg_base())
                .active(|s| s.background(t.primary()))
                .disabled(|s| {
                    s.background(t.bg_elevated().with_alpha(0.3))
                        .color(t.text_muted())
                })
        })
        .transition(Background, Transition::linear(100.millis()))
        .focus(|s| s.with_theme(|s, t| s.hover(|s| s.background(t.bg_overlay()))))
        .apply(border_style(true))
        .apply(hover_style.clone())
        .apply(focus_style());

    let labeled_checkbox_style = Style::new()
        .with_theme(|s, t| {
            s.hover(|s| s.background(t.primary_muted().with_alpha(0.7)))
                .col_gap(t.padding())
                .padding(t.padding())
                .border_radius(t.border_radius())
                .active(|s| {
                    s.class(CheckboxClass, |s| s.background(t.primary()))
                        .background(t.primary())
                })
                .disabled(|s| {
                    s.color(t.text_muted()).class(CheckboxClass, |s| {
                        s.background(t.bg_disabled())
                            .color(t.text_muted())
                            .hover(|s| s.background(t.bg_elevated().with_alpha(0.3)))
                    })
                })
        })
        .transition(Background, Transition::linear(100.millis()))
        .focus(|s| {
            s.class(CheckboxClass, |_| focus_applied_style())
                .with_theme(|s, t| s.hover(|s| s.background(t.bg_overlay())))
        })
        .apply(hover_style.clone())
        .apply(focus_style());

    let radio_button_style = Style::new()
        .size(20, 20)
        .items_center()
        .justify_center()
        .with_theme(|s, t| {
            s.background(t.bg_base())
                .active(|s| s.background(t.primary()))
                .hover(|s| s.background(t.bg_elevated()))
                .disabled(|s| s.background(t.bg_elevated()).color(t.text_muted()))
        })
        .transition(Background, Transition::linear(100.millis()))
        .focus(|s| s.with_theme(|s, t| s.hover(|s| s.background(t.bg_overlay()))))
        .border_radius(100.pct())
        .apply(border_style(false))
        .apply(focus_style());

    let radio_button_dot_style = Style::new()
        .size(8, 8)
        .border_radius(100.0)
        .with_theme(|s, t| {
            s.background(t.text()).disabled(|s| {
                s.background(t.text_muted())
                    .hover(|s| s.background(t.text_muted()))
            })
        });

    let labeled_radio_button_style = Style::new()
        .with_theme(move |s, t| {
            s.col_gap(t.padding())
                .padding(t.padding())
                .border_radius(t.border_radius())
                // .apply(item_selected_style())
                .hover(|s| s.background(t.primary_muted().with_alpha(0.7)))
                .active(|s| s.class(RadioButtonClass, |s| s.apply(brighten_bg())))
                .selected(|s| s.disabled(|s| s.color(t.bg_elevated())))
                .disabled(|s| {
                    s.color(t.text_muted()).class(RadioButtonClass, |s| {
                        s.background(t.bg_disabled())
                            .color(t.text_muted())
                            .hover(|s| s.background(t.bg_elevated().with_alpha(0.3)))
                    })
                })
        })
        .transition(Background, Transition::linear(100.millis()))
        .focus(|s| {
            s.class(RadioButtonClass, |_| focus_applied_style())
                .with_theme(|s, t| s.hover(|s| s.background(t.primary().with_alpha(0.7))))
                .apply(focus_style())
        });

    let toggle_button_style = Style::new()
        .with_theme(|s, t| {
            s.background(t.bg_elevated())
                .with_context::<FontSize>(|s, fs| s.apply_opt(*fs, |s, fs| s.height(fs * 1.75)))
                .padding(t.padding())
                .set(Foreground, Brush::Solid(t.text_muted()))
                .active(|s| {
                    s.background(t.primary())
                        .color(t.bg_base())
                        .set(Foreground, Brush::Solid(t.bg_base()))
                })
                .hover(|s| s.background(t.bg_overlay()))
        })
        .aspect_ratio(2.)
        .border_radius(50.pct())
        .border(1.)
        .focus(|s| s.with_theme(|s, t| s.hover(|s| s.background(t.bg_overlay()))))
        .set(ToggleButtonCircleRad, 75.pct())
        .set(ToggleButtonInset, 10.pct())
        .apply(border_style(true))
        .apply(focus_style());

    let input_style = Style::new()
        .with_theme(|s, t| {
            s.background(t.bg_base())
                .padding(t.padding())
                .cursor_color(t.primary_muted().with_alpha(0.5))
                .hover(|s| s.background(t.bg_elevated()))
                .disabled(|s| s.background(t.bg_disabled()).color(t.text_muted()))
        })
        .focus(|s| s.with_theme(|s, t| s.hover(|s| s.background(t.bg_overlay()))))
        .apply(border_style(true))
        .apply(focus_style())
        .cursor(CursorStyle::Text);

    let tab_selector_style = Style::new()
        .custom_style_class(|s: LabelCustomStyle| s.selectable(false))
        .with_theme(|s, t| {
            s.background(t.bg_base())
                .padding(t.padding())
                .color(t.text_muted())
                .border_bottom(2.)
                .border_color(Color::TRANSPARENT)
                .disabled(|s| s.background(t.bg_disabled()).color(t.text_muted()))
                .selected(|s| {
                    s.background(t.bg_elevated())
                        .color(t.text())
                        .border_color(t.primary())
                })
                .hover(|s| s.background(t.bg_elevated()).color(t.text()))
        })
        .transition(Background, Transition::linear(100.millis()))
        .transition(Foreground, Transition::linear(100.millis()))
        .justify_center()
        .items_center()
        .apply(focus_style())
        .apply(hover_style.clone());

    // let item_unfocused_style = Style::new().with_theme(|s, t| {
    //     s.hover(|s| s.background(t.bg_elevated())).selected(|s| {
    //         s.background(t.bg_elevated())
    //             .hover(|s| s.background(t.bg_overlay()))
    //     })
    // });

    Style::new()
        .apply_if(os_theme == winit::window::Theme::Light, |s| {
            let light = DesignSystem::light();
            s.color(light.text())
                .font_size(light.font_size())
                .background(light.bg_base())
                .color(light.text())
                .theme(light)
        })
        .apply_if(os_theme == winit::window::Theme::Dark, |s| {
            let dark = DesignSystem::dark();
            s.color(dark.text())
                .font_size(dark.font_size())
                .background(dark.bg_base())
                .color(dark.text())
                .theme(dark)
        })
        .class(LabelClass, |s| {
            s.with_theme(|s, t| {
                s.custom(|s: LabelCustomStyle| s.selection_color(t.primary_muted().with_alpha(0.5)))
            })
        })
        .class(ListClass, |s| {
            s.apply(focus_style()).class(ListItemClass, |s| {
                s.apply(hover_style.clone())
                    .apply(item_selected_style())
                    .with_theme(|s, t| s.border_radius(t.border_radius()))
            })
        })
        .class(LabeledCheckboxClass, |_| labeled_checkbox_style)
        .class(CheckboxClass, |_| checkbox_style)
        .class(RadioButtonClass, |_| radio_button_style)
        .class(RadioButtonDotClass, |_| radio_button_dot_style)
        .class(LabeledRadioButtonClass, |_| labeled_radio_button_style)
        .class(TextInputClass, |_| input_style)
        .class(ButtonClass, |_| button_style)
        .class(TabSelectorClass, |_| tab_selector_style)
        .custom_style_class(|s: scroll::ScrollCustomStyle| {
            s.handle_border_radius(4.0)
                .handle_thickness(16.0)
                .handle_rounded(false)
                .apply_if(cfg!(target_os = "macos"), |s| {
                    s.handle_rounded(true).handle_thickness(10)
                })
        })
        .class(scroll::Handle, |s| {
            s.with_theme(|s, t| {
                s.background(t.border())
                    .active(|s| s.background(t.text_muted()))
                    .hover(|s| s.background(t.text_muted()))
            })
        })
        .class(scroll::Track, |s| {
            s.with_theme(|s, t| s.hover(|s| s.background(t.border().with_alpha(0.3))))
        })
        .class(ToggleButtonClass, |_| toggle_button_style)
        .class(SliderClass, |s| {
            s.with_theme(|s, t| {
                s.custom(|cs: SliderCustomStyle| {
                    cs.bar_radius(100.pct())
                        .accent_bar_radius(100.pct())
                        .handle_radius(100.pct())
                        .edge_align(true)
                        .bar_color(t.border())
                        .accent_bar_color(t.primary())
                        .handle_color(Brush::Solid(t.text()))
                })
            })
        })
        .class(PlaceholderTextClass, |s| {
            s.with_theme(|s, t| {
                s.color(t.text_muted()).disabled(|s| {
                    s.color(t.text_muted().with_alpha(0.5))
                        .background(css::BLACK)
                })
            })
        })
        .class(TooltipClass, |s| {
            s.with_theme(|s, t| {
                s.border_color(t.border())
                    .padding(t.padding())
                    .color(t.text())
                    .background(t.bg_elevated())
                    .box_shadow_color(t.text().with_alpha(0.2))
            })
            .border(0.5)
            .border_radius(2.0)
            .margin(10.0)
            .box_shadow_blur(2.0)
            .box_shadow_h_offset(2.0)
            .box_shadow_v_offset(2.0)
        })
        .class(dropdown::DropdownClass, move |s| {
            s.min_width(75)
                .padding(3)
                .apply(border_style(true))
                .class(SvgClass, |s| {
                    s.with_theme(|s, t| {
                        s.hover(|s| s.background(t.primary_muted()))
                            .padding(5.)
                            .border_radius(t.border_radius())
                            .color(t.text())
                    })
                    .size(18, 18)
                })
                .class(scroll::ScrollClass, |s| {
                    s.width_full()
                        .margin_top(3)
                        .padding_vert(3)
                        .with_theme(|s, t| {
                            s.background(t.bg_elevated())
                                .box_shadow_color(t.text().with_alpha(0.4))
                        })
                        .box_shadow_blur(2.0)
                        .box_shadow_h_offset(2.0)
                        .box_shadow_v_offset(2.0)
                        .border_radius(5.pct())
                        .items_center()
                        .class(ListItemClass, |s| {
                            s.padding(6)
                                .items_center()
                                .apply(hover_style)
                                .hover(|s| s.apply(brighten_bg()))
                        })
                })
        })
        .class(ResizableClass, |s| {
            s.with_theme(|s, t| {
                s.custom(|cs: ResizableCustomStyle| {
                    cs.handle_thickness(3.)
                        .handle_color(t.primary_muted().with_alpha(0.5))
                        .hover(|s| s.handle_color(t.primary()))
                })
            })
        })
}
