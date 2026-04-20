use std::time::Duration;

use super::unit::{DurationUnitExt, UnitExt};
use super::*;
use crate::style::Selectable;
use crate::view::View;
use crate::views::editor::SelectionColor;
use crate::views::resizable::{ResizableClass, ResizableHandleClass};
use crate::{
    prop, style_class, style_debug_group,
    views::{
        ButtonClass, CheckboxClass, LabelClass, LabelCustomExprStyle, LabelCustomStyle,
        LabeledCheckboxClass, LabeledRadioButtonClass, ListClass, ListItemClass,
        PlaceholderTextClass, RadioButtonClass, RadioButtonDotClass, SvgClass, TabSelectorClass,
        TextInputClass, ToggleButtonCircleRad, ToggleButtonClass, ToggleButtonInset, TooltipClass,
        dropdown,
        resizable::{ResizableCustomExprStyle, ResizableCustomStyle},
        scroll,
        slider::{SliderClass, SliderCustomExprStyle, SliderCustomStyle},
    },
};
use peniko::{Brush, Color, color::palette::css};
use smallvec::smallvec;

style_class!(pub HoverTargetClass);

fn debug_view_of<T: PropDebugView>(value: T) -> Option<Box<dyn View>> {
    value
        .debug_view(&FloemInspectorRender)
        .and_then(|any| any.downcast::<Box<dyn View>>().ok().map(|b| *b))
}

fn border_debug_view(style: &Style) -> Option<Box<dyn View>> {
    debug_view_of(Border {
        left: style.get_prop::<BorderLeft>(),
        top: style.get_prop::<BorderTop>(),
        right: style.get_prop::<BorderRight>(),
        bottom: style.get_prop::<BorderBottom>(),
    })
}

fn border_color_debug_view(style: &Style) -> Option<Box<dyn View>> {
    debug_view_of(BorderColor {
        left: style.get_prop::<BorderLeftColor>().flatten(),
        top: style.get_prop::<BorderTopColor>().flatten(),
        right: style.get_prop::<BorderRightColor>().flatten(),
        bottom: style.get_prop::<BorderBottomColor>().flatten(),
    })
}

fn border_radius_debug_view(style: &Style) -> Option<Box<dyn View>> {
    debug_view_of(BorderRadius {
        top_left: style.get_prop::<BorderTopLeftRadius>(),
        top_right: style.get_prop::<BorderTopRightRadius>(),
        bottom_left: style.get_prop::<BorderBottomLeftRadius>(),
        bottom_right: style.get_prop::<BorderBottomRightRadius>(),
    })
}

fn padding_debug_view(style: &Style) -> Option<Box<dyn View>> {
    debug_view_of(Padding {
        left: style.get_prop::<PaddingLeft>(),
        top: style.get_prop::<PaddingTop>(),
        right: style.get_prop::<PaddingRight>(),
        bottom: style.get_prop::<PaddingBottom>(),
    })
}

fn margin_debug_view(style: &Style) -> Option<Box<dyn View>> {
    debug_view_of(Margin {
        left: style.get_prop::<MarginLeft>(),
        top: style.get_prop::<MarginTop>(),
        right: style.get_prop::<MarginRight>(),
        bottom: style.get_prop::<MarginBottom>(),
    })
}

style_debug_group!(
    pub BorderDebugGroup,
    inherited = inherited,
    members = [BorderLeft, BorderTop, BorderRight, BorderBottom],
    view = border_debug_view
);
style_debug_group!(
    pub BorderColorDebugGroup,
    inherited = inherited,
    members = [BorderLeftColor, BorderTopColor, BorderRightColor, BorderBottomColor],
    view = border_color_debug_view
);
style_debug_group!(
    pub BorderRadiusDebugGroup,
    inherited = inherited,
    members = [BorderTopLeftRadius, BorderTopRightRadius, BorderBottomLeftRadius, BorderBottomRightRadius],
    view = border_radius_debug_view
);
style_debug_group!(
    pub PaddingDebugGroup,
    inherited = inherited,
    members = [PaddingLeft, PaddingTop, PaddingRight, PaddingBottom],
    view = padding_debug_view
);
style_debug_group!(
    pub MarginDebugGroup,
    inherited = inherited,
    members = [MarginLeft, MarginTop, MarginRight, MarginBottom],
    view = margin_debug_view
);

