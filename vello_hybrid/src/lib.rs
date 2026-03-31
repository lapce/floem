use std::sync::Arc;

use anyhow::{Result, anyhow};
use floem_renderer::{
    BeginFrame, CpuBufferFormat, CpuBufferTarget, CustomRenderer, DisplayCommandExt, RenderCore,
    RenderOutput, Renderer, TargetRenderer,
};
use imaging::{CustomPaintSink, PaintSink};
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

struct VelloHybridCanvas<'a> {
    inner: imaging_vello_hybrid::VelloHybridSceneSink<'a>,
}

impl PaintSink for VelloHybridCanvas<'_> {
    fn push_clip(&mut self, clip: imaging::ClipRef<'_>) {
        self.inner.push_clip(clip);
    }

    fn pop_clip(&mut self) {
        self.inner.pop_clip();
    }

    fn push_group(&mut self, group: imaging::GroupRef<'_>) {
        self.inner.push_group(group);
    }

    fn pop_group(&mut self) {
        self.inner.pop_group();
    }

    fn fill(&mut self, draw: imaging::FillRef<'_>) {
        self.inner.fill(draw);
    }

    fn stroke(&mut self, draw: imaging::StrokeRef<'_>) {
        self.inner.stroke(draw);
    }

    fn glyph_run(
        &mut self,
        draw: imaging::GlyphRunRef<'_>,
        glyphs: &mut dyn Iterator<Item = imaging::record::Glyph>,
    ) {
        self.inner.glyph_run(draw, glyphs);
    }

    fn blurred_rounded_rect(&mut self, draw: imaging::BlurredRoundedRect) {
        self.inner.blurred_rounded_rect(draw);
    }
}

impl CustomPaintSink<DisplayCommandExt> for VelloHybridCanvas<'_> {
    fn custom(&mut self, _command: &DisplayCommandExt) {}
}

pub struct VelloHybridRenderer {
    renderer: imaging_vello_hybrid::VelloHybridRenderer,
    scene: vello_hybrid::Scene,
    width: u32,
    height: u32,
    finished_image: Option<ImageData>,
}

pub struct VelloHybridTargetRenderer<'a> {
    renderer: imaging_vello_hybrid::VelloHybridRenderer,
    scene: vello_hybrid::Scene,
    target: CpuBufferTarget<'a>,
    finished_image: Option<ImageData>,
}

impl VelloHybridRenderer {
    pub fn new(width: u32, height: u32, _scale: f64, _font_embolden: f32) -> Result<Self> {
        let width_u16 =
            u16::try_from(width).map_err(|_| anyhow!("width exceeds vello_hybrid limit"))?;
        let height_u16 =
            u16::try_from(height).map_err(|_| anyhow!("height exceeds vello_hybrid limit"))?;
        Ok(Self {
            renderer: imaging_vello_hybrid::VelloHybridRenderer::try_new(width_u16, height_u16)
                .map_err(|err| anyhow!("{err:?}"))?,
            scene: vello_hybrid::Scene::new(width_u16, height_u16),
            width,
            height,
            finished_image: None,
        })
    }

    pub fn debug_info(&self) -> String {
        "name: Vello Hybrid\ninfo: imaging_vello_hybrid".to_string()
    }

    fn reset_scene(&mut self) {
        self.scene.reset();
    }

    fn with_scene_sink<R>(&mut self, f: &mut dyn FnMut(&mut VelloHybridCanvas<'_>) -> R) -> R {
        let mut sink = VelloHybridCanvas {
            inner: imaging_vello_hybrid::VelloHybridSceneSink::with_renderer(
                &mut self.scene,
                &mut self.renderer,
            ),
        };
        let out = f(&mut sink);
        let _ = sink.inner.finish();
        out
    }

    fn read_image(&mut self) -> Option<ImageData> {
        self.renderer
            .render_vello_hybrid_scene_rgba8(&self.scene)
            .ok()
    }
}

impl VelloHybridTargetRenderer<'_> {
    pub fn debug_info(&self) -> String {
        "name: Vello Hybrid\ninfo: imaging_vello_hybrid".to_string()
    }

