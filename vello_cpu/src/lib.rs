use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use floem_renderer::text::fontdb::ID;
use floem_renderer::text::{FONT_SYSTEM, LayoutGlyph, LayoutRun};
use floem_renderer::{Img, Renderer};
use peniko::kurbo::Size;
use peniko::{
    Blob, BrushRef, Color,
    color::palette,
    kurbo::{Affine, Point, Rect, Shape, Stroke},
};
use peniko::{ImageAlphaType, ImageBrush, ImageData};
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use vello_cpu::{ImageSource, Mask, PaintType, Pixmap, RenderContext};

thread_local! {
#[allow(clippy::type_complexity)]
static IMAGE_CACHE: RefCell<HashMap<Vec<u8>,  Arc<Pixmap>>> = RefCell::new(HashMap::new());
}

trait BrushRefExt<'a> {
    fn to_vello_cpu_paint(self) -> PaintType;
}

impl<'a> BrushRefExt<'a> for BrushRef<'a> {
    fn to_vello_cpu_paint(self) -> PaintType {
        match self {
            BrushRef::Solid(alpha_color) => PaintType::Solid(alpha_color),
            BrushRef::Gradient(gradient) => PaintType::Gradient(gradient.clone()),
            BrushRef::Image(image) => PaintType::Image(ImageBrush {
                image: ImageSource::from_peniko_image_data(image.image),
                sampler: image.sampler,
            }),
        }
    }
}

pub struct VelloCpuRenderer<W> {
    context: RenderContext,
    surface: Surface<W, W>,
    width: u32,
    height: u32,
    window_scale: f64,
    transform: Affine,
    capture: bool,
    font_cache: HashMap<ID, peniko::FontData>,
}

impl<W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle>
    VelloCpuRenderer<W>
