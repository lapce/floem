//! Style component types for borders, shadows, padding, and margins.
//!
//! This module provides composite style types that represent multi-value
//! CSS properties like borders and padding.

use floem_renderer::text::Weight;
use peniko::color::palette;
use peniko::{Brush, Color};

use crate::theme::StyleThemeExt;
use crate::unit::{PxPct, PxPctAuto};
use crate::view::{IntoView, View};
use crate::views::{ContainerExt, Decorators, TooltipExt, h_stack, v_stack, v_stack_from_iter};

use super::values::{CombineResult, StylePropValue, StrokeWrap};

/// Pointer event handling mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerEvents {
    Auto,
    None,
}

/// Text overflow behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOverflow {
    Wrap,
    Clip,
    Ellipsis,
}

/// Cursor style
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CursorStyle {
    #[default]
    Default,
    Pointer,
    Progress,
    Wait,
    Crosshair,
    Text,
    Move,
    Grab,
    Grabbing,
    ColResize,
    RowResize,
    WResize,
    EResize,
    SResize,
    NResize,
    NwResize,
    NeResize,
    SwResize,
    SeResize,
    NeswResize,
    NwseResize,
}

/// Structure holding data about the shadow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub blur_radius: PxPct,
    pub color: Color,
    pub spread: PxPct,

    pub left_offset: PxPct,
    pub right_offset: PxPct,
    pub top_offset: PxPct,
    pub bottom_offset: PxPct,
}

impl BoxShadow {
    /// Create new default shadow.
    pub fn new() -> Self {
        Self::default()
    }

    /// Specifies shadow blur. The larger this value, the bigger the blur,
    /// so the shadow becomes bigger and lighter.
    pub fn blur_radius(mut self, radius: impl Into<PxPct>) -> Self {
        self.blur_radius = radius.into();
        self
    }

    /// Specifies shadow blur spread. Positive values will cause the shadow
    /// to expand and grow bigger, negative values will cause the shadow to shrink.
    pub fn spread(mut self, spread: impl Into<PxPct>) -> Self {
        self.spread = spread.into();
        self
    }

    /// Specifies color for the current shadow.
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = color.into();
        self
    }

    /// Specifies the offset of the left edge.
    pub fn left_offset(mut self, left_offset: impl Into<PxPct>) -> Self {
        self.left_offset = left_offset.into();
        self
    }

    /// Specifies the offset of the right edge.
    pub fn right_offset(mut self, right_offset: impl Into<PxPct>) -> Self {
        self.right_offset = right_offset.into();
        self
    }

    /// Specifies the offset of the top edge.
    pub fn top_offset(mut self, top_offset: impl Into<PxPct>) -> Self {
        self.top_offset = top_offset.into();
        self
    }

    /// Specifies the offset of the bottom edge.
    pub fn bottom_offset(mut self, bottom_offset: impl Into<PxPct>) -> Self {
        self.bottom_offset = bottom_offset.into();
        self
    }

    /// Specifies the offset on vertical axis.
    /// Negative offset value places the shadow above the element.
    pub fn v_offset(mut self, v_offset: impl Into<PxPct>) -> Self {
        let offset = v_offset.into();
        self.top_offset = -offset;
        self.bottom_offset = offset;
        self
    }

    /// Specifies the offset on horizontal axis.
    /// Negative offset value places the shadow to the left of the element.
    pub fn h_offset(mut self, h_offset: impl Into<PxPct>) -> Self {
        let offset = h_offset.into();
        self.left_offset = -offset;
        self.right_offset = offset;
        self
    }
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self {
            blur_radius: PxPct::Px(0.),
            color: palette::css::BLACK,
            spread: PxPct::Px(0.),
            left_offset: PxPct::Px(0.),
            right_offset: PxPct::Px(0.),
            top_offset: PxPct::Px(0.),
            bottom_offset: PxPct::Px(0.),
        }
    }
}

/// Structure holding border widths for all four sides
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Border {
    pub left: Option<StrokeWrap>,
    pub top: Option<StrokeWrap>,
    pub right: Option<StrokeWrap>,
    pub bottom: Option<StrokeWrap>,
}

