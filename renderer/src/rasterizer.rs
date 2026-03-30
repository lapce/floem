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

pub trait RenderCore {
    fn render(&mut self, f: &mut dyn FnMut(&mut dyn PaintSink));
    fn finish(&mut self);
    fn readback(&mut self) -> Option<RasterizerOutput>;
}

pub trait Renderer: RenderCore {
    type Target;

    fn set_size(&mut self, frame: BeginFrame);
    fn reset(&mut self);
    fn read_target(&mut self) -> Option<Self::Target>;
}

pub trait TargetRenderer: RenderCore + Sized {
    type Target;

    fn create(frame: BeginFrame, target: Self::Target) -> Result<Self, String>;
}

pub trait CustomRenderer {
    fn with_custom_paint_sink(
        &mut self,
        f: &mut dyn FnMut(&mut dyn CustomPaintSink<DisplayCommandExt>),
    );
    fn debug_info(&self) -> String;
}

pub trait SceneRenderer: RenderCore + CustomRenderer {}
impl<T> SceneRenderer for T where T: RenderCore + CustomRenderer {}

pub trait SceneTargetRenderer: TargetRenderer + CustomRenderer {}
impl<T> SceneTargetRenderer for T where T: TargetRenderer + CustomRenderer {}

pub type GlyphIter<'a> = dyn Iterator<Item = Glyph> + 'a;
