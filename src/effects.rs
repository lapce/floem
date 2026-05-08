//! Compositor-aware filters, shaders, and composites.
//!
//! Floem uses Imaging's generic group API with [`Filter`] and
//! [`Composite`] payloads. Standard Imaging filters/composites keep flowing to
//! renderers. Floem-only shader filters and sources are consumed while lowering
//! the display list and executed as compositor render passes when needed.

use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use floem_reactive::UpdaterEffect;
use imaging::{
    Composite as ImagingComposite, Filter as ImagingFilter, GroupRef as ImagingGroupRef,
    record::Clip,
};
use peniko::{
    ImageData,
    kurbo::{Affine, Size},
};
use subduction::wgpu::SurfaceColorSpace;

use crate::{
    Application,
    animate::easing::{Bezier, Easing, Linear, Spring},
    app::UserEvent,
    compositor_surface::SurfaceSlotId,
    gradient::Gradient,
    platform::{Duration, Instant},
};

/// A Floem group filter.
///
/// Standard Imaging filters are forwarded to renderers unchanged. Floem-only filters are consumed
/// during Floem's compositor lowering step and are not exposed to renderer backends that do not
/// understand them.
#[derive(Clone, Debug, PartialEq)]
pub enum Filter {
    /// A renderer-supported Imaging filter.
    Imaging(ImagingFilter),
    /// A color-only shader that transforms the current pixel color.
    Color(ColorFilter),
    /// A shader that can sample an isolated input layer.
    Layer(LayerFilter),
}

/// An executable compositor shader pass.
#[derive(Clone, Debug, PartialEq)]
pub enum CompositorShader {
    /// A shader pass that transforms the current pixel color.
    Color(ColorFilter),
    /// A shader pass that samples the previous pass output.
    Layer(LayerFilter),
    /// A shader pass that fills a target without sampling previous output.
    Source(ShaderSource),
}

/// A compositor shader pass plus the isolated group clip that constrains its output.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompositorShaderPass {
    pub shader: CompositorShader,
    pub clip: Option<Clip>,
    pub position_transform: Affine,
}

impl From<ImagingFilter> for Filter {
    fn from(filter: ImagingFilter) -> Self {
        Self::Imaging(filter)
    }
}

impl From<ColorFilter> for Filter {
    fn from(filter: ColorFilter) -> Self {
        Self::Color(filter)
    }
}

impl From<LayerFilter> for Filter {
    fn from(filter: LayerFilter) -> Self {
        Self::Layer(filter)
    }
}

/// Image payload accepted by Floem image brushes.
#[derive(Clone, Debug, PartialEq)]
pub enum Image {
    /// Raster image content.
    Raster(RasterImage),
    /// Retained scene image content.
    Scene(SceneImage),
    /// Compositor/external surface content lowered to an Imaging external image
    /// during display-list processing.
    Surface(SurfaceImage),
    /// Compositor-generated image content.
    Source(ShaderSourceImage),
}

impl Image {
    /// Borrow this image payload.
    #[must_use]
    pub fn as_ref(&self) -> ImageRef<'_> {
        match self {
            Self::Raster(image) => ImageRef::Raster(image),
            Self::Scene(image) => ImageRef::Scene(image),
            Self::Surface(image) => ImageRef::Surface(image),
            Self::Source(source) => ImageRef::Source(source),
        }
    }

    #[must_use]
    pub fn intrinsic_size(&self) -> Size {
        match self {
            Self::Raster(image) => image.intrinsic_size,
            Self::Scene(image) => image.intrinsic_size,
            Self::Surface(surface) => surface.intrinsic_size,
            Self::Source(source) => source.intrinsic_size,
        }
    }
}

impl TryFrom<imaging::Image> for Image {
    type Error = imaging::ExternalImage;

    fn try_from(image: imaging::Image) -> Result<Self, Self::Error> {
        match image {
            imaging::Image::Raster(image) => Ok(Self::Raster(image.into())),
            imaging::Image::Scene(image) => Ok(Self::Scene(image.into())),
            imaging::Image::External(image) => Err(image),
        }
    }
}

impl From<ImageData> for Image {
    fn from(image: ImageData) -> Self {
        Self::Raster(image.into())
    }
}

impl From<imaging::SceneImage> for Image {
    fn from(image: imaging::SceneImage) -> Self {
        Self::Scene(image.into())
    }
}

impl From<SurfaceImage> for Image {
    fn from(image: SurfaceImage) -> Self {
        Self::Surface(image)
    }
}

impl From<ShaderSourceImage> for Image {
    fn from(image: ShaderSourceImage) -> Self {
        Self::Source(image)
    }
}

/// Floem brush payload.
///
/// This is Imaging's generic brush container fully specialized for Floem image
/// payloads and Peniko gradients. Renderer-backed images and
/// compositor-generated shader sources share the same brush path until the
/// display-list recorder lowers source images into compositor passes.
pub type Brush = imaging::Brush<ImageBrush, Gradient>;

/// Alpha helpers for Floem's concrete brush specialization.
pub trait BrushAlphaExt {
    /// Return the brush with the alpha component set to `alpha`.
    #[must_use]
    fn with_alpha(self, alpha: f32) -> Self;

    /// Return the brush with its alpha component multiplied by `alpha`.
    #[must_use]
    #[track_caller]
    fn multiply_alpha(self, alpha: f32) -> Self;
}

impl BrushAlphaExt for Brush {
    fn with_alpha(self, alpha: f32) -> Self {
        match self {
            Self::Solid(color) => Self::Solid(color.with_alpha(alpha)),
            Self::Gradient(gradient) => Self::Gradient(gradient.with_alpha(alpha)),
            Self::Image(image) => Self::Image(image.with_alpha(alpha)),
        }
    }

