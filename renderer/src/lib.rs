//! Rendering abstraction layer for the Floem UI framework.
//!
//! This crate defines the [`Renderer`] trait that all Floem rendering backends must
//! implement, along with supporting types for passing images and SVGs to the renderer.
//! Floem ships with three backend implementations:
//!
//! - **Vello** (`floem_vello_renderer`) — GPU-accelerated renderer using the Vello scene graph (default).
//! - **Vger** (`floem_vger_renderer`) — GPU-accelerated renderer using the Vger library.
//! - **tiny-skia** (`floem_tiny_skia_renderer`) — CPU software renderer using tiny-skia.
//!
//! # Modules
//!
//! - [`text`] — Text layout, shaping, and font management built on [Parley](https://docs.rs/parley).
//! - [`gpu_resources`] — Asynchronous wgpu adapter/device acquisition for GPU backends.
//!
//! # Re-exports
//!
//! [`tiny_skia`] and [`usvg`] are re-exported from [`resvg`](https://docs.rs/resvg) so that
//! renderer backends and downstream crates can use consistent versions of these libraries
//! without adding them as direct dependencies.

pub mod text;

use peniko::{
    kurbo::{Affine, Point, Rect, Shape, Stroke},
    BlendMode, BrushRef,
};
pub use resvg::tiny_skia;
pub use resvg::usvg;
use text::TextLayout;

pub mod gpu_resources;

/// A reference to a parsed SVG tree paired with a cache key.
///
/// # Example
///
/// ```no_run
/// use floem_renderer::{Svg, usvg};
///
/// let svg_text = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
///   <circle cx="12" cy="12" r="10" fill="currentColor"/>
/// </svg>"#;
///
/// let tree = usvg::Tree::from_str(svg_text, &usvg::Options::default()).unwrap();
/// let hash = b"my-icon-cache-key";
///
/// let svg = Svg {
///     tree: &tree,
///     hash: hash.as_slice(),
/// };
/// ```
pub struct Svg<'a> {
    /// The parsed SVG document.
    pub tree: &'a usvg::Tree,
    /// An opaque byte slice used as a cache key by the renderer.
    pub hash: &'a [u8],
}

/// A raster image paired with a cache key.
///
/// # Example
///
/// ```no_run
/// use floem_renderer::Img;
///
/// # fn make_image_brush() -> peniko::ImageBrush { todo!() }
/// let brush = make_image_brush();
/// let hash = b"photo-abc123";
///
/// let img = Img {
///     img: brush,
///     hash: hash.as_slice(),
/// };
/// ```
pub struct Img<'a> {
    /// The image data as a [`peniko::ImageBrush`].
    pub img: peniko::ImageBrush,
    /// An opaque byte slice used as a cache key by the renderer.
    pub hash: &'a [u8],
}

/// The core rendering trait that every Floem backend must implement.
///
/// A frame is bracketed by [`begin`](Renderer::begin) and [`finish`](Renderer::finish).
/// Between those calls the framework issues drawing commands — fills, strokes, text,
/// images, SVGs — which the backend records or executes immediately depending on its
/// architecture.
///
/// The typical call sequence within a single frame looks like:
///
/// ```text
/// renderer.begin(capture);
/// renderer.set_transform(..);
/// renderer.fill(..);          // background
/// renderer.clip(..);          // restrict drawing area
/// renderer.draw_text(..);     // labels, editors
/// renderer.draw_svg(..);      // icons
/// renderer.draw_img(..);      // photos
/// renderer.stroke(..);        // borders
/// renderer.clear_clip();
/// renderer.finish();
/// ```
pub trait Renderer {
    /// Begin a new frame.
    ///
    /// Must be called exactly once before any drawing commands.
    /// When `capture` is `true` the renderer should record the frame into an
    /// off-screen buffer so that [`finish`](Renderer::finish) can return it as
    /// an [`ImageBrush`](peniko::ImageBrush). This is used by the Floem
    /// inspector to take snapshots.
    fn begin(&mut self, capture: bool);

