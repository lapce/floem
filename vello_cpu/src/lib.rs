use std::sync::Arc;

use anyhow::{Result, anyhow};
use floem_renderer::{
    BeginFrame, CpuBufferFormat, CpuBufferTarget, CustomRenderer, DisplayCommandExt,
    RenderCore, RenderOutput, Renderer, TargetRenderer,
};
use imaging::{
    BlurredRoundedRect, ClipRef, CustomPaintSink, FillRef, GlyphRunRef, GroupRef, PaintSink,
    RetainedDrawRef, StrokeRef,
};
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

struct VelloCpuCanvas<'a> {
    inner: &'a mut imaging_vello_cpu::VelloCpuRenderer,
}

impl PaintSink for VelloCpuCanvas<'_> {
    fn push_clip(&mut self, clip: ClipRef<'_>) {
        self.inner.push_clip(clip);
    }

    fn pop_clip(&mut self) {
        self.inner.pop_clip();
    }

    fn push_group(&mut self, group: GroupRef<'_>) {
        self.inner.push_group(group);
    }

    fn pop_group(&mut self) {
        self.inner.pop_group();
    }

    fn retained(&mut self, draw: RetainedDrawRef<'_>) {
        self.inner.retained(draw);
    }

    fn fill(&mut self, draw: FillRef<'_>) {
        self.inner.fill(draw);
    }

    fn stroke(&mut self, draw: StrokeRef<'_>) {
        self.inner.stroke(draw);
    }

    fn glyph_run(
        &mut self,
        draw: GlyphRunRef<'_>,
        glyphs: &mut dyn Iterator<Item = imaging::record::Glyph>,
    ) {
        self.inner.glyph_run(draw, glyphs);
    }

    fn blurred_rounded_rect(&mut self, draw: BlurredRoundedRect) {
        self.inner.blurred_rounded_rect(draw);
    }
}

impl CustomPaintSink<DisplayCommandExt> for VelloCpuCanvas<'_> {
    fn custom(&mut self, _command: &DisplayCommandExt) {}
}

pub struct VelloCpuRenderer {
    inner: imaging_vello_cpu::VelloCpuRenderer,
    width: u32,
    height: u32,
    finished_image: Option<ImageData>,
}

pub struct VelloCpuTargetRenderer<'a> {
    inner: imaging_vello_cpu::VelloCpuRenderer,
    target: CpuBufferTarget<'a>,
    finished_image: Option<ImageData>,
}

impl VelloCpuRenderer {
    pub fn new(width: u32, height: u32, _scale: f64, _font_embolden: f32) -> Result<Self> {
        let width_u16 =
            u16::try_from(width).map_err(|_| anyhow!("width exceeds vello_cpu limit"))?;
        let height_u16 =
            u16::try_from(height).map_err(|_| anyhow!("height exceeds vello_cpu limit"))?;
        Ok(Self {
            inner: imaging_vello_cpu::VelloCpuRenderer::new(width_u16, height_u16),
            width,
            height,
            finished_image: None,
        })
    }

    pub fn debug_info(&self) -> String {
        "name: Vello CPU\ninfo: imaging_vello_cpu".to_string()
    }

    pub fn inner(&mut self) -> &mut imaging_vello_cpu::VelloCpuRenderer {
        &mut self.inner
    }

    fn read_image(&mut self) -> Option<ImageData> {
        let data = self.inner.read_rgba8().ok()?;
        Some(ImageData {
            data: Blob::new(Arc::new(data)),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: self.width,
            height: self.height,
        })
    }
}

impl<'a> VelloCpuTargetRenderer<'a> {
    pub fn debug_info(&self) -> String {
        "name: Vello CPU\ninfo: imaging_vello_cpu".to_string()
    }

    pub fn inner(&mut self) -> &mut imaging_vello_cpu::VelloCpuRenderer {
        &mut self.inner
    }

