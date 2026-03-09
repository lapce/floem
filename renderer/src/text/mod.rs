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

use peniko::{
    kurbo::{Affine, Point},
    BrushRef, Fill, FontData, StyleRef,
};

pub use attrs::{Attrs, AttrsList, AttrsOwned, FamilyOwned, LineHeightValue};
pub use fontique::{FontStyle, FontWeight, FontWidth};
pub use layout::{HitPoint, HitPosition, TextLayout, FONT_CONTEXT};
pub use parley::layout::Glyph;
pub use parley::Affinity;
pub use parley::Alignment;
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

pub struct LayoutLine<'a> {
    line: parley::layout::Line<'a, TextBrush>,
    origin: Point,
}

pub struct LayoutRun<'a> {
    glyph_run: parley::layout::GlyphRun<'a, TextBrush>,
    origin: Point,
}

impl TextRun for LayoutRun<'_> {
    fn props(&self) -> TextGlyphsProps<'_> {
        let run = self.glyph_run.run();
        let synthesis = run.synthesis();
        let glyph_transform = synthesis
            .skew()
            .map(|angle| Affine::skew((angle as f64).to_radians().tan(), 0.0));

        TextGlyphsProps::new(run.font())
            .font_size(run.font_size())
            .hint(false)
            .normalized_coords(run.normalized_coords())
            .style(Fill::NonZero)
            .brush(self.glyph_run.style().brush.0)
            .transform(Affine::translate((self.origin.x, self.origin.y)))
            .glyph_transform(glyph_transform)
    }

    fn glyphs(&self) -> impl Iterator<Item = Glyph> + Clone + '_ {
        self.glyph_run.positioned_glyphs()
    }
}

impl TextLine for LayoutLine<'_> {
    type Run<'a>
        = LayoutRun<'a>
    where
        Self: 'a;

    fn runs(&self) -> impl Iterator<Item = Self::Run<'_>> + Clone + '_ {
        self.line.items().filter_map(move |item| {
            let parley::layout::PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                return None;
            };

            Some(LayoutRun {
                glyph_run,
                origin: self.origin,
            })
        })
    }
}

impl TextLayout {
    /// Adapts this layout into an iterator of visual lines and glyph runs.
    pub fn layout_lines(
        &self,
        origin: impl Into<Point>,
    ) -> impl Iterator<Item = LayoutLine<'_>> + Clone {
        let origin = origin.into();
        self.parley_layout()
            .lines()
            .map(move |line| LayoutLine { line, origin })
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