impl Border {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(border: impl Into<StrokeWrap>) -> Self {
        let border = border.into();
        Self {
            left: Some(border.clone()),
            top: Some(border.clone()),
            right: Some(border.clone()),
            bottom: Some(border),
        }
    }

    pub fn left(mut self, border: impl Into<StrokeWrap>) -> Self {
        self.left = Some(border.into());
        self
    }

    pub fn top(mut self, border: impl Into<StrokeWrap>) -> Self {
        self.top = Some(border.into());
        self
    }

    pub fn right(mut self, border: impl Into<StrokeWrap>) -> Self {
        self.right = Some(border.into());
        self
    }

    pub fn bottom(mut self, border: impl Into<StrokeWrap>) -> Self {
        self.bottom = Some(border.into());
        self
    }

    pub fn horiz(mut self, border: impl Into<StrokeWrap>) -> Self {
        let border = border.into();
        self.left = Some(border.clone());
        self.right = Some(border);
        self
    }

    pub fn vert(mut self, border: impl Into<StrokeWrap>) -> Self {
        let border = border.into();
        self.top = Some(border.clone());
        self.bottom = Some(border);
        self
    }
}

impl StylePropValue for Border {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let border = self.clone();
        let details_view = move || {
            let sides = [
                ("Left:", border.left),
                ("Top:", border.top),
                ("Right:", border.right),
                ("Bottom:", border.bottom),
            ];

            v_stack_from_iter(
                sides
                    .into_iter()
                    .filter_map(|(l, v)| v.map(|v| (l, v)))
                    .map(|(label, value)| {
                        h_stack((
                            label.style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                            value.debug_view().unwrap(),
                        ))
                        .style(|s| s.items_center().gap(4.0))
                        .into_any()
                    }),
            )
            .style(|s| s.gap(4.0).padding(8.0))
        };
        Some(details_view().into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            left: self.left.interpolate(&other.left, value)?,
            top: self.top.interpolate(&other.top, value)?,
            right: self.right.interpolate(&other.right, value)?,
            bottom: self.bottom.interpolate(&other.bottom, value)?,
        })
    }

    fn combine(&self, other: &Self) -> CombineResult<Self> {
        let result = Border {
            left: other.left.clone().or_else(|| self.left.clone()),
            top: other.top.clone().or_else(|| self.top.clone()),
            right: other.right.clone().or_else(|| self.right.clone()),
            bottom: other.bottom.clone().or_else(|| self.bottom.clone()),
        };

        if result == *other {
            CombineResult::Other
        } else {
            CombineResult::New(result)
        }
    }
}

/// Structure holding border colors for all four sides
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BorderColor {
    pub left: Option<Brush>,
    pub top: Option<Brush>,
    pub right: Option<Brush>,
    pub bottom: Option<Brush>,
}

impl BorderColor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(color: impl Into<Brush>) -> Self {
        let color = color.into();
        Self {
            left: Some(color.clone()),
            top: Some(color.clone()),
            right: Some(color.clone()),
            bottom: Some(color),
        }
    }

    pub fn left(mut self, color: impl Into<Brush>) -> Self {
        self.left = Some(color.into());
        self
    }

    pub fn top(mut self, color: impl Into<Brush>) -> Self {
        self.top = Some(color.into());
        self
    }

    pub fn right(mut self, color: impl Into<Brush>) -> Self {
        self.right = Some(color.into());
        self
    }

    pub fn bottom(mut self, color: impl Into<Brush>) -> Self {
        self.bottom = Some(color.into());
        self
    }

    pub fn horiz(mut self, color: impl Into<Brush>) -> Self {
        let color = color.into();
        self.left = Some(color.clone());
        self.right = Some(color);
        self
    }

    pub fn vert(mut self, color: impl Into<Brush>) -> Self {
        let color = color.into();
        self.top = Some(color.clone());
        self.bottom = Some(color);
        self
    }
}

