use std::collections::HashMap;
use std::mem;
use std::num::NonZero;
use std::sync::Arc;
use std::sync::mpsc::sync_channel;

use anyhow::Result;
use floem_renderer::gpu_resources::GpuResources;
use floem_renderer::text::{Glyph, GlyphRunProps};
use floem_renderer::{Img, Renderer};
use peniko::kurbo::Size;
use peniko::{
    Blob, BrushRef,
    color::palette,
    kurbo::{Affine, Point, Rect, Shape},
};
use peniko::{Compose, Fill, ImageAlphaType, ImageData, Mix};
use vello::kurbo::Stroke;
use vello::util::RenderSurface;
use vello::wgpu::Device;
use vello::{AaConfig, RendererOptions, Scene};
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

pub struct VelloRenderer {
    device: Device,
    #[allow(unused)]
    queue: Queue,
    surface: RenderSurface<'static>,
    renderer: vello::Renderer,
    scene: Scene,
    alt_scene: Option<Scene>,
    window_scale: f64,
    transform: Affine,
    capture: bool,
    adapter: Adapter,
    // TODO: Apply once vello's DrawGlyphs gains embolden support.
    #[allow(dead_code)]
    font_embolden: f32,
    /// Cached vello scenes keyed by SVG content hash.
    /// The bool tracks the current generation for eviction.
    svg_cache: HashMap<Vec<u8>, (bool, Scene)>,
    /// Current cache generation; toggled each frame so stale entries are evicted.
    cache_generation: bool,
    premultiply_pipeline: PremultiplyPipeline,
    compositor_scratch: Option<PremultiplyScratch>,
}

impl VelloRenderer {
    fn device_transform(&self) -> Affine {
        self.transform
    }

