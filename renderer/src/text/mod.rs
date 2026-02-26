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

// --- Cursor/Hit Testing ---

/// Cursor affinity — which side of a character boundary the cursor is on.
///
/// When a byte index falls at a line break, affinity determines whether the
/// cursor is drawn at the end of the previous visual line ([`Before`](Affinity::Before))
/// or the start of the next ([`After`](Affinity::After)).
///
/// Converts to and from [`parley::layout::Affinity`] (`Upstream` / `Downstream`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Affinity {
    /// The cursor sits before (upstream of) the character at this index.
    #[default]
    Before,
    /// The cursor sits after (downstream of) the character at this index.
    After,
}

impl From<Affinity> for parley::layout::Affinity {
    fn from(a: Affinity) -> Self {
        match a {
            Affinity::Before => parley::layout::Affinity::Upstream,
            Affinity::After => parley::layout::Affinity::Downstream,
        }
    }
}

impl From<parley::layout::Affinity> for Affinity {
    fn from(a: parley::layout::Affinity) -> Self {
        match a {
            parley::layout::Affinity::Upstream => Affinity::Before,
            parley::layout::Affinity::Downstream => Affinity::After,
        }
    }
}

/// A text cursor position expressed as a paragraph line and a byte offset within
/// that line.
///
/// Produced by [`TextLayout::hit`] when converting an (x, y) point to a text
/// position, and consumed by [`TextLayout::hit_point`] and
/// [`TextLayout::cursor_to_byte_index`].
///
/// # Example
///
/// ```
/// use floem_renderer::text::{Affinity, Cursor};
///
/// // Cursor at the start of the second paragraph line.
/// let cursor = Cursor::new(1, 0);
/// assert_eq!(cursor.line, 1);
/// assert_eq!(cursor.index, 0);
/// assert_eq!(cursor.affinity, Affinity::Before);
///
/// // With explicit affinity.
/// let cursor = Cursor::new_with_affinity(0, 5, Affinity::After);
/// assert_eq!(cursor.affinity, Affinity::After);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cursor {
    /// Paragraph line index (zero-based).
    pub line: usize,
    /// Byte offset within the paragraph line.
    pub index: usize,
    /// Which side of the character boundary this cursor is on.
    pub affinity: Affinity,
}

impl Cursor {
    /// Create a cursor with [`Affinity::Before`] (the default).
    pub fn new(line: usize, index: usize) -> Self {
        Self {
            line,
            index,
            affinity: Affinity::Before,
        }
    }

    /// Create a cursor with an explicit affinity.
    pub fn new_with_affinity(line: usize, index: usize, affinity: Affinity) -> Self {
        Self {
            line,
            index,
            affinity,
        }
    }
}

impl PartialOrd for Affinity {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Affinity {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
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

    // -- Affinity --

    #[test]
    fn affinity_default_is_before() {
        assert_eq!(Affinity::default(), Affinity::Before);
    }

    #[test]
    fn affinity_ordering() {
        assert!(Affinity::Before < Affinity::After);
        assert!(Affinity::Before <= Affinity::Before);
    }

    #[test]
    fn affinity_parley_roundtrip() {
        let before: parley::layout::Affinity = Affinity::Before.into();
        let after: parley::layout::Affinity = Affinity::After.into();
        assert_eq!(Affinity::from(before), Affinity::Before);
        assert_eq!(Affinity::from(after), Affinity::After);
    }

    // -- Cursor --

    #[test]
    fn cursor_new_has_before_affinity() {
        let c = Cursor::new(2, 10);
        assert_eq!(c.line, 2);
        assert_eq!(c.index, 10);
        assert_eq!(c.affinity, Affinity::Before);
    }

    #[test]
    fn cursor_new_with_affinity() {
        let c = Cursor::new_with_affinity(1, 5, Affinity::After);
        assert_eq!(c.line, 1);
        assert_eq!(c.index, 5);
        assert_eq!(c.affinity, Affinity::After);
    }

    #[test]
    fn cursor_ordering() {
        // Cursor derives Ord: line first, then index, then affinity.
        let a = Cursor::new(0, 5);
        let b = Cursor::new(1, 0);
        assert!(a < b, "different lines");

        let c = Cursor::new(0, 3);
        assert!(c < a, "same line, different index");

        let d = Cursor::new_with_affinity(0, 5, Affinity::After);
        assert!(a < d, "same line+index, different affinity");
    }

    // -- TextBrush --

    #[test]
    fn text_brush_default_is_opaque_black() {
        let b = TextBrush::default();
        let c: peniko::Color = b.into();
        assert_eq!(c, peniko::Color::from_rgba8(0, 0, 0, 255));
    }

    #[test]
    fn text_brush_color_roundtrip() {
        let red = peniko::Color::from_rgba8(255, 0, 0, 128);
        let brush = TextBrush::from(red);
        assert_eq!(brush.0, red);
        let back: peniko::Color = brush.into();
        assert_eq!(back, red);
    }
}
