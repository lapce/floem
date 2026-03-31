mod cpu;

pub use cpu::{SkiaCpuRenderer, SkiaCpuTargetRenderer};

use anyhow::{Result, anyhow};
use floem_renderer::{
    BeginFrame, CustomRenderer, DisplayCommandExt, GpuTextureTarget, RenderCore, RenderOutput,
    Renderer, TargetRenderer, gpu_resources::GpuResources,
};
use imaging::{
    BlurredRoundedRect, ClipRef, CustomPaintSink, FillRef, GlyphRunRef, GroupRef, PaintSink,
    StrokeRef,
};
use imaging_skia::SkCanvasSink;
use wgpu::TextureUsages;

struct SkiaCanvas<'a, 'b> {
    inner: &'a mut SkCanvasSink<'b>,
}

impl PaintSink for SkiaCanvas<'_, '_> {
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

impl CustomPaintSink<DisplayCommandExt> for SkiaCanvas<'_, '_> {
    fn custom(&mut self, _command: &DisplayCommandExt) {}
}

pub struct SkiaRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    inner: imaging_skia::SkiaRenderer,
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
        Self::from_texture(device, queue, texture_format, texture).map_err(|err| anyhow!(err))
    }

    pub fn debug_info(&self) -> String {
        "name: Skia\ninfo: imaging_skia::SkiaRenderer".to_string()
    }

    fn from_texture(
        device: wgpu::Device,
        queue: wgpu::Queue,
        texture_format: wgpu::TextureFormat,
        texture: wgpu::Texture,
    ) -> Result<Self, String> {
        let inner = imaging_skia::SkiaRenderer::try_new_from_wgpu_texture(
            texture_format,
            &device,
            &queue,
            texture,
        )
        .map_err(|err| format!("{err:?}"))?;
        Ok(Self {
            device,
            queue,
            inner,
        })
    }

    fn with_canvas<R>(&mut self, f: &mut dyn FnMut(&mut SkiaCanvas<'_, '_>) -> R) -> R {
        self.inner
            .with_canvas_sink(|sink| {
                let mut canvas = SkiaCanvas { inner: sink };
                f(&mut canvas)
            })
            .expect("render into imaging_skia canvas sink")
    }
}

impl RenderCore for SkiaRenderer {
    fn render(&mut self, f: &mut dyn FnMut(&mut dyn PaintSink)) {
        self.with_canvas(&mut |canvas| f(canvas));
    }

    fn finish(&mut self) {
        let _ = self.inner.with_canvas_sink(|c| c.finish());
    }

    fn readback(&mut self) -> Option<RenderOutput> {
        self.inner
            .wgpu_texture_view()
            .cloned()
            .map(RenderOutput::GpuTexture)
    }
}

impl Renderer for SkiaRenderer {
    type Target = wgpu::TextureView;

    fn set_size(&mut self, frame: BeginFrame) {
        if self.inner.wgpu_texture().is_none_or(|t| {
            t.size().width != frame.size.width as u32 || t.size().height != frame.size.height as u32
        }) {
            let texture_format = self
                .inner
                .wgpu_texture_view()
                .map(|view| view.texture().format())
                .expect("missing skia output texture");
            let texture = create_output_texture(
                &self.device,
                frame.size.width as u32,
                frame.size.height as u32,
                texture_format,
            );
            self.inner
                .replace_wgpu_texture(texture_format, texture, &self.device, &self.queue)
                .expect("failed to recreate SkiaRenderer");
        }
    }

    fn reset(&mut self) {
        self.inner
            .reset()
            .expect("failed to reset imaging_skia renderer");
    }

    fn read_target(&mut self) -> Option<Self::Target> {
        self.inner.wgpu_texture_view().cloned()
    }
}

impl TargetRenderer for SkiaRenderer {
    type Target = GpuTextureTarget;

    fn create(frame: BeginFrame, target: Self::Target) -> Result<Self, String> {
        let device = target.device;
        let queue = target.queue;
        let texture = target.texture_view.texture().clone();
        let texture_format = texture.format();
        let mut renderer = Self::from_texture(device, queue, texture_format, texture)?;
        renderer.set_size(frame);
        renderer.reset();
        Ok(renderer)
    }
}

impl CustomRenderer for SkiaRenderer {
    fn with_custom_paint_sink(
        &mut self,
        f: &mut dyn FnMut(&mut dyn CustomPaintSink<DisplayCommandExt>),
    ) {
        self.with_canvas(&mut |canvas| f(canvas));
    }

    fn debug_info(&self) -> String {
        Self::debug_info(self)
    }
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
