use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use anyhow::{Result, anyhow};
use floem_renderer::DisplayCommandExt;
use imaging::{
    BlurredRoundedRect, ClipRef, CustomPaintSink, FillRef, GlyphRunRef, GroupRef, PaintSink,
    StrokeRef,
};
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

pub struct VelloCpuRenderer {
    renderer: imaging_vello_cpu::VelloCpuRenderer,
    width: u32,
    height: u32,
}

impl VelloCpuRenderer {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        let width_u16 =
            u16::try_from(width).map_err(|_| anyhow!("width exceeds vello_cpu limit"))?;
        let height_u16 =
            u16::try_from(height).map_err(|_| anyhow!("height exceeds vello_cpu limit"))?;
        Ok(Self {
            renderer: imaging_vello_cpu::VelloCpuRenderer::new(width_u16, height_u16),
            width,
            height,
        })
    }

    pub fn finish(&mut self) -> Option<ImageData> {
        let data = self.renderer.finish_rgba8().ok()?;
        Some(ImageData {
            data: Blob::new(Arc::new(data)),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: self.width,
            height: self.height,
        })
    }

    pub fn debug_info(&self) -> String {
        "name: Vello CPU\ninfo: imaging_vello_cpu".to_string()
    }
}

impl Deref for VelloCpuRenderer {
    type Target = imaging_vello_cpu::VelloCpuRenderer;

    fn deref(&self) -> &Self::Target {
        &self.renderer
    }
}

impl DerefMut for VelloCpuRenderer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.renderer
    }
}

impl PaintSink for VelloCpuRenderer {
    fn push_clip(&mut self, clip: ClipRef<'_>) {
        PaintSink::push_clip(&mut self.renderer, clip);
    }

    fn pop_clip(&mut self) {
        PaintSink::pop_clip(&mut self.renderer);
    }

    fn push_group(&mut self, group: GroupRef<'_>) {
        PaintSink::push_group(&mut self.renderer, group);
    }

    fn pop_group(&mut self) {
        PaintSink::pop_group(&mut self.renderer);
    }

    fn fill(&mut self, draw: FillRef<'_>) {
        PaintSink::fill(&mut self.renderer, draw);
    }

    fn stroke(&mut self, draw: StrokeRef<'_>) {
        PaintSink::stroke(&mut self.renderer, draw);
    }

    fn glyph_run(
        &mut self,
        draw: GlyphRunRef<'_>,
        glyphs: &mut dyn Iterator<Item = imaging::record::Glyph>,
    ) {
        PaintSink::glyph_run(&mut self.renderer, draw, glyphs);
    }

    fn blurred_rounded_rect(&mut self, draw: BlurredRoundedRect) {
        PaintSink::blurred_rounded_rect(&mut self.renderer, draw);
    }
}

impl CustomPaintSink<DisplayCommandExt> for VelloCpuRenderer {
    fn custom(&mut self, _command: &DisplayCommandExt) {}
}
