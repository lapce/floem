//! Composite style value types.
//!
//! This module provides multi-value style types like [`Border`], [`Padding`],
//! [`Margin`], and [`BoxShadow`]. They are data types only. The
//! `PropDebugView` impls delegate to [`crate::InspectorRender`] for the
//! actual widget construction, so the view code stays in the `floem` crate.

use std::any::Any;

use peniko::color::palette;
use peniko::kurbo::Stroke;
use peniko::{Brush, Color};

use crate::debug_view::PropDebugView;
use crate::inspector_render::InspectorRender;
use crate::prop_value::StylePropValue;
use crate::unit::{FontSizeCx, Length, LengthAuto};
use crate::values::StrokeWrap;

/// Structure holding data about the shadow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub blur_radius: Length,
    pub color: Color,
    pub spread: Length,

    pub left_offset: Length,
    pub right_offset: Length,
    pub top_offset: Length,
    pub bottom_offset: Length,
}

impl BoxShadow {
    /// Create new default shadow.
    pub fn new() -> Self {
        Self::default()
    }

    /// Specifies shadow blur. The larger this value, the bigger the blur,
    /// so the shadow becomes bigger and lighter.
    pub fn blur_radius(mut self, radius: impl Into<Length>) -> Self {
        self.blur_radius = radius.into();
        self
    }

    /// Specifies shadow blur spread. Positive values will cause the shadow
    /// to expand and grow bigger, negative values will cause the shadow to shrink.
    pub fn spread(mut self, spread: impl Into<Length>) -> Self {
        self.spread = spread.into();
        self
    }

    /// Specifies color for the current shadow.
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = color.into();
        self
    }

    /// Specifies the offset of the left edge.
    pub fn left_offset(mut self, left_offset: impl Into<Length>) -> Self {
        self.left_offset = left_offset.into();
        self
    }

    /// Specifies the offset of the right edge.
    pub fn right_offset(mut self, right_offset: impl Into<Length>) -> Self {
        self.right_offset = right_offset.into();
        self
    }

    /// Specifies the offset of the top edge.
    pub fn top_offset(mut self, top_offset: impl Into<Length>) -> Self {
        self.top_offset = top_offset.into();
        self
    }

    /// Specifies the offset of the bottom edge.
    pub fn bottom_offset(mut self, bottom_offset: impl Into<Length>) -> Self {
        self.bottom_offset = bottom_offset.into();
        self
    }

    /// Specifies the offset on vertical axis.
    /// Negative offset value places the shadow above the element.
    pub fn v_offset(mut self, v_offset: impl Into<Length>) -> Self {
        let offset = v_offset.into();
        self.top_offset = -offset;
        self.bottom_offset = offset;
        self
    }

    /// Specifies the offset on horizontal axis.
    /// Negative offset value places the shadow to the left of the element.
    pub fn h_offset(mut self, h_offset: impl Into<Length>) -> Self {
        let offset = h_offset.into();
        self.left_offset = -offset;
        self.right_offset = offset;
        self
    }
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self {
            blur_radius: Length::Pt(0.),
            color: palette::css::BLACK,
            spread: Length::Pt(0.),
            left_offset: Length::Pt(0.),
            right_offset: Length::Pt(0.),
            top_offset: Length::Pt(0.),
            bottom_offset: Length::Pt(0.),
        }
    }
}

impl StylePropValue for BoxShadow {
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        self.blur_radius.content_hash().hash(&mut h);
        self.color.content_hash().hash(&mut h);
        self.spread.content_hash().hash(&mut h);
        self.left_offset.content_hash().hash(&mut h);
        self.right_offset.content_hash().hash(&mut h);
        self.top_offset.content_hash().hash(&mut h);
        self.bottom_offset.content_hash().hash(&mut h);
        h.finish()
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

/// Structure holding border widths for all four sides
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Border {
    pub left: Option<Stroke>,
    pub top: Option<Stroke>,
    pub right: Option<Stroke>,
    pub bottom: Option<Stroke>,
}

impl Border {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(border: impl Into<StrokeWrap>) -> Self {
        let border = border.into();
        Self {
            left: Some(border.0.clone()),
            top: Some(border.0.clone()),
            right: Some(border.0.clone()),
            bottom: Some(border.0),
        }
    }

    pub fn left(mut self, border: impl Into<StrokeWrap>) -> Self {
        self.left = Some(border.into().0);
        self
    }

    pub fn top(mut self, border: impl Into<StrokeWrap>) -> Self {
        self.top = Some(border.into().0);
        self
    }

