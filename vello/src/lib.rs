use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::sync_channel;

use anyhow::Result;
use floem_renderer::gpu_resources::GpuResources;
use floem_renderer::text::{Glyph, GlyphRunRef};
use floem_renderer::{Renderer, Svg};
use imaging::record::{CustomCommand, ExtendedScene, Glyph as ImagingGlyph, replay_ext};
use imaging::{
    BlurredRoundedRect, ClipRef, Composite, CustomPaintSink, FillRef, GroupRef, PaintSink,
    StrokeRef,
};
use peniko::kurbo::Size;
use peniko::{
    Blob, BrushRef,
    color::palette,
    kurbo::{Affine, Point, Rect, Shape},
};
use peniko::{ImageAlphaType, ImageData};
use vello::util::RenderSurface;
use vello::{AaConfig, RenderParams, RendererOptions, Scene};
use wgpu::util::TextureBlitter;
use wgpu::{Adapter, DeviceType, Queue, TextureAspect, TextureFormat};

const PREMULTIPLY_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2(-1.0, -3.0),
        vec2(-1.0, 1.0),
        vec2(3.0, 1.0),
    );
    let pos = positions[vertex_index];
    var out: VertexOutput;
    out.position = vec4(pos, 0.0, 1.0);
    out.uv = 0.5 * vec2(pos.x + 1.0, 1.0 - pos.y);
    return out;
}

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(source_texture, source_sampler, in.uv);
    return vec4(color.rgb * color.a, color.a);
}
"#;

struct PremultiplyPipeline {
    render_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

struct PremultiplyScratch {
    size: (u32, u32),
    #[allow(dead_code)]
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

#[derive(Clone)]
enum VelloCommand {
    DrawSvg {
        svg: SvgCommand,
        rect: Rect,
        transform: Affine,
        brush: Option<peniko::Brush>,
    },
}

impl CustomCommand for VelloCommand {
    fn prepend_transform(&self, prefix: Affine) -> Self {
        match self {
            Self::DrawSvg {
                svg,
                rect,
                transform,
                brush,
            } => Self::DrawSvg {
                svg: svg.clone(),
                rect: *rect,
                transform: prefix * *transform,
                brush: brush.clone(),
            },
        }
    }
}

#[derive(Clone)]
struct SvgCommand {
    hash: Arc<[u8]>,
}

#[derive(Default)]
struct SvgCache {
    entries: HashMap<Vec<u8>, SvgCacheEntry>,
}

impl SvgCache {
    fn touch(&mut self, svg: &SvgCommand) {
        self.entries.entry(svg.hash.to_vec()).or_default();
    }
}

#[derive(Default)]
struct SvgCacheEntry {
    #[allow(dead_code)]
    alpha_mask_scene: AlphaMaskScene,
}

#[derive(Default)]
struct AlphaMaskScene {
    #[allow(dead_code)]
    scene: Option<Scene>,
}

struct VelloSceneAdapter<'a, 'b> {
    inner: &'a mut imaging_vello::VelloSceneSink<'b>,
    svg_cache: &'a mut SvgCache,
}

impl PaintSink for VelloSceneAdapter<'_, '_> {
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
        draw: imaging::GlyphRunRef<'_>,
        glyphs: &mut dyn Iterator<Item = ImagingGlyph>,
    ) {
        self.inner.glyph_run(draw, glyphs);
    }

    fn blurred_rounded_rect(&mut self, draw: BlurredRoundedRect) {
        self.inner.blurred_rounded_rect(draw);
    }
}

impl CustomPaintSink<VelloCommand> for VelloSceneAdapter<'_, '_> {
    fn custom(&mut self, command: &VelloCommand) {
        let VelloCommand::DrawSvg {
            svg,
            rect,
            transform,
            brush,
        } = command;
        self.svg_cache.touch(svg);
        let _ = (rect, transform, brush);
    }
}

pub struct VelloRenderer {
    device: wgpu::Device,
    queue: Queue,
    surface: RenderSurface<'static>,
    renderer: vello::Renderer,
    scene: ExtendedScene<VelloCommand>,
    window_scale: f64,
    transform: Affine,
    capture: bool,
    adapter: Adapter,
    #[allow(dead_code)]
    font_embolden: f32,
    svg_cache: SvgCache,
    premultiply_pipelines: HashMap<wgpu::TextureFormat, PremultiplyPipeline>,
    compositor_scratch: Option<PremultiplyScratch>,
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
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        scale: f64,
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