impl StylePropValue for BorderColor {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let border_color = self.clone();
        let details_view = move || {
            let sides = [
                ("Left:", border_color.left),
                ("Top:", border_color.top),
                ("Right:", border_color.right),
                ("Bottom:", border_color.bottom),
            ];

            v_stack_from_iter(
                sides
                    .into_iter()
                    .filter_map(|(l, v)| v.map(|v| (l, v)))
                    .map(|(label, color)| {
                        h_stack((
                            label.style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                            color.debug_view().unwrap(),
                        ))
                        .style(|s| s.items_center().gap(4.0))
                    }),
            )
            .style(|s| s.gap(4.0).padding(8.0))
        };
        Some(details_view().into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            left: self.left.interpolate(&other.left, value)?,
            top: self.top.interpolate(&other.top, value)?,
            right: self.right.interpolate(&other.right, value)?,
            bottom: self.bottom.interpolate(&other.bottom, value)?,
        })
    }

    fn combine(&self, other: &Self) -> CombineResult<Self> {
        let result = BorderColor {
            left: other.left.clone().or_else(|| self.left.clone()),
            top: other.top.clone().or_else(|| self.top.clone()),
            right: other.right.clone().or_else(|| self.right.clone()),
            bottom: other.bottom.clone().or_else(|| self.bottom.clone()),
        };

        if result == *other {
            CombineResult::Other
        } else {
            CombineResult::New(result)
        }
    }
}

/// Structure holding border radius for all four corners
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BorderRadius {
    pub top_left: Option<PxPct>,
    pub top_right: Option<PxPct>,
    pub bottom_left: Option<PxPct>,
    pub bottom_right: Option<PxPct>,
}

impl BorderRadius {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(radius: impl Into<PxPct>) -> Self {
        let radius = radius.into();
        Self {
            top_left: Some(radius),
            top_right: Some(radius),
            bottom_left: Some(radius),
            bottom_right: Some(radius),
        }
    }

    pub fn top_left(mut self, radius: impl Into<PxPct>) -> Self {
        self.top_left = Some(radius.into());
        self
    }

    pub fn top_right(mut self, radius: impl Into<PxPct>) -> Self {
        self.top_right = Some(radius.into());
        self
    }

    pub fn bottom_left(mut self, radius: impl Into<PxPct>) -> Self {
        self.bottom_left = Some(radius.into());
        self
    }

    pub fn bottom_right(mut self, radius: impl Into<PxPct>) -> Self {
        self.bottom_right = Some(radius.into());
        self
    }

    pub fn top(mut self, radius: impl Into<PxPct>) -> Self {
        let radius = radius.into();
        self.top_left = Some(radius);
        self.top_right = Some(radius);
        self
    }

    pub fn bottom(mut self, radius: impl Into<PxPct>) -> Self {
        let radius = radius.into();
        self.bottom_left = Some(radius);
        self.bottom_right = Some(radius);
        self
    }

    pub fn left(mut self, radius: impl Into<PxPct>) -> Self {
        let radius = radius.into();
        self.top_left = Some(radius);
        self.bottom_left = Some(radius);
        self
    }

    pub fn right(mut self, radius: impl Into<PxPct>) -> Self {
        let radius = radius.into();
        self.top_right = Some(radius);
        self.bottom_right = Some(radius);
        self
    }
}

impl StylePropValue for BorderRadius {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let border_radius = *self;
        let details_view = move || {
            let corners = [
                ("Top Left:", border_radius.top_left),
                ("Top Right:", border_radius.top_right),
                ("Bottom Left:", border_radius.bottom_left),
                ("Bottom Right:", border_radius.bottom_right),
            ];

            v_stack_from_iter(
                corners
                    .into_iter()
                    .filter_map(|(l, v)| v.map(|v| (l, v)))
                    .map(|(label, radius)| {
                        h_stack((
                            label.style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                            radius.debug_view().unwrap(),
                        ))
                        .style(|s| s.items_center().gap(4.0))
                    }),
            )
            .style(|s| s.gap(4.0).padding(8.0))
        };
        Some(details_view().into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            top_left: self.top_left.interpolate(&other.top_left, value)?,
            top_right: self.top_right.interpolate(&other.top_right, value)?,
            bottom_left: self.bottom_left.interpolate(&other.bottom_left, value)?,
            bottom_right: self.bottom_right.interpolate(&other.bottom_right, value)?,
        })
    }

    fn combine(&self, other: &Self) -> CombineResult<Self> {
        let result = BorderRadius {
            top_left: other.top_left.or(self.top_left),
            top_right: other.top_right.or(self.top_right),
            bottom_left: other.bottom_left.or(self.bottom_left),
            bottom_right: other.bottom_right.or(self.bottom_right),
        };

        if result == *other {
            CombineResult::Other
        } else {
            CombineResult::New(result)
        }
    }
}