    fn multiply_alpha(self, alpha: f32) -> Self {
        debug_assert!(
            alpha.is_finite() && alpha >= 0.0,
            "A non-finite or negative alpha ({alpha}) is meaningless."
        );
        if alpha == 1.0 {
            self
        } else {
            match self {
                Self::Solid(color) => Self::Solid(color.multiply_alpha(alpha)),
                Self::Gradient(gradient) => Self::Gradient(gradient.multiply_alpha(alpha)),
                Self::Image(image) => Self::Image(image.multiply_alpha(alpha)),
            }
        }
    }
}

/// Borrowed Floem image payload.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ImageRef<'a> {
    /// Borrowed raster image content.
    Raster(&'a RasterImage),
    /// Borrowed retained scene image content.
    Scene(&'a SceneImage),
    /// Borrowed compositor/external surface image content.
    Surface(&'a SurfaceImage),
    /// Borrowed compositor-generated image content.
    Source(&'a ShaderSourceImage),
}

impl ImageRef<'_> {
    #[must_use]
    pub fn to_owned(&self) -> Image {
        match self {
            Self::Raster(image) => Image::Raster((*image).clone()),
            Self::Scene(image) => Image::Scene((*image).clone()),
            Self::Surface(image) => Image::Surface((*image).clone()),
            Self::Source(source) => Image::Source((*source).clone()),
        }
    }
}

/// Floem-owned image brush.
///
/// This mirrors [`peniko::ImageBrush`] and Imaging's image brush wrapper: the
/// generic `D` is the image payload, while Peniko owns the sampler, extend
/// modes, quality hint, and alpha multiplier. Use [`Image`] for owned Floem
/// image content and [`ImageRef`] for borrowed content.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageBrush<D = Image> {
    /// Peniko image brush payload and sampler.
    pub brush: peniko::ImageBrush<D>,
}

impl<D> Deref for ImageBrush<D> {
    type Target = peniko::ImageBrush<D>;

    fn deref(&self) -> &Self::Target {
        &self.brush
    }
}

impl<D> DerefMut for ImageBrush<D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.brush
    }
}

impl<D> ImageBrush<D> {
    /// Builder method for setting the image extend mode in both directions.
    #[must_use]
    pub fn with_extend(mut self, mode: peniko::Extend) -> Self {
        self.brush = self.brush.with_extend(mode);
        self
    }

    /// Builder method for setting the image extend mode in the horizontal direction.
    #[must_use]
    pub fn with_x_extend(mut self, mode: peniko::Extend) -> Self {
        self.brush = self.brush.with_x_extend(mode);
        self
    }

    /// Builder method for setting the image extend mode in the vertical direction.
    #[must_use]
    pub fn with_y_extend(mut self, mode: peniko::Extend) -> Self {
        self.brush = self.brush.with_y_extend(mode);
        self
    }

    /// Builder method for setting the desired image quality hint.
    #[must_use]
    pub fn with_quality(mut self, quality: peniko::ImageQuality) -> Self {
        self.brush = self.brush.with_quality(quality);
        self
    }

    /// Return the image with the alpha multiplier set to `alpha`.
    #[must_use]
    #[track_caller]
    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self.brush = self.brush.with_alpha(alpha);
        self
    }

    /// Return the image with its alpha multiplier multiplied by `alpha`.
    #[must_use]
    #[track_caller]
    pub fn multiply_alpha(mut self, alpha: f32) -> Self {
        self.brush = self.brush.multiply_alpha(alpha);
        self
    }
}

impl ImageBrush {
    /// Create a new image brush with default sampling.
    #[must_use]
    pub fn new(image: impl Into<Image>) -> Self {
        Self {
            brush: peniko::ImageBrush {
                image: image.into(),
                sampler: peniko::ImageSampler::default(),
            },
        }
    }

    /// Borrow this image brush.
    #[must_use]
    pub fn as_ref(&self) -> ImageBrushRef<'_> {
        ImageBrush {
            brush: peniko::ImageBrush {
                image: self.image.as_ref(),
                sampler: self.sampler,
            },
        }
    }
}

impl From<Image> for ImageBrush {
    fn from(image: Image) -> Self {
        Self::new(image)
    }
}

impl From<imaging::ImageBrush> for ImageBrush {
    fn from(image: imaging::ImageBrush) -> Self {
        let image_payload = match image.image.clone() {
            imaging::Image::Raster(image) => Image::Raster(image.into()),
            imaging::Image::Scene(image) => Image::Scene(image.into()),
            imaging::Image::External(_) => {
                panic!("imaging::ExternalImage cannot be stored in a Floem image brush")
            }
        };
        Self {
            brush: peniko::ImageBrush {
                image: image_payload,
                sampler: image.sampler,
            },
        }
    }
}

impl From<imaging::ImageBrushRef<'_>> for ImageBrush {
    fn from(image: imaging::ImageBrushRef<'_>) -> Self {
        let image_payload = match image.image {
            imaging::ImageRef::Raster(image) => Image::Raster(image.clone().into()),
            imaging::ImageRef::Scene(image) => Image::Scene(image.clone().into()),
            imaging::ImageRef::External(_) => {
                panic!("imaging::ExternalImage cannot be stored in a Floem image brush")
            }
        };
        Self {
            brush: peniko::ImageBrush {
                image: image_payload,
                sampler: image.sampler,
            },
        }
    }
}

impl From<ShaderSourceImage> for ImageBrush {
    fn from(image: ShaderSourceImage) -> Self {
        Self::new(image)
    }
}

impl From<SurfaceImage> for ImageBrush {
    fn from(image: SurfaceImage) -> Self {
        Self::new(image)
    }
}

/// Borrowed Floem image brush.
pub type ImageBrushRef<'a> = ImageBrush<ImageRef<'a>>;

