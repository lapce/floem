//! Value types for style properties.
//!
//! These types are the pure data counterparts of values used in the Floem
//! style system. Their inspector `PropDebugView` impls remain in `floem`
//! because they reference `crate::view::View`.

use parley::style::{OverflowWrap, WordBreakStrength};
use peniko::kurbo::Stroke;

use crate::debug_view::PropDebugView;
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

/// Pointer event handling mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerEvents {
    Auto,
    None,
}

/// Text overflow behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOverflow {
    NoWrap(NoWrapOverflow),
    Wrap {
        overflow_wrap: OverflowWrap,
        word_break: WordBreakStrength,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoWrapOverflow {
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

impl StylePropValue for CursorStyle {
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        std::mem::discriminant(self).hash(&mut h);
        h.finish()
    }
}
impl PropDebugView for CursorStyle {}

impl StylePropValue for TextOverflow {
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        std::mem::discriminant(self).hash(&mut h);
        h.finish()
    }
}
impl PropDebugView for TextOverflow {}

impl StylePropValue for PointerEvents {
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        std::mem::discriminant(self).hash(&mut h);
        h.finish()
    }
}
impl PropDebugView for PointerEvents {}

/// Controls whether and how a view can receive focus.
///
/// Focus determines which element receives keyboard input and is used for accessibility
/// and keyboard navigation. This enum provides three levels of focus behavior, where
/// each level includes the capabilities of the previous level. In particular,
/// [`Focus::Keyboard`] always implies full focusability for pointer and
/// programmatic focus too.
///
/// # Focus Sources
///
/// - **Programmatic**: Focus set via code (e.g., `view.request_focus()`)
/// - **Pointer**: Focus set by clicking or tapping the view
/// - **Keyboard**: Focus set by sequential navigation (Tab key, arrow keys, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Focus {
    /// The view cannot receive focus through any means.
    ///
    /// Use this for decorative elements, labels, or containers that should never
    /// be interactive. Clicking the view will not focus it, and programmatic
    /// focus requests will be ignored.
    #[default]
    None,

    /// The view can receive focus programmatically or via pointer, but is not
    /// included in keyboard navigation order.
    ///
    /// Use this for:
    /// - Custom containers that need focus for scroll or keyboard handling
    /// - Elements that should be clickable but not tab-able
    /// - Roving tabindex scenarios where only one item is keyboard navigable
    /// - Dialog/modal containers that need programmatic focus
    ///
    /// The view will not be reachable via Tab or arrow key navigation, but can
    /// be focused by clicking or calling `request_focus()`.
    PointerAndProgrammatic,

    /// The view can receive focus through all means: programmatically, via pointer,
    /// and via keyboard navigation.
    ///
    /// Use this for interactive controls like buttons, inputs, links, and custom
    /// widgets that should be fully accessible via keyboard. The view will be
    /// included in the sequential focus navigation order (Tab/Shift+Tab) and
    /// spatial navigation (arrow keys).
    ///
    /// This is the recommended setting for all interactive UI elements.
    Keyboard,
}

impl Focus {
    /// Returns `true` if the view can receive focus in any way.
    ///
    /// This includes programmatic focus, pointer focus, and keyboard navigation.
    #[inline]
    pub fn is_focusable(self) -> bool {
        !matches!(self, Focus::None)
    }

    /// Returns `true` if the view can receive focus via pointer (click/tap).
    #[inline]
    pub fn allows_pointer_focus(self) -> bool {
        matches!(self, Focus::PointerAndProgrammatic | Focus::Keyboard)
    }

    /// Returns `true` if the view can receive focus programmatically (via code).
    #[inline]
    pub fn allows_programmatic_focus(self) -> bool {
        matches!(self, Focus::PointerAndProgrammatic | Focus::Keyboard)
    }

    /// Returns `true` if the view is included in keyboard navigation order.
    ///
    /// This means the view can be reached via Tab, Shift+Tab, or arrow key navigation.
    #[inline]
    pub fn allows_keyboard_navigation(self) -> bool {
        matches!(self, Focus::Keyboard)
    }

    /// Returns `true` if the view should be excluded from all focus mechanisms.
    #[inline]
    pub fn is_none(self) -> bool {
        matches!(self, Focus::None)
    }
}

impl StylePropValue for Focus {
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        self.hash(&mut h);
        h.finish()
    }
}
impl PropDebugView for Focus {}
