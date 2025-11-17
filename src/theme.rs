use std::time::Duration;

use crate::{
    AnyView, prop,
    style::{
        Background, BoxShadow, CursorStyle, CustomStyle, FontSize, Foreground, Style,
        StylePropValue, Transition,
    },
    style_class,
    unit::{DurationUnitExt, UnitExt},
    views::{
        ButtonClass, CheckboxClass, LabelClass, LabelCustomStyle, LabeledCheckboxClass,
        LabeledRadioButtonClass, ListClass, ListItemClass, PlaceholderTextClass, RadioButtonClass,
        RadioButtonDotClass, SvgClass, TabSelectorClass, TextInputClass, ToggleButtonCircleRad,
        ToggleButtonClass, ToggleButtonInset, TooltipClass, dropdown,
        resizable::{ResizableClass, ResizableCustomStyle},
        scroll,
        slider::{SliderClass, SliderCustomStyle},
    },
};
use floem_renderer::text::Weight;
use peniko::{Brush, Color, color::palette::css};
use smallvec::smallvec;

style_class!(pub HoverTargetClass);

/// A theme builder. Create your own or use provided default [light](DesignSystem::light)
/// and [dark](DesignSystem::dark) default themes.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignSystem {
    /// The base background color.
    pub bg_base: Color,
    /// Lightness of the background elevation from 0-1.
    pub bg_elevate: f32,
    /// Lightness of the background overlays from 0-1.
    pub bg_overlay: f32,
    /// Lightness of the disabled elements (0-1) based on background.
    pub disabled: f32,

    /// Base text color.
    pub text_base: Color,
    /// Lightness of the text color (0-1).
    pub text_lightness: f32,
    /// Lightness of the text color (0-1) when muted.
    pub text_muted: f32,
    /// Size of the font.
    pub font_size: f32,

    /// The primary theme accent color.
    pub primary_base: Color,
    /// The success theme accent color.
    pub success_base: Color,
    /// The warning theme accent color.
    pub warning_base: Color,
    /// The danger theme accent color.
    pub danger_base: Color,

    /// Lightness of the border (0-1) based on background.
    pub border: f32,
    /// Theme border radius.
    pub border_radius: f32,
    /// Default theme padding.
    pub padding: f32,
    /// Is the theme a dark variant.
    pub is_dark: bool,
}

impl DesignSystem {
    /// Create a default light mode design system.
    pub fn light() -> Self {
        Self {
            bg_base: hsl(0., 0., 97.),
            text_base: hsl(0., 0., 0.),
            text_lightness: 0.05,
            primary_base: Color::from_rgb8(24, 150, 194),
            success_base: Color::from_rgb8(45, 157, 103),
            warning_base: Color::from_rgb8(229, 162, 35),
            danger_base: Color::from_rgb8(215, 55, 69),
            padding: 5.,
            border_radius: 5.,
            font_size: 14.,
            is_dark: false,
            bg_elevate: -0.03,
            bg_overlay: 0.1,
            border: -0.15,
            disabled: -0.1,
            text_muted: 0.25,
        }
    }

    /// Create a default dark mode design system.
    pub fn dark() -> Self {
        Self {
            bg_base: hsl(0., 0., 15.),
            text_base: hsl(0., 0., 100.),
            text_lightness: 0.95,
            primary_base: Color::from_rgb8(58, 170, 216),
            success_base: Color::from_rgb8(74, 190, 138),
            warning_base: Color::from_rgb8(245, 184, 78),
            danger_base: Color::from_rgb8(240, 86, 84),
            padding: 5.,
            border_radius: 5.,
            font_size: 14.,
            is_dark: true,
            bg_elevate: 0.05,
            bg_overlay: 0.1,
            border: 0.15,
            disabled: -0.05,
            text_muted: -0.25,
        }
    }

    // Background levels

    /// The base background theme color.
    pub const fn bg_base(&self) -> Color {
        self.bg_base
    }

