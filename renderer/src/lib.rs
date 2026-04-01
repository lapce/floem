//! Shared renderer-facing types for the Floem UI framework.
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

use std::sync::Arc;

use imaging::record::CustomCommand;
use peniko::kurbo::{Affine, Rect};

pub mod text;
pub use resvg::tiny_skia;
pub use resvg::usvg;

pub mod gpu_resources;
pub mod rasterizer;
pub use rasterizer::{
    BeginFrame, CpuBufferFormat, CpuBufferTarget, CustomRenderer, GpuTextureTarget, RenderCore,
    Renderer, SceneRenderer, SceneTargetRenderer, TargetRenderer,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinishMode {
    GpuTexture,
    CpuImage,
}

pub enum RenderOutput {
    Image(peniko::ImageData),
    GpuTexture(wgpu::TextureView),
}

#[derive(Clone, Debug)]
pub struct OwnedSvg {
    pub tree: Arc<usvg::Tree>,
    pub hash: Arc<[u8]>,
}

impl PartialEq for OwnedSvg {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DisplayCommandExt {
    DrawSvg {
        svg: OwnedSvg,
        rect: Rect,
        transform: Affine,
        brush: Option<peniko::Brush>,
    },
}

impl CustomCommand for DisplayCommandExt {
    fn prepend_transform(&self, prefix: Affine) -> Self {
        match self {
            Self::DrawSvg {
                svg,
                rect,
                transform,
                brush,
            } => Self::DrawSvg {
                svg: svg.clone(),
                rect: *rect,
                transform: prefix * *transform,
                brush: brush.clone(),
            },
        }
    }
}

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
