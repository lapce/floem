use anyhow::Result;
use floem_renderer::gpu_resources::GpuResources;
use floem_renderer::{BeginFrame, RenderCore, RenderOutput, Renderer};
use imaging::record::{Glyph as ImagingGlyph, ReplaySource, Scene as RecordingScene};
use imaging::{BlurredRoundedRect, ClipRef, FillRef, GroupRef, PaintSink, RetainedDrawRef, StrokeRef};
use peniko::color::palette;
use peniko::kurbo::Rect;
use vello::{AaConfig, RenderParams, RendererOptions, Scene};
use wgpu::{Adapter, DeviceType, Queue, TextureAspect};

pub struct VelloRenderer {
    device: wgpu::Device,
    queue: Queue,
    renderer: vello::Renderer,
    texture: Option<wgpu::Texture>,
    view: Option<wgpu::TextureView>,
    scene: RecordingScene,
    size: (u32, u32),
    adapter: Adapter,
    #[allow(dead_code)]
    font_embolden: f32,
    finished_output: Option<RenderOutput>,
}

impl VelloRenderer {
    fn render_params(width: u32, height: u32) -> RenderParams {
        RenderParams {
            base_color: palette::css::TRANSPARENT,
            width,
            height,
            antialiasing_method: AaConfig::Area,
        }
    }

    pub fn new(
        gpu_resources: GpuResources,
        width: u32,
        height: u32,
        _texture_format: wgpu::TextureFormat,
        _scale: f64,
        font_embolden: f32,
    ) -> Result<Self> {
        let GpuResources {
            adapter,
            device,
            queue,
            ..
        } = gpu_resources;

        if adapter.get_info().device_type == DeviceType::Cpu {
            return Err(anyhow::anyhow!("only cpu adapter found"));
        }

        let mut required_downlevel_flags = wgpu::DownlevelFlags::empty();
        required_downlevel_flags.set(wgpu::DownlevelFlags::VERTEX_STORAGE, true);

        if !adapter
            .get_downlevel_capabilities()
            .flags
            .contains(required_downlevel_flags)
        {
            return Err(anyhow::anyhow!(
                "adapter doesn't support required downlevel flags"
            ));
        }

        let renderer = vello::Renderer::new(&device, RendererOptions::default())?;
        Ok(Self {
            device,
            queue,
            renderer,
            texture: None,
            view: None,
            scene: RecordingScene::new(),
            size: (width, height),
            adapter,
            font_embolden,
            finished_output: None,
        })
    }

    pub fn begin(&mut self, width: u32, height: u32, _scale: f64, font_embolden: f32) {
        if self.size != (width, height) && self.texture.is_some() {
            self.texture = None;
            self.view = None;
        }
        self.size = (width, height);
        self.font_embolden = font_embolden;
        self.scene = RecordingScene::new();
        self.finished_output = None;
    }

    fn build_scene(&mut self, width: u32, height: u32) -> Result<Scene> {
        let mut scene = Scene::new();
        let bounds = Rect::new(0.0, 0.0, width as f64, height as f64);
        let mut sink = imaging_vello::VelloSceneSink::new(&mut scene, bounds);
        self.scene.replay_into(&mut sink);
        sink.finish().map_err(|err| anyhow::anyhow!("{err:?}"))?;
        Ok(scene)
    }

    fn render_scene_to_texture_view(
        &mut self,
        target_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let scene = self.build_scene(width, height)?;
        Ok(self.renderer.render_to_texture(
            &self.device,
            &self.queue,
            &scene,
            target_view,
            &Self::render_params(width, height),
        )?)
    }
}

impl VelloRenderer {
    pub fn debug_info(&mut self) -> String {
        use std::fmt::Write;

        let mut out = String::new();
        writeln!(out, "name: Vello").ok();
        writeln!(out, "info: {:#?}", self.adapter.get_info()).ok();
        out
    }
    fn render_to_texture_output(&mut self) -> Option<wgpu::TextureView> {
        self.ensure_offscreen_target().ok()?;
        let size = self.size;
        let view = self.view.as_ref()?.clone();
        self.render_scene_to_texture_view(&view, size.0, size.1)
            .ok()?;
        Some(view)
    }

    fn ensure_offscreen_target(&mut self) -> Result<()> {
        if self.texture.is_some() && self.view.is_some() {
            return Ok(());
        }
        let (texture, view) = create_output_texture(
            &self.device,
            self.size.0,
            self.size.1,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        self.texture = Some(texture);
        self.view = Some(view);
        Ok(())
    }
}

fn create_output_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    texture_format: wgpu::TextureFormat,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Floem Vello Output"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::STORAGE_BINDING,
        format: texture_format,
        view_formats: &[texture_format],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("Floem Vello Output View"),
        format: Some(texture_format),
        dimension: Some(wgpu::TextureViewDimension::D2),
        aspect: TextureAspect::default(),
        base_mip_level: 0,
        mip_level_count: None,
        base_array_layer: 0,
        array_layer_count: None,
        ..Default::default()
    });
    (texture, view)
}

impl PaintSink for VelloRenderer {
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

    fn retained(&mut self, draw: RetainedDrawRef<'_>) {
        PaintSink::retained(&mut self.scene, draw);
    }

    fn fill(&mut self, draw: FillRef<'_>) {
        PaintSink::fill(&mut self.scene, draw);
    }

    fn stroke(&mut self, draw: StrokeRef<'_>) {
        PaintSink::stroke(&mut self.scene, draw);
    }

    fn glyph_run(
        &mut self,
        draw: imaging::GlyphRunRef<'_>,
        glyphs: &mut dyn Iterator<Item = ImagingGlyph>,
    ) {
        PaintSink::glyph_run(&mut self.scene, draw, glyphs);
    }

    fn blurred_rounded_rect(&mut self, draw: BlurredRoundedRect) {
        PaintSink::blurred_rounded_rect(&mut self.scene, draw);
    }
}

impl RenderCore for VelloRenderer {
    fn render(&mut self, f: &mut dyn FnMut(&mut dyn PaintSink)) {
        f(self)
    }

    fn finish(&mut self) {
        self.finished_output = self
            .render_to_texture_output()
            .map(RenderOutput::GpuTexture);
    }

    fn readback(&mut self) -> Option<RenderOutput> {
        self.finished_output.take().or_else(|| {
            self.render_to_texture_output()
                .map(RenderOutput::GpuTexture)
        })
    }
}

impl Renderer for VelloRenderer {
    type Target = wgpu::TextureView;

    fn set_size(&mut self, frame: BeginFrame) {
        Self::begin(
            self,
            frame.size.width as u32,
            frame.size.height as u32,
            frame.scale,
            frame.font_embolden,
        );
    }

    fn reset(&mut self) {
        self.finished_output = None;
    }

    fn read_target(&mut self) -> Option<Self::Target> {
        self.finished_output.take().and_then(|output| match output {
            RenderOutput::GpuTexture(texture) => Some(texture),
            RenderOutput::Image(_) => None,
        })
    }
}