impl From<ImageBrushRef<'_>> for ImageBrush {
    fn from(value: ImageBrushRef<'_>) -> Self {
        Self {
            brush: peniko::ImageBrush {
                image: value.image.to_owned(),
                sampler: value.sampler,
            },
        }
    }
}

impl From<ImageBrush> for Brush {
    fn from(value: ImageBrush) -> Self {
        Self::Image(value)
    }
}

impl From<Image> for Brush {
    fn from(value: Image) -> Self {
        Self::Image(ImageBrush::from(value))
    }
}

impl From<SurfaceImage> for Brush {
    fn from(value: SurfaceImage) -> Self {
        Self::Image(ImageBrush::from(value))
    }
}

impl From<ShaderSourceImage> for Brush {
    fn from(value: ShaderSourceImage) -> Self {
        Self::Image(ImageBrush::from(value))
    }
}

impl From<&ImageBrush> for Brush {
    fn from(value: &ImageBrush) -> Self {
        Self::Image(value.as_ref().into())
    }
}

impl From<Gradient> for Brush {
    fn from(value: Gradient) -> Self {
        Self::Gradient(value)
    }
}

/// Raster image content with its intrinsic source size.
#[derive(Clone, Debug, PartialEq)]
pub struct RasterImage {
    pub image: ImageData,
    pub intrinsic_size: Size,
}

impl RasterImage {
    #[must_use]
    pub fn new(image: ImageData) -> Self {
        let intrinsic_size = Size::new(f64::from(image.width), f64::from(image.height));
        Self {
            image,
            intrinsic_size,
        }
    }
}

impl From<ImageData> for RasterImage {
    fn from(image: ImageData) -> Self {
        Self::new(image)
    }
}

/// Retained scene image content with its intrinsic source size.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneImage {
    pub image: imaging::SceneImage,
    pub intrinsic_size: Size,
}

impl SceneImage {
    #[must_use]
    pub fn new(image: imaging::SceneImage) -> Self {
        let intrinsic_size = Size::new(f64::from(image.width()), f64::from(image.height()));
        Self {
            image,
            intrinsic_size,
        }
    }
}

impl From<imaging::SceneImage> for SceneImage {
    fn from(image: imaging::SceneImage) -> Self {
        Self::new(image)
    }
}

/// Shader source image content with its intrinsic source size.
///
/// Shader sources are shader programs, not image brushes. Create one of these
/// explicit image payloads with [`ShaderSource::image`] before using a source
/// as an [`ImageBrush`] payload.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderSourceImage {
    pub source: ShaderSource,
    pub intrinsic_size: Size,
}

/// Floem image identity for compositor/external surface content.
///
/// Surface images stay as Floem image payloads in paint commands. They are
/// converted to [`imaging::ExternalImage`] only when Floem lowers the display
/// list for renderer/compositor consumption.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceImage {
    /// Stable image-placement identity. Multiple image handles can reference
    /// the same producer surface with independent source sizing.
    pub(crate) image_id: SurfaceImageId,
    /// Stable slot identity used to resolve submitted surface content.
    pub slot_id: SurfaceSlotId,
    /// Intrinsic source size for brush sampling.
    pub intrinsic_size: Size,
}

impl SurfaceImage {
    /// Create a new surface image identity with a concrete intrinsic source size.
    #[must_use]
    pub fn new(slot_id: SurfaceSlotId, intrinsic_size: Size) -> Self {
        Self {
            image_id: SurfaceImageId::next(),
            slot_id,
            intrinsic_size,
        }
    }
}

static NEXT_SURFACE_IMAGE_ID: AtomicU64 = AtomicU64::new(1);

/// Internal stable identity for one Floem image handle backed by a compositor surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SurfaceImageId(u64);