    fn render_params(width: u32, height: u32) -> vello::RenderParams {
        vello::RenderParams {
            base_color: palette::css::TRANSPARENT,
            width,
            height,
            antialiasing_method: vello::AaConfig::Msaa16,
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

        let scene = Scene::new();
        let renderer = vello::Renderer::new(
            &device,
            RendererOptions {
                pipeline_cache: None,
                use_cpu: false,
                antialiasing_support: vello::AaSupport::all(),
                num_init_threads: Some(NonZero::new(1).unwrap()),
            },
        )
        .unwrap();
        let premultiply_pipeline =
            create_premultiply_pipeline(&device, wgpu::TextureFormat::Rgba8Unorm);

        Ok(Self {
            device,
            queue,
            surface: render_surface,
            renderer,
            scene,
            alt_scene: None,
            window_scale: scale,
            transform: Affine::IDENTITY,
            capture: false,
            adapter,
            font_embolden,
            svg_cache: HashMap::new(),
            cache_generation: false,
            premultiply_pipeline,
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

    pub fn render_scene_to_texture_view(
        &mut self,
        target_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Result<()> {
        Ok(self.renderer.render_to_texture(
            &self.device,
            &self.queue,
            &self.scene,
            target_view,
            &Self::render_params(width, height),
        )?)
    }

    pub fn render_scene_to_premultiplied_texture_view(
        &mut self,
        target_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let scratch_view = self.ensure_compositor_scratch((width, height));
        self.render_scene_to_texture_view(&scratch_view, width, height)?;
        self.premultiply_into_view(&scratch_view, target_view);
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
        &self,
        source_view: &wgpu::TextureView,
        target_view: &wgpu::TextureView,
    ) {
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Floem Premultiply Bind Group"),
            layout: &self.premultiply_pipeline.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.premultiply_pipeline.sampler),
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
            });
            pass.set_pipeline(&self.premultiply_pipeline.render_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
    }
}

impl Renderer for VelloRenderer {
    fn begin(&mut self, capture: bool) {
        if self.capture == capture {
            self.scene.reset();
        } else {
            self.capture = capture;
            if self.alt_scene.is_none() {
                self.alt_scene = Some(Scene::new());
            }
            if let Some(scene) = self.alt_scene.as_mut() {
                scene.reset();
            }
            self.scene.reset();
            mem::swap(&mut self.scene, self.alt_scene.as_mut().unwrap());
        }
        self.transform = Affine::IDENTITY;

        // Evict SVG scenes not used in the previous frame, then flip generation.
        let generation = self.cache_generation;
        self.svg_cache.retain(|_, (g, _)| *g == generation);
        self.cache_generation = !generation;
    }

    fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s Stroke,
    ) {
        self.scene
            .stroke(stroke, self.device_transform(), brush, None, shape);
    }

    fn fill<'b>(&mut self, path: &impl Shape, brush: impl Into<BrushRef<'b>>, blur_radius: f64) {
        let brush: BrushRef<'b> = brush.into();

        // For solid colors with specific shapes, use optimized methods
        if blur_radius > 0.0
            && let BrushRef::Solid(color) = brush
        {
            if let Some(rounded) = path.as_rounded_rect() {
                if rounded.radii().top_left == rounded.radii().top_right
                    && rounded.radii().top_left == rounded.radii().bottom_left
                    && rounded.radii().top_left == rounded.radii().bottom_right
                {
                    let rect_radius = rounded.radii().top_left;
                    let rect = rounded.rect();
                    self.scene.draw_blurred_rounded_rect(
                        self.device_transform(),
                        rect,
                        color,
                        rect_radius,
                        blur_radius,
                    );
                    return;
                }
            } else if let Some(rect) = path.as_rect() {
                self.scene.draw_blurred_rounded_rect(
                    self.device_transform(),
                    rect,
                    color,
                    0.,
                    blur_radius,
                );
                return;
            }
        }

        self.scene.fill(
            vello::peniko::Fill::NonZero,
            self.device_transform(),
            brush,
            None,
            path,
        );
    }

    fn push_layer(
        &mut self,
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        self.scene.push_layer(
            Fill::NonZero,
            blend,
            alpha,
            self.transform * transform,
            clip,
        );
    }

    fn pop_layer(&mut self) {
        self.scene.pop_layer();
    }

    fn draw_glyphs<'a>(
        &mut self,
        origin: Point,
        props: &GlyphRunProps<'a>,
        glyphs: impl Iterator<Item = Glyph> + 'a,
    ) {
        // TODO: Vello 0.7's DrawGlyphs API has no embolden support.
        // Synthetic bold from layout synthesis and `self.font_embolden` are not applied.
        let transform =
            self.device_transform() * Affine::translate((origin.x, origin.y)) * props.transform;
        self.scene
            .draw_glyphs(&props.font)
            .brush(props.brush)
            .brush_alpha(props.brush_alpha)
            .hint(props.hint)
            .transform(transform)
            .glyph_transform(props.glyph_transform)
            .font_size(props.font_size)
            .normalized_coords(props.normalized_coords)
            .draw(
                props.style,
                glyphs.map(|glyph| vello::Glyph {
                    id: glyph.id,
                    x: glyph.x,
                    y: glyph.y,
                }),
            );
    }

    fn draw_img(&mut self, img: Img<'_>, rect: Rect) {
        let rect_width = rect.width().max(1.);
        let rect_height = rect.height().max(1.);

        let scale_x = rect_width / img.img.image.width as f64;
        let scale_y = rect_height / img.img.image.height as f64;

        let translate_x = rect.min_x();
        let translate_y = rect.min_y();

        self.scene.draw_image(
            &img.img,
            self.device_transform()
                .pre_scale_non_uniform(scale_x, scale_y)
                .pre_translate((translate_x, translate_y).into()),
        );
    }

    fn draw_svg<'b>(
        &mut self,
        svg: floem_renderer::Svg<'b>,
        rect: Rect,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        let rect_width = rect.width().max(1.);
        let rect_height = rect.height().max(1.);

        let svg_size = svg.tree.size();

        let scale_x = rect_width / f64::from(svg_size.width());
        let scale_y = rect_height / f64::from(svg_size.height());

        let translate_x = rect.min_x();
        let translate_y = rect.min_y();

        let transform = self
            .device_transform()
            .pre_scale_non_uniform(scale_x, scale_y)
            .pre_translate((translate_x, translate_y).into());