    fn read_image_from_target(&self) -> ImageData {
        let data = match self.target.format {
            CpuBufferFormat::Rgba8Opaque => self.target.buffer.to_vec(),
            CpuBufferFormat::Bgra8Opaque => {
                let mut rgba = Vec::with_capacity(self.target.buffer.len());
                for pixel in self.target.buffer.chunks_exact(4) {
                    rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
                }
                rgba
            }
        };
        ImageData {
            data: Blob::new(Arc::new(data)),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: self.target.width,
            height: self.target.height,
        }
    }
}

impl RenderCore for VelloCpuRenderer {
    fn render(&mut self, f: &mut dyn FnMut(&mut dyn PaintSink)) {
        let mut canvas = VelloCpuCanvas {
            inner: &mut self.inner,
        };
        f(&mut canvas)
    }

    fn finish(&mut self) {
        self.finished_image = self.read_image();
    }

    fn readback(&mut self) -> Option<RenderOutput> {
        self.finished_image
            .clone()
            .or_else(|| self.read_image())
            .map(RenderOutput::Image)
    }
}

impl Renderer for VelloCpuRenderer {
    type Target = ImageData;

    fn set_size(&mut self, frame: BeginFrame) {
        let width = frame.size.width as u32;
        let height = frame.size.height as u32;
        if self.width != width || self.height != height {
            *self = Self::new(width, height, frame.scale, frame.font_embolden)
                .expect("failed to recreate VelloCpuRenderer");
        }
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.finished_image = None;
    }

    fn read_target(&mut self) -> Option<Self::Target> {
        self.finished_image.clone().or_else(|| self.read_image())
    }
}

impl CustomRenderer for VelloCpuRenderer {
    fn with_custom_paint_sink(
        &mut self,
        f: &mut dyn FnMut(&mut dyn CustomPaintSink<DisplayCommandExt>),
    ) {
        let mut canvas = VelloCpuCanvas {
            inner: &mut self.inner,
        };
        f(&mut canvas)
    }

    fn debug_info(&self) -> String {
        Self::debug_info(self)
    }
}

impl RenderCore for VelloCpuTargetRenderer<'_> {
    fn render(&mut self, f: &mut dyn FnMut(&mut dyn PaintSink)) {
        let mut canvas = VelloCpuCanvas {
            inner: &mut self.inner,
        };
        f(&mut canvas)
    }

    fn finish(&mut self) {
        let result = match self.target.format {
            CpuBufferFormat::Rgba8Opaque => self
                .inner
                .read_into_rgba8_opaque(self.target.buffer, self.target.bytes_per_row),
            CpuBufferFormat::Bgra8Opaque => self
                .inner
                .read_into_bgra8_opaque(self.target.buffer, self.target.bytes_per_row),
        };
        self.finished_image = result.ok().map(|_| self.read_image_from_target());
    }

    fn readback(&mut self) -> Option<RenderOutput> {
        self.finished_image
            .clone()
            .or_else(|| Some(self.read_image_from_target()))
            .map(RenderOutput::Image)
    }
}

impl<'a> CustomRenderer for VelloCpuTargetRenderer<'a> {
    fn with_custom_paint_sink(
        &mut self,
        f: &mut dyn FnMut(&mut dyn CustomPaintSink<DisplayCommandExt>),
    ) {
        let mut canvas = VelloCpuCanvas {
            inner: &mut self.inner,
        };
        f(&mut canvas)
    }

    fn debug_info(&self) -> String {
        Self::debug_info(self)
    }
}

impl<'a> TargetRenderer for VelloCpuTargetRenderer<'a> {
    type Target = CpuBufferTarget<'a>;

    fn create(_frame: BeginFrame, target: Self::Target) -> Result<Self, String> {
        let width_u16 =
            u16::try_from(target.width).map_err(|_| "width exceeds vello_cpu limit".to_string())?;
        let height_u16 = u16::try_from(target.height)
            .map_err(|_| "height exceeds vello_cpu limit".to_string())?;
        Ok(Self {
            inner: imaging_vello_cpu::VelloCpuRenderer::new(width_u16, height_u16),
            target,
            finished_image: None,
        })
    }
}
