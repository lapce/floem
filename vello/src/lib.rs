use std::collections::HashMap;
use std::mem;
use std::num::NonZero;
use std::sync::mpsc::sync_channel;
use std::sync::Arc;

use anyhow::Result;
use floem_renderer::gpu_resources::GpuResources;
use floem_renderer::text::fontdb::ID;
use floem_renderer::text::{LayoutGlyph, LayoutRun, FONT_SYSTEM};
use floem_renderer::{Img, Renderer};
use peniko::kurbo::Size;
use peniko::{
    color::palette,
    kurbo::{Affine, Point, Rect, Shape},
    Blob, BrushRef, Color,
};
use peniko::{Compose, Fill, ImageAlphaType, ImageData, Mix};
use vello::kurbo::Stroke;
use vello::util::RenderSurface;
use vello::wgpu::Device;
use vello::{AaConfig, RendererOptions, Scene};
use wgpu::util::TextureBlitter;
use wgpu::{Adapter, DeviceType, Queue, TextureAspect, TextureFormat};

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
    font_cache: HashMap<ID, vello::peniko::FontData>,
    adapter: Adapter,
}

impl VelloRenderer {
    pub fn new(
        gpu_resources: GpuResources,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        scale: f64,
        _font_embolden: f32,
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

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: texture_format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
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
            font_cache: HashMap::new(),
            adapter,
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

    pub const fn scale(&self) -> f64 {
        self.window_scale
    }

    pub const fn size(&self) -> Size {
        Size::new(
            self.surface.config.width as f64,
            self.surface.config.height as f64,
        )
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
    }

    fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s Stroke,
    ) {
        self.scene.stroke(
            stroke,
            self.transform.then_scale(self.window_scale),
            brush,
            None,
            shape,
        );
    }

