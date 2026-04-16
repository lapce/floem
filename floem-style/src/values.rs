//! Value types for style properties.
//!
//! These types are the pure data counterparts of values used in the Floem
//! style system. Their inspector `PropDebugView` impls remain in `floem`
//! because they reference `crate::view::View`.

use peniko::kurbo::Stroke;

use crate::prop_value::{StylePropValue, hash_value};
use crate::unit::Length;

/// How the content of a replaced element, such as an img, should be resized to fit its container.
/// Corresponds to the CSS `object-fit` property.
/// See <https://developer.mozilla.org/en-US/docs/Web/CSS/object-fit>.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ObjectFit {
    /// The replaced content is sized to fill the element's content box.
    /// The entire object will completely fill the box.
    /// If the object's aspect ratio does not match the aspect ratio of its box,
    /// then the object will be stretched to fit.
    #[default]
    Fill,
    /// The replaced content is scaled to maintain its aspect ratio while fitting
    /// within the element's content box. The entire object is made to fill the box,
    /// while preserving its aspect ratio, so the object will be "letterboxed"
    /// if its aspect ratio does not match the aspect ratio of the box.
    Contain,
    /// The content is sized to maintain its aspect ratio while filling the element's
    /// entire content box. If the object's aspect ratio does not match the aspect
    /// ratio of its box, then the object will be clipped to fit.
    Cover,
    /// The content is sized as if none or contain were specified, whichever would
    /// result in a smaller concrete object size.
    ScaleDown,
    /// The replaced content is not resized.
    None,
}

/// Where the content of a replaced element should be positioned inside its container.
/// Corresponds to common CSS `object-position` keyword combinations.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ObjectPosition {
    /// Align content to the top-left corner.
    TopLeft,
    /// Align content to the top edge and center horizontally.
    Top,
    /// Align content to the top-right corner.
    TopRight,
    /// Align content to the left edge and center vertically.
    Left,
    /// Center content both horizontally and vertically.
    #[default]
    Center,
    /// Align content to the right edge and center vertically.
    Right,
    /// Align content to the bottom-left corner.
    BottomLeft,
    /// Align content to the bottom edge and center horizontally.
    Bottom,
    /// Align content to the bottom-right corner.
    BottomRight,
    /// Position content using explicit horizontal and vertical offsets.
    ///
    /// Percentage values are resolved against the remaining free space on each axis,
    /// matching CSS object-position behavior.
    Custom(Length, Length),
}

impl StylePropValue for ObjectFit {
    fn content_hash(&self) -> u64 {
        hash_value(self)
    }
}

impl StylePropValue for ObjectPosition {
    fn content_hash(&self) -> u64 {
        hash_value(&std::mem::discriminant(self))
    }
}

/// Convenience wrapper so border/outline setters can accept numeric widths.
#[derive(Clone, Debug, Default)]
pub struct StrokeWrap(pub Stroke);

impl StrokeWrap {
    pub fn new(width: f64) -> Self {
        Self(Stroke::new(width))
    }
}

impl From<Stroke> for StrokeWrap {
    fn from(value: Stroke) -> Self {
        Self(value)
    }
}
impl From<f32> for StrokeWrap {
    fn from(value: f32) -> Self {
        Self(Stroke::new(value.into()))
    }
}
impl From<f64> for StrokeWrap {
    fn from(value: f64) -> Self {
        Self(Stroke::new(value))
    }
}
impl From<i32> for StrokeWrap {
    fn from(value: i32) -> Self {
        Self(Stroke::new(value.into()))
    }
}