/// Structure holding padding values for all four sides
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Padding {
    pub left: Option<PxPct>,
    pub top: Option<PxPct>,
    pub right: Option<PxPct>,
    pub bottom: Option<PxPct>,
}

impl Padding {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(padding: impl Into<PxPct>) -> Self {
        let padding = padding.into();
        Self {
            left: Some(padding),
            top: Some(padding),
            right: Some(padding),
            bottom: Some(padding),
        }
    }

    pub fn left(mut self, padding: impl Into<PxPct>) -> Self {
        self.left = Some(padding.into());
        self
    }

    pub fn top(mut self, padding: impl Into<PxPct>) -> Self {
        self.top = Some(padding.into());
        self
    }

    pub fn right(mut self, padding: impl Into<PxPct>) -> Self {
        self.right = Some(padding.into());
        self
    }

    pub fn bottom(mut self, padding: impl Into<PxPct>) -> Self {
        self.bottom = Some(padding.into());
        self
    }

    pub fn horiz(mut self, padding: impl Into<PxPct>) -> Self {
        let padding = padding.into();
        self.left = Some(padding);
        self.right = Some(padding);
        self
    }

    pub fn vert(mut self, padding: impl Into<PxPct>) -> Self {
        let padding = padding.into();
        self.top = Some(padding);
        self.bottom = Some(padding);
        self
    }
}

impl StylePropValue for Padding {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let padding = *self;
        let details_view = move || {
            let sides = [
                ("Left:", padding.left),
                ("Top:", padding.top),
                ("Right:", padding.right),
                ("Bottom:", padding.bottom),
            ];

            v_stack_from_iter(
                sides
                    .into_iter()
                    .filter_map(|(l, v)| v.map(|v| (l, v)))
                    .map(|(label, padding)| {
                        h_stack((
                            label.style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                            padding.debug_view().unwrap(),
                        ))
                        .style(|s| s.items_center().gap(4.0))
                    }),
            )
            .style(|s| s.gap(4.0).padding(8.0))
        };
        Some(details_view().into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            left: self.left.interpolate(&other.left, value)?,
            top: self.top.interpolate(&other.top, value)?,
            right: self.right.interpolate(&other.right, value)?,
            bottom: self.bottom.interpolate(&other.bottom, value)?,
        })
    }

    fn combine(&self, other: &Self) -> CombineResult<Self> {
        let result = Padding {
            left: other.left.or(self.left),
            top: other.top.or(self.top),
            right: other.right.or(self.right),
            bottom: other.bottom.or(self.bottom),
        };

        if result == *other {
            CombineResult::Other
        } else {
            CombineResult::New(result)
        }
    }
}

/// Structure holding margin values for all four sides
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Margin {
    pub left: Option<PxPctAuto>,
    pub top: Option<PxPctAuto>,
    pub right: Option<PxPctAuto>,
    pub bottom: Option<PxPctAuto>,
}

impl Margin {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(margin: impl Into<PxPctAuto>) -> Self {
        let margin = margin.into();
        Self {
            left: Some(margin),
            top: Some(margin),
            right: Some(margin),
            bottom: Some(margin),
        }
    }

    pub fn left(mut self, margin: impl Into<PxPctAuto>) -> Self {
        self.left = Some(margin.into());
        self
    }

    pub fn top(mut self, margin: impl Into<PxPctAuto>) -> Self {
        self.top = Some(margin.into());
        self
    }

    pub fn right(mut self, margin: impl Into<PxPctAuto>) -> Self {
        self.right = Some(margin.into());
        self
    }

    pub fn bottom(mut self, margin: impl Into<PxPctAuto>) -> Self {
        self.bottom = Some(margin.into());
        self
    }

