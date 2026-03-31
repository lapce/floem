use std::{sync::Arc, sync::mpsc};

use imaging::{CustomPaintSink, PaintSink, record::Glyph};
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat, kurbo::Size};

use crate::{DisplayCommandExt, RenderOutput};

#[derive(Clone, Copy, Debug)]
pub struct BeginFrame {
    pub size: Size,
    pub scale: f64,
    pub font_embolden: f32,
}

impl RenderOutput {
    pub fn into_image(self) -> Option<ImageData> {
        match self {
            RenderOutput::Image(image) => Some(image),
            RenderOutput::GpuTexture(_) => None,
        }
    }

    pub fn into_image_with(self, device: &wgpu::Device, queue: &wgpu::Queue) -> Option<ImageData> {
        match self {
            RenderOutput::Image(image) => Some(image),
            RenderOutput::GpuTexture(texture) => {
                read_texture_view_to_image(&texture, device, queue).ok()
            }
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
    fn readback(&mut self) -> Option<RenderOutput>;
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

fn read_texture_view_to_image(
    texture_view: &wgpu::TextureView,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<ImageData, String> {
    let texture = texture_view.texture();
    let size = texture.size();
    let width = size.width;
    let height = size.height;
    let (image_format, bytes_per_pixel) = match texture.format() {
        wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => {
            (ImageFormat::Rgba8, 4usize)
        }
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => {
            (ImageFormat::Bgra8, 4usize)
        }
        format => {
            return Err(format!(
                "unsupported texture format for readback: {format:?}"
            ));
        }
    };
    let width_bytes = width as usize * bytes_per_pixel;
    let padded_bytes_per_row = (width_bytes as u32).div_ceil(256) * 256;

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Floem Renderer Readback"),
        size: u64::from(padded_bytes_per_row) * u64::from(height),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Floem Renderer Readback"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        size,
    );
    queue.submit([encoder.finish()]);

    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|_| "device poll failed".to_string())?;
    rx.recv()
        .map_err(|_| "map_async callback dropped".to_string())?
        .map_err(|_| "buffer map failed".to_string())?;

    let mapped = slice.get_mapped_range();
    let mut data = Vec::with_capacity(width_bytes * height as usize);
    for row in mapped.chunks_exact(padded_bytes_per_row as usize) {
        data.extend_from_slice(&row[..width_bytes]);
    }
    drop(mapped);
    readback.unmap();

    Ok(ImageData {
        data: Blob::new(Arc::new(data)),
        format: image_format,
        width,
        height,
        alpha_type: ImageAlphaType::Alpha,
    })
}
