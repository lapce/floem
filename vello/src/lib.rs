use std::collections::HashMap;
use std::mem;
use std::sync::mpsc::sync_channel;
use std::sync::Arc;

use anyhow::Result;
use floem_renderer::gpu_resources::GpuResources;
use floem_renderer::text::{TextLayout, FONT_SYSTEM};
use floem_renderer::{Img, Renderer};
use peniko::kurbo::Size;
use peniko::{
    kurbo::{Affine, Point, Rect, Shape},
    Blob, BrushRef, Color,
};
use peniko::{Compose, Fill, Mix};
use vello::kurbo::Stroke;
use vello::{AaConfig, Glyph, RendererOptions, Scene};
use wgpu::{
    Device, DeviceType, Queue, Surface, SurfaceConfiguration, TextureAspect, TextureFormat,
};

pub struct VelloRenderer {
    device: Arc<Device>,
    #[allow(unused)]
    queue: Arc<Queue>,
    surface: Surface<'static>,
    renderer: vello::Renderer,
    scene: Scene,
    alt_scene: Option<Scene>,
    config: SurfaceConfiguration,
    window_scale: f64,
    transform: Affine,
    capture: bool,
    font_cache: HashMap<floem_renderer::text::fontdb::ID, vello::peniko::Font>,
}

