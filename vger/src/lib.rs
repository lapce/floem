use std::cell::RefCell;
use std::sync::Arc;
use std::sync::mpsc;

use anyhow::Result;
use floem_vger_rs::{GlyphImage, PaintIndex, Vger};
use imaging::{
    BlurredRoundedRect, ClipRef, FillRef, GlyphRunRef, GroupRef, ImageBufferFormat,
    ImageBufferTarget, ImageRenderer, ImageRendererError, PaintSink, RenderSource, StrokeRef,
    record::Glyph, render::ImageTargetError,
};
use imaging_wgpu::{TextureRenderer, TextureRendererError, TextureTargetError, TextureViewTarget};
use peniko::kurbo::Stroke;
use peniko::{Blob, LinearGradientPosition};
use peniko::{
    BrushRef, Color, GradientKind,
    kurbo::{Affine, Point, Rect, Shape},
};
use swash::FontRef;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::Format;
use wgpu::{Device, DeviceType, Queue, StoreOp, TextureFormat};

thread_local! {
    /// Swash [`ScaleContext`] used for CPU glyph rasterization on vger cache misses.
    /// Thread-local so the `FnOnce` closure passed to `Vger::render_glyph` can
    /// borrow it without conflicting with the `&mut self` borrow on [`VgerRenderer`].
    static SCALE_CONTEXT: RefCell<ScaleContext> = RefCell::new(ScaleContext::new());
}

struct ResolvedTextRun {
    raster_scale: f64,
    raster_origin: Point,
    transform: Affine,
}

struct ResolvedGlyph {
    glyph_id: u16,
    baseline_x: f32,
    baseline_y: f32,
    subpx: (u8, u8),
}

pub struct VgerRenderer {
    device: Arc<Device>,
    queue: Arc<Queue>,
    vger: Vger,
    size: (u32, u32),
    transform: Affine,
    clip: Option<Rect>,
}

impl VgerRenderer {
    pub fn new(
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        width: u32,
        height: u32,
    ) -> Result<Self> {
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
        let vger =
            floem_vger_rs::Vger::new(device.clone(), queue.clone(), TextureFormat::Rgba8Unorm);

        Ok(Self {
            device,
            queue,
            vger,
            size: (width, height),
            transform: Affine::IDENTITY,
            clip: None,
        })
    }

    pub fn begin(&mut self, width: u32, height: u32) {
        self.size = (width, height);
        self.transform = Affine::IDENTITY;
        self.clip = None;
        self.vger.begin(self.size.0 as f32, self.size.1 as f32, 1.0);
    }
}

impl PaintSink for VgerRenderer {
    fn push_clip(&mut self, clip: ClipRef<'_>) {
        let (transform, rect, radius) = match clip {
            ClipRef::Fill {
                transform, shape, ..
            }
            | ClipRef::Stroke {
                transform, shape, ..
            } => {
                let (rect, radius) = match shape {
                    imaging::GeometryRef::Rect(rect) => (rect, 0.0),
                    imaging::GeometryRef::RoundedRect(rect) => (rect.rect(), rect.radii().top_left),
                    imaging::GeometryRef::Path(path) => (path.bounding_box(), 0.0),
                    imaging::GeometryRef::OwnedPath(path) => (path.bounding_box(), 0.0),
                };
                (transform, rect, radius)
            }
        };

        self.set_transform(transform);
        self.clip(&rect.to_rounded_rect(radius));
    }

    fn pop_clip(&mut self) {
        self.clear_clip();
    }