    /// The theme background elevated color.
    pub fn bg_elevated(&self) -> Color {
        self.bg_base.map_lightness(|c| c + self.bg_elevate)
    }

    /// The theme background overlay color.
    pub fn bg_overlay(&self) -> Color {
        self.bg_base.map_lightness(|c| c + self.bg_overlay)
    }

    /// The theme background overlay color for disabled elements.
    pub fn bg_disabled(&self) -> Color {
        self.bg_base.map_lightness(|c| c + self.disabled)
    }

    // Border

    /// The theme border color.
    pub fn border(&self) -> Color {
        self.bg_base.map_lightness(|c| c + self.border)
    }

    /// The theme muted border color.
    pub fn border_muted(&self) -> Color {
        self.border()
            .map_lightness(|c| c + self.border)
            .with_alpha(0.8)
    }

    // Text

    /// The theme text color.
    pub fn text(&self) -> Color {
        self.text_base.map_lightness(|_| self.text_lightness)
    }

    /// The theme muted text color.
    pub fn text_muted(&self) -> Color {
        self.text_base
            .map_lightness(|c| c + self.text_muted)
            .with_alpha(0.5)
    }

    // Primary

    /// The primary theme accent color.
    pub const fn primary(&self) -> Color {
        self.primary_base
    }

    /// The muted primary theme accent color.
    pub fn primary_muted(&self) -> Color {
        self.primary_base.map_lightness(|c| c - 0.05)
    }

    // Semantic colors

    /// The success theme accent color.
    pub const fn success(&self) -> Color {
        self.success_base
    }

    /// The warning theme accent color.
    pub const fn warning(&self) -> Color {
        self.warning_base
    }

    /// The danger theme accent color.
    pub const fn danger(&self) -> Color {
        self.danger_base
    }

    /// The info theme accent color.
    pub const fn info(&self) -> Color {
        self.primary_base
    }

    /// The theme default padding.
    pub const fn padding(&self) -> f32 {
        self.padding
    }

    /// The theme default border radius.
    pub const fn border_radius(&self) -> f32 {
        self.border_radius
    }

