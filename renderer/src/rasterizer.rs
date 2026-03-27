use imaging::{CustomPaintSink, PaintSink, record::Glyph};
use peniko::{ImageData, kurbo::Size};

use crate::{DisplayCommandExt, RenderOutput};

#[derive(Clone, Copy, Debug)]
pub struct BeginFrame {
    pub size: Size,
    pub scale: f64,
    pub font_embolden: f32,
}

#[derive(Debug)]
pub enum RasterizerOutput {
    Image(ImageData),
    GpuTexture(wgpu::TextureView),
}

impl RasterizerOutput {
    pub fn into_image(self) -> Option<ImageData> {
        match self {
            Self::Image(image) => Some(image),
            Self::GpuTexture(_) => None,
        }
    }
}

impl From<RenderOutput> for RasterizerOutput {
    fn from(value: RenderOutput) -> Self {
        match value {
            RenderOutput::Image(image) => Self::Image(image),
            RenderOutput::GpuTexture(texture) => Self::GpuTexture(texture),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum CpuBufferFormat {
    Rgba8Opaque,
    Bgra8Opaque,
}

pub struct CpuBufferTarget<'a> {
    pub buffer: &'a mut [u8],
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: usize,
    pub format: CpuBufferFormat,
}

pub struct GpuTextureTarget {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub texture_view: wgpu::TextureView,
}

pub enum RasterIntoBackend<R, G, C> {
    Null,
    Rasterizer(R),
    Gpu(G),
    Cpu(C),
}

pub enum GpuOrRasterizer<G = (), R = Box<dyn SceneRasterizer>> {
    Gpu(G),
    Rasterizer(R),
}

pub enum CpuOrRasterizer<'a, C = (), R = Box<dyn SceneRasterizer>> {
    Cpu(C),
    Rasterizer(R),
    _Marker(std::marker::PhantomData<&'a mut ()>),
}

pub trait RasterCore {
    fn with_paint_sink(&mut self, f: &mut dyn FnMut(&mut dyn PaintSink));
    fn finish(&mut self);
    fn readback(&mut self) -> Option<RasterizerOutput>;
}

pub trait Rasterizer: RasterCore {
    fn begin(&mut self, frame: BeginFrame);
}

pub trait RasterTarget: RasterCore + Sized {
    type Target;

    fn create(target: Self::Target) -> Result<Self, String>;
}

pub trait CustomRasterizer: RasterCore {
    fn with_custom_paint_sink(
        &mut self,
        f: &mut dyn FnMut(&mut dyn CustomPaintSink<DisplayCommandExt>),
    );
    fn debug_info(&self) -> String;
}

pub trait SceneRasterizer: Rasterizer + CustomRasterizer {}
impl<T> SceneRasterizer for T where T: Rasterizer + CustomRasterizer {}

pub trait SceneTargetRasterizer: RasterTarget + CustomRasterizer {}
impl<T> SceneTargetRasterizer for T where T: RasterTarget + CustomRasterizer {}

pub type GlyphIter<'a> = dyn Iterator<Item = Glyph> + 'a;