impl SurfaceImageId {
    fn next() -> Self {
        Self(NEXT_SURFACE_IMAGE_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// A Floem group composite operation.
///
/// This currently wraps Imaging's composite type and leaves room for compositor-only shader
/// compositing without changing renderer-facing Imaging APIs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Composite {
    /// A renderer-supported Imaging composite operation.
    Imaging(ImagingComposite),
    /// A compositor-only shader composite.
    Shader(ShaderComposite),
}

impl Default for Composite {
    fn default() -> Self {
        Self::Imaging(ImagingComposite::default())
    }
}

impl From<ImagingComposite> for Composite {
    fn from(composite: ImagingComposite) -> Self {
        Self::Imaging(composite)
    }
}

impl From<ShaderComposite> for Composite {
    fn from(effect: ShaderComposite) -> Self {
        Self::Shader(effect)
    }
}

/// Group reference used by Floem's painter.
///
/// Filters are applied in slice order. Floem may insert intermediate render
/// passes so each shader observes the output of the previous filter/effect.
pub type GroupRef<'a> = ImagingGroupRef<'a, Filter, Composite>;

/// Returns an empty Floem shader group.
///
/// Start from this when using `Painter::with_group` with Floem-specific
/// shader filters or sources:
///
/// ```ignore
/// let filters = [filter.into(), imaging::Filter::blur(5.0).into()];
/// painter.with_group(group_ref().with_filters(&filters), |painter| {
///     // isolated content
/// });
/// ```
#[must_use]
pub fn group_ref<'a>() -> GroupRef<'a> {
    GroupRef {
        clip: None,
        mask: None,
        filters: &[],
        composite: Composite::default(),
    }
}

static NEXT_COLOR_EFFECT_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_SHADER_EFFECT_ID: AtomicU64 = AtomicU64::new(1);

fn next_color_filter_id() -> ColorFilterId {
    ColorFilterId(NEXT_COLOR_EFFECT_ID.fetch_add(1, Ordering::Relaxed))
}

fn next_shader_effect_id() -> ShaderEffectId {
    ShaderEffectId(NEXT_SHADER_EFFECT_ID.fetch_add(1, Ordering::Relaxed))
}

/// Internal stable identifier for a reusable compositor shader program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ColorFilterId(pub u64);

/// Internal stable identifier for reusable compositor source/shader programs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ShaderEffectId(pub u64);

/// A SwiftUI-style color filter applied to an isolated compositor subtree.
///
/// The effect receives the already-sampled source color for the destination
/// pixel. It cannot sample neighboring pixels or sample the input layer at a
/// different coordinate. Use [`LayerFilter`] when the shader needs layer
/// sampling.
///
/// Shader bodies are written in logical window coordinates by default. Use
/// [`ColorFilter::wgsl`] for the generated shader wrapper and available
/// parameters. Animated or app-driven values should be passed explicitly
/// through [`ShaderUniform`].
#[derive(Clone, Debug, PartialEq)]
pub struct ColorFilter {
    pub(crate) id: ColorFilterId,
    pub shader: ColorFilterShader,
    pub args: ShaderArgs,
    pub color_space: SurfaceColorSpace,
}

impl ColorFilter {
    /// Creates a WGSL color filter from a function body.
    ///
    /// The body must be valid WGSL statements for a function that returns
    /// `vec4<f32>`. Floem wraps it in a complete fullscreen shader. The
    /// generated source has this shape:
    ///
    /// ```wgsl
    /// struct ShaderArgs {
    ///     data: vec4<u32>,
    /// };
    ///
    /// struct ShaderFrame {
    ///     effective_scale: f32,
    ///     target_width: f32,
    ///     target_height: f32,
    ///     clip_mask_enabled: f32,
    ///     position_transform0: vec4<f32>,
    ///     position_transform1: vec4<f32>,
    /// };
    ///
    /// // Floem samples the source texture before calling `color_filter`.
    /// // The texture and sampler bindings are internal; use `LayerFilter`
    /// // when shader code needs to sample the input layer.
    /// @group(0) @binding(2) var<uniform> args: ShaderArgs;
    /// @group(0) @binding(3) var<uniform> frame: ShaderFrame;
    ///
    /// struct VsOut {
    ///     @builtin(position) position: vec4<f32>,
    ///     @location(0) uv: vec2<f32>,
    /// };
    ///
    /// @vertex
    /// fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    ///     var out: VsOut;
    ///     let x = f32(i32(vi & 1u)) * 4.0 - 1.0;
    ///     let y = f32(i32(vi >> 1u)) * 4.0 - 1.0;
    ///     out.position = vec4<f32>(x, y, 0.0, 1.0);
    ///     out.uv = vec2<f32>(x, -y) * 0.5 + vec2<f32>(0.5, 0.5);
    ///     return out;
    /// }
    ///
    /// fn color_filter(
    ///     position: vec2<f32>, // logical pixels, top-left origin
    ///     uv: vec2<f32>,       // normalized texture coordinates
    ///     color: vec4<f32>,    // sampled source color at uv
    ///     args: ShaderArgs,
    ///     frame: ShaderFrame,
    /// ) -> vec4<f32> {
    ///     // fragment_body
    /// }
    ///
    /// @fragment
    /// fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    ///     let target_position = in.position.xy / vec2<f32>(frame.effective_scale);
    ///     let logical_position = vec2<f32>(
    ///         frame.position_transform0.x * target_position.x + frame.position_transform0.z * target_position.y + frame.position_transform1.x,
    ///         frame.position_transform0.y * target_position.x + frame.position_transform0.w * target_position.y + frame.position_transform1.y,
    ///     );
    ///     let color = /* sampled source color */;
    ///     return color_filter(logical_position, in.uv, color, args, frame);
    /// }
    /// ```
    #[must_use]
    pub fn wgsl(fragment_body: impl Into<Arc<str>>) -> Self {
        Self::wgsl_with_id(next_color_filter_id(), fragment_body)
    }

    pub(crate) fn wgsl_with_id(id: ColorFilterId, fragment_body: impl Into<Arc<str>>) -> Self {
        Self {
            id,
            shader: ColorFilterShader::Wgsl {
                label: None,
                fragment_body: fragment_body.into(),
            },
            args: ShaderArgs::default(),
            color_space: SurfaceColorSpace::ExtendedLinearSrgb,
        }
    }

    /// Adds a human-readable label for GPU debugging and pipeline caching.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<Arc<str>>) -> Self {
        match &mut self.shader {
            ColorFilterShader::Wgsl { label: slot, .. } => *slot = Some(label.into()),
        }
        self
    }

    /// Sets raw uniform argument bytes for the effect.
    #[must_use]
    pub fn with_args(mut self, args: impl Into<Vec<u8>>) -> Self {
        self.args = ShaderArgs::new(args);
        self
    }

    #[must_use]
    pub fn with_uniforms<T: ShaderUniforms + Send + 'static>(
        mut self,
        uniforms: ShaderUniform<T>,
    ) -> Self {
        uniforms.attach_current_view_if_unset();
        self.args = ShaderArgs::from_uniforms(uniforms);
        self
    }

    #[must_use]
    pub fn with_derived_uniforms<T, F>(mut self, uniforms: F) -> Self
    where
        T: ShaderUniforms + Send + 'static,
        F: Fn() -> T + 'static,
    {
        self.args = ShaderArgs::from_uniforms(ShaderUniform::derived(uniforms));
        self
    }

    /// Sets the working/output color space for this effect.
    #[must_use]
    pub fn with_color_space(mut self, color_space: SurfaceColorSpace) -> Self {
        self.color_space = color_space;
        self
    }
}