    pub fn horiz(mut self, margin: impl Into<PxPctAuto>) -> Self {
        let margin = margin.into();
        self.left = Some(margin);
        self.right = Some(margin);
        self
    }

    pub fn vert(mut self, margin: impl Into<PxPctAuto>) -> Self {
        let margin = margin.into();
        self.top = Some(margin);
        self.bottom = Some(margin);
        self
    }
}

impl StylePropValue for Margin {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let margin = *self;
        let details_view = move || {
            let sides = [
                ("Left:", margin.left),
                ("Top:", margin.top),
                ("Right:", margin.right),
                ("Bottom:", margin.bottom),
            ];

            v_stack_from_iter(
                sides
                    .into_iter()
                    .filter_map(|(l, v)| v.map(|v| (l, v)))
                    .map(|(label, margin)| {
                        h_stack((
                            label.style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                            margin.debug_view().unwrap(),
                        ))
                        .style(|s| s.items_center().gap(4.0))
                    }),
            )
            .style(|s| s.gap(4.0).padding(8.0))
        };
        Some(details_view().into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            left: self.left.interpolate(&other.left, value)?,
            top: self.top.interpolate(&other.top, value)?,
            right: self.right.interpolate(&other.right, value)?,
            bottom: self.bottom.interpolate(&other.bottom, value)?,
        })
    }

    fn combine(&self, other: &Self) -> CombineResult<Self> {
        let result = Margin {
            left: other.left.or(self.left),
            top: other.top.or(self.top),
            right: other.right.or(self.right),
            bottom: other.bottom.or(self.bottom),
        };

        if result == *other {
            CombineResult::Other
        } else {
            CombineResult::New(result)
        }
    }
}

// Simple StylePropValue implementations for enums
impl StylePropValue for CursorStyle {}
impl StylePropValue for TextOverflow {}
impl StylePropValue for PointerEvents {}

impl StylePropValue for BoxShadow {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        // Create a preview container that shows a visual representation of the shadow
        let shadow = *self;

        // Shadow preview box
        let shadow_preview =
            ().style(move |s| s.width(50.0).height(50.0))
                .container()
                .style(move |s| {
                    s.with_theme(|s, t| {
                        s.background(Color::TRANSPARENT)
                            .border_color(t.border())
                            .border(1.)
                            .border_radius(t.border_radius())
                    })
                    .apply_box_shadows(vec![shadow])
                    .margin(10.0)
                });

        // Create a details section showing the shadow properties
        let details_view = move || {
            v_stack((
                h_stack((
                    "Color:".style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                    shadow.color.debug_view().unwrap(),
                ))
                .style(|s| s.items_center().gap(4.0)),
                h_stack((
                    "Blur:".style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                    format!("{:?}", shadow.blur_radius),
                ))
                .style(|s| s.items_center().gap(4.0)),
                h_stack((
                    "Spread:".style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                    format!("{:?}", shadow.spread),
                ))
                .style(|s| s.items_center().gap(4.0)),
                h_stack((
                    "Offset:".style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                    format!(
                        "L: {:?}, R: {:?}, T: {:?}, B: {:?}",
                        shadow.left_offset,
                        shadow.right_offset,
                        shadow.top_offset,
                        shadow.bottom_offset
                    ),
                ))
                .style(|s| s.items_center().gap(4.0)),
            ))
            .style(|s| s.gap(4.0).padding(8.0))
        };

        // Combine preview and details
        let view = shadow_preview.tooltip(details_view);

        Some(view.into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            blur_radius: self
                .blur_radius
                .interpolate(&other.blur_radius, value)
                .unwrap(),
            color: self.color.interpolate(&other.color, value).unwrap(),
            spread: self.spread.interpolate(&other.spread, value).unwrap(),
            left_offset: self
                .left_offset
                .interpolate(&other.left_offset, value)
                .unwrap(),
            right_offset: self
                .right_offset
                .interpolate(&other.right_offset, value)
                .unwrap(),
            top_offset: self
                .top_offset
                .interpolate(&other.top_offset, value)
                .unwrap(),
            bottom_offset: self
                .bottom_offset
                .interpolate(&other.bottom_offset, value)
                .unwrap(),
        })
    }
}
