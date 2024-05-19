use std::mem;
use std::sync::mpsc::sync_channel;
use std::sync::Arc;

use anyhow::Result;
use floem_renderer::cosmic_text::{SubpixelBin, SwashCache, TextLayout};
use floem_renderer::{tiny_skia, Img, Renderer};
use floem_vger_rs::{Image, PaintIndex, PixelFormat, Vger};
use image::{DynamicImage, EncodableLayout, RgbaImage};
use peniko::{
    kurbo::{Affine, Point, Rect, Shape, Vec2},
    BrushRef, Color, GradientKind,
};
use wgpu::{
    Backends, Device, DeviceType, Queue, StoreOp, Surface, SurfaceConfiguration, TextureFormat,
};

pub struct VgerRenderer {
    device: Arc<Device>,
    #[allow(unused)]
    queue: Arc<Queue>,
    surface: Surface<'static>,
    vger: Vger,
    alt_vger: Option<Vger>,
    config: SurfaceConfiguration,
    scale: f64,
    transform: Affine,
    clip: Option<Rect>,
    capture: bool,
}

impl VgerRenderer {
    pub fn new<W: wgpu::WindowHandle + 'static>(
        window: W,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::util::backend_bits_from_env().unwrap_or(Backends::all()),
            ..Default::default()
        });

        let surface = instance.create_surface(window)?;

        let adapter =
            futures::executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            }))
            .ok_or_else(|| anyhow::anyhow!("can't get adapter"))?;

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

        let (device, queue) = futures::executor::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                ..Default::default()
            },
            None,
        ))?;
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

        let vger = floem_vger_rs::Vger::new(device.clone(), queue.clone(), texture_format);

        Ok(Self {
            device,
            queue,
            surface,
            vger,
            alt_vger: None,
            scale,
            config,
            transform: Affine::IDENTITY,
            clip: None,
            capture: false,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32, scale: f64) {
        if width != self.config.width || height != self.config.height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
        self.scale = scale;
    }

    pub fn set_scale(&mut self, scale: f64) {
        self.scale = scale;
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }
}

impl VgerRenderer {
    fn brush_to_paint<'b>(&mut self, brush: impl Into<BrushRef<'b>>) -> Option<PaintIndex> {
        let paint = match brush.into() {
            BrushRef::Solid(color) => self.vger.color_paint(vger_color(color)),
            BrushRef::Gradient(g) => match g.kind {
                GradientKind::Linear { start, end } => {
                    let mut stops = g.stops.iter();
                    let inner_color = stops.next()?;
                    let outer_color = stops.next()?;
                    let inner_color = vger_color(inner_color.color);
                    let outer_color = vger_color(outer_color.color);
                    let start =
                        floem_vger_rs::defs::LocalPoint::new(start.x as f32, start.y as f32);
                    let end = floem_vger_rs::defs::LocalPoint::new(end.x as f32, end.y as f32);
                    self.vger
                        .linear_gradient(start, end, inner_color, outer_color, 0.0)
                }
                GradientKind::Radial { .. } => return None,
                GradientKind::Sweep { .. } => return None,
            },
            BrushRef::Image(_) => return None,
        };
        Some(paint)
    }

    fn vger_point(&self, point: Point) -> floem_vger_rs::defs::LocalPoint {
        let coeffs = self.transform.as_coeffs();
        let point = point + Vec2::new(coeffs[4], coeffs[5]);
        floem_vger_rs::defs::LocalPoint::new(
            (point.x * self.scale) as f32,
            (point.y * self.scale) as f32,
        )
    }

    fn vger_rect(&self, rect: Rect) -> floem_vger_rs::defs::LocalRect {
        let origin = rect.origin();
        let origin = self.vger_point(origin);

        let end = Point::new(rect.x1, rect.y1);
        let end = self.vger_point(end);

        let size = (end - origin).to_size();
        floem_vger_rs::defs::LocalRect::new(origin, size)
    }

    fn render_image(&mut self) -> Option<DynamicImage> {
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            label: Some("render_texture"),
            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
        };
        let texture = self.device.create_texture(&texture_desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let desc = wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
            })],
            ..Default::default()
        };

        self.vger.encode(&desc);

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

        RgbaImage::from_raw(self.config.width, height, cropped_buffer).map(DynamicImage::ImageRgba8)
    }
}

impl Renderer for VgerRenderer {
    fn begin(&mut self, capture: bool) {
        // Switch to the capture Vger if needed
        if self.capture != capture {
            self.capture = capture;
            if self.alt_vger.is_none() {
                self.alt_vger = Some(floem_vger_rs::Vger::new(
                    self.device.clone(),
                    self.queue.clone(),
                    TextureFormat::Rgba8Unorm,
                ));
            }
            mem::swap(&mut self.vger, self.alt_vger.as_mut().unwrap())
        }

        self.transform = Affine::IDENTITY;
        self.vger.begin(
            self.config.width as f32,
            self.config.height as f32,
            self.scale as f32,
        );
    }