/// A SwiftUI-style layer filter applied to an isolated compositor subtree.
///
/// The effect is evaluated over an input texture containing the already-rendered
/// subtree. Backends expose the input texture and sampler to the shader, so the
/// shader may either use the pre-sampled `color` value or sample the input
/// texture at another `uv`.
///
/// Like [`ColorFilter`], `position` is in logical pixels and `uv` is normalized
/// texture space. Use `frame.effective_scale` to convert to physical pixels.
/// Use [`LayerFilter::wgsl`] for the generated shader wrapper and available
/// parameters. Animated or app-driven values should be passed explicitly
/// through [`ShaderUniform`].
#[derive(Clone, Debug, PartialEq)]
pub struct LayerFilter {
    pub(crate) id: ColorFilterId,
    pub shader: LayerFilterShader,
    pub args: ShaderArgs,
    pub color_space: SurfaceColorSpace,
}

impl LayerFilter {
    /// Creates a WGSL layer filter from a function body.
    ///
    /// The body must be valid WGSL statements for a function that returns
    /// `vec4<f32>`. Floem wraps it in a complete fullscreen shader. The
    /// generated source has this shape:
    ///
    /// ```wgsl
    /// struct ShaderArgs {
    ///     data: vec4<u32>,
    /// };
    ///
    /// struct ShaderFrame {
    ///     effective_scale: f32,
    ///     target_width: f32,
    ///     target_height: f32,
    ///     clip_mask_enabled: f32,
    ///     position_transform0: vec4<f32>,
    ///     position_transform1: vec4<f32>,
    /// };
    ///
    /// @group(0) @binding(0) var input_texture: texture_2d<f32>;
    /// @group(0) @binding(1) var input_sampler: sampler;
    /// @group(0) @binding(2) var<uniform> args: ShaderArgs;
    /// @group(0) @binding(3) var<uniform> frame: ShaderFrame;
    ///
    /// struct VsOut {
    ///     @builtin(position) position: vec4<f32>,
    ///     @location(0) uv: vec2<f32>,
    /// };
    ///
    /// @vertex
    /// fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    ///     var out: VsOut;
    ///     let x = f32(i32(vi & 1u)) * 4.0 - 1.0;
    ///     let y = f32(i32(vi >> 1u)) * 4.0 - 1.0;
    ///     out.position = vec4<f32>(x, y, 0.0, 1.0);
    ///     out.uv = vec2<f32>(x, -y) * 0.5 + vec2<f32>(0.5, 0.5);
    ///     return out;
    /// }
    ///
    /// fn layer_filter(
    ///     position: vec2<f32>, // logical pixels, top-left origin
    ///     uv: vec2<f32>,       // normalized texture coordinates
    ///     color: vec4<f32>,    // sampled input-layer color at uv
    ///     args: ShaderArgs,
    ///     frame: ShaderFrame,
    /// ) -> vec4<f32> {
    ///     // fragment_body
    /// }
    ///
    /// @fragment
    /// fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    ///     let target_position = in.position.xy / vec2<f32>(frame.effective_scale);
    ///     let logical_position = vec2<f32>(
    ///         frame.position_transform0.x * target_position.x + frame.position_transform0.z * target_position.y + frame.position_transform1.x,
    ///         frame.position_transform0.y * target_position.x + frame.position_transform0.w * target_position.y + frame.position_transform1.y,
    ///     );
    ///     let color = textureSample(input_texture, input_sampler, in.uv);
    ///     return layer_filter(logical_position, in.uv, color, args, frame);
    /// }
    /// ```
    #[must_use]
    pub fn wgsl(fragment_body: impl Into<Arc<str>>) -> Self {
        Self::wgsl_with_id(next_color_filter_id(), fragment_body)
    }

    pub(crate) fn wgsl_with_id(id: ColorFilterId, fragment_body: impl Into<Arc<str>>) -> Self {
        Self {
            id,
            shader: LayerFilterShader::Wgsl {
                label: None,
                fragment_body: fragment_body.into(),
            },
            args: ShaderArgs::default(),
            color_space: SurfaceColorSpace::ExtendedLinearSrgb,
        }
    }

    /// Adds a human-readable label for GPU debugging and pipeline caching.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<Arc<str>>) -> Self {
        match &mut self.shader {
            LayerFilterShader::Wgsl { label: slot, .. } => *slot = Some(label.into()),
        }
        self
    }

    /// Sets raw uniform argument bytes for the effect.
    #[must_use]
    pub fn with_args(mut self, args: impl Into<Vec<u8>>) -> Self {
        self.args = ShaderArgs::new(args);
        self
    }

    #[must_use]
    pub fn with_uniforms<T: ShaderUniforms + Send + 'static>(
        mut self,
        uniforms: ShaderUniform<T>,
    ) -> Self {
        uniforms.attach_current_view_if_unset();
        self.args = ShaderArgs::from_uniforms(uniforms);
        self
    }

    #[must_use]
    pub fn with_derived_uniforms<T, F>(mut self, uniforms: F) -> Self
    where
        T: ShaderUniforms + Send + 'static,
        F: Fn() -> T + 'static,
    {
        self.args = ShaderArgs::from_uniforms(ShaderUniform::derived(uniforms));
        self
    }

    /// Sets the working/output color space for this effect.
    #[must_use]
    pub fn with_color_space(mut self, color_space: SurfaceColorSpace) -> Self {
        self.color_space = color_space;
        self
    }
}

/// A no-input shader that generates a compositor texture from position, uv, args, and frame data.
///
/// This is useful for SwiftUI-style generated visual content: procedural gradients, noise,
/// checkerboards, animated backgrounds, or any shader-backed image that does not sample an input
/// layer. Like [`ColorFilter`], `position` is in logical pixels and `uv` is normalized texture
/// space. Use `frame.effective_scale` to convert to physical pixels. Use [`ShaderSource::wgsl`]
/// for the generated shader wrapper and available parameters. Animated or app-driven values should
/// be passed explicitly through [`ShaderUniform`].
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderSource {
    pub(crate) id: ShaderEffectId,
    pub shader: ShaderSourceShader,
    pub args: ShaderArgs,
    pub color_space: SurfaceColorSpace,
}