        let surface_caps = surface.get_capabilities(&adapter);
        let texture_format = surface_caps
            .formats
            .into_iter()
            .find(|it| matches!(it, TextureFormat::Rgba8Unorm | TextureFormat::Bgra8Unorm))
            .ok_or_else(|| anyhow::anyhow!("surface should support Rgba8Unorm or Bgra8Unorm"))?;

        let latency = match adapter.get_info().backend {
            wgpu::Backend::Vulkan => 2,
            _ => 1,
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: texture_format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: latency,
        };

        surface.configure(&device, &config);

        let target_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            format: TextureFormat::Rgba8Unorm,
            view_formats: &[],
        });

        let target_view = target_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let render_surface = RenderSurface {
            surface,
            config,
            dev_id: 0,
            format: texture_format,
            target_texture,
            target_view,
            blitter: TextureBlitter::new(&device, texture_format),
        };

        let renderer = vello::Renderer::new(&device, RendererOptions::default())?;
        Ok(Self {
            device,
            queue,
            surface: render_surface,
            renderer,
            scene: ExtendedScene::new(),
            window_scale: scale,
            transform: Affine::IDENTITY,
            capture: false,
            adapter,
            font_embolden,
            svg_cache: SvgCache::default(),
            premultiply_pipelines: HashMap::default(),
            compositor_scratch: None,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32, scale: f64) {
        if width != self.surface.config.width || height != self.surface.config.height {
            let target_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                format: TextureFormat::Rgba8Unorm,
                view_formats: &[],
            });
            let target_view = target_texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.surface.target_texture = target_texture;
            self.surface.target_view = target_view;
            self.surface.config.width = width;
            self.surface.config.height = height;
            self.surface
                .surface
                .configure(&self.device, &self.surface.config);
        }
        self.window_scale = scale;
    }

    pub const fn set_scale(&mut self, scale: f64) {
        self.window_scale = scale;
    }

    pub const fn size(&self) -> Size {
        Size::new(
            self.surface.config.width as f64,
            self.surface.config.height as f64,
        )
    }

    fn build_scene(&mut self, width: u32, height: u32) -> Result<Scene> {
        let mut scene = Scene::new();
        let bounds = Rect::new(0.0, 0.0, width as f64, height as f64);
        let mut sink = imaging_vello::VelloSceneSink::new(&mut scene, bounds);
        {
            let mut adapter = VelloSceneAdapter {
                inner: &mut sink,
                svg_cache: &mut self.svg_cache,
            };
            replay_ext(&self.scene, &mut adapter);
        }
        sink.finish().map_err(|err| anyhow::anyhow!("{err:?}"))?;
        Ok(scene)
    }

    pub fn render_scene_to_texture_view(
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

    pub fn render_scene_to_premultiplied_texture_view(
        &mut self,
        target_view: &wgpu::TextureView,
        target_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let scratch_view = self.ensure_compositor_scratch((width, height));
        self.render_scene_to_texture_view(&scratch_view, width, height)?;
        self.premultiply_into_view(&scratch_view, target_view, target_format);
        Ok(())
    }

    pub fn present_composited_output(
        &mut self,
        composite: impl FnOnce(&wgpu::TextureView),
    ) -> bool {
        let Ok(surface_texture) = self.surface.surface.get_current_texture() else {
            return false;
        };
        let output_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        composite(&output_view);
        surface_texture.present();
        true
    }

    fn ensure_compositor_scratch(&mut self, size: (u32, u32)) -> wgpu::TextureView {
        let needs_new_texture = self
            .compositor_scratch
            .as_ref()
            .is_none_or(|scratch| scratch.size != size);

        if needs_new_texture {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Floem Compositor Scratch"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::STORAGE_BINDING,
                format: wgpu::TextureFormat::Rgba8Unorm,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.compositor_scratch = Some(PremultiplyScratch {
                size,
                texture,
                view,
            });
        }

        self.compositor_scratch
            .as_ref()
            .expect("scratch texture just created")
            .view
            .clone()
    }

    fn premultiply_into_view(
        &mut self,
        source_view: &wgpu::TextureView,
        target_view: &wgpu::TextureView,
        target_format: wgpu::TextureFormat,
    ) {
        if !self.premultiply_pipelines.contains_key(&target_format) {
            let pipeline = create_premultiply_pipeline(&self.device, target_format);
            self.premultiply_pipelines.insert(target_format, pipeline);
        }
        let pipeline = self
            .premultiply_pipelines
            .get(&target_format)
            .expect("premultiply pipeline just inserted");
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Floem Premultiply Bind Group"),
            layout: &pipeline.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Floem Premultiply Encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Floem Premultiply Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline.render_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
    }
}

