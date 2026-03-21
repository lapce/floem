//! Text layout, shaping, and font management for Floem.
//!
//! This module provides the renderer-facing text vocabulary built on
//! [Parley](https://docs.rs/parley).
//!
//! # Re-exports
//!
//! [`FontStyle`] and [`FontWidth`] come from [`fontique`](https://docs.rs/fontique).

mod attrs;

pub use attrs::{Attrs, AttrsList, AttrsOwned, FamilyOwned, LineHeightValue};
pub use fontique::{FontStyle, FontWeight, FontWidth};
pub use imaging::{GlyphRunRef, NormalizedCoord};
pub use parley::layout::Glyph;

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

pub trait GlyphDrawer {
    fn draw_glyphs<'a>(
        &mut self,
        origin: peniko::kurbo::Point,
        run: &GlyphRunRef<'a>,
        glyphs: impl Iterator<Item = Glyph> + 'a,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- TextBrush --

    #[test]
    fn text_brush_default_is_opaque_black() {
        let b = TextBrush::default();
        let c: peniko::Color = b.into();
        assert_eq!(c, peniko::Color::from_rgba8(0, 0, 0, 255));
    }

    #[test]
    fn text_glyphs_props_default_is_usable() {
        let font = peniko::FontData::new(peniko::Blob::new(std::sync::Arc::new([])), 0);
        let style = peniko::Style::Fill(peniko::Fill::NonZero);
        let run = GlyphRunRef::new(&font, &style, peniko::color::palette::css::BLACK);
        assert_eq!(run.font, &font);
        assert_eq!(run.font_size, 16.0);
        assert_eq!(run.composite.alpha, 1.0);
        assert_eq!(run.transform, peniko::kurbo::Affine::IDENTITY);
        assert!(run.normalized_coords.is_empty());
    }
}