pub use floem_style::DesignSystem;

prop!(
    pub Theme: DesignSystem { inherited } = DesignSystem::light()
);

#[derive(Clone, Copy)]
pub struct ThemeExpr(pub(crate) ContextRef<Theme>);

impl ThemeExpr {
    pub fn def<T>(self, f: impl Fn(DesignSystem) -> T + 'static) -> ContextValue<T>
    where
        T: 'static,
    {
        self.0.def(f)
    }

    pub fn bg_base(self) -> ContextValue<Color> {
        self.def(|t| t.bg_base())
    }
    pub fn bg_elevated(self) -> ContextValue<Color> {
        self.def(|t| t.bg_elevated())
    }
    pub fn bg_overlay(self) -> ContextValue<Color> {
        self.def(|t| t.bg_overlay())
    }
    pub fn bg_disabled(self) -> ContextValue<Color> {
        self.def(|t| t.bg_disabled())
    }
    pub fn border(self) -> ContextValue<Color> {
        self.def(|t| t.border())
    }
    pub fn border_muted(self) -> ContextValue<Color> {
        self.def(|t| t.border_muted())
    }
    pub fn text(self) -> ContextValue<Color> {
        self.def(|t| t.text())
    }
    pub fn text_muted(self) -> ContextValue<Color> {
        self.def(|t| t.text_muted())
    }
    pub fn primary(self) -> ContextValue<Color> {
        self.def(|t| t.primary())
    }
    pub fn primary_muted(self) -> ContextValue<Color> {
        self.def(|t| t.primary_muted())
    }
    pub fn success(self) -> ContextValue<Color> {
        self.def(|t| t.success())
    }
    pub fn warning(self) -> ContextValue<Color> {
        self.def(|t| t.warning())
    }
    pub fn danger(self) -> ContextValue<Color> {
        self.def(|t| t.danger())
    }
    pub fn info(self) -> ContextValue<Color> {
        self.def(|t| t.info())
    }
    pub fn padding(self) -> ContextValue<Length> {
        self.def(|t| t.padding().into())
    }
    pub fn border_radius(self) -> ContextValue<Length> {
        self.def(|t| t.border_radius().into())
    }
    pub fn font_size(self) -> ContextValue<f64> {
        self.def(|t| t.font_size())
    }
    pub fn is_dark(self) -> ContextValue<bool> {
        self.def(|t| t.is_dark)
    }
    pub fn warning_base(self) -> ContextValue<Color> {
        self.def(|t| t.warning_base)
    }
}

pub trait StyleThemeExt {
    fn theme(self, theme: DesignSystem) -> Self;
    fn with_theme(self, f: impl Fn(ExprStyle, ThemeExpr) -> ExprStyle + 'static) -> Self
    where
        Self: std::marker::Sized;
}

impl StyleThemeExt for Style {
    fn theme(self, theme: DesignSystem) -> Self {
        self.set(Theme, theme)
    }
    fn with_theme(self, f: impl Fn(ExprStyle, ThemeExpr) -> ExprStyle + 'static) -> Self {
        self.with::<Theme>(|s, t| f(s, ThemeExpr(t)))
    }
}

impl StyleThemeExt for ExprStyle {
    fn theme(self, theme: DesignSystem) -> Self {
        self.set(Theme, theme)
    }
    fn with_theme(self, f: impl Fn(ExprStyle, ThemeExpr) -> ExprStyle + 'static) -> Self {
        self.with::<Theme>(|s, t| f(s, ThemeExpr(t)))
    }
}

// pub fn hover_style() -> Style {
//     Style::new().hover(|s| s.with::<Theme>(|s, t| s.translate_x(t.def(|t| t.padding))))
// }
pub fn hover_style() -> Style {
    Style::new().hover(|s| s.with_theme(|s, t| s.background(t.def(|t| t.bg_elevated()))))
}

pub fn focus_style() -> Style {
    let focus_visible_applied_style = Style::new().outline(3.0);

    Style::new()
        .keyboard_navigable()
        .with::<Theme>(|s, t| s.outline_color(t.def(|t| t.primary().with_alpha(0.5))))
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
                .color(t.bg_base())
                .hover(|s| s.background(t.primary_muted()))
        })
        .transition_background(Transition::linear(100.millis()))
    })
}