    fn push_group(&mut self, _group: GroupRef<'_>) {}

    fn pop_group(&mut self) {}

    fn fill(&mut self, draw: FillRef<'_>) {
        self.set_transform(draw.transform);
        match draw.shape {
            imaging::GeometryRef::Rect(rect) => self.fill(&rect, draw.brush, 0.0),
            imaging::GeometryRef::RoundedRect(rect) => self.fill(&rect, draw.brush, 0.0),
            imaging::GeometryRef::Path(path) => self.fill(path, draw.brush, 0.0),
            imaging::GeometryRef::OwnedPath(path) => self.fill(&path, draw.brush, 0.0),
        }
    }

    fn stroke(&mut self, draw: StrokeRef<'_>) {
        self.set_transform(draw.transform);
        match draw.shape {
            imaging::GeometryRef::Rect(rect) => self.stroke(&rect, draw.brush, draw.stroke),
            imaging::GeometryRef::RoundedRect(rect) => self.stroke(&rect, draw.brush, draw.stroke),
            imaging::GeometryRef::Path(path) => self.stroke(path, draw.brush, draw.stroke),
            imaging::GeometryRef::OwnedPath(path) => self.stroke(&path, draw.brush, draw.stroke),
        }
    }

    fn glyph_run(&mut self, draw: GlyphRunRef<'_>, glyphs: &mut dyn Iterator<Item = Glyph>) {
        self.draw_glyphs(draw, glyphs);
    }

    fn blurred_rounded_rect(&mut self, draw: BlurredRoundedRect) {
        self.set_transform(draw.transform);
        self.fill(
            &draw.rect.to_rounded_rect(draw.radius),
            draw.color,
            draw.std_dev,
        );
    }
}

impl VgerRenderer {
    fn create_texture(&self, width: u32, height: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            label: Some("floem_vger render_texture"),
            view_formats: &[TextureFormat::Rgba8Unorm],
        })
    }
}

fn supported_texture_formats() -> Vec<TextureFormat> {
    vec![TextureFormat::Rgba8Unorm]
}

fn texture_format_for_image(format: ImageBufferFormat) -> Option<TextureFormat> {
    match format {
        ImageBufferFormat::Rgba8Unorm => Some(TextureFormat::Rgba8Unorm),
        ImageBufferFormat::Rgba8UnormSrgb
        | ImageBufferFormat::Bgra8Unorm
        | ImageBufferFormat::Bgra8UnormSrgb => None,
        ImageBufferFormat::Rgb10a2Unorm
        | ImageBufferFormat::Rgba16Unorm
        | ImageBufferFormat::Rgba16Float => None,
    }
}

impl TextureRenderer for VgerRenderer {
    type TextureTarget = TextureViewTarget;
    type Texture = wgpu::Texture;

    fn supported_texture_formats(&self) -> Vec<TextureFormat> {
        supported_texture_formats()
    }

    fn render_source_into_texture(
        &mut self,
        source: &mut dyn RenderSource,
        target: Self::TextureTarget,
    ) -> Result<(), TextureRendererError> {
        if target.view.texture().format() != TextureFormat::Rgba8Unorm {
            return Err(TextureRendererError::Target(
                TextureTargetError::UnsupportedTextureFormat,
            ));
        }
        self.begin(target.width, target.height);
        source.paint_into(self);
        self.encode_to_view(&target.view);
        Ok(())
    }

    fn render_source_texture(
        &mut self,
        source: &mut dyn RenderSource,
        width: u32,
        height: u32,
    ) -> Result<Self::Texture, TextureRendererError> {
        self.begin(width, height);
        source.paint_into(self);
        let texture = self.create_texture(width, height);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.encode_to_view(&view);
        Ok(texture)
    }
}

impl ImageRenderer for VgerRenderer {
    fn supported_image_formats(&self) -> Vec<ImageBufferFormat> {
        vec![ImageBufferFormat::Rgba8Unorm]
    }

    fn render_source_into(
        &mut self,
        source: &mut dyn RenderSource,
        target: ImageBufferTarget<'_>,
    ) -> Result<(), ImageRendererError> {
        texture_format_for_image(target.format).ok_or(ImageRendererError::Target(
            ImageTargetError::UnsupportedTargetFormat,
        ))?;
        let texture = <Self as TextureRenderer>::render_source_texture(
            self,
            source,
            target.width,
            target.height,
        )
        .map_err(map_texture_to_image_error)?;
        read_texture_into(
            self.device.as_ref(),
            self.queue.as_ref(),
            &texture,
            target.width,
            target.height,
            target.data,
            target.bytes_per_row,
        )
        .map_err(|err| {
            ImageRendererError::backend(std::io::Error::other(format!(
                "vger backend failed to read rendered image: {err}"
            )))
        })
    }
}