    pub fn right(mut self, border: impl Into<StrokeWrap>) -> Self {
        self.right = Some(border.into().0);
        self
    }

    pub fn bottom(mut self, border: impl Into<StrokeWrap>) -> Self {
        self.bottom = Some(border.into().0);
        self
    }

    pub fn horiz(mut self, border: impl Into<StrokeWrap>) -> Self {
        let border = border.into();
        self.left = Some(border.0.clone());
        self.right = Some(border.0);
        self
    }

    pub fn vert(mut self, border: impl Into<StrokeWrap>) -> Self {
        let border = border.into();
        self.top = Some(border.0.clone());
        self.bottom = Some(border.0);
        self
    }
}

impl StylePropValue for Border {
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        self.left.content_hash().hash(&mut h);
        self.top.content_hash().hash(&mut h);
        self.right.content_hash().hash(&mut h);
        self.bottom.content_hash().hash(&mut h);
        h.finish()
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            left: self.left.interpolate(&other.left, value)?,
            top: self.top.interpolate(&other.top, value)?,
            right: self.right.interpolate(&other.right, value)?,
            bottom: self.bottom.interpolate(&other.bottom, value)?,
        })
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
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        self.left.content_hash().hash(&mut h);
        self.top.content_hash().hash(&mut h);
        self.right.content_hash().hash(&mut h);
        self.bottom.content_hash().hash(&mut h);
        h.finish()
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            left: self.left.interpolate(&other.left, value)?,
            top: self.top.interpolate(&other.top, value)?,
            right: self.right.interpolate(&other.right, value)?,
            bottom: self.bottom.interpolate(&other.bottom, value)?,
        })
    }
}

/// Structure holding border radius for all four corners
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BorderRadius {
    pub top_left: Option<Length>,
    pub top_right: Option<Length>,
    pub bottom_left: Option<Length>,
    pub bottom_right: Option<Length>,
}

impl BorderRadius {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(radius: impl Into<Length>) -> Self {
        let radius = radius.into();
        Self {
            top_left: Some(radius),
            top_right: Some(radius),
            bottom_left: Some(radius),
            bottom_right: Some(radius),
        }
    }

    pub fn top_left(mut self, radius: impl Into<Length>) -> Self {
        self.top_left = Some(radius.into());
        self
    }

    pub fn top_right(mut self, radius: impl Into<Length>) -> Self {
        self.top_right = Some(radius.into());
        self
    }

    pub fn bottom_left(mut self, radius: impl Into<Length>) -> Self {
        self.bottom_left = Some(radius.into());
        self
    }

    pub fn bottom_right(mut self, radius: impl Into<Length>) -> Self {
        self.bottom_right = Some(radius.into());
        self
    }

    pub fn top(mut self, radius: impl Into<Length>) -> Self {
        let radius = radius.into();
        self.top_left = Some(radius);
        self.top_right = Some(radius);
        self
    }

    pub fn bottom(mut self, radius: impl Into<Length>) -> Self {
        let radius = radius.into();
        self.bottom_left = Some(radius);
        self.bottom_right = Some(radius);
        self
    }

    pub fn left(mut self, radius: impl Into<Length>) -> Self {
        let radius = radius.into();
        self.top_left = Some(radius);
        self.bottom_left = Some(radius);
        self
    }

    pub fn right(mut self, radius: impl Into<Length>) -> Self {
        let radius = radius.into();
        self.top_right = Some(radius);
        self.bottom_right = Some(radius);
        self
    }

    /// Resolve border radii to absolute pixels given the min side of the element.
    /// Percentage values are resolved relative to the min side.
    pub fn resolve_border_radii(
        &self,
        min_side: f64,
        resolve_cx: &FontSizeCx,
    ) -> peniko::kurbo::RoundedRectRadii {
        fn resolve(val: Option<Length>, min_side: f64, resolve_cx: &FontSizeCx) -> f64 {
            val.map(|length| length.resolve(min_side, resolve_cx))
                .unwrap_or(0.0)
        }
        peniko::kurbo::RoundedRectRadii {
            top_left: resolve(self.top_left, min_side, resolve_cx),
            top_right: resolve(self.top_right, min_side, resolve_cx),
            bottom_right: resolve(self.bottom_right, min_side, resolve_cx),
            bottom_left: resolve(self.bottom_left, min_side, resolve_cx),
        }
    }
}

