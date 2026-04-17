//! Style component types for borders, shadows, padding, and margins.
//!
//! The composite value types ([`Border`], [`BorderColor`], [`BorderRadius`],
//! [`Padding`], [`Margin`], [`BoxShadow`]) live in `floem_style` together
//! with their `PropDebugView` impls (which delegate to
//! [`InspectorRender`]); the actual widget-building bodies live in
//! [`crate::style::FloemInspectorRender`]. The simpler enum types
//! (`PointerEvents`, `TextOverflow`, `CursorStyle`, `Focus`) remain here
//! because they have empty debug views and don't need to live in
//! `floem_style` yet.

use parley::style::{OverflowWrap, WordBreakStrength};

use super::values::StylePropValue;
use super::PropDebugView;

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

// Simple StylePropValue implementations for enums
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
///
/// # Examples
///
/// ```rust
/// # use floem::prelude::*;
/// // A button that can be focused by any means
/// let _button = Button::new("Click me")
///     .style(|s| s.keyboard_navigable());
///
/// // A custom scrollable container: focusable for arrow keys, but not in tab order
/// let _scrollable = Empty::new()
///     .scroll()
///     .style(|s| s.focusable());
///
/// // A purely decorative element that should never receive focus
/// let _decorative = Container::new(Empty::new())
///     .style(|s| s.focus_none());
/// ```
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
