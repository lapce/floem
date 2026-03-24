use std::ops::{Deref, DerefMut};

use anyhow::{Result, anyhow};
use floem_renderer::DisplayCommandExt;
use imaging::record::Scene;
use imaging::{
    BlurredRoundedRect, ClipRef, CustomPaintSink, FillRef, GlyphRunRef, GroupRef, PaintSink,
    StrokeRef,
};

pub struct VelloHybridRenderer {
    renderer: imaging_vello_hybrid::VelloHybridRenderer,
    scene: Scene,
}

impl VelloHybridRenderer {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        let width =
            u16::try_from(width).map_err(|_| anyhow!("width exceeds vello_hybrid limit"))?;
        let height =
            u16::try_from(height).map_err(|_| anyhow!("height exceeds vello_hybrid limit"))?;
        Ok(Self {
            renderer: imaging_vello_hybrid::VelloHybridRenderer::try_new(width, height)
                .map_err(|err| anyhow!("{err:?}"))?,
            scene: Scene::new(),
        })
    }

    pub fn reset(&mut self) {
        self.scene.clear();
    }

    pub fn finish_rgba8(&mut self) -> Result<Vec<u8>> {
        self.renderer
            .render_scene_rgba8(&self.scene)
            .map_err(|err| anyhow!("{err:?}"))
    }

    pub fn debug_info(&self) -> String {
        "name: Vello Hybrid\ninfo: imaging_vello_hybrid".to_string()
    }
}

impl Deref for VelloHybridRenderer {
    type Target = imaging_vello_hybrid::VelloHybridRenderer;

    fn deref(&self) -> &Self::Target {
        &self.renderer
    }
}

impl DerefMut for VelloHybridRenderer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.renderer
    }
}

impl PaintSink for VelloHybridRenderer {
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

impl CustomPaintSink<DisplayCommandExt> for VelloHybridRenderer {
    fn custom(&mut self, _command: &DisplayCommandExt) {}
}