fn read_texture_into(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    bytes: &mut [u8],
    output_bytes_per_row: usize,
) -> Result<(), &'static str> {
    let width_bytes = width * 4;
    let bytes_per_row = width_bytes.div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("floem_vger readback"),
        size: u64::from(bytes_per_row) * u64::from(height),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("floem_vger readback"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|_| "wgpu device poll failed")?;
    rx.recv()
        .map_err(|_| "wgpu readback callback dropped")?
        .map_err(|_| "wgpu readback buffer map failed")?;

    let mapped = slice.get_mapped_range();
    let width_bytes = width_bytes as usize;
    for (row, out_row) in mapped
        .chunks_exact(bytes_per_row as usize)
        .zip(bytes.chunks_exact_mut(output_bytes_per_row))
    {
        out_row[..width_bytes].copy_from_slice(&row[..width_bytes]);
    }
    drop(mapped);
    readback.unmap();
    Ok(())
}

impl VgerRenderer {
    fn device_transform(&self) -> Affine {
        self.transform
    }

    fn scale_components(&self) -> (f64, f64, f64) {
        let coeffs = self.device_transform().as_coeffs();
        let scale_x = coeffs[0].hypot(coeffs[1]);
        let scale_y = coeffs[2].hypot(coeffs[3]);
        let uniform = (scale_x + scale_y) * 0.5;
        (scale_x, scale_y, uniform)
    }

    fn affine_scale_components(transform: Affine) -> (f64, f64, f64) {
        let coeffs = transform.as_coeffs();
        let scale_x = coeffs[0].hypot(coeffs[1]);
        let scale_y = coeffs[2].hypot(coeffs[3]);
        let uniform = (scale_x + scale_y) * 0.5;
        (scale_x, scale_y, uniform)
    }

    fn normalize_affine(transform: Affine, include_translation: bool) -> Affine {
        let coeffs = transform.as_coeffs();
        let (scale_x, scale_y, _) = Self::affine_scale_components(transform);
        let tx = if include_translation { coeffs[4] } else { 0.0 };
        let ty = if include_translation { coeffs[5] } else { 0.0 };
        Affine::new([
            if scale_x != 0.0 {
                coeffs[0] / scale_x
            } else {
                0.0
            },
            if scale_x != 0.0 {
                coeffs[1] / scale_x
            } else {
                0.0
            },
            if scale_y != 0.0 {
                coeffs[2] / scale_y
            } else {
                0.0
            },
            if scale_y != 0.0 {
                coeffs[3] / scale_y
            } else {
                0.0
            },
            tx,
            ty,
        ])
    }

    fn resolve_text_run(transform: Affine) -> ResolvedTextRun {
        let (_, _, raster_scale) = Self::affine_scale_components(transform);
        let normalized_transform = Self::normalize_affine(transform, false);
        let raster_origin = normalized_transform.inverse() * (transform * Point::ZERO);
        ResolvedTextRun {
            raster_scale,
            raster_origin,
            transform: normalized_transform,
        }
    }

    fn subpixel_bin(value: f32) -> u8 {
        ((value.fract() + 1.0).fract() * 4.0).min(3.0) as u8
    }

    fn resolve_glyph(text_run: &ResolvedTextRun, glyph: Glyph) -> ResolvedGlyph {
        let glyph_x = text_run.raster_origin.x as f32 + glyph.x * text_run.raster_scale as f32;
        let glyph_y = text_run.raster_origin.y as f32 + glyph.y * text_run.raster_scale as f32;
        let baseline_x = glyph_x.floor();
        let baseline_y = glyph_y.floor();

        ResolvedGlyph {
            glyph_id: glyph.id as u16,
            baseline_x,
            baseline_y,
            subpx: (Self::subpixel_bin(glyph_x), Self::subpixel_bin(glyph_y)),
        }
    }