impl Renderer for VelloRenderer {
    fn begin(&mut self, capture: bool) {
        self.capture = capture;
        self.scene = ExtendedScene::new();
        self.transform = Affine::IDENTITY;
    }

    fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s peniko::kurbo::Stroke,
    ) {
        let draw = imaging::record::Draw::Stroke {
            transform: self.transform,
            stroke: stroke.clone(),
            brush: brush.into().to_owned(),
            brush_transform: None,
            shape: shape_to_geometry(shape).to_owned(),
            composite: Composite::default(),
        };
        self.scene.draw(draw);
    }

    fn fill<'b>(&mut self, path: &impl Shape, brush: impl Into<BrushRef<'b>>, blur_radius: f64) {
        let brush = brush.into();

        if blur_radius > 0.0
            && let BrushRef::Solid(color) = brush
        {
            if let Some(rounded) = path.as_rounded_rect() {
                let radii = rounded.radii();
                if radii.top_left == radii.top_right
                    && radii.top_left == radii.bottom_left
                    && radii.top_left == radii.bottom_right
                {
                    self.scene.blurred_rounded_rect(BlurredRoundedRect {
                        transform: self.transform,
                        rect: rounded.rect(),
                        color,
                        radius: radii.top_left,
                        std_dev: blur_radius,
                        composite: Composite::default(),
                    });
                    return;
                }
            } else if let Some(rect) = path.as_rect() {
                self.scene.blurred_rounded_rect(BlurredRoundedRect {
                    transform: self.transform,
                    rect,
                    color,
                    radius: 0.0,
                    std_dev: blur_radius,
                    composite: Composite::default(),
                });
                return;
            }
        }

        self.scene
            .fill(FillRef::new(shape_to_geometry(path), brush).transform(self.transform));
    }

    fn push_layer(
        &mut self,
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        PaintSink::push_group(
            &mut self.scene,
            GroupRef::new()
                .with_clip(ClipRef::fill(shape_to_geometry(clip)).with_transform(
                    self.transform * transform,
                ))
                .with_composite(Composite::new(blend.into(), alpha)),
        );
    }

    fn pop_layer(&mut self) {
        self.scene.pop_group();
    }

    fn draw_glyphs<'a>(
        &mut self,
        origin: Point,
        run: &GlyphRunRef<'a>,
        glyphs: impl Iterator<Item = Glyph> + 'a,
    ) {
        let draw = imaging::GlyphRunRef {
            font: run.font,
            transform: self.transform * Affine::translate((origin.x, origin.y)) * run.transform,
            glyph_transform: run.glyph_transform,
            font_size: run.font_size,
            hint: run.hint,
            normalized_coords: run.normalized_coords,
            style: run.style,
            brush: run.brush,
            composite: run.composite,
        };
        let mut glyphs = glyphs.map(|glyph| ImagingGlyph {
            id: glyph.id,
            x: glyph.x,
            y: glyph.y,
        });
        self.scene.glyph_run(draw, &mut glyphs);
    }

    fn draw_svg<'b>(&mut self, svg: Svg<'b>, rect: Rect, brush: Option<impl Into<BrushRef<'b>>>) {
        CustomPaintSink::custom(&mut self.scene, &VelloCommand::DrawSvg {
            svg: SvgCommand {
                hash: Arc::from(svg.hash),
            },
            rect,
            transform: self.transform,
            brush: brush.map(|brush| brush.into().to_owned()),
        });
    }

    fn set_transform(&mut self, transform: Affine) {
        self.transform = transform;
    }

    fn clip(&mut self, shape: &impl Shape) {
        PaintSink::push_clip(
            &mut self.scene,
            ClipRef::fill(shape_to_geometry(shape)).with_transform(self.transform),
        );
    }

    fn clear_clip(&mut self) {
        self.scene.pop_clip();
    }

    fn finish(&mut self) -> Option<peniko::ImageBrush> {
        if self.capture {
            self.render_capture_image()
        } else {
            if let Ok(surface_texture) = self.surface.surface.get_current_texture() {
                let target_view = self.surface.target_view.clone();
                let width = self.surface.config.width;
                let height = self.surface.config.height;
                self.render_scene_to_texture_view(&target_view, width, height)
                    .unwrap();

                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Surface Blit"),
                        });
                self.surface.blitter.copy(
                    &self.device,
                    &mut encoder,
                    &self.surface.target_view,
                    &surface_texture
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                );
                self.queue.submit([encoder.finish()]);
                surface_texture.present();
            }
            None
        }
    }

    fn debug_info(&self) -> String {
        use std::fmt::Write;

        let mut out = String::new();
        writeln!(out, "name: Vello").ok();
        writeln!(out, "info: {:#?}", self.adapter.get_info()).ok();
        out
    }
}