        // Look up (or create) the cached base scene for this SVG.
        let generation = self.cache_generation;
        let base = self
            .svg_cache
            .entry(svg.hash.to_owned())
            .and_modify(|(g, _)| *g = generation)
            .or_insert_with(|| (generation, vello_svg::render_tree(svg.tree)));

        // When a brush is applied (tinted icons), composite through an alpha mask.
        // The base scene is cached; only the masking composite is rebuilt per frame.
        let composited;
        let scene_to_append = match brush {
            Some(brush) => {
                let brush = brush.into();
                let size = Size::new(svg_size.width() as _, svg_size.height() as _);
                let fill_rect = Rect::from_origin_size(Point::ZERO, size);
                let base_scene = &base.1;
                composited = alpha_mask_scene(
                    size,
                    |scene| scene.append(base_scene, None),
                    move |scene| {
                        scene.fill(Fill::NonZero, Affine::IDENTITY, brush, None, &fill_rect);
                    },
                );
                &composited
            }
            None => &base.1,
        };

        self.scene.append(scene_to_append, Some(transform));
    }

    fn set_transform(&mut self, transform: Affine) {
        self.transform = transform;
    }

    fn clip(&mut self, shape: &impl Shape) {
        self.scene.push_layer(
            Fill::NonZero,
            vello::peniko::BlendMode::default(),
            1.0,
            self.transform,
            shape,
        );
    }

    fn clear_clip(&mut self) {
        self.scene.pop_layer();
    }

    fn finish(&mut self) -> Option<vello::peniko::ImageBrush> {
        if self.capture {
            self.render_capture_image()
        } else {
            if let Ok(surface_texture) = self.surface.surface.get_current_texture() {
                let target_view = self.surface.target_view.clone();
                let width = self.surface.config.width;
                let height = self.surface.config.height;
                self.render_scene_to_texture_view(
                    &target_view,
                    width,
                    height,
                )
                .unwrap();

                // Perform the copy
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

                // Queue the texture to be presented on the surface
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

        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                &self.scene,
                &view,
                &vello::RenderParams {
                    antialiasing_method: AaConfig::Area,
                    ..Self::render_params(self.surface.config.width, self.surface.config.height)
                },
            )
            .unwrap();

        let bytes_per_pixel = 4;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (u64::from(width * height) * bytes_per_pixel),
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

        Some(vello::peniko::ImageBrush::new(ImageData {
            data: Blob::new(Arc::new(cropped_buffer)),
            format: vello::peniko::ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
            width: self.surface.config.width,
            height,
        }))
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
        push_constant_ranges: &[],
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
        multiview: None,
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

fn common_alpha_mask_scene(
    size: Size,
    alpha_mask: impl FnOnce(&mut Scene),
    item: impl FnOnce(&mut Scene),
    compose_mode: Compose,
) -> Scene {
    let mut scene = Scene::new();
    scene.push_layer(
        Fill::NonZero,
        Mix::Normal,
        1.0,
        Affine::IDENTITY,
        &Rect::from_origin_size((0., 0.), size),
    );

    alpha_mask(&mut scene);

    scene.push_layer(
        Fill::NonZero,
        vello::peniko::BlendMode {
            mix: Mix::Normal,
            compose: compose_mode,
        },
        1.,
        Affine::IDENTITY,
        &Rect::from_origin_size((0., 0.), size),
    );

    item(&mut scene);

    scene.pop_layer();
    scene.pop_layer();
    scene
}

fn alpha_mask_scene(
    size: Size,
    alpha_mask: impl FnOnce(&mut Scene),
    item: impl FnOnce(&mut Scene),
) -> Scene {
    common_alpha_mask_scene(size, alpha_mask, item, Compose::SrcIn)
}
#[allow(unused)]
fn invert_alpha_mask_scene(
    size: Size,
    alpha_mask: impl FnOnce(&mut Scene),
    item: impl FnOnce(&mut Scene),
) -> Scene {
    common_alpha_mask_scene(size, alpha_mask, item, Compose::SrcOut)
}