impl ShaderSource {
    /// Creates an image payload for this shader source.
    ///
    /// Shader sources must be given a concrete intrinsic source size before
    /// they can be painted with an [`ImageBrush`]. Use
    /// [`ShaderSourceImage::view`] for percentage- or length-based brush views.
    #[must_use]
    pub fn image(self, intrinsic_size: impl Into<Size>) -> ShaderSourceImage {
        ShaderSourceImage {
            source: self,
            intrinsic_size: intrinsic_size.into(),
        }
    }

    /// Creates a WGSL shader source from a function body.
    ///
    /// The body must be valid WGSL statements for a function that returns
    /// `vec4<f32>`. Floem wraps it in a complete fullscreen shader. The
    /// generated source has this shape:
    ///
    /// ```wgsl
    /// struct ShaderArgs {
    ///     data: vec4<u32>,
    /// };
    ///
    /// struct ShaderFrame {
    ///     effective_scale: f32,
    ///     target_width: f32,
    ///     target_height: f32,
    ///     clip_mask_enabled: f32,
    ///     position_transform0: vec4<f32>,
    ///     position_transform1: vec4<f32>,
    /// };
    ///
    /// // Floem uses an internal texture only to apply the generated source
    /// // to the requested image shape. Shader sources do not receive an
    /// // input texture; use `LayerFilter` when shader code needs sampling.
    /// @group(0) @binding(2) var<uniform> args: ShaderArgs;
    /// @group(0) @binding(3) var<uniform> frame: ShaderFrame;
    ///
    /// struct VsOut {
    ///     @builtin(position) position: vec4<f32>,
    ///     @location(0) uv: vec2<f32>,
    /// };
    ///
    /// @vertex
    /// fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    ///     var out: VsOut;
    ///     let x = f32(i32(vi & 1u)) * 4.0 - 1.0;
    ///     let y = f32(i32(vi >> 1u)) * 4.0 - 1.0;
    ///     out.position = vec4<f32>(x, y, 0.0, 1.0);
    ///     out.uv = vec2<f32>(x, -y) * 0.5 + vec2<f32>(0.5, 0.5);
    ///     return out;
    /// }
    ///
    /// fn shader_source(
    ///     position: vec2<f32>, // logical pixels, top-left origin
    ///     uv: vec2<f32>,       // normalized texture coordinates
    ///     args: ShaderArgs,
    ///     frame: ShaderFrame,
    /// ) -> vec4<f32> {
    ///     // fragment_body
    /// }
    ///
    /// @fragment
    /// fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    ///     let target_position = in.position.xy / vec2<f32>(frame.effective_scale);
    ///     let logical_position = vec2<f32>(
    ///         frame.position_transform0.x * target_position.x + frame.position_transform0.z * target_position.y + frame.position_transform1.x,
    ///         frame.position_transform0.y * target_position.x + frame.position_transform0.w * target_position.y + frame.position_transform1.y,
    ///     );
    ///     return shader_source(logical_position, in.uv, args, frame);
    /// }
    /// ```
    #[must_use]
    pub fn wgsl(fragment_body: impl Into<Arc<str>>) -> Self {
        Self::wgsl_with_id(next_shader_effect_id(), fragment_body)
    }

    pub(crate) fn wgsl_with_id(id: ShaderEffectId, fragment_body: impl Into<Arc<str>>) -> Self {
        Self {
            id,
            shader: ShaderSourceShader::Wgsl {
                label: None,
                fragment_body: fragment_body.into(),
            },
            args: ShaderArgs::default(),
            color_space: SurfaceColorSpace::ExtendedLinearSrgb,
        }
    }

    /// Adds a human-readable label for GPU debugging and pipeline caching.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<Arc<str>>) -> Self {
        match &mut self.shader {
            ShaderSourceShader::Wgsl { label: slot, .. } => *slot = Some(label.into()),
        }
        self
    }

    /// Sets raw uniform argument bytes for the source shader.
    #[must_use]
    pub fn with_args(mut self, args: impl Into<Vec<u8>>) -> Self {
        self.args = ShaderArgs::new(args);
        self
    }

    #[must_use]
    pub fn with_uniforms<T: ShaderUniforms + Send + 'static>(
        mut self,
        uniforms: ShaderUniform<T>,
    ) -> Self {
        uniforms.attach_current_view_if_unset();
        self.args = ShaderArgs::from_uniforms(uniforms);
        self
    }

    #[must_use]
    pub fn with_derived_uniforms<T, F>(mut self, uniforms: F) -> Self
    where
        T: ShaderUniforms + Send + 'static,
        F: Fn() -> T + 'static,
    {
        self.args = ShaderArgs::from_uniforms(ShaderUniform::derived(uniforms));
        self
    }

    /// Sets the working/output color space for this shader source.
    #[must_use]
    pub fn with_color_space(mut self, color_space: SurfaceColorSpace) -> Self {
        self.color_space = color_space;
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ShaderSourceShader {
    /// WGSL function body for the generated `shader_source` function.
    ///
    /// Prefer [`ShaderSource::wgsl`] when constructing this variant; that
    /// method documents the generated wrapper and available shader inputs.
    Wgsl {
        label: Option<Arc<str>>,
        fragment_body: Arc<str>,
    },
}

/// A future compositor shader blend between source and backdrop.
///
/// This is intentionally separate from filters: a composite shader needs both the isolated source
/// layer and the already-rendered backdrop. The current compositor can carry this in the generic
/// group API, but execution requires a backdrop render pass and currently fails loudly if used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShaderComposite {
    /// Stable program identifier for the composite shader.
    pub(crate) id: ShaderEffectId,
}