    /// The theme default font size.
    pub const fn font_size(&self) -> f32 {
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
            // This view here should be getting set to have a height of just the two
            // children combined, I think this is a bug in taffy.
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
            bg_elevate: self.bg_elevate * inv_t + other.bg_elevate * t,
            bg_overlay: self.bg_overlay * inv_t + other.bg_overlay * t,
            disabled: self.disabled * inv_t + other.disabled * t,
            text_muted: self.text_muted * inv_t + other.text_muted * t,
            border: self.border + inv_t + other.border * t,
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

pub fn hover_style() -> Style {
    Style::new().hover(|s| s.apply(Style::new().with_theme(|s, t| s.background(t.bg_elevated()))))
}

pub fn focus_style() -> Style {
    let focus_visible_applied_style = Style::new().outline(3.0);

    Style::new()
        .focusable(true)
        .with_theme(|s, t| s.outline_color(t.primary().with_alpha(0.5)))
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
}

pub fn item_selected_style() -> Style {
    Style::new().selected(|s| {
        s.with_theme(|s, t| {
            s.background(t.primary())
                .color(t.bg_base)
                .hover(|s| s.background(t.primary_muted()))
        })
        .transition_background(Transition::linear(100.millis()))
    })
}

pub fn overlay_style() -> Style {
    Style::new()
        .with_theme(|s, t| {
            let shadow_color = Color::from_rgb8(0, 0, 0);
            let base_opacity = if t.is_dark { 0.7 } else { 0.18 };

            s.border_color(t.border())
                .border_radius(t.border_radius())
                .padding(t.padding())
                .color(t.text())
                .background(t.bg_overlay())
                .apply_box_shadows(smallvec![
                    // Small, tight shadow for definition at the edge
                    BoxShadow::new()
                        .color(shadow_color.with_alpha(base_opacity * 1.2))
                        .v_offset(1.)
                        .blur_radius(2.)
                        .spread(0.),
                    // Medium shadow for perceived elevation
                    BoxShadow::new()
                        .color(shadow_color.with_alpha(base_opacity * 0.8))
                        .v_offset(4.)
                        .blur_radius(8.)
                        .spread(-1.),
                    // Large, soft shadow for ambient depth
                    BoxShadow::new()
                        .color(shadow_color.with_alpha(base_opacity * 0.5))
                        .v_offset(12.)
                        .blur_radius(24.)
                        .spread(-4.),
                ])
        })
        .dark_mode(|s| s.border(1).border_top(2.))
}

pub(crate) fn default_theme(os_theme: winit::window::Theme) -> Style {
    let button_style = Style::new()
        .custom_style_class(|s: LabelCustomStyle| s.selectable(false))
        .with_theme(|s, t| {
            s.background(t.bg_elevated())
                .padding(t.padding())
                .disabled(|s| s.background(t.bg_disabled()).color(t.text_muted()))
                .active(|s| s.background(t.bg_elevated()))
        })
        .transition(Background, Transition::linear(100.millis()))
        .justify_center()
        .items_center()
        .hover(|s| s.with_theme(|s, t| s.background(t.bg_overlay())))
        .apply(focus_style())
        .apply(border_style(true));

    let checkbox_style = Style::new()
        .size(20, 20)
        .with_theme(|s, t| {
            s.background(t.bg_base())
                .active(|s| s.background(t.bg_elevated()))
                .disabled(|s| {
                    s.background(t.bg_elevated().with_alpha(0.3))
                        .color(t.text_muted())
                })
        })
        .transition(Background, Transition::linear(100.millis()))
        .focus(|s| s.with_theme(|s, t| s.hover(|s| s.background(t.bg_overlay()))))
        .apply(border_style(true))
        .apply(hover_style())
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
        .class(CheckboxClass, |s| s.focusable(false))
        .focus(|s| {
            s.class(CheckboxClass, |s| {
                s.with_theme(|s, t| s.border_color(t.primary()))
            })
            .with_theme(|s, t| s.hover(|s| s.background(t.bg_overlay())))
        })
        .apply(hover_style())
        .apply(focus_style());

    let radio_button_style = Style::new()
        .size(20, 20)
        .items_center()
        .justify_center()
        .with_theme(|s, t| {
            s.background(t.bg_base())
                .active(|s| s.background(t.bg_base()))
                .hover(|s| s.background(t.bg_elevated()))
                .disabled(|s| s.background(t.bg_disabled()).color(t.text_muted()))
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
                .hover(|s| s.background(t.primary_muted().with_alpha(0.7)))
                .active(|s| {
                    s.class(RadioButtonClass, |s| {
                        s.apply(Style::new().with_theme(|s, t| s.background(t.bg_elevated())))
                    })
                })
                .selected(|s| s.disabled(|s| s.color(t.bg_elevated())))
                .disabled(|s| s.color(t.text_muted()))
        })
        .class(RadioButtonClass, |s| s.focusable(false))
        .transition(Background, Transition::linear(100.millis()))
        .focus(|s| {
            s.with_theme(|s, t| s.hover(|s| s.background(t.primary().with_alpha(0.7))))
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
        .border(1.)
        // .focus(|s| s.with_theme(|s, t| s.hover(|s| s.background(t.bg_overlay()))))
        .border_radius(50.pct())
        .set(ToggleButtonCircleRad, 75.pct())
        .set(ToggleButtonInset, 10.pct())
        .apply(border_style(false))
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
        .apply(hover_style());

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
                s.with_theme(|s, t| {
                    s.hover(|s| s.background(t.bg_elevated())).selected(|s| {
                        s.background(t.primary())
                            .color(t.bg_base)
                            .hover(|s| s.background(t.primary_muted()))
                            .transition_background(Transition::linear(100.millis()))
                    })
                })
                .with_theme(|s, t| s.border_radius(t.border_radius()).padding_left(t.padding()))
                .items_center()
            })
        })
        .class(CheckboxClass, |_| checkbox_style)
        .class(LabeledCheckboxClass, |_| labeled_checkbox_style)
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
            s.apply(focus_style()).with_theme(|s, t| {
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
        .class(TooltipClass, |s| s.apply(overlay_style()))
        .class(dropdown::DropdownClass, move |s| {
            s.min_width(75)
                .padding(3)
                .apply(focus_style())
                .apply(border_style(true))
                .class(SvgClass, |s| {
                    s.with_theme(|s, t| {
                        s.hover(|s| s.background(t.bg_elevated()))
                            .padding(5.)
                            .border_radius(t.border_radius())
                            .color(t.text())
                    })
                    .size(18, 18)
                })
                .class(scroll::ScrollClass, move |s| {
                    s.width_full()
                        .margin_top(3)
                        .padding_vert(3)
                        .apply(overlay_style())
                        .items_center()
                        .class(ListItemClass, move |s| {
                            s.padding(6).with_theme(|s, t| {
                                s.hover(|s| {
                                    s.background(t.bg_elevated())
                                        .selected(|s| s.background(t.primary_muted()))
                                })
                            })
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
        .class(HoverTargetClass, |s| {
            s.with_theme(|s, t| {
                s.padding(t.padding())
                    .border_radius(t.border_radius())
                    .cursor(CursorStyle::Pointer)
                    .background(t.bg_elevated())
                    .outline(3)
                    // TODO: implement file hover in event handling
                    .file_hover(|s| s.background(t.bg_overlay()).outline_color(t.primary()))
            })
            .transition(Background, Transition::linear(100.millis()))
        })
}

/// Contruct sRGB [Color] from HSL values.
pub const fn hsl(h: f32, s: f32, l: f32) -> Color {
    let sat = s * 0.01;
    let light = l * 0.01;
    let a = sat * light.min(1.0 - light);

    let hue = transform(0., h, light, a);
    let sat = transform(8., h, light, a);
    let lum = transform(4., h, light, a);
    Color::new([hue, sat, lum, 1.])
}

const fn transform(n: f32, h: f32, light: f32, a: f32) -> f32 {
    let x = n + h * (1.0 / 30.0);
    let k = x - 12.0 * (x * (1.0 / 12.0)).floor();
    light - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
}

#[test]
fn rgb_hsl_conversion() {
    let rgb_bg_base = Color::from_rgb8(242, 242, 242);
    let hsl_bg_base = hsl(0., 0., 95.);
    assert_eq!(rgb_bg_base.to_rgba8(), hsl_bg_base.to_rgba8());

    let rgb_text_base = Color::from_rgb8(0, 0, 0);
    let hsl_text_base = hsl(0., 0., 0.);
    assert_eq!(rgb_text_base.to_rgba8(), hsl_text_base.to_rgba8());

    let rgb_text_lightness = Color::from_rgb8(0, 0, 0);
    let hsl_text_lightness = hsl(0., 0., 0.);
    assert_eq!(rgb_text_lightness.to_rgba8(), hsl_text_lightness.to_rgba8());

    let rgb_primary_base = Color::from_rgb8(24, 146, 191);
    let hsl_primary_base = hsl(196., 78., 42.);
    assert_eq!(rgb_primary_base.to_rgba8(), hsl_primary_base.to_rgba8());

    let rgb_success_base = Color::from_rgb8(46, 158, 104);
    let hsl_success_base = hsl(151., 55., 40.);
    assert_eq!(rgb_success_base.to_rgba8(), hsl_success_base.to_rgba8());

    let rgb_warning_base = Color::from_rgb8(229, 162, 36);
    let hsl_warning_base = hsl(39., 79., 52.);
    assert_eq!(rgb_warning_base.to_rgba8(), hsl_warning_base.to_rgba8());

    let rgb_danger_base = Color::from_rgb8(215, 55, 68);
    let hsl_danger_base = hsl(355., 67., 53.);
    assert_eq!(rgb_danger_base.to_rgba8(), hsl_danger_base.to_rgba8());
}
