//! Text layout, shaping, and font management for Floem.
//!
//! This module provides the renderer-facing text vocabulary built on
//! [Parley](https://docs.rs/parley).
//!
//! # Re-exports
//!
//! [`FontStyle`] and [`FontWidth`] come from [`fontique`](https://docs.rs/fontique).

mod attrs;

use peniko::{
    BrushRef, Fill, FontData, StyleRef,
    kurbo::Affine,
};

pub use attrs::{Attrs, AttrsList, AttrsOwned, FamilyOwned, LineHeightValue};
pub use fontique::{FontStyle, FontWeight, FontWidth};
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

/// Variable font design-space coordinate.
pub type NormalizedCoord = i16;

/// Rendering properties shared by a glyph run.
#[derive(Clone, Debug)]
pub struct TextGlyphsProps<'a> {
    pub font: FontData,
    pub font_size: f32,
    pub hint: bool,
    pub normalized_coords: &'a [NormalizedCoord],
    pub style: StyleRef<'a>,
    pub brush: BrushRef<'a>,
    pub brush_alpha: f32,
    pub transform: Affine,
    pub glyph_transform: Option<Affine>,
}

impl<'a> TextGlyphsProps<'a> {
    pub fn new(font: &FontData) -> Self {
        Self {
            font: font.clone(),
            font_size: 16.0,
            hint: false,
            normalized_coords: &[],
            style: Fill::NonZero.into(),
            brush: peniko::color::palette::css::BLACK.into(),
            brush_alpha: 1.0,
            transform: Affine::IDENTITY,
            glyph_transform: None,
        }
    }

    pub fn font(mut self, font: &FontData) -> Self {
        self.font = font.clone();
        self
    }

    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn hint(mut self, hint: bool) -> Self {
        self.hint = hint;
        self
    }

    pub fn normalized_coords(mut self, normalized_coords: &'a [NormalizedCoord]) -> Self {
        self.normalized_coords = normalized_coords;
        self
    }

    pub fn style(mut self, style: impl Into<StyleRef<'a>>) -> Self {
        self.style = style.into();
        self
    }

    pub fn brush(mut self, brush: impl Into<BrushRef<'a>>) -> Self {
        self.brush = brush.into();
        self
    }

    pub fn brush_alpha(mut self, brush_alpha: f32) -> Self {
        self.brush_alpha = brush_alpha;
        self
    }

    pub fn transform(mut self, transform: Affine) -> Self {
        self.transform = transform;
        self
    }

    pub fn glyph_transform(mut self, glyph_transform: Option<Affine>) -> Self {
        self.glyph_transform = glyph_transform;
        self
    }
}

pub trait TextRun {
    fn props(&self) -> TextGlyphsProps<'_>;
    fn glyphs(&self) -> impl Iterator<Item = Glyph> + Clone + '_;
}

pub trait TextLine {
    type Run<'a>: TextRun
    where
        Self: 'a;

    fn runs(&self) -> impl Iterator<Item = Self::Run<'_>> + Clone + '_;
}

/// A generic glyph run backed by any cloneable glyph iterator.
pub struct GlyphRun<'a, G> {
    props: TextGlyphsProps<'a>,
    glyphs: G,
}

impl<'a, G> GlyphRun<'a, G> {
    pub fn new(font: &'a FontData, glyphs: G) -> Self {
        Self {
            props: TextGlyphsProps::new(font),
            glyphs,
        }
    }

    pub fn props(mut self, props: TextGlyphsProps<'a>) -> Self {
        self.props = props;
        self
    }
}

impl<G> TextRun for GlyphRun<'_, G>
where
    G: Iterator<Item = Glyph> + Clone,
{
    fn props(&self) -> TextGlyphsProps<'_> {
        self.props.clone()
    }

    fn glyphs(&self) -> impl Iterator<Item = Glyph> + Clone + '_ {
        self.glyphs.clone()
    }
}

/// A generic text line backed by any cloneable run iterator.
pub struct Line<R> {
    runs: R,
}

impl<R> Line<R> {
    pub fn new(runs: R) -> Self {
        Self { runs }
    }
}

impl<R, Run> TextLine for Line<R>
where
    R: Iterator<Item = Run> + Clone,
    Run: TextRun,
{
    type Run<'a>
        = Run
    where
        Self: 'a;

    fn runs(&self) -> impl Iterator<Item = Self::Run<'_>> + Clone + '_ {
        self.runs.clone()
    }
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
        let font = FontData::new(peniko::Blob::new(std::sync::Arc::new([])), 0);
        let props = TextGlyphsProps::new(&font);
        assert_eq!(props.font, font);
        assert_eq!(props.font_size, 16.0);
        assert_eq!(props.brush_alpha, 1.0);
        assert_eq!(props.transform, Affine::IDENTITY);
        assert!(props.normalized_coords.is_empty());
    }
}