impl VelloRenderer {
    fn render_capture_image(&mut self) -> Option<peniko::ImageBrush> {
        let width_align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - 1;
        let width = (self.surface.config.width + width_align) & !width_align;
        let height = self.surface.config.height;
        let texture_desc = wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: self.surface.config.width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::STORAGE_BINDING,
            label: Some("render_texture"),
            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
        };
        let texture = self.device.create_texture(&texture_desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Floem Inspector Preview"),
            format: Some(TextureFormat::Rgba8Unorm),
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: TextureAspect::default(),
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
            ..Default::default()
        });

        let scene = self
            .build_scene(self.surface.config.width, self.surface.config.height)
            .ok()?;
        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                &scene,
                &view,
                &RenderParams {
                    antialiasing_method: AaConfig::Area,
                    ..Self::render_params(self.surface.config.width, self.surface.config.height)
                },
            )
            .ok()?;

        let bytes_per_pixel = 4u64;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(width * height) * bytes_per_pixel,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bytes_per_row = width * bytes_per_pixel as u32;
        assert!(bytes_per_row.is_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: None,
                },
            },
            texture_desc.size,
        );
        let command_buffer = encoder.finish();
        self.queue.submit(Some(command_buffer));
        self.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;

        let slice = buffer.slice(..);
        let (tx, rx) = sync_channel(1);
        slice.map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());

        self.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        rx.recv().ok()?.ok()?;

        let buffer: Vec<u8> = slice.get_mapped_range().to_owned();

        let row_size = self.surface.config.width as usize * bytes_per_pixel as usize;
        let mut cropped_buffer = Vec::with_capacity(row_size * height as usize);
        let mut cursor = 0;
        for _ in 0..height {
            cropped_buffer.extend_from_slice(&buffer[cursor..(cursor + row_size)]);
            cursor += bytes_per_row as usize;
        }

        Some(peniko::ImageBrush::new(ImageData {
            data: Blob::new(Arc::new(cropped_buffer)),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
            width: self.surface.config.width,
            height,
        }))
    }
}

fn shape_to_geometry(shape: &impl Shape) -> imaging::GeometryRef<'static> {
    if let Some(rect) = shape.as_rect() {
        imaging::GeometryRef::Rect(rect)
    } else if let Some(rect) = shape.as_rounded_rect() {
        imaging::GeometryRef::RoundedRect(rect)
    } else {
        imaging::GeometryRef::OwnedPath(shape.to_path(0.1))
    }
}

fn create_premultiply_pipeline(
    device: &wgpu::Device,
    target_format: wgpu::TextureFormat,
) -> PremultiplyPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Floem Premultiply Shader"),
        source: wgpu::ShaderSource::Wgsl(PREMULTIPLY_SHADER.into()),
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Floem Premultiply BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Floem Premultiply Layout"),
        bind_group_layouts: &[&bind_group_layout],
        immediate_size: 0,
    });
    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Floem Premultiply Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Floem Premultiply Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    PremultiplyPipeline {
        render_pipeline,
        bind_group_layout,
        sampler,
    }
}