    fn fill<'b>(&mut self, path: &impl Shape, brush: impl Into<BrushRef<'b>>, blur_radius: f64) {
        let brush: BrushRef<'b> = brush.into();

        // For solid colors with specific shapes, use optimized methods
        if blur_radius > 0.0 {
            if let BrushRef::Solid(color) = brush {
                if let Some(rounded) = path.as_rounded_rect() {
                    if rounded.radii().top_left == rounded.radii().top_right
                        && rounded.radii().top_left == rounded.radii().bottom_left
                        && rounded.radii().top_left == rounded.radii().bottom_right
                    {
                        let rect_radius = rounded.radii().top_left;
                        let rect = rounded.rect();
                        self.scene.draw_blurred_rounded_rect(
                            self.transform.then_scale(self.window_scale),
                            rect,
                            color,
                            rect_radius,
                            blur_radius,
                        );
                        return;
                    }
                } else if let Some(rect) = path.as_rect() {
                    self.scene.draw_blurred_rounded_rect(
                        self.transform.then_scale(self.window_scale),
                        rect,
                        color,
                        0.,
                        blur_radius,
                    );
                    return;
                }
            }
        }

        self.scene.fill(
            vello::peniko::Fill::NonZero,
            self.transform.then_scale(self.window_scale),
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
            blend,
            alpha,
            self.transform.then_scale(self.window_scale) * transform,
            clip,
        );
    }

    fn pop_layer(&mut self) {
        self.scene.pop_layer();
    }

    fn draw_text_with_layout<'b>(
        &mut self,
        layout: impl Iterator<Item = LayoutRun<'b>>,
        pos: impl Into<Point>,
    ) {
        let pos: Point = pos.into();
        let transform = self
            .transform
            .pre_translate((pos.x, pos.y).into())
            .then_scale(self.window_scale);

        for line in layout {
            let mut current_run: Option<GlyphRun> = None;

            for glyph in line.glyphs {
                let color = glyph.color_opt.map_or(palette::css::BLACK, |c| {
                    Color::from_rgba8(c.r(), c.g(), c.b(), c.a())
                });
                let font_size = glyph.font_size;
                let font_id = glyph.font_id;
                let metadata = glyph.metadata;

                if current_run.as_ref().is_none_or(|run| {
                    run.color != color
                        || run.font_size != font_size
                        || run.font_id != font_id
                        || run.metadata != metadata
                }) {
                    if let Some(run) = current_run.take() {
                        self.draw_glyph_run(
                            run,
                            transform.pre_translate((0., line.line_y.into()).into()),
                        );
                    }
                    current_run = Some(GlyphRun {
                        color,
                        font_size,
                        font_id,
                        metadata,
                        glyphs: Vec::new(),
                    });
                }

                if let Some(run) = &mut current_run {
                    run.glyphs.push(glyph);
                }
            }

            if let Some(run) = current_run.take() {
                self.draw_glyph_run(
                    run,
                    transform.pre_translate((0., line.line_y.into()).into()),
                );
            }
        }
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
            self.transform
                .pre_scale_non_uniform(scale_x, scale_y)
                .then_translate((translate_x, translate_y).into())
                .then_scale(self.window_scale),
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

        let new = brush.map_or_else(
            || vello_svg::render_tree(svg.tree),
            |brush| {
                let brush = brush.into();
                let size = Size::new(svg_size.width() as _, svg_size.height() as _);
                let fill_rect = Rect::from_origin_size(Point::ZERO, size);

                alpha_mask_scene(
                    size,
                    |scene| {
                        scene.append(&vello_svg::render_tree(svg.tree), None);
                    },
                    move |scene| {
                        scene.fill(Fill::NonZero, Affine::IDENTITY, brush, None, &fill_rect);
                    },
                )
            },
        );

        // Apply transformations to fit the SVG within the provided rectangle
        self.scene.append(
            &new,
            Some(
                self.transform
                    .pre_scale_non_uniform(scale_x, scale_y)
                    .pre_translate((translate_x, translate_y).into())
                    .then_scale(self.window_scale),
            ),
        );
    }

    fn set_transform(&mut self, transform: Affine) {
        self.transform = transform;
    }

    fn set_z_index(&mut self, _z_index: i32) {}

    fn clip(&mut self, shape: &impl Shape) {
        // if shape.bounding_box().is_zero_area() {
        //     return;
        // }
        // self.scene.pop_layer();
        // self.scene.push_layer(
        //     vello::peniko::BlendMode::default(),
        //     1.,
        //     self.transform.then_scale(self.window_scale),
        //     shape,
        // );
    }

    fn clear_clip(&mut self) {
        // self.scene.pop_layer();
    }

    fn finish(&mut self) -> Option<vello::peniko::ImageBrush> {
        if self.capture {
            self.render_capture_image()
        } else {
            if let Ok(surface_texture) = self.surface.surface.get_current_texture() {
                self.renderer
                    .render_to_texture(
                        &self.device,
                        &self.queue,
                        &self.scene,
                        &self.surface.target_view,
                        &vello::RenderParams {
                            base_color: palette::css::TRANSPARENT, // Background color
                            width: self.surface.config.width,
                            height: self.surface.config.height,
                            antialiasing_method: vello::AaConfig::Msaa16,
                        },
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
                    base_color: palette::css::BLACK, // Background color
                    width: self.surface.config.width * self.window_scale as u32,
                    height: self.surface.config.height * self.window_scale as u32,
                    antialiasing_method: AaConfig::Area,
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
        self.device.poll(wgpu::PollType::Wait).ok()?;

        let slice = buffer.slice(..);
        let (tx, rx) = sync_channel(1);
        slice.map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());

        loop {
            if let Ok(r) = rx.try_recv() {
                break r.ok()?;
            }
            if matches!(
                self.device.poll(wgpu::PollType::Wait).ok()?,
                wgpu::PollStatus::WaitSucceeded
            ) {
                rx.recv().ok()?.ok()?;
                break;
            }
        }

        let mut cropped_buffer = Vec::new();
        let buffer: Vec<u8> = slice.get_mapped_range().to_owned();

        let mut cursor = 0;
        let row_size = self.surface.config.width as usize * bytes_per_pixel as usize;
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

fn common_alpha_mask_scene(
    size: Size,
    alpha_mask: impl FnOnce(&mut Scene),
    item: impl FnOnce(&mut Scene),
    compose_mode: Compose,
) -> Scene {
    let mut scene = Scene::new();
    scene.push_layer(
        Mix::Normal,
        1.0,
        Affine::IDENTITY,
        &Rect::from_origin_size((0., 0.), size),
    );

    alpha_mask(&mut scene);

    scene.push_layer(
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

struct GlyphRun<'a> {
    color: Color,
    font_size: f32,
    font_id: ID,
    metadata: usize,
    glyphs: Vec<&'a LayoutGlyph>,
}

impl VelloRenderer {
    fn get_font(&mut self, font_id: ID) -> vello::peniko::FontData {
        self.font_cache.get(&font_id).cloned().unwrap_or_else(|| {
            let mut font_system = FONT_SYSTEM.lock();
            let font = font_system.get_font(font_id).unwrap();
            let face = font_system.db().face(font_id).unwrap();
            let font_data = font.data();
            let font_index = face.index;
            drop(font_system);
            let font =
                vello::peniko::FontData::new(Blob::new(Arc::new(font_data.to_vec())), font_index);
            self.font_cache.insert(font_id, font.clone());
            font
        })
    }

    fn draw_glyph_run(&mut self, run: GlyphRun, transform: Affine) {
        let font = self.get_font(run.font_id);
        self.scene
            .draw_glyphs(&font)
            .font_size(run.font_size)
            .brush(run.color)
            .hint(false)
            .transform(transform)
            .draw(
                Fill::NonZero,
                run.glyphs.into_iter().map(|glyph| vello::Glyph {
                    id: glyph.glyph_id.into(),
                    x: glyph.x,
                    y: glyph.y,
                }),
            );
    }
}