impl VelloRenderer {
    pub fn new(
        gpu_resources: GpuResources,
        width: u32,
        height: u32,
        scale: f64,
        _font_embolden: f32,
    ) -> Result<Self> {
        let GpuResources {
            surface,
            adapter,
            device,
            queue,
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

        let device = Arc::new(device);
        let queue = Arc::new(queue);

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
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let scene = Scene::new();
        let renderer = vello::Renderer::new(
            &device,
            RendererOptions {
                surface_format: Some(texture_format),
                use_cpu: false,
                antialiasing_support: vello::AaSupport::all(),
                num_init_threads: None,
            },
        )
        .unwrap();

        Ok(Self {
            device,
            queue,
            surface,
            renderer,
            scene,
            alt_scene: None,
            window_scale: scale,
            config,
            transform: Affine::IDENTITY,
            capture: false,
            font_cache: HashMap::new(),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32, scale: f64) {
        if width != self.config.width || height != self.config.height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
        self.window_scale = scale;
    }

    pub fn set_scale(&mut self, scale: f64) {
        self.window_scale = scale;
    }

    pub fn scale(&self) -> f64 {
        self.window_scale
    }

    pub fn size(&self) -> Size {
        Size::new(self.config.width as f64, self.config.height as f64)
    }
}

impl Renderer for VelloRenderer {
    fn begin(&mut self, capture: bool) {
        if self.capture != capture {
            self.capture = capture;
            if self.alt_scene.is_none() {
                self.alt_scene = Some(Scene::new());
            }
            if let Some(scene) = self.alt_scene.as_mut() {
                scene.reset()
            }
            self.scene.reset();
            mem::swap(&mut self.scene, self.alt_scene.as_mut().unwrap())
        } else {
            self.scene.reset();
        };
        self.transform = Affine::IDENTITY;
    }

    fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s Stroke,
    ) {
        if stroke.width * self.window_scale < 2. {
            let brush: BrushRef = brush.into();
            match &brush {
                BrushRef::Solid(color) => {
                    let mut stroke = stroke.clone();
                    // this special handlign is done to make thin strokes look better
                    stroke.width *= 1.5;
                    let color = color.multiply_alpha(0.5);
                    self.scene.stroke(
                        &stroke,
                        self.transform.then_scale(self.window_scale),
                        BrushRef::Solid(color),
                        None,
                        shape,
                    );
                }

                _ => {
                    self.scene.stroke(
                        stroke,
                        self.transform.then_scale(self.window_scale),
                        brush,
                        None,
                        shape,
                    );
                }
            }
        } else {
            self.scene.stroke(
                stroke,
                self.transform.then_scale(self.window_scale),
                brush,
                None,
                shape,
            );
        }
    }

    fn fill<'b>(&mut self, path: &impl Shape, brush: impl Into<BrushRef<'b>>, blur_radius: f64) {
        let brush: BrushRef<'b> = brush.into();
        if blur_radius > 0. {
            let color = match brush {
                BrushRef::Solid(c) => c,
                _ => return,
            };
            let rect = path.as_rounded_rect().unwrap_or_default().rect();

            self.scene.draw_blurred_rounded_rect(
                self.transform.then_scale(self.window_scale),
                rect,
                color,
                blur_radius,
                2.5,
            );
        } else {
            self.scene.fill(
                vello::peniko::Fill::NonZero,
                self.transform.then_scale(self.window_scale),
                brush,
                None,
                path,
            );
        }
    }

    fn draw_text(&mut self, layout: &TextLayout, pos: impl Into<Point>) {
        let offset = self.transform.translation();
        let pos: Point = pos.into();
        let mut glyphs_to_draw = Vec::new();

        for line in layout.layout_runs() {
            let mut glyph_runs = line.glyphs.iter().peekable();

            while glyph_runs.peek().is_some() {
                // Start processing the first glyph
                if let Some(start_glyph) = glyph_runs.next() {
                    let start_color = match start_glyph.color_opt {
                        Some(c) => Color::rgba8(c.r(), c.g(), c.b(), c.a()),
                        None => Color::BLACK,
                    };
                    let start_font_size = start_glyph.font_size;
                    let start_font_id = start_glyph.font_id;

                    let font = self
                        .font_cache
                        .get(&start_font_id)
                        .cloned()
                        .unwrap_or_else(|| {
                            let font = FONT_SYSTEM.lock().get_font(start_font_id).unwrap();
                            let font = vello::peniko::Font::new(
                                Blob::new(Arc::new(font.data().to_vec())),
                                0,
                            );
                            self.font_cache.insert(start_font_id, font.clone());
                            font
                        });

                    glyphs_to_draw.clear();
                    glyphs_to_draw.push(start_glyph);

                    // Collect all glyphs with the same properties
                    while let Some(peeked_glyph) = glyph_runs.peek() {
                        if peeked_glyph.color_opt == start_glyph.color_opt
                            && peeked_glyph.font_size == start_font_size
                            && peeked_glyph.font_id == start_font_id
                        {
                            glyphs_to_draw.push(glyph_runs.next().unwrap());
                        } else {
                            break;
                        }
                    }

                    // Draw the collected glyphs
                    self.scene
                        .draw_glyphs(&font)
                        .font_size(start_font_size)
                        .brush(start_color)
                        .hint(false)
                        .glyph_transform(Some(Affine::IDENTITY.then_scale(self.window_scale)))
                        .draw(
                            Fill::NonZero,
                            glyphs_to_draw.iter().map(|glyph| {
                                let x = glyph.x + pos.x as f32 + offset.x as f32;
                                let y = line.line_y + pos.y as f32 + offset.y as f32;
                                let glyph_x = x * self.window_scale as f32;
                                let glyph_y = (y * self.window_scale as f32).round();
                                Glyph {
                                    id: glyph.glyph_id as u32,
                                    x: glyph_x,
                                    y: glyph_y,
                                }
                            }),
                        );
                }
            }
        }
    }

    fn draw_img(&mut self, img: Img<'_>, rect: Rect) {
        let rect_width = rect.width().max(1.);
        let rect_height = rect.height().max(1.);

        let scale_x = rect_width / img.img.width as f64;
        let scale_y = rect_height / img.img.height as f64;

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

        let scale_x = rect_width / svg_size.width() as f64;
        let scale_y = rect_height / svg_size.height() as f64;

        let translate_x = rect.min_x();
        let translate_y = rect.min_y();

        let new = if let Some(brush) = brush {
            let brush = brush.into();
            alpha_mask_scene(
                rect.size(),
                |scene| {
                    scene.append(&vello_svg::render_tree(svg.tree), None);
                },
                move |scene| {
                    scene.fill(Fill::NonZero, Affine::IDENTITY, brush, None, &rect);
                },
            )
        } else {
            vello_svg::render_tree(svg.tree)
        };

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

    fn transform(&mut self, transform: Affine) {
        self.transform = transform;
    }

    fn set_z_index(&mut self, _z_index: i32) {}

    fn clip(&mut self, shape: &impl Shape) {
        if shape.bounding_box().is_zero_area() {
            return;
        }
        self.scene.pop_layer();
        self.scene.push_layer(
            vello::peniko::BlendMode::default(),
            1.,
            // Affine::IDENTITY,
            self.transform.then_scale(self.window_scale),
            shape,
        );
    }

    fn clear_clip(&mut self) {
        self.scene.pop_layer();
    }

    fn finish(&mut self) -> Option<vello::peniko::Image> {
        if self.capture {
            self.render_capture_image()
        } else {
            if let Ok(frame) = self.surface.get_current_texture() {
                // Render the scene using Vello's `render_to_surface` function
                self.renderer
                    .render_to_surface(
                        &self.device,
                        &self.queue,
                        &self.scene,
                        &frame,
                        &vello::RenderParams {
                            base_color: Color::BLACK, // Background color
                            width: self.config.width,
                            height: self.config.height,
                            antialiasing_method: vello::AaConfig::Area,
                        },
                    )
                    .unwrap();

                frame.present();
            }
            None
        }
    }
}

impl VelloRenderer {
    fn render_capture_image(&mut self) -> Option<peniko::Image> {
        let width_align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - 1;
        let width = (self.config.width + width_align) & !width_align;
        let height = self.config.height;
        let texture_desc = wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: self.config.width,
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
        });

        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                &self.scene,
                &view,
                &vello::RenderParams {
                    base_color: Color::BLACK, // Background color
                    width: self.config.width * self.window_scale as u32,
                    height: self.config.height * self.window_scale as u32,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .unwrap();

        let bytes_per_pixel = 4;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (width as u64 * height as u64 * bytes_per_pixel),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bytes_per_row = width * bytes_per_pixel as u32;
        assert!(bytes_per_row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT == 0);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::ImageCopyBuffer {
                buffer: &buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: None,
                },
            },
            texture_desc.size,
        );
        let command_buffer = encoder.finish();
        self.queue.submit(Some(command_buffer));
        self.device.poll(wgpu::Maintain::Wait);

        let slice = buffer.slice(..);
        let (tx, rx) = sync_channel(1);
        slice.map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());

        loop {
            if let Ok(r) = rx.try_recv() {
                break r.ok()?;
            }
            if let wgpu::MaintainResult::Ok = self.device.poll(wgpu::MaintainBase::Wait) {
                rx.recv().ok()?.ok()?;
                break;
            }
        }

        let mut cropped_buffer = Vec::new();
        let buffer: Vec<u8> = slice.get_mapped_range().to_owned();

        let mut cursor = 0;
        let row_size = self.config.width as usize * bytes_per_pixel as usize;
        for _ in 0..height {
            cropped_buffer.extend_from_slice(&buffer[cursor..(cursor + row_size)]);
            cursor += bytes_per_row as usize;
        }

        Some(vello::peniko::Image::new(
            Blob::new(Arc::new(cropped_buffer)),
            vello::peniko::Format::Rgba8,
            self.config.width,
            height,
        ))
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
            mix: Mix::Clip,
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