    fn brush_to_paint<'b>(&mut self, brush: impl Into<BrushRef<'b>>) -> Option<PaintIndex> {
        let paint = match brush.into() {
            BrushRef::Solid(color) => self.vger.color_paint(vger_color(color)),
            BrushRef::Gradient(g) => match g.kind {
                GradientKind::Linear(LinearGradientPosition { start, end }) => {
                    let mut stops = g.stops.iter();
                    let first_stop = stops.next()?;
                    let second_stop = stops.next()?;
                    let inner_color = vger_color(first_stop.color.to_alpha_color());
                    let outer_color = vger_color(second_stop.color.to_alpha_color());
                    let start = floem_vger_rs::defs::LocalPoint::new(
                        start.x as f32 * first_stop.offset,
                        start.y as f32 * first_stop.offset,
                    );
                    let end = floem_vger_rs::defs::LocalPoint::new(
                        end.x as f32 * second_stop.offset,
                        end.y as f32 * second_stop.offset,
                    );
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
        let coeffs = self.device_transform().as_coeffs();

        let transformed_x = coeffs[0] * point.x + coeffs[2] * point.y + coeffs[4];
        let transformed_y = coeffs[1] * point.x + coeffs[3] * point.y + coeffs[5];

        floem_vger_rs::defs::LocalPoint::new(transformed_x as f32, transformed_y as f32)
    }

    fn vger_rect(&self, rect: Rect) -> floem_vger_rs::defs::LocalRect {
        let origin = rect.origin();
        let origin = self.vger_point(origin);

        let end = Point::new(rect.x1, rect.y1);
        let end = self.vger_point(end);

        let size = (end - origin).to_size();
        floem_vger_rs::defs::LocalRect::new(origin, size)
    }

    fn encode_to_view(&mut self, view: &wgpu::TextureView) {
        let desc = wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
                depth_slice: None,
            })],
            ..Default::default()
        };

        self.vger.encode(&desc);
    }
}

fn map_texture_to_image_error(error: TextureRendererError) -> ImageRendererError {
    match error {
        TextureRendererError::Content(error) => ImageRendererError::Content(error),
        TextureRendererError::Target(error) => match error {
            TextureTargetError::InvalidTarget(_) | TextureTargetError::UnsupportedTextureFormat => {
                ImageRendererError::Target(ImageTargetError::UnsupportedTargetFormat)
            }
            TextureTargetError::DimensionsTooLarge => {
                ImageRendererError::backend(std::io::Error::other(error))
            }
            TextureTargetError::CreateGpuContext(_)
            | TextureTargetError::CreateGpuSurface
            | TextureTargetError::UnsupportedGpuBackend => {
                ImageRendererError::backend(std::io::Error::other(error))
            }
        },
        TextureRendererError::Unsupported(error) => ImageRendererError::Unsupported(error),
        TextureRendererError::Backend(error) => ImageRendererError::Backend(error),
    }
}