where
    W: Clone,
{
    pub fn new(
        window: W,
        width: u32,
        height: u32,
        scale: f64,
        _font_embolden: f32,
    ) -> Result<Self> {
        let renderer = RenderContext::new(width as u16, height as u16);

        let context = Context::new(window.clone())
            .map_err(|err| anyhow::anyhow!("unable to create context: {}", err))?;
        let mut surface = Surface::new(&context, window)
            .map_err(|err| anyhow::anyhow!("unable to create surface: {}", err))?;
        surface
            .resize(
                NonZeroU32::new(width).unwrap_or(NonZeroU32::new(1).unwrap()),
                NonZeroU32::new(height).unwrap_or(NonZeroU32::new(1).unwrap()),
            )
            .map_err(|_| anyhow::anyhow!("failed to resize surface"))?;

        Ok(Self {
            context: renderer,
            surface,
            width,
            height,
            window_scale: scale,
            transform: Affine::IDENTITY,
            capture: false,
            font_cache: HashMap::new(),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32, scale: f64) {
        self.width = width;
        self.height = height;
        self.window_scale = scale;
        self.context = RenderContext::new(width as u16, height as u16);

        let _ = self.surface.resize(
            NonZeroU32::new(width).unwrap_or(NonZeroU32::new(1).unwrap()),
            NonZeroU32::new(height).unwrap_or(NonZeroU32::new(1).unwrap()),
        );
    }

    pub fn set_scale(&mut self, scale: f64) {
        self.window_scale = scale;
    }

    pub const fn scale(&self) -> f64 {
        self.window_scale
    }

    pub const fn size(&self) -> Size {
        Size::new(self.width as f64, self.height as f64)
    }
}

impl<W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle> Renderer
    for VelloCpuRenderer<W>
where
    W: Clone,
{
    fn begin(&mut self, capture: bool) {
        self.capture = capture;
        self.transform = Affine::IDENTITY;
        // Reset the renderer for a new frame
        self.context = RenderContext::new(self.width as u16, self.height as u16);
    }

    fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s Stroke,
    ) {
        let brush_ref = brush.into();
        self.context
            .set_transform(self.transform.then_scale(self.window_scale));
        self.context.set_paint(brush_ref.to_vello_cpu_paint());
        self.context.set_stroke(stroke.clone());
        // Note: stroke_path implementation would go here
        // For now, we'll implement a basic stroke as fill
        self.context.stroke_path(&shape.into_path(0.1));
    }

    fn fill<'b>(&mut self, path: &impl Shape, brush: impl Into<BrushRef<'b>>, blur_radius: f64) {
        let brush_ref = brush.into();
        self.context
            .set_transform(self.transform.then_scale(self.window_scale));
        self.context.set_paint(brush_ref.to_vello_cpu_paint());
        if blur_radius > 0.
            && let Some(rect) = path.as_rect()
        {
            self.context
                .fill_blurred_rounded_rect(&rect, blur_radius as f32, 1.);
        } else if let Some(rect) = path.as_rect() {
            self.context.fill_rect(&rect);
        } else {
            self.context.fill_path(&path.to_path(0.1));
        }
    }

    fn push_layer(
        &mut self,
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        self.transform *= transform;
        self.context
            .set_transform(self.transform.then_scale(self.window_scale));
        let blend = blend.into();
        self.context
            .push_layer(Some(&clip.to_path(0.1)), Some(blend), Some(alpha), None);
    }

    fn pop_layer(&mut self) {
        self.context.pop_layer();
    }

    fn draw_text_with_layout<'b>(
        &mut self,
        layout: impl Iterator<Item = LayoutRun<'b>>,
        pos: impl Into<Point>,
    ) {
        let pos: Point = pos.into();
        self.context.reset_transform();
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
        // TODO: use an image cache
        let paint = PaintType::Image(ImageBrush {
            image: ImageSource::from_peniko_image_data(&img.img.image),
            sampler: img.img.sampler,
        });
        self.context.set_paint(paint);

        let rect_width = rect.width().max(1.);
        let rect_height = rect.height().max(1.);

        let scale_x = rect_width / img.img.image.width as f64;
        let scale_y = rect_height / img.img.image.height as f64;

        let translate_x = rect.min_x();
        let translate_y = rect.min_y();

        let transform = self.transform.then_scale(self.window_scale);

        self.context.set_paint_transform(
            Affine::IDENTITY
                .pre_scale_non_uniform(scale_x, scale_y)
                .then_translate((translate_x, translate_y).into()),
        );
        self.context.set_transform(transform);

        self.context.fill_rect(&rect);
        self.context.reset_paint_transform();
        self.context.reset_paint_transform();
    }

    fn draw_svg<'b>(
        &mut self,
        svg: floem_renderer::Svg<'b>,
        rect: Rect,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        let width = (rect.width() * self.window_scale).round() as u32;
        let height = (rect.height() * self.window_scale).round() as u32;

        // Check cache first
        let cached_pixmap = IMAGE_CACHE.with_borrow_mut(|ic| ic.get(svg.hash).cloned());

        if let Some(pixmap) = cached_pixmap {
            self.render_svg_with_brush(&pixmap, rect, brush);
            return;
        }

        // Store the hash before moving svg
        let svg_hash = svg.hash.to_owned();

        // Create vello_cpu pixmap directly and render SVG into it
        let vello_pixmap = Arc::new(self.render_svg_to_pixmap(svg, width, height));

        // Render the SVG
        self.render_svg_with_brush(&vello_pixmap, rect, brush);

        // Cache the result
        IMAGE_CACHE.with_borrow_mut(|ic| {
            ic.insert(svg_hash, vello_pixmap);
        });
    }

    fn set_transform(&mut self, transform: Affine) {
        self.transform = transform;
    }

    fn set_z_index(&mut self, _z_index: i32) {}

    fn clip(&mut self, _shape: &impl Shape) {
        // Clipping in vello_cpu would need to be implemented
        // This is a placeholder
    }

    fn clear_clip(&mut self) {
        // Clear clipping in vello_cpu
        // This is a placeholder
    }

    fn finish(&mut self) -> Option<peniko::ImageBrush> {
        if self.capture {
            self.render_capture_image()
        } else {
            // Render to display surface like tiny_skia does
            let mut buffer = self
                .surface
                .buffer_mut()
                .expect("failed to get the surface buffer");

            // Create a pixmap to render into
            let mut pixmap = Pixmap::new(self.width as u16, self.height as u16);

            // Flush the context and render to pixmap
            self.context.flush();
            self.context.render_to_pixmap(&mut pixmap);

            // Copy from vello_cpu::Pixmap to the format specified by softbuffer::Buffer
            for (out_pixel, pixel) in buffer.iter_mut().zip(pixmap.data().iter()) {
                *out_pixel = ((pixel.r as u32) << 16) | ((pixel.g as u32) << 8) | (pixel.b as u32);
            }

            buffer
                .present()
                .expect("failed to present the surface buffer");

            None
        }
    }

    fn debug_info(&self) -> String {
        format!("name: Vello CPU\nsize: {}x{}", self.width, self.height)
    }
}

