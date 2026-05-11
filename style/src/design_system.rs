//! Design system color and spacing primitives.
//!
//! [`DesignSystem`] holds the colors and dimensions that the built-in
//! `floem` theme (light/dark modes) resolves from. The value type lives in
//! `floem_style` so it can be referenced from inspector previews and
//! interpolated alongside other style values. The `Theme` prop and the
//! `StyleThemeExt` trait stay in `floem`.

use std::any::Any;

use peniko::Color;

use crate::debug_view::PropDebugView;
use crate::inspector_render::InspectorRender;
use crate::prop_value::StylePropValue;

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
    pub font_size: f64,
}

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
        let adjustment = if self.is_dark { 0.25 } else { -0.25 };
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

    pub fn font_size(&self) -> f64 {
        self.font_size
    }
}

impl StylePropValue for DesignSystem {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        use peniko::color::HueDirection;
        let t = value as f32;
        let inv_t = 1.0 - t;
        let t64 = value;
        let inv_t64 = 1.0 - t64;

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
            font_size: self.font_size * inv_t64 + other.font_size * t64,
        })
    }
}

impl PropDebugView for DesignSystem {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.design_system(self))
    }
}