impl ShaderComposite {
    /// Creates a compositor composite effect with an automatically assigned
    /// internal identity.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: next_shader_effect_id(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ColorFilterShader {
    /// WGSL function body for the generated `color_filter` function.
    ///
    /// Prefer [`ColorFilter::wgsl`] when constructing this variant; that
    /// method documents the generated wrapper and available shader inputs.
    Wgsl {
        label: Option<Arc<str>>,
        fragment_body: Arc<str>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayerFilterShader {
    /// WGSL function body for the generated `layer_filter` function.
    ///
    /// Prefer [`LayerFilter::wgsl`] when constructing this variant; that
    /// method documents the generated wrapper and available shader inputs.
    Wgsl {
        label: Option<Arc<str>>,
        fragment_body: Arc<str>,
    },
}

#[derive(Clone)]
pub struct ShaderArgs {
    source: ShaderArgsSource,
}

#[derive(Clone)]
enum ShaderArgsSource {
    Static(Arc<[u8]>),
    Dynamic(Arc<dyn ShaderUniformProvider>),
}

impl Default for ShaderArgs {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Debug for ShaderArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShaderArgs")
            .field("bytes_len", &self.bytes().len())
            .field("revision", &self.revision())
            .finish()
    }
}

impl PartialEq for ShaderArgs {
    fn eq(&self, other: &Self) -> bool {
        self.revision() == other.revision() && self.bytes() == other.bytes()
    }
}

impl Eq for ShaderArgs {}

impl ShaderArgs {
    #[must_use]
    pub fn new(args: impl Into<Vec<u8>>) -> Self {
        Self {
            source: ShaderArgsSource::Static(args.into().into()),
        }
    }

    #[must_use]
    pub fn from_uniforms<T: ShaderUniforms + Send + 'static>(uniforms: ShaderUniform<T>) -> Self {
        Self {
            source: ShaderArgsSource::Dynamic(uniforms.inner),
        }
    }

    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        match &self.source {
            ShaderArgsSource::Static(bytes) => bytes.to_vec(),
            ShaderArgsSource::Dynamic(provider) => provider.bytes(),
        }
    }

    #[must_use]
    pub fn revision(&self) -> u64 {
        match &self.source {
            ShaderArgsSource::Static(_) => 0,
            ShaderArgsSource::Dynamic(provider) => provider.revision(),
        }
    }
}

trait ShaderUniformProvider: Send + Sync {
    fn bytes(&self) -> Vec<u8>;
    fn revision(&self) -> u64;
}

/// Typed uniform payload accepted by Floem compositor effects.
pub trait ShaderUniforms: Clone + PartialEq + Debug + 'static {
    fn uniform_bytes(&self) -> Vec<u8>;
}

/// Uniform payload that can be interpolated by [`ShaderUniform::transition`].
pub trait AnimatableShaderUniforms: ShaderUniforms {
    fn interpolate(from: &Self, to: &Self, t: f64) -> Self;
}

impl ShaderUniforms for Vec<u8> {
    fn uniform_bytes(&self) -> Vec<u8> {
        self.clone()
    }
}

impl<const N: usize> ShaderUniforms for [u8; N] {
    fn uniform_bytes(&self) -> Vec<u8> {
        self.to_vec()
    }
}

impl ShaderUniforms for f32 {
    fn uniform_bytes(&self) -> Vec<u8> {
        self.to_ne_bytes().to_vec()
    }
}

impl AnimatableShaderUniforms for f32 {
    fn interpolate(from: &Self, to: &Self, t: f64) -> Self {
        *from + (*to - *from) * t as f32
    }
}

impl<const N: usize> ShaderUniforms for [f32; N] {
    fn uniform_bytes(&self) -> Vec<u8> {
        self.iter().flat_map(|value| value.to_ne_bytes()).collect()
    }
}

impl<const N: usize> AnimatableShaderUniforms for [f32; N] {
    fn interpolate(from: &Self, to: &Self, t: f64) -> Self {
        std::array::from_fn(|index| f32::interpolate(&from[index], &to[index], t))
    }
}

/// Sendable transition descriptor for effect uniforms.
#[derive(Clone)]
pub struct ShaderTransition {
    pub duration: Duration,
    pub easing: Arc<dyn Easing + Send + Sync>,
}

impl Debug for ShaderTransition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShaderTransition")
            .field("duration", &self.duration)
            .field("easing", &self.easing)
            .finish()
    }
}

impl ShaderTransition {
    #[must_use]
    pub fn new(duration: Duration, easing: impl Easing + Send + Sync + 'static) -> Self {
        Self {
            duration,
            easing: Arc::new(easing),
        }
    }

    #[must_use]
    pub fn linear(duration: Duration) -> Self {
        Self::new(duration, Linear)
    }

    #[must_use]
    pub fn ease_in_out(duration: Duration) -> Self {
        Self::new(duration, Bezier::ease_in_out())
    }

    #[must_use]
    pub fn spring(duration: Duration) -> Self {
        Self::new(duration, Spring::default())
    }
}

struct RunningShaderTransition<T> {
    transition: ShaderTransition,
    started_at: Instant,
    from: T,
    to: T,
}

struct ShaderUniformState<T> {
    target: T,
    presentation: T,
    running: Option<RunningShaderTransition<T>>,
    revision: u64,
    window_id: Option<winit::window::WindowId>,
}

/// App-owned handle for updating and animating effect uniforms.
pub struct ShaderUniform<T: ShaderUniforms + Send + 'static> {
    inner: Arc<ShaderUniformInner<T>>,
}

impl<T: ShaderUniforms + Send + 'static> Clone for ShaderUniform<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