impl<W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle>
    VelloCpuRenderer<W>
where
    W: Clone,
{
    fn render_svg_to_pixmap(
        &self,
        svg: floem_renderer::Svg<'_>,
        width: u32,
        height: u32,
    ) -> Pixmap {
        // Create a tiny_skia pixmap for resvg to render into
        let mut tiny_skia_pixmap = match resvg::tiny_skia::Pixmap::new(width, height) {
            Some(pixmap) => pixmap,
            None => return Pixmap::new(width as u16, height as u16), // fallback to empty pixmap
        };

        // Calculate transform for scaling SVG to fit the target size
        let svg_transform = resvg::tiny_skia::Transform::from_scale(
            width as f32 / svg.tree.size().width(),
            height as f32 / svg.tree.size().height(),
        );

        // Render SVG using resvg
        resvg::render(svg.tree, svg_transform, &mut tiny_skia_pixmap.as_mut());

        // Take ownership of the pixel data from tiny_skia and convert to vello_cpu format
        let tiny_skia_data = tiny_skia_pixmap.take();
        self.convert_raw_data_to_vello_pixmap(tiny_skia_data, width as u16, height as u16)
    }

    fn convert_raw_data_to_vello_pixmap(
        &self,
        raw_data: Vec<u8>,
        width: u16,
        height: u16,
    ) -> Pixmap {
        // Both tiny_skia and vello_cpu use premultiplied RGBA with 4 bytes per pixel
        // Verify the data size matches expectations
        let expected_len = (width as usize) * (height as usize) * 4;
        if raw_data.len() != expected_len {
            // Fallback to creating empty pixmap if size mismatch
            return Pixmap::new(width, height);
        }

        let mut vello_pixmap = Pixmap::new(width, height);

        // Try to do a direct memory copy using bytemuck if possible
        let dst_slice = vello_pixmap.data_mut();

        // Attempt to cast both slices to byte slices for memcpy-like operation
        if let Ok(dst_bytes) = bytemuck::try_cast_slice_mut::<_, u8>(dst_slice) {
            // Direct memory copy - fastest possible conversion
            dst_bytes.copy_from_slice(&raw_data);
        } else {
            // Fallback: per-pixel conversion
            for (src_chunk, dst_pixel) in raw_data.chunks_exact(4).zip(dst_slice.iter_mut()) {
                dst_pixel.r = src_chunk[0];
                dst_pixel.g = src_chunk[1];
                dst_pixel.b = src_chunk[2];
                dst_pixel.a = src_chunk[3];
            }
        }

        vello_pixmap
    }

    fn render_svg_with_brush<'b>(
        &mut self,
        pixmap: &Arc<Pixmap>,
        rect: Rect,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        self.context
            .set_transform(self.transform.then_scale(self.window_scale));

        if let Some(brush) = brush {
            // Create a temporary context sized to the SVG for recoloring
            let recolored_pixmap = self.recolor_svg_pixmap(pixmap, brush);

            // Draw the recolored pixmap
            let paint = PaintType::Image(ImageBrush {
                image: ImageSource::Pixmap(recolored_pixmap),
                sampler: peniko::ImageSampler::new(),
            });
            self.context.set_paint(paint);

            let rect_width = rect.width().max(1.);
            let rect_height = rect.height().max(1.);
            let scale_x = rect_width / pixmap.width() as f64;
            let scale_y = rect_height / pixmap.height() as f64;
            let translate_x = rect.min_x();
            let translate_y = rect.min_y();

            self.context.set_paint_transform(
                Affine::IDENTITY
                    .pre_scale_non_uniform(scale_x, scale_y)
                    .then_translate((translate_x, translate_y).into()),
            );
            self.context.fill_rect(&rect);
            self.context.reset_paint_transform();
        } else {
            // Render the SVG directly without recoloring
            let paint = PaintType::Image(ImageBrush {
                image: ImageSource::Pixmap(pixmap.clone()),
                sampler: peniko::ImageSampler::new(),
            });
            self.context.set_paint(paint);

            let rect_width = rect.width().max(1.);
            let rect_height = rect.height().max(1.);
            let scale_x = rect_width / pixmap.width() as f64;
            let scale_y = rect_height / pixmap.height() as f64;
            let translate_x = rect.min_x();
            let translate_y = rect.min_y();

            self.context.set_paint_transform(
                Affine::IDENTITY
                    .pre_scale_non_uniform(scale_x, scale_y)
                    .then_translate((translate_x, translate_y).into()),
            );
            self.context.fill_rect(&rect);
            self.context.reset_paint_transform();
        }
    }

    fn recolor_svg_pixmap<'b>(
        &self,
        original_pixmap: &Arc<Pixmap>,
        brush: impl Into<BrushRef<'b>>,
    ) -> Arc<Pixmap> {
        let width = original_pixmap.width();
        let height = original_pixmap.height();

        // Create a temporary context sized to the SVG
        let mut temp_context = RenderContext::new(width, height);

        // Set up the brush paint
        let brush_ref = brush.into();
        temp_context.set_paint(brush_ref.to_vello_cpu_paint());

        // Create mask from the original SVG pixmap
        let mask = Mask::new_alpha(original_pixmap);

        // Apply the mask and fill the entire SVG area with the brush color
        temp_context.push_mask_layer(mask);
        temp_context.fill_rect(&Rect::from_origin_size(
            (0.0, 0.0),
            (width as f64, height as f64),
        ));
        temp_context.pop_layer();

        // Render to a new pixmap
        let mut result_pixmap = Pixmap::new(width, height);
        temp_context.flush();
        temp_context.render_to_pixmap(&mut result_pixmap);

        Arc::new(result_pixmap)
    }

    fn render_capture_image(&mut self) -> Option<peniko::ImageBrush> {
        if !self.capture {
            return None;
        }

        let width = self.width as u16;
        let height = self.height as u16;

        // Create a pixmap to render into
        let mut target = Pixmap::new(width, height);

        // Flush the context and render to pixmap
        self.context.flush();
        self.context.render_to_pixmap(&mut target);

        // Convert pixmap data to the format expected by ImageBrush
        let data = target.data();
        let mut buffer = Vec::with_capacity(data.len() * 4);
        for pixel in data {
            buffer.extend_from_slice(&[pixel.r, pixel.g, pixel.b, pixel.a]);
        }

        Some(peniko::ImageBrush::new(ImageData {
            data: Blob::new(Arc::new(buffer)),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::AlphaPremultiplied,
            width: self.width,
            height: self.height,
        }))
    }

    fn get_font(&mut self, font_id: ID) -> peniko::FontData {
        self.font_cache.get(&font_id).cloned().unwrap_or_else(|| {
            let mut font_system = FONT_SYSTEM.lock();
            let font = font_system.get_font(font_id).unwrap();
            let face = font_system.db().face(font_id).unwrap();
            let font_data = font.data();
            let font_index = face.index;
            drop(font_system);
            let font = peniko::FontData::new(Blob::new(Arc::new(font_data.to_vec())), font_index);
            self.font_cache.insert(font_id, font.clone());
            font
        })
    }

    fn draw_glyph_run(&mut self, run: GlyphRun, transform: Affine) {
        let font = self.get_font(run.font_id);

        // Set the paint color for the glyphs
        self.context.set_paint(run.color);
        self.context.set_transform(transform);

        // Convert glyphs to vello_cpu format
        let glyphs = run.glyphs.iter().map(|glyph| vello_cpu::Glyph {
            id: glyph.glyph_id as u32,
            x: glyph.x,
            y: glyph.y,
        });

        // Render glyphs using vello_cpu's glyph_run method
        self.context
            .glyph_run(&font)
            .font_size(run.font_size)
            .hint(true)
            .fill_glyphs(glyphs);
    }
}

struct GlyphRun<'a> {
    color: Color,
    font_size: f32,
    font_id: ID,
    metadata: usize,
    glyphs: Vec<&'a LayoutGlyph>,
}
