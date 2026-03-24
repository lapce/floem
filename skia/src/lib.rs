use anyhow::{Result, anyhow};
use floem_renderer::DisplayCommandExt;
use imaging::record::Scene;
use imaging::{
    BlurredRoundedRect, ClipRef, CustomPaintSink, FillRef, GlyphRunRef, GroupRef, PaintSink,
    StrokeRef,
};
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use std::sync::Arc;

pub struct SkiaRenderer {
    renderer: imaging_skia::SkiaRenderer,
    scene: Scene,
    width: u32,
    height: u32,
}

impl SkiaRenderer {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        let width = u16::try_from(width).map_err(|_| anyhow!("width exceeds skia limit"))?;
        let height = u16::try_from(height).map_err(|_| anyhow!("height exceeds skia limit"))?;
        Ok(Self {
            renderer: imaging_skia::SkiaRenderer::new(width, height),
            scene: Scene::new(),
            width: u32::from(width),
            height: u32::from(height),
        })
    }

    pub fn reset(&mut self) {
        self.scene.clear();
    }

    pub fn finish(&mut self) -> Option<ImageData> {
        let data = self
            .renderer
            .render_scene_rgba8(&self.scene)
            .map_err(|err| anyhow!("{err:?}"))
            .ok()?;
        Some(ImageData {
            data: Blob::new(Arc::new(data)),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: self.width,
            height: self.height,
        })
    }

    pub fn debug_info(&self) -> String {
        "name: Skia\ninfo: imaging_skia".to_string()
    }
}

impl PaintSink for SkiaRenderer {
    fn push_clip(&mut self, clip: ClipRef<'_>) {
        PaintSink::push_clip(&mut self.scene, clip);
    }

    fn pop_clip(&mut self) {
        PaintSink::pop_clip(&mut self.scene);
    }

    fn push_group(&mut self, group: GroupRef<'_>) {
        PaintSink::push_group(&mut self.scene, group);
    }

    fn pop_group(&mut self) {
        PaintSink::pop_group(&mut self.scene);
    }

    fn fill(&mut self, draw: FillRef<'_>) {
        PaintSink::fill(&mut self.scene, draw);
    }

    fn stroke(&mut self, draw: StrokeRef<'_>) {
        PaintSink::stroke(&mut self.scene, draw);
    }

    fn glyph_run(
        &mut self,
        draw: GlyphRunRef<'_>,
        glyphs: &mut dyn Iterator<Item = imaging::record::Glyph>,
    ) {
        PaintSink::glyph_run(&mut self.scene, draw, glyphs);
    }

    fn blurred_rounded_rect(&mut self, draw: BlurredRoundedRect) {
        PaintSink::blurred_rounded_rect(&mut self.scene, draw);
    }
}

impl CustomPaintSink<DisplayCommandExt> for SkiaRenderer {
    fn custom(&mut self, _command: &DisplayCommandExt) {}
}