pub fn overlay_style() -> Style {
    Style::new()
        .with_theme(|s, t| {
            let shadow_color = Color::from_rgb8(0, 0, 0);
            s.border_color(t.border())
                .border_radius(t.border_radius())
                .padding(t.padding())
                .color(t.text())
                .background(t.bg_overlay())
                .set_context(
                    BoxShadowProp,
                    t.def(move |theme| {
                        let base_opacity = if theme.is_dark { 0.7 } else { 0.18 };
                        smallvec![
                            BoxShadow::new()
                                .color(shadow_color.with_alpha(base_opacity * 1.2))
                                .v_offset(1.)
                                .blur_radius(2.)
                                .spread(0.),
                            BoxShadow::new()
                                .color(shadow_color.with_alpha(base_opacity * 0.8))
                                .v_offset(4.)
                                .blur_radius(8.)
                                .spread(-1.),
                            BoxShadow::new()
                                .color(shadow_color.with_alpha(base_opacity * 0.5))
                                .v_offset(12.)
                                .blur_radius(24.)
                                .spread(-4.),
                        ]
                    }),
                )
        })
        .dark_mode(|s| s.border(1).border_top(2.))
}

pub(crate) fn default_theme(os_theme: winit::window::Theme) -> Style {
    let button_style = Style::new()
        .selectable(false)
        .with_theme(|s, t| {
            s.background(t.bg_elevated())
                .padding(t.padding())
                .disabled(|s| {
                    s.background(t.bg_disabled())
                        .color(t.text_muted())
                        .unset_cursor()
                })
                .hover(|s| s.background(t.bg_overlay()))
                .active(move |s| {
                    s.background(t.def(|theme| {
                        let adjustment = if theme.is_dark { 0.1 } else { -0.2 };
                        Brush::Solid(theme.bg_overlay().map_lightness(|l| l + adjustment))
                    }))
                })
        })
        .transition(Background, Transition::linear(100.millis()))
        .justify_center()
        .items_center()
        .cursor(CursorStyle::Pointer)
        .apply(focus_style())
        .apply(border_style(true));

    let checkbox_style = Style::new()
        .size(20, 20)
        .with_theme(|s, t| {
            s.background(t.bg_base())
                .active(|s| s.background(t.bg_elevated()))
                .disabled(|s| {
                    s.background(t.def(|t| t.bg_elevated().with_alpha(0.3)))
                        .color(t.text_muted())
                        .unset_cursor()
                })
        })
        .transition(Background, Transition::linear(100.millis()))
        .focus(|s| s.with_theme(|s, t| s.hover(|s| s.background(t.bg_overlay()))))
        .cursor(CursorStyle::Pointer)
        .apply(border_style(true))
        .apply(hover_style())
        .apply(focus_style());

    let labeled_checkbox_style = Style::new()
        .with_theme(|s, t| {
            s.hover(|s| s.background(t.def(|t| t.primary_muted().with_alpha(0.7))))
                .col_gap(t.padding())
                .padding(t.padding())
                .border_radius(t.border_radius())
                .active(|s| {
                    s.class(CheckboxClass, |s| s.background(t.primary()))
                        .background(t.primary())
                })
                .disabled(|s| {
                    s.unset_cursor()
                        .color(t.text_muted())
                        .class(CheckboxClass, |s| {
                            s.background(t.bg_disabled())
                                .color(t.text_muted())
                                .hover(|s| s.background(t.def(|t| t.bg_elevated().with_alpha(0.3))))
                        })
                })
        })
        .cursor(CursorStyle::Pointer)
        .transition(Background, Transition::linear(100.millis()))
        .class(CheckboxClass, |s| s.focus_none())
        .selectable(false)
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
                .disabled(|s| {
                    s.background(t.bg_disabled())
                        .color(t.text_muted())
                        .unset_cursor()
                })
        })
        .cursor(CursorStyle::Pointer)
        .transition(Background, Transition::linear(100.millis()))
        .focus(|s| s.with_theme(|s, t| s.hover(|s| s.background(t.bg_overlay()))))
        .border_radius(100.pct())
        .flex_shrink(0.)
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
                .set(Selectable, false)
                .border_radius(t.border_radius())
                .hover(|s| s.background(t.def(|t| t.primary_muted().with_alpha(0.7))))
                .active(|s| s.class(RadioButtonClass, |s| s.background(t.bg_elevated())))
                .selected(|s| s.disabled(|s| s.color(t.bg_elevated())))
                .disabled(|s| s.color(t.text_muted()).unset_cursor())
        })
        .cursor(CursorStyle::Pointer)
        .class(RadioButtonClass, |s| s.focus_none())
        .transition(Background, Transition::linear(100.millis()))
        .focus(|s| {
            s.with_theme(|s, t| s.hover(|s| s.background(t.def(|t| t.primary().with_alpha(0.7)))))
        });

    let toggle_button_style = Style::new()
        .height(1.75.em())
        .with_theme(|s, t| {
            s.background(t.bg_elevated())
                .padding(t.padding())
                .set_context_opt(Foreground, t.def(|t| Some(Brush::Solid(t.text_muted()))))
                .active(|s| {
                    s.background(t.primary())
                        .color(t.bg_base())
                        .set_context_opt(Foreground, t.def(|t| Some(Brush::Solid(t.bg_base()))))
                })
                .hover(|s| s.background(t.bg_overlay()))
        })
        .aspect_ratio(2.)
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
                .set_context(
                    SelectionColor,
                    t.def(|t| Brush::Solid(t.primary_muted().with_alpha(0.5))),
                )
                .cursor_color(t.primary_muted())
                .hover(|s| s.background(t.bg_elevated()))
                .disabled(|s| {
                    s.background(t.bg_disabled())
                        .color(t.text_muted())
                        .unset_cursor()
                })
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
                .disabled(|s| s.background(t.bg_disabled()).color(t.text_muted()))
                .selected(|s| {
                    s.background(t.bg_elevated())
                        .color(t.text())
                        .border_color(t.primary())
                })
                .hover(|s| s.background(t.bg_elevated()).color(t.text()))
        })
        .border_color(Color::TRANSPARENT)
        .transition(Background, Transition::linear(100.millis()))
        .transition(Foreground, Transition::linear(100.millis()))
        .justify_center()
        .items_center()
        .text_clip()
        .selectable(false)
        .apply(focus_style())
        .apply(hover_style());

    // let item_unfocused_style = Style::new().with_theme(|s, t| {
    //     s.hover(|s| s.background(t.bg_elevated())).selected(|s| {
    //         s.background(t.bg_elevated())
    //             .hover(|s| s.background(t.bg_overlay()))
    //     })
    // });

    Style::new()
        .debug_group(BorderDebugGroup)
        .debug_group(BorderColorDebugGroup)
        .debug_group(BorderRadiusDebugGroup)
        .debug_group(PaddingDebugGroup)
        .debug_group(MarginDebugGroup)
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
        .line_height(1.2)
        .class(LabelClass, |s| {
            s.with_theme(|s, t| {
                s.custom(|s: LabelCustomExprStyle| {
                    s.selection_color(t.def(|t| Brush::Solid(t.primary_muted().with_alpha(0.5))))
                })
            })
            .with::<Selectable>(|s, selectable| {
                s.set_context_opt(
                    Cursor,
                    selectable.def(|selectable| {
                        if selectable {
                            Some(CursorStyle::Text)
                        } else {
                            None
                        }
                    }),
                )
            })
            .focusable()
        })
        .class(ListClass, |s| {
            s.apply(focus_style()).class(ListItemClass, |s| {
                s.with_theme(|s, t| {
                    s.hover(|s| s.background(t.bg_elevated())).selected(|s| {
                        s.background(t.primary())
                            .color(t.bg_base())
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
                .handle_rounded(false)
                .apply_if(cfg!(target_os = "macos"), |s| s.handle_rounded(true))
        })
        .class(scroll::Handle, |s| {
            s.with_theme(|s, t| {
                s.background(t.border())
                    .active(|s| s.background(t.text_muted()))
                    .hover(|s| s.background(t.text_muted()))
            })
            .transition_background(Transition::ease_in_out(Duration::from_millis(300)))
        })
        .class(scroll::Track, |s| {
            s.with_theme(|s, t| s.hover(|s| s.background(t.def(|t| t.border().with_alpha(0.3)))))
                .background(css::TRANSPARENT)
                .transition_background(Transition::ease_in_out(Duration::from_millis(300)))
        })
        .class(ToggleButtonClass, |_| toggle_button_style)
        .class(SliderClass, |s| {
            s.apply(focus_style())
                .custom(|cs: SliderCustomStyle| {
                    cs.bar_radius(100.pct())
                        .accent_bar_radius(100.pct())
                        .handle_radius(100.pct())
                        .edge_align(true)
                })
                .with_theme(|s, t| {
                    s.custom(|cs: SliderCustomExprStyle| {
                        cs.bar_color(t.def(|t| Some(Brush::Solid(t.border()))))
                            .accent_bar_color(t.def(|t| Brush::Solid(t.primary())))
                            .handle_color(t.def(|t| Some(Brush::Solid(t.text()))))
                    })
                })
        })
        .class(PlaceholderTextClass, |s| {
            s.with_theme(|s, t| {
                s.color(t.text_muted()).disabled(|s| {
                    s.color(t.def(|t| t.text_muted().with_alpha(0.5)))
                        .set(Background, Some(Brush::Solid(css::BLACK)))
                })
            })
        })
        .class(TooltipClass, |s| s.apply(overlay_style()))
        .class(dropdown::DropdownClass, move |s| {
            s.padding(3)
                .apply(focus_style())
                .apply(border_style(true))
                .selectable(false)
                .class(dropdown::DropdownPreviewClass, |s| {
                    s.gap(0.75.em()).class(SvgClass, |s| {
                        s.with_theme(|s, t| {
                            s.hover(|s| s.background(t.bg_elevated()))
                                .border_radius(t.border_radius())
                                .color(t.text())
                        })
                        .padding(5.)
                        .size(1.em(), 1.em())
                    })
                })
                .class(scroll::ScrollClass, move |s| {
                    s.width_full()
                        .scrollbar_width(0)
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
        .class(ResizableClass, |s| s.padding_right(3))
        .class(ResizableHandleClass, |s| {
            s.custom(|cs: ResizableCustomStyle| cs.handle_thickness(3.))
                .with_theme(|s, t| {
                    s.custom(|cs: ResizableCustomExprStyle| {
                        cs.handle_color(t.def(|t| Brush::Solid(t.primary_muted().with_alpha(0.5))))
                            .hover(|s| s.handle_color(t.def(|t| Brush::Solid(t.primary()))))
                    })
                })
        })
        .class(HoverTargetClass, |s| {
            s.with_theme(|s, t| {
                s.padding(t.padding())
                    .border_radius(t.border_radius())
                    .background(t.bg_elevated())
                    .outline(3)
                    .file_hover(|s| s.background(t.bg_overlay()).outline_color(t.primary()))
            })
            .cursor(CursorStyle::Pointer)
            .transition(Background, Transition::linear(100.millis()))
        })
}
