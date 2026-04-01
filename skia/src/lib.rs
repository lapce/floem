use anyhow::{Result, anyhow};
use floem_renderer::{
    BeginFrame, GpuTextureTarget, RenderCore, RenderOutput, Renderer, TargetRenderer,
    gpu_resources::GpuResources,
};
use imaging::PaintSink;
use wgpu::TextureUsages;

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
}

impl RenderCore for SkiaRenderer {
    fn render(&mut self, f: &mut dyn FnMut(&mut dyn PaintSink)) {
        RenderCore::render(&mut self.inner, f);
    }

    fn finish(&mut self) {
        RenderCore::finish(&mut self.inner);
    }

    fn readback(&mut self) -> Option<RenderOutput> {
        self.inner
            .wgpu_texture_view()
            .cloned()
            .map(RenderOutput::GpuTexture)
    }

    fn debug_info(&self) -> String {
        self.inner.debug_info()
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
