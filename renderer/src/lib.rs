pub mod swash;
pub mod text;

use crate::text::LayoutRun;
use peniko::{
    kurbo::{Affine, Point, Rect, Shape, Stroke},
    BlendMode, BrushRef,
};
pub use resvg::tiny_skia;
pub use resvg::usvg;
use text::TextLayout;

pub mod gpu_resources;

pub struct Svg<'a> {
    pub tree: &'a usvg::Tree,
    pub hash: &'a [u8],
}

pub struct Img<'a> {
    pub img: peniko::ImageBrush,
    pub hash: &'a [u8],
}

pub trait Renderer {
    fn begin(&mut self, capture: bool);

    fn set_transform(&mut self, transform: Affine);

    fn set_z_index(&mut self, z_index: i32);

    /// Clip to a [`Shape`].
    fn clip(&mut self, shape: &impl Shape);

    fn clear_clip(&mut self);

    /// Stroke a [`Shape`].
    fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<peniko::BrushRef<'b>>,
        stroke: &'s Stroke,
    );

    /// Fill a [`Shape`], using the [non-zero fill rule].
    ///
    /// [non-zero fill rule]: https://en.wikipedia.org/wiki/Nonzero-rule
    fn fill<'b>(&mut self, path: &impl Shape, brush: impl Into<BrushRef<'b>>, blur_radius: f64);

    /// Push a layer (This is not supported with Vger)
    fn push_layer(
        &mut self,
        blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    );

    /// Pop a layer (This is not supported with Vger)
    fn pop_layer(&mut self);

    /// Draw a [`TextLayout`].
    ///
    /// The `pos` parameter specifies the upper-left corner of the layout object
    /// (even for right-to-left text).
    fn draw_text(&mut self, layout: &TextLayout, pos: impl Into<Point>) {
        self.draw_text_with_layout(layout.layout_runs(), pos);
    }

    fn draw_text_with_layout<'b>(
        &mut self,
        layout: impl Iterator<Item = LayoutRun<'b>>,
        pos: impl Into<Point>,
    );

    fn draw_svg<'b>(&mut self, svg: Svg<'b>, rect: Rect, brush: Option<impl Into<BrushRef<'b>>>);

    fn draw_img(&mut self, img: Img<'_>, rect: Rect);

    fn finish(&mut self) -> Option<peniko::ImageBrush>;

    fn debug_info(&self) -> String {
        "Unknown".into()
    }
}