impl VgerRenderer {
    pub fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s Stroke,
    ) {
        let (_, _, scale) = self.scale_components();
        let paint = match self.brush_to_paint(brush) {
            Some(paint) => paint,
            None => return,
        };
        let width = (stroke.width * scale).round() as f32;
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
            return;
        } else if let Some(rect) = shape.as_rounded_rect() {
            if rect.radii().top_left == rect.radii().top_right
                && rect.radii().top_left == rect.radii().bottom_left
                && rect.radii().top_left == rect.radii().bottom_right
            {
                let min = rect.origin();
                let max = min + rect.rect().size().to_vec2();
                let radius = (rect.radii().top_left * scale) as f32;
                self.vger.stroke_rect(
                    self.vger_point(min),
                    self.vger_point(max),
                    radius,
                    width,
                    paint,
                );
                return;
            }
        } else if let Some(line) = shape.as_line() {
            self.vger.stroke_segment(
                self.vger_point(line.p0),
                self.vger_point(line.p1),
                width,
                paint,
            );
            return;
        } else if let Some(circle) = shape.as_circle() {
            self.vger.stroke_arc(
                self.vger_point(circle.center),
                (circle.radius * scale) as f32,
                width,
                0.0,
                std::f32::consts::PI,
                paint,
            );
            return;
        }

        for segment in shape.path_segments(0.001) {
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

                peniko::kurbo::PathSeg::Cubic(cubic) => {
                    // Approximates the cubic curve (p0, p1, p2, p3) with a quadratic curve (p0, q1, p3)
                    let q1 = ((cubic.p1.to_vec2() + cubic.p2.to_vec2()) * 3.0
                        - cubic.p0.to_vec2()
                        - cubic.p3.to_vec2())
                        / 4.0;

                    self.vger.stroke_bezier(
                        self.vger_point(cubic.p0),
                        self.vger_point(q1.to_point()),
                        self.vger_point(cubic.p3),
                        width,
                        paint,
                    );
                }
            }
        }
    }

    pub fn fill<'b>(
        &mut self,
        path: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        blur_radius: f64,
    ) {
        let (_, _, scale) = self.scale_components();
        let paint = match self.brush_to_paint(brush) {
            Some(paint) => paint,
            None => return,
        };
        if let Some(rect) = path.as_rect() {
            self.vger.fill_rect(
                self.vger_rect(rect),
                0.0,
                paint,
                (blur_radius * scale) as f32,
            );
            return;
        } else if let Some(rect) = path.as_rounded_rect() {
            if rect.radii().top_left == rect.radii().top_right
                && rect.radii().top_left == rect.radii().bottom_left
                && rect.radii().top_left == rect.radii().bottom_right
            {
                // Use `fill_rect` for uniform radii
                self.vger.fill_rect(
                    self.vger_rect(rect.rect()),
                    (rect.radii().top_left * scale) as f32,
                    paint,
                    (blur_radius * scale) as f32,
                );
                return;
            }
        } else if let Some(circle) = path.as_circle() {
            self.vger.fill_circle(
                self.vger_point(circle.center),
                (circle.radius * scale) as f32,
                paint,
            );
            return;
        }

        let mut first = true;
        for segment in path.path_segments(0.001) {
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
                peniko::kurbo::PathSeg::Cubic(cubic) => {
                    if first {
                        first = false;
                        self.vger.move_to(self.vger_point(cubic.p0));
                    }

                    // Approximates the cubic curve (p0, p1, p2, p3) with a quadratic curve (p0, q1, p3)
                    let q1 = ((cubic.p1.to_vec2() + cubic.p2.to_vec2()) * 3.0
                        - cubic.p0.to_vec2()
                        - cubic.p3.to_vec2())
                        / 4.0;
                    self.vger
                        .quad_to(self.vger_point(q1.to_point()), self.vger_point(cubic.p3));
                }
            }
        }
        self.vger.fill(paint);
    }

    pub fn draw_glyphs(&mut self, run: GlyphRunRef<'_>, glyphs: &mut dyn Iterator<Item = Glyph>) {
        let font = run.font;
        let text_run = Self::resolve_text_run(run.transform);
        let scale = text_run.raster_scale;

        let clip = self.clip;
        let Some(font_ref) = FontRef::from_index(font.data.data(), font.index as usize) else {
            return;
        };
        let font_blob_id = font.data.id();
        let _color = match &run.brush {
            peniko::Brush::Solid(color) => Color::from(*color),
            _ => return,
        };
        let Some(paint) = self.brush_to_paint(run.brush) else {
            return;
        };
        let skew = run
            .glyph_transform
            .map(|transform| transform.as_coeffs()[0].atan().to_degrees() as f32);
        let embolden = 0.0;

        // Match tiny-skia's split: raster glyphs in run-local space and keep a single
        // normalized transform for the whole run. `vger-rs` cannot consume this yet.
        let _run_transform = text_run.transform;

        for glyph in glyphs {
            let glyph = Self::resolve_glyph(&text_run, glyph);

            if let Some(rect) = clip
                && (glyph.baseline_x as f64) > rect.x1
            {
                break;
            }

            let scaled_font_size = (run.font_size * scale as f32).round() as u32;
            let scaled_size = run.font_size * scale as f32;
            let coords = run.normalized_coords;

            let synthesis_bits = skew.unwrap_or(0.0).to_bits() & 0xFFFF_FFFE;

            self.vger.render_glyph(
                glyph.baseline_x,
                glyph.baseline_y,
                font_blob_id,
                glyph.glyph_id,
                scaled_font_size,
                glyph.subpx,
                synthesis_bits,
                || {
                    let image = SCALE_CONTEXT.with_borrow_mut(|ctx| {
                        let mut scaler = ctx
                            .builder(font_ref)
                            .size(scaled_size)
                            .hint(run.hint)
                            .normalized_coords(coords)
                            .build();
                        let mut render = Render::new(&[
                            Source::ColorOutline(0),
                            Source::ColorBitmap(StrikeWith::BestFit),
                            Source::Outline,
                        ]);
                        render
                            .format(Format::Alpha)
                            .offset(swash::zeno::Vector::new(
                                glyph.subpx.0 as f32 / 4.0,
                                glyph.subpx.1 as f32 / 4.0,
                            ))
                            .embolden(embolden);
                        if let Some(angle) = skew {
                            render.transform(Some(swash::zeno::Transform::skew(
                                swash::zeno::Angle::from_degrees(angle),
                                swash::zeno::Angle::ZERO,
                            )));
                        }
                        render.render(&mut scaler, glyph.glyph_id)
                    });
                    match image {
                        Some(img) => GlyphImage {
                            colored: img.content != swash::scale::image::Content::Mask,
                            data: img.data.into(),
                            width: img.placement.width,
                            height: img.placement.height,
                            left: img.placement.left,
                            top: img.placement.top,
                        },
                        None => GlyphImage {
                            data: Blob::new(Arc::new([])),
                            width: 0,
                            height: 0,
                            left: 0,
                            top: 0,
                            colored: false,
                        },
                    }
                },
                paint,
            );
        }
    }

    pub fn set_transform(&mut self, transform: Affine) {
        self.transform = transform;
    }

    pub fn clip(&mut self, shape: &impl Shape) {
        let (rect, radius) = if let Some(rect) = shape.as_rect() {
            (rect, 0.0)
        } else if let Some(rect) = shape.as_rounded_rect() {
            (rect.rect(), rect.radii().top_left)
        } else {
            (shape.bounding_box(), 0.0)
        };

        let (_, _, scale) = self.scale_components();
        self.vger
            .scissor(self.vger_rect(rect), (radius * scale) as f32);

        let transformed_rect = self.device_transform().transform_rect_bbox(rect);

        self.clip = Some(transformed_rect);
    }

    pub fn clear_clip(&mut self) {
        self.vger.reset_scissor();
        self.clip = None;
    }

    pub fn push_layer(
        &mut self,
        _blend: impl Into<peniko::BlendMode>,
        _alpha: f32,
        _transform: Affine,
        _clip: &impl Shape,
    ) {
    }

    pub fn pop_layer(&mut self) {}

    pub fn debug_info(&mut self) -> String {
        use std::fmt::Write;

        let mut out = String::new();
        writeln!(out, "name: Vger").ok();
        out
    }
}

fn vger_color(color: Color) -> floem_vger_rs::Color {
    floem_vger_rs::Color {
        r: color.components[0],
        g: color.components[1],
        b: color.components[2],
        a: color.components[3],
    }
}
