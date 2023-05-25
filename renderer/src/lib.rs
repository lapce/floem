pub use cosmic_text;
use cosmic_text::TextLayout;
use peniko::{
    kurbo::{Affine, Point, Rect, Shape},
    BrushRef,
};
pub use resvg::tiny_skia;
pub use resvg::usvg;

pub struct Svg<'a> {
    pub tree: &'a usvg::Tree,
    pub hash: &'a [u8],
}

pub trait Renderer {
    fn begin(&mut self);

    fn transform(&mut self, transform: Affine);

    /// Clip to a [`Shape`].
    fn clip(&mut self, shape: &impl Shape);

    fn clear_clip(&mut self);

    /// Stroke a [`Shape`], using the default [`StrokeStyle`].
    fn stroke<'b>(&mut self, shape: &impl Shape, brush: impl Into<BrushRef<'b>>, width: f64);

    /// Fill a [`Shape`], using the [non-zero fill rule].
    ///
    /// [non-zero fill rule]: https://en.wikipedia.org/wiki/Nonzero-rule
    fn fill<'b>(&mut self, path: &impl Shape, brush: impl Into<BrushRef<'b>>);

    /// Draw a [`TextLayout`].
    ///
    /// The `pos` parameter specifies the upper-left corner of the layout object
    /// (even for right-to-left text). To draw on a baseline, you can use
    /// [`TextLayout::line_metric`] to get the baseline position of a specific line.
    fn draw_text(&mut self, layout: &TextLayout, pos: impl Into<Point>);

    fn draw_svg<'b>(&mut self, svg: Svg<'b>, rect: Rect, brush: Option<impl Into<BrushRef<'b>>>);

    fn finish(&mut self);
}