    fn stroke<'b>(&mut self, shape: &impl Shape, brush: impl Into<BrushRef<'b>>, width: f64) {
        let paint = match self.brush_to_paint(brush) {
            Some(paint) => paint,
            None => return,
        };
        let width = (width * self.scale).round() as f32;
        if let Some(rect) = shape.as_rect() {
            let min = rect.origin();
            let max = min + rect.size().to_vec2();
            self.vger.stroke_rect(
                self.vger_point(min),
                self.vger_point(max),
                0.0,
                width,
                paint,
            );
        } else if let Some(rect) = shape.as_rounded_rect() {
            let min = rect.origin();
            let max = min + rect.rect().size().to_vec2();
            let radius = (rect.radii().top_left * self.scale) as f32;
            self.vger.stroke_rect(
                self.vger_point(min),
                self.vger_point(max),
                radius,
                width,
                paint,
            );
        } else if let Some(line) = shape.as_line() {
            self.vger.stroke_segment(
                self.vger_point(line.p0),
                self.vger_point(line.p1),
                width,
                paint,
            );
        } else if let Some(circle) = shape.as_circle() {
            self.vger.stroke_arc(
                self.vger_point(circle.center),
                (circle.radius * self.scale) as f32,
                width,
                0.0,
                std::f32::consts::PI,
                paint,
            );
        } else {
            for segment in shape.path_segments(0.0) {
                match segment {
                    peniko::kurbo::PathSeg::Line(ln) => self.vger.stroke_segment(
                        self.vger_point(ln.p0),
                        self.vger_point(ln.p1),
                        width,
                        paint,
                    ),
                    peniko::kurbo::PathSeg::Quad(bez) => {
                        self.vger.stroke_bezier(
                            self.vger_point(bez.p0),
                            self.vger_point(bez.p1),
                            self.vger_point(bez.p2),
                            width,
                            paint,
                        );
                    }

                    peniko::kurbo::PathSeg::Cubic(_) => todo!(),
                }
            }
        }
    }

    fn fill<'b>(&mut self, path: &impl Shape, brush: impl Into<BrushRef<'b>>, blur_radius: f64) {
        let paint = match self.brush_to_paint(brush) {
            Some(paint) => paint,
            None => return,
        };
        if let Some(rect) = path.as_rect() {
            self.vger.fill_rect(
                self.vger_rect(rect),
                0.0,
                paint,
                (blur_radius * self.scale) as f32,
            );
        } else if let Some(rect) = path.as_rounded_rect() {
            self.vger.fill_rect(
                self.vger_rect(rect.rect()),
                (rect.radii().top_left * self.scale) as f32,
                paint,
                (blur_radius * self.scale) as f32,
            );
        } else if let Some(circle) = path.as_circle() {
            self.vger.fill_circle(
                self.vger_point(circle.center),
                (circle.radius * self.scale) as f32,
                paint,
            )
        } else {
            let mut first = true;
            for segment in path.path_segments(0.1) {
                match segment {
                    peniko::kurbo::PathSeg::Line(line) => {
                        if first {
                            first = false;
                            self.vger.move_to(self.vger_point(line.p0));
                        }
                        self.vger
                            .quad_to(self.vger_point(line.p1), self.vger_point(line.p1));
                    }
                    peniko::kurbo::PathSeg::Quad(quad) => {
                        if first {
                            first = false;
                            self.vger.move_to(self.vger_point(quad.p0));
                        }
                        self.vger
                            .quad_to(self.vger_point(quad.p1), self.vger_point(quad.p2));
                    }
                    peniko::kurbo::PathSeg::Cubic(_) => {}
                }
            }
            self.vger.fill(paint);
        }
    }

    fn draw_text(&mut self, layout: &TextLayout, pos: impl Into<Point>) {
        let mut swash_cache = SwashCache::new();
        let transform = self.transform.as_coeffs();
        let offset = Vec2::new(transform[4], transform[5]);
        let pos: Point = pos.into();
        let clip = self.clip;
        for line in layout.layout_runs() {
            if let Some(rect) = clip {
                let y = pos.y + offset.y + line.line_y as f64;
                if y + (line.line_height as f64) < rect.y0 {
                    continue;
                }
                if y - (line.line_height as f64) > rect.y1 {
                    break;
                }
            }
            'line_loop: for glyph_run in line.glyphs {
                let x = glyph_run.x + pos.x as f32 + offset.x as f32;
                let y = line.line_y + pos.y as f32 + offset.y as f32;

                if let Some(rect) = clip {
                    if ((x + glyph_run.w) as f64) < rect.x0 {
                        continue;
                    } else if x as f64 > rect.x1 {
                        break 'line_loop;
                    }
                }

                if glyph_run.is_tab {
                    continue;
                }

                if let Some(paint) = self.brush_to_paint(glyph_run.color) {
                    let glyph_x = x * self.scale as f32;
                    let (new_x, subpx_x) = SubpixelBin::new(glyph_x);
                    let glyph_x = new_x as f32;

                    let glyph_y = (y * self.scale as f32).round();
                    let (new_y, subpx_y) = SubpixelBin::new(glyph_y);
                    let glyph_y = new_y as f32;

                    let font_size = (glyph_run.font_size * self.scale as f32).round() as u32;
                    self.vger.render_glyph(
                        glyph_x,
                        glyph_y,
                        glyph_run.cache_key.font_id,
                        glyph_run.cache_key.glyph_id,
                        font_size,
                        (subpx_x, subpx_y),
                        || {
                            let mut cache_key = glyph_run.cache_key;
                            cache_key.font_size = font_size;
                            cache_key.x_bin = subpx_x;
                            cache_key.y_bin = subpx_y;
                            let image = swash_cache.get_image_uncached(cache_key);
                            image.unwrap_or_default()
                        },
                        paint,
                    );
                }
            }
        }
    }

    fn draw_img(&mut self, img: Img<'_>, rect: Rect) {
        let transform = self.transform.as_coeffs();
        let width = (rect.width() * self.scale).round() as u32;
        let height = (rect.height() * self.scale).round() as u32;
        let width = width.max(1);
        let height = height.max(1);
        let origin = rect.origin();
        let x = ((origin.x + transform[4]) * self.scale).round() as f32;
        let y = ((origin.y + transform[5]) * self.scale).round() as f32;

        self.vger.render_image(x, y, img.hash, width, height, || {
            let rgba = img.img.clone().into_rgba8();
            let data = rgba.as_bytes().to_vec();

            let (width, height) = rgba.dimensions();
            Image {
                width,
                height,
                data,
                pixel_format: PixelFormat::Rgba,
            }
        });
    }

    fn draw_svg<'b>(
        &mut self,
        svg: floem_renderer::Svg<'b>,
        rect: Rect,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        let transform = self.transform.as_coeffs();
        let width = (rect.width() * self.scale).round() as u32;
        let height = (rect.height() * self.scale).round() as u32;
        let width = width.max(1);
        let height = height.max(1);
        let origin = rect.origin();
        let x = ((origin.x + transform[4]) * self.scale).round() as f32;
        let y = ((origin.y + transform[5]) * self.scale).round() as f32;

        let paint = brush.and_then(|brush| self.brush_to_paint(brush));
        self.vger.render_svg(
            x,
            y,
            svg.hash,
            width,
            height,
            || {
                let mut img = tiny_skia::Pixmap::new(width, height).unwrap();
                let scale = (width as f32 / svg.tree.size().width())
                    .min(height as f32 / svg.tree.size().height());
                let transform = tiny_skia::Transform::from_scale(scale, scale);
                resvg::render(svg.tree, transform, &mut img.as_mut());
                img.take()
            },
            paint,
        );
    }

    fn transform(&mut self, transform: Affine) {
        self.transform = transform;
    }

    fn set_z_index(&mut self, z_index: i32) {
        self.vger.set_z_index(z_index);
    }

    fn clip(&mut self, shape: &impl Shape) {
        let (rect, radius) = if let Some(rect) = shape.as_rect() {
            (rect, 0.0)
        } else if let Some(rect) = shape.as_rounded_rect() {
            (rect.rect(), rect.radii().top_left)
        } else {
            (shape.bounding_box(), 0.0)
        };

        self.vger
            .scissor(self.vger_rect(rect), (radius * self.scale) as f32);

        let transform = self.transform.as_coeffs();
        let offset = Vec2::new(transform[4], transform[5]);
        self.clip = Some(rect + offset);
    }

    fn clear_clip(&mut self) {
        self.vger.reset_scissor();
        self.clip = None;
    }

    fn finish(&mut self) -> Option<DynamicImage> {
        if self.capture {
            self.render_image()
        } else {
            if let Ok(frame) = self.surface.get_current_texture() {
                let texture_view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let desc = wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &texture_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                };

                self.vger.encode(&desc);
                frame.present();
            }
            None
        }
    }
}

fn vger_color(color: Color) -> floem_vger_rs::Color {
    floem_vger_rs::Color {
        r: color.r as f32 / 255.0,
        g: color.g as f32 / 255.0,
        b: color.b as f32 / 255.0,
        a: color.a as f32 / 255.0,
    }
}