    fn with_scene_sink<R>(&mut self, f: &mut dyn FnMut(&mut VelloHybridCanvas<'_>) -> R) -> R {
        let mut sink = VelloHybridCanvas {
            inner: imaging_vello_hybrid::VelloHybridSceneSink::with_renderer(
                &mut self.scene,
                &mut self.renderer,
            ),
        };
        let out = f(&mut sink);
        let _ = sink.inner.finish();
        out
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
            alpha_type: ImageAlphaType::AlphaPremultiplied,
            width: self.target.width,
            height: self.target.height,
        }
    }
}

impl RenderCore for VelloHybridRenderer {
    fn render(&mut self, f: &mut dyn FnMut(&mut dyn PaintSink)) {
        self.with_scene_sink(&mut |canvas| f(canvas));
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

impl Renderer for VelloHybridRenderer {
    type Target = ImageData;

    fn set_size(&mut self, frame: BeginFrame) {
        let width = frame.size.width as u32;
        let height = frame.size.height as u32;
        if self.width != width || self.height != height {
            *self = Self::new(width, height, frame.scale, frame.font_embolden)
                .expect("failed to recreate VelloHybridRenderer");
        }
    }

    fn reset(&mut self) {
        self.reset_scene();
        self.finished_image = None;
    }

    fn read_target(&mut self) -> Option<Self::Target> {
        self.finished_image.clone().or_else(|| self.read_image())
    }
}

impl CustomRenderer for VelloHybridRenderer {
    fn with_custom_paint_sink(
        &mut self,
        f: &mut dyn FnMut(&mut dyn CustomPaintSink<DisplayCommandExt>),
    ) {
        self.with_scene_sink(&mut |canvas| f(canvas));
    }

    fn debug_info(&self) -> String {
        Self::debug_info(self)
    }
}

impl RenderCore for VelloHybridTargetRenderer<'_> {
    fn render(&mut self, f: &mut dyn FnMut(&mut dyn PaintSink)) {
        self.with_scene_sink(&mut |canvas| f(canvas));
    }

    fn finish(&mut self) {
        let output = match self.target.format {
            CpuBufferFormat::Rgba8Opaque => {
                imaging_vello_hybrid::ImageOutputFormat::RGBA8_PREMULTIPLIED
            }
            CpuBufferFormat::Bgra8Opaque => {
                imaging_vello_hybrid::ImageOutputFormat::BGRA8_PREMULTIPLIED
            }
        };
        let result = self.renderer.render_vello_hybrid_scene_into(
            &self.scene,
            self.target.buffer,
            self.target.bytes_per_row,
            output,
        );
        self.finished_image = result.ok().map(|_| self.read_image_from_target());
    }

    fn readback(&mut self) -> Option<RenderOutput> {
        self.finished_image
            .clone()
            .or_else(|| Some(self.read_image_from_target()))
            .map(RenderOutput::Image)
    }
}

impl<'a> TargetRenderer for VelloHybridTargetRenderer<'a> {
    type Target = CpuBufferTarget<'a>;

    fn create(_frame: BeginFrame, target: Self::Target) -> Result<Self, String> {
        let width_u16 = u16::try_from(target.width)
            .map_err(|_| "width exceeds vello_hybrid limit".to_string())?;
        let height_u16 = u16::try_from(target.height)
            .map_err(|_| "height exceeds vello_hybrid limit".to_string())?;
        Ok(Self {
            renderer: imaging_vello_hybrid::VelloHybridRenderer::try_new(width_u16, height_u16)
                .map_err(|err| format!("{err:?}"))?,
            scene: vello_hybrid::Scene::new(width_u16, height_u16),
            target,
            finished_image: None,
        })
    }
}

impl CustomRenderer for VelloHybridTargetRenderer<'_> {
    fn with_custom_paint_sink(
        &mut self,
        f: &mut dyn FnMut(&mut dyn CustomPaintSink<DisplayCommandExt>),
    ) {
        self.with_scene_sink(&mut |canvas| f(canvas));
    }

    fn debug_info(&self) -> String {
        Self::debug_info(self)
    }
}
