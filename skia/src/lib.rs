use anyhow::{Result, anyhow};
use floem_renderer::{DisplayCommandExt, FinishMode, RenderOutput, gpu_resources::GpuResources};
use imaging::record::Scene;
use imaging::{
    BlurredRoundedRect, ClipRef, CustomPaintSink, FillRef, GlyphRunRef, GroupRef, PaintSink,
    StrokeRef,
};
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use std::sync::Arc;
use wgpu::TextureUsages;

pub struct SkiaRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: imaging_skia::SkiaRenderer,
    scene: Scene,
}

impl SkiaRenderer {
    pub fn new(
        gpu_resources: GpuResources,
        width: u32,
        height: u32,
        texture_format: wgpu::TextureFormat,
        _scale: f64,
        _font_embolden: f32,
    ) -> Result<Self> {
        let device = gpu_resources.device;
        let queue = gpu_resources.queue;
        let texture = create_output_texture(&device, width, height, texture_format);
        let renderer = imaging_skia::SkiaRenderer::try_new_from_wgpu_texture(
            texture_format,
            &device,
            &queue,
            texture,
        )
        .map_err(|err| anyhow!("{err:?}"))?;
        Ok(Self {
            device,
            queue,
            renderer,
            scene: Scene::new(),
        })
    }

    pub fn begin(&mut self, width: u32, height: u32, scale: f64, font_embolden: f32) {
        if self
            .renderer
            .wgpu_texture()
            .is_none_or(|t| t.size().width != width || t.size().height != height)
        {
            self.recreate_output(width, height, scale, font_embolden)
                .expect("failed to recreate SkiaRenderer");
        }
        self.reset();
    }

    pub fn reset(&mut self) {
        self.scene.clear();
    }

    pub fn finish(&mut self, mode: FinishMode) -> Option<RenderOutput> {
        if mode == FinishMode::CpuImage {
            return self.finish_image().map(RenderOutput::Image);
        }
        self.render_to_output_texture()
            .map(RenderOutput::GpuTexture)
    }

    fn finish_image(&mut self) -> Option<ImageData> {
        let data = self
            .renderer
            .render_scene_rgba8(&self.scene)
            .map_err(|err| anyhow!("{err:?}"))
            .ok()?;
        let size = self.renderer.wgpu_texture()?.size();
        Some(ImageData {
            data: Blob::new(Arc::new(data)),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: size.width,
            height: size.height,
        })
    }

    pub fn debug_info(&self) -> String {
        "name: Skia\ninfo: imaging_skia::SkiaRenderer".to_string()
    }

    fn render_to_output_texture(&mut self) -> Option<wgpu::TextureView> {
        self.renderer.render_scene(&self.scene).ok()?;
        self.renderer.wgpu_texture_view().cloned()
    }

    fn recreate_output(
        &mut self,
        width: u32,
        height: u32,
        _scale: f64,
        _font_embolden: f32,
    ) -> Result<()> {
        let texture_format = self
            .renderer
            .wgpu_texture_view()
            .map(|view| view.texture().format())
            .ok_or_else(|| anyhow!("missing skia output texture"))?;
        let texture = create_output_texture(&self.device, width, height, texture_format);
        self.renderer
            .replace_wgpu_texture(texture_format, texture, &self.device, &self.queue)
            .map_err(|err| anyhow!("{err:?}"))?;
        Ok(())
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

fn create_output_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    texture_format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Floem Skia Output"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: texture_format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[texture_format],
    })
}
