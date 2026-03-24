use std::ops::{Deref, DerefMut};

use anyhow::{Result, anyhow};
use floem_renderer::DisplayCommandExt;
use imaging::{
    BlurredRoundedRect, ClipRef, CustomPaintSink, FillRef, GlyphRunRef, GroupRef, PaintSink,
    StrokeRef,
};

pub struct VelloCpuRenderer(imaging_vello_cpu::VelloCpuRenderer);

impl VelloCpuRenderer {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        let width = u16::try_from(width).map_err(|_| anyhow!("width exceeds vello_cpu limit"))?;
        let height =
            u16::try_from(height).map_err(|_| anyhow!("height exceeds vello_cpu limit"))?;
        Ok(Self(imaging_vello_cpu::VelloCpuRenderer::new(
            width, height,
        )))
    }

    pub fn debug_info(&self) -> String {
        "name: Vello CPU\ninfo: imaging_vello_cpu".to_string()
    }
}

impl Deref for VelloCpuRenderer {
    type Target = imaging_vello_cpu::VelloCpuRenderer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for VelloCpuRenderer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PaintSink for VelloCpuRenderer {
    fn push_clip(&mut self, clip: ClipRef<'_>) {
        PaintSink::push_clip(&mut self.0, clip);
    }

    fn pop_clip(&mut self) {
        PaintSink::pop_clip(&mut self.0);
    }

    fn push_group(&mut self, group: GroupRef<'_>) {
        PaintSink::push_group(&mut self.0, group);
    }

    fn pop_group(&mut self) {
        PaintSink::pop_group(&mut self.0);
    }

    fn fill(&mut self, draw: FillRef<'_>) {
        PaintSink::fill(&mut self.0, draw);
    }

    fn stroke(&mut self, draw: StrokeRef<'_>) {
        PaintSink::stroke(&mut self.0, draw);
    }

    fn glyph_run(
        &mut self,
        draw: GlyphRunRef<'_>,
        glyphs: &mut dyn Iterator<Item = imaging::record::Glyph>,
    ) {
        PaintSink::glyph_run(&mut self.0, draw, glyphs);
    }

    fn blurred_rounded_rect(&mut self, draw: BlurredRoundedRect) {
        PaintSink::blurred_rounded_rect(&mut self.0, draw);
    }
}

impl CustomPaintSink<DisplayCommandExt> for VelloCpuRenderer {
    fn custom(&mut self, _command: &DisplayCommandExt) {}
}