    /// Set the current affine transform in device/render-target coordinates.
    ///
    /// All subsequent drawing commands are transformed by `transform` until it
    /// is changed again. The framework provides this as the final transform for
    /// the current visual node, including window scaling, so backends should not
    /// apply an additional global window-scale multiply to ordinary geometry.
    ///
    /// Raster-backed operations such as glyph and SVG caching may still derive
    /// a rasterization scale from this transform to choose an appropriate pixel
    /// resolution.
    fn set_transform(&mut self, transform: Affine);

    /// Set the z-index for subsequent draw commands.
    ///
    /// Not all backends honour this — Vello and tiny-skia rely on painter's
    /// order instead.
    fn set_z_index(&mut self, z_index: i32);

    /// Clip all subsequent drawing to the interior of `shape`.
    ///
    /// The clip remains in effect until [`clear_clip`](Renderer::clear_clip) is
    /// called. On the Vello backend clipping is implemented via
    /// [`push_layer`](Renderer::push_layer) / [`pop_layer`](Renderer::pop_layer)
    /// instead, so this method may be a no-op there.
    fn clip(&mut self, shape: &impl Shape);

    /// Remove the current clip region, allowing drawing to the full surface.
    fn clear_clip(&mut self);

    /// Stroke the outline of a [`Shape`].
    ///
    /// The `brush` defines the color or gradient and `stroke` controls the
    /// line width, join style, dash pattern, etc.
    fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<peniko::BrushRef<'b>>,
        stroke: &'s Stroke,
    );

    /// Fill the interior of a [`Shape`] using the [non-zero fill rule].
    ///
    /// When `blur_radius` is greater than zero the fill is drawn with a
    /// Gaussian blur, which is used for box shadows.
    ///
    /// [non-zero fill rule]: https://en.wikipedia.org/wiki/Nonzero-rule
    fn fill<'b>(&mut self, path: &impl Shape, brush: impl Into<BrushRef<'b>>, blur_radius: f64);

    /// Push a compositing layer onto the layer stack.
    ///
    /// Drawing commands issued after this call are composited into the layer.
    /// Call [`pop_layer`](Renderer::pop_layer) to flatten the layer back into
    /// the parent using the specified `blend` mode and `alpha`.
    ///
    /// The `clip` shape restricts drawing within the layer, and `transform` is
    /// applied to the layer contents.
    fn push_layer(
        &mut self,
        blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    );

    /// Pop the topmost compositing layer pushed by [`push_layer`](Renderer::push_layer).
    ///
    /// The layer contents are composited into the parent surface using the
    /// blend mode and alpha that were specified when the layer was pushed.
    fn pop_layer(&mut self);

    /// Draw a [`TextLayout`] at the given position.
    ///
    /// The `pos` parameter specifies the upper-left corner of the layout object
    /// (even for right-to-left text).
    fn draw_text(&mut self, layout: &TextLayout, pos: impl Into<Point>);

    /// Draw an SVG image inside `rect`.
    ///
    /// When `brush` is `Some`, the SVG is rendered as a mask and filled with
    /// the given brush — this is how Floem applies a color override to icons.
    fn draw_svg<'b>(&mut self, svg: Svg<'b>, rect: Rect, brush: Option<impl Into<BrushRef<'b>>>);

    /// Draw a raster image inside `rect`.
    ///
    /// The image is scaled to fit the destination rectangle.
    fn draw_img(&mut self, img: Img<'_>, rect: Rect);

    /// Finish the current frame and present it.
    ///
    /// If the frame was started with `capture = true`, the rendered content is
    /// returned as an [`ImageBrush`](peniko::ImageBrush). Otherwise returns
    /// `None` after presenting the frame to the screen.
    fn finish(&mut self) -> Option<peniko::ImageBrush>;

    /// Return a human-readable string identifying the renderer backend.
    ///
    /// Used by the Floem inspector. Implementations typically return a string
    /// like `"name: Vello\ninfo: …"`.
    fn debug_info(&self) -> String {
        "Unknown".into()
    }
}