struct ShaderUniformInner<T: ShaderUniforms + Send + 'static> {
    state: Mutex<ShaderUniformState<T>>,
    frame_requested: AtomicBool,
}

impl<T: ShaderUniforms + Send + 'static> ShaderUniform<T> {
    #[must_use]
    pub fn new(initial: T) -> Self {
        Self {
            inner: Arc::new(ShaderUniformInner {
                state: Mutex::new(ShaderUniformState {
                    target: initial.clone(),
                    presentation: initial,
                    running: None,
                    revision: 0,
                    window_id: None,
                }),
                frame_requested: AtomicBool::new(false),
            }),
        }
    }

    #[must_use]
    pub fn derived(uniforms: impl Fn() -> T + 'static) -> Self {
        let initial = uniforms();
        let handle = Self::new(initial);
        handle.attach_current_view_if_unset();
        let update_handle = handle.clone();
        UpdaterEffect::new(uniforms, move |uniforms| {
            update_handle.set(uniforms);
        });
        handle
    }

    pub fn set(&self, value: T) {
        let changed = {
            let mut state = self.inner.state.lock().unwrap();
            state.running = None;
            state.target = value.clone();
            if state.presentation == value {
                false
            } else {
                state.presentation = value;
                state.revision = state.revision.wrapping_add(1);
                true
            }
        };
        if changed {
            self.request_repaint();
        }
    }

    pub fn update(&self, update: impl FnOnce(&mut T)) {
        let mut value = self.target();
        update(&mut value);
        self.set(value);
    }

    #[must_use]
    pub fn get(&self) -> T {
        self.inner.state.lock().unwrap().presentation.clone()
    }

    #[must_use]
    pub fn target(&self) -> T {
        self.inner.state.lock().unwrap().target.clone()
    }

    pub fn cancel_animation(&self) {
        let changed = {
            let mut state = self.inner.state.lock().unwrap();
            let was_running = state.running.take().is_some();
            if was_running {
                state.target = state.presentation.clone();
                state.revision = state.revision.wrapping_add(1);
            }
            was_running
        };
        if changed {
            self.request_repaint();
        }
    }

    pub fn finish_animation(&self) {
        let changed = {
            let mut state = self.inner.state.lock().unwrap();
            let Some(running) = state.running.take() else {
                return;
            };
            state.presentation = running.to.clone();
            state.target = running.to;
            state.revision = state.revision.wrapping_add(1);
            true
        };
        if changed {
            self.request_repaint();
        }
    }

    fn attach_current_view_if_unset(&self) {
        let mut state = self.inner.state.lock().unwrap();
        if state.window_id.is_none() {
            state.window_id = crate::window::handle::get_current_view().window_id();
        }
    }

    fn request_repaint(&self) {
        let window_id = self.inner.state.lock().unwrap().window_id;
        if let Some(window_id) = window_id {
            Application::send_proxy_event(UserEvent::WindowPaint { window_id });
        }
    }
}

impl<T: AnimatableShaderUniforms + Send + 'static> ShaderUniform<T> {
    pub fn transition(&self, transition: ShaderTransition, update: impl FnOnce(&mut T)) {
        let changed = {
            let mut state = self.inner.state.lock().unwrap();
            let mut target = state.target.clone();
            update(&mut target);
            if state.target == target && state.running.is_none() {
                false
            } else {
                let from = state.presentation.clone();
                state.target = target.clone();
                state.running = Some(RunningShaderTransition {
                    transition,
                    started_at: Instant::now(),
                    from,
                    to: target,
                });
                true
            }
        };
        if changed {
            self.request_repaint();
            self.request_animation_frame();
        }
    }

    fn step_at(&self, now: Instant) -> bool {
        let mut state = self.inner.state.lock().unwrap();
        let Some(running) = state.running.as_ref() else {
            return false;
        };
        let transition = running.transition.clone();
        let started_at = running.started_at;
        let from = running.from.clone();
        let to = running.to.clone();
        let elapsed = now.saturating_duration_since(started_at);
        let time_percent = elapsed.as_secs_f64() / transition.duration.as_secs_f64();
        let finished = elapsed >= transition.duration && transition.easing.finished(time_percent);
        let next = if finished {
            to.clone()
        } else {
            T::interpolate(&from, &to, transition.easing.eval(time_percent))
        };
        let changed = state.presentation != next;
        if changed {
            state.presentation = next;
            state.revision = state.revision.wrapping_add(1);
        }
        if finished {
            state.running = None;
            state.presentation = to;
        }
        changed || finished
    }

    fn is_animating(&self) -> bool {
        self.inner.state.lock().unwrap().running.is_some()
    }

    fn request_animation_frame(&self) {
        if self.inner.frame_requested.swap(true, Ordering::Relaxed) {
            return;
        }
        let handle = self.clone();
        crate::action::request_animation_frame(move |frame_time| {
            let now = frame_time
                .interval
                .predicted_present
                .unwrap_or(frame_time.now);
            if handle.step_at(now) {
                handle.request_repaint();
            }
            if !handle.is_animating() {
                handle.inner.frame_requested.store(false, Ordering::Relaxed);
            }
        });
    }
}

impl<T: ShaderUniforms + Send + 'static> ShaderUniformProvider for ShaderUniformInner<T> {
    fn bytes(&self) -> Vec<u8> {
        self.state.lock().unwrap().presentation.uniform_bytes()
    }

    fn revision(&self) -> u64 {
        self.state.lock().unwrap().revision
    }
}

/// Uniform values made available to shader programs from frame timing.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShaderFrameUniform {
    pub effective_scale: f32,
    pub target_width: f32,
    pub target_height: f32,
    pub clip_mask_enabled: f32,
    pub position_transform0: [f32; 4],
    pub position_transform1: [f32; 4],
}