impl StylePropValue for BorderRadius {
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        self.top_left.content_hash().hash(&mut h);
        self.top_right.content_hash().hash(&mut h);
        self.bottom_left.content_hash().hash(&mut h);
        self.bottom_right.content_hash().hash(&mut h);
        h.finish()
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            top_left: self.top_left.interpolate(&other.top_left, value)?,
            top_right: self.top_right.interpolate(&other.top_right, value)?,
            bottom_left: self.bottom_left.interpolate(&other.bottom_left, value)?,
            bottom_right: self.bottom_right.interpolate(&other.bottom_right, value)?,
        })
    }
}

/// Structure holding padding values for all four sides
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Padding {
    pub left: Option<Length>,
    pub top: Option<Length>,
    pub right: Option<Length>,
    pub bottom: Option<Length>,
}

impl Padding {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(padding: impl Into<Length>) -> Self {
        let padding = padding.into();
        Self {
            left: Some(padding),
            top: Some(padding),
            right: Some(padding),
            bottom: Some(padding),
        }
    }

    pub fn left(mut self, padding: impl Into<Length>) -> Self {
        self.left = Some(padding.into());
        self
    }

    pub fn top(mut self, padding: impl Into<Length>) -> Self {
        self.top = Some(padding.into());
        self
    }

    pub fn right(mut self, padding: impl Into<Length>) -> Self {
        self.right = Some(padding.into());
        self
    }

    pub fn bottom(mut self, padding: impl Into<Length>) -> Self {
        self.bottom = Some(padding.into());
        self
    }

    pub fn horiz(mut self, padding: impl Into<Length>) -> Self {
        let padding = padding.into();
        self.left = Some(padding);
        self.right = Some(padding);
        self
    }

    pub fn vert(mut self, padding: impl Into<Length>) -> Self {
        let padding = padding.into();
        self.top = Some(padding);
        self.bottom = Some(padding);
        self
    }
}

impl StylePropValue for Padding {
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        self.left.content_hash().hash(&mut h);
        self.top.content_hash().hash(&mut h);
        self.right.content_hash().hash(&mut h);
        self.bottom.content_hash().hash(&mut h);
        h.finish()
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            left: self.left.interpolate(&other.left, value)?,
            top: self.top.interpolate(&other.top, value)?,
            right: self.right.interpolate(&other.right, value)?,
            bottom: self.bottom.interpolate(&other.bottom, value)?,
        })
    }
}

/// Structure holding margin values for all four sides
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Margin {
    pub left: Option<LengthAuto>,
    pub top: Option<LengthAuto>,
    pub right: Option<LengthAuto>,
    pub bottom: Option<LengthAuto>,
}

impl Margin {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(margin: impl Into<LengthAuto>) -> Self {
        let margin = margin.into();
        Self {
            left: Some(margin),
            top: Some(margin),
            right: Some(margin),
            bottom: Some(margin),
        }
    }

    pub fn left(mut self, margin: impl Into<LengthAuto>) -> Self {
        self.left = Some(margin.into());
        self
    }

    pub fn top(mut self, margin: impl Into<LengthAuto>) -> Self {
        self.top = Some(margin.into());
        self
    }

    pub fn right(mut self, margin: impl Into<LengthAuto>) -> Self {
        self.right = Some(margin.into());
        self
    }

    pub fn bottom(mut self, margin: impl Into<LengthAuto>) -> Self {
        self.bottom = Some(margin.into());
        self
    }

    pub fn horiz(mut self, margin: impl Into<LengthAuto>) -> Self {
        let margin = margin.into();
        self.left = Some(margin);
        self.right = Some(margin);
        self
    }

    pub fn vert(mut self, margin: impl Into<LengthAuto>) -> Self {
        let margin = margin.into();
        self.top = Some(margin);
        self.bottom = Some(margin);
        self
    }
}

impl StylePropValue for Margin {
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        self.left.content_hash().hash(&mut h);
        self.top.content_hash().hash(&mut h);
        self.right.content_hash().hash(&mut h);
        self.bottom.content_hash().hash(&mut h);
        h.finish()
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            left: self.left.interpolate(&other.left, value)?,
            top: self.top.interpolate(&other.top, value)?,
            right: self.right.interpolate(&other.right, value)?,
            bottom: self.bottom.interpolate(&other.bottom, value)?,
        })
    }
}

impl PropDebugView for Border {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.border(self))
    }
}

impl PropDebugView for BorderColor {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.border_color(self))
    }
}

impl PropDebugView for BorderRadius {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.border_radius(self))
    }
}

impl PropDebugView for Padding {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.padding(self))
    }
}

impl PropDebugView for Margin {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.margin(self))
    }
}

impl PropDebugView for BoxShadow {
    fn debug_view(&self, r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        Some(r.box_shadow(self))
    }
}
