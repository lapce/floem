//! Text layout, shaping, and font management for Floem.
//!
//! This module provides the text rendering infrastructure built on
//! [Parley](https://docs.rs/parley). The central type is [`TextLayout`], which shapes
//! and positions text for display. Attributes such as font family, weight, and color
//! are described with [`Attrs`] and collected into an [`AttrsList`] that maps byte ranges
//! to styling.
//!
//! # Quick start
//!
//! ```no_run
//! use floem_renderer::text::{Attrs, AttrsList, FontWeight, TextLayout};
//!
//! let attrs = Attrs::new()
//!     .font_size(18.0)
//!     .weight(FontWeight::BOLD);
//! let attrs_list = AttrsList::new(attrs);
//!
//! let layout = TextLayout::new_with_text("Hello, Floem!", attrs_list, None);
//! let size = layout.size();
//! ```
//!
//! # Re-exports
//!
//! [`FontStyle`] and [`FontWidth`] come from [`fontique`](https://docs.rs/fontique), and
//! [`Alignment`] from [`parley`](https://docs.rs/parley). They are re-exported here so
//! that downstream crates do not need to depend on those libraries directly.

mod attrs;
mod layout;

pub use attrs::{Attrs, AttrsList, AttrsOwned, FamilyOwned, LineHeightValue};
pub use fontique::{FontStyle, FontWeight, FontWidth};
pub use layout::{HitPoint, HitPosition, TextLayout, FONT_CONTEXT};
pub use parley::Alignment;
pub use parley::Affinity;
pub use parley::Cursor;

// --- Font Properties ---

/// Text wrapping strategy.
///
/// Controls how [`TextLayout`] breaks long lines when a maximum width is set
/// via [`TextLayout::set_size`].
///
/// # Example
///
/// ```no_run
/// use floem_renderer::text::{Attrs, AttrsList, TextLayout, Wrap};
///
/// let mut layout = TextLayout::new();
/// layout.set_wrap(Wrap::WordOrGlyph);
/// layout.set_text("A long paragraph…", AttrsList::new(Attrs::new()), None);
/// layout.set_size(200.0, f32::MAX);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Wrap {
    /// No wrapping — text extends beyond the layout width.
    None,
    /// Break at any glyph boundary.
    Glyph,
    /// Break at word boundaries (default).
    #[default]
    Word,
    /// Break at word boundaries, but fall back to glyph boundaries when a
    /// single word is wider than the available width.
    WordOrGlyph,
}

/// Line ending style.
///
/// Represents the newline convention of a text document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum LineEnding {
    /// Unix-style line feed (`\n`).
    #[default]
    Lf,
    /// Windows-style carriage return + line feed (`\r\n`).
    CrLf,
    /// Classic Mac-style carriage return (`\r`).
    Cr,
    /// No line ending.
    None,
}

// --- Brush type for Parley ---

/// A brush wrapper that satisfies Parley's `Brush` trait bound.
///
/// Parley requires its brush type to implement `Default + Clone`. This newtype
/// wraps [`peniko::Color`] and provides a `Default` of opaque black.
/// It is used internally to parameterise [`parley::layout::Layout<TextBrush>`]
/// and is not typically constructed by application code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextBrush(pub peniko::Color);

impl Default for TextBrush {
    fn default() -> Self {
        TextBrush(peniko::Color::from_rgba8(0, 0, 0, 255))
    }
}

impl From<peniko::Color> for TextBrush {
    fn from(c: peniko::Color) -> Self {
        TextBrush(c)
    }
}

impl From<TextBrush> for peniko::Color {
    fn from(b: TextBrush) -> Self {
        b.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Wrap --

    #[test]
    fn wrap_default_is_word() {
        assert_eq!(Wrap::default(), Wrap::Word);
    }

    // -- LineEnding --

    #[test]
    fn line_ending_default_is_lf() {
        assert_eq!(LineEnding::default(), LineEnding::Lf);
    }

    // -- TextBrush --

    #[test]
    fn text_brush_default_is_opaque_black() {
        let b = TextBrush::default();
        let c: peniko::Color = b.into();
        assert_eq!(c, peniko::Color::from_rgba8(0, 0, 0, 255));
    }
}
