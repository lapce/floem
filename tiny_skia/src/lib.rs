use anyhow::{anyhow, Result};
use floem_renderer::swash::SwashScaler;
use floem_renderer::text::{CacheKey, SwashContent, TextLayout};
use floem_renderer::tiny_skia::{
    self, FillRule, FilterQuality, GradientStop, LinearGradient, Mask, MaskType, Paint, Path,
    PathBuilder, Pattern, Pixmap, RadialGradient, Shader, SpreadMode, Stroke, Transform,
};
use floem_renderer::Img;
use floem_renderer::Renderer;
use image::DynamicImage;
use peniko::kurbo::PathEl;
use peniko::{
    kurbo::{Affine, Point, Rect, Shape},
    BrushRef, Color, GradientKind,
};
use softbuffer::{Context, Surface};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;

macro_rules! try_ret {
    ($e:expr) => {
        if let Some(e) = $e {
            e
        } else {
            return;
        }
    };
}

struct Glyph {
    pixmap: Pixmap,
    left: f32,
    top: f32,
}

#[derive(PartialEq, Clone, Copy)]
struct CacheColor(bool);

pub struct TinySkiaRenderer<W> {
    #[allow(unused)]
    context: Context<W>,
    surface: Surface<W, W>,
    pixmap: Pixmap,
    mask: Mask,
    scale: f64,
    transform: Affine,
    clip: Option<Rect>,

    /// The cache color value set for cache entries accessed this frame.
    cache_color: CacheColor,

    image_cache: HashMap<Vec<u8>, (CacheColor, Rc<Pixmap>)>,
    #[allow(clippy::type_complexity)]
    glyph_cache: HashMap<(CacheKey, Color), (CacheColor, Option<Rc<Glyph>>)>,
    swash_scaler: SwashScaler,
}

impl<W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle>
    TinySkiaRenderer<W>
{
    pub fn new(window: W, width: u32, height: u32, scale: f64, font_embolden: f32) -> Result<Self>
    where
        W: Clone,
    {
        let context = Context::new(window.clone())
            .map_err(|err| anyhow!("unable to create context: {}", err))?;
        let mut surface = Surface::new(&context, window)
            .map_err(|err| anyhow!("unable to create surface: {}", err))?;
        surface
            .resize(
                NonZeroU32::new(width).unwrap_or(NonZeroU32::new(1).unwrap()),
                NonZeroU32::new(height).unwrap_or(NonZeroU32::new(1).unwrap()),
            )
            .map_err(|_| anyhow!("failed to resize surface"))?;

        let pixmap =
            Pixmap::new(width, height).ok_or_else(|| anyhow!("unable to create pixmap"))?;

        let mask = Mask::new(width, height).ok_or_else(|| anyhow!("unable to create mask"))?;

        Ok(Self {
            context,
            surface,
            pixmap,
            mask,
            scale,
            transform: Affine::IDENTITY,
            clip: None,
            cache_color: CacheColor(false),
            image_cache: Default::default(),
            glyph_cache: Default::default(),
            swash_scaler: SwashScaler::new(font_embolden),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32, scale: f64) {
        if width != self.pixmap.width() || height != self.pixmap.width() {
            self.surface
                .resize(
                    NonZeroU32::new(width).unwrap_or(NonZeroU32::new(1).unwrap()),
                    NonZeroU32::new(height).unwrap_or(NonZeroU32::new(1).unwrap()),
                )
                .expect("failed to resize surface");
            self.pixmap = Pixmap::new(width, height).expect("unable to create pixmap");
            self.mask = Mask::new(width, height).expect("unable to create mask");
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

fn to_color(color: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(color.r, color.g, color.b, color.a)
}

fn to_point(point: Point) -> tiny_skia::Point {
    tiny_skia::Point::from_xy(point.x as f32, point.y as f32)
}

impl<W> TinySkiaRenderer<W> {
    fn shape_to_path(&self, shape: &impl Shape) -> Option<Path> {
        let mut builder = PathBuilder::new();
        for element in shape.path_elements(0.1) {
            match element {
                PathEl::ClosePath => builder.close(),
                PathEl::MoveTo(p) => builder.move_to(p.x as f32, p.y as f32),
                PathEl::LineTo(p) => builder.line_to(p.x as f32, p.y as f32),
                PathEl::QuadTo(p1, p2) => {
                    builder.quad_to(p1.x as f32, p1.y as f32, p2.x as f32, p2.y as f32)
                }
                PathEl::CurveTo(p1, p2, p3) => builder.cubic_to(
                    p1.x as f32,
                    p1.y as f32,
                    p2.x as f32,
                    p2.y as f32,
                    p3.x as f32,
                    p3.y as f32,
                ),
            }
        }
        builder.finish()
    }

    fn brush_to_paint<'b>(&self, brush: impl Into<BrushRef<'b>>) -> Option<Paint<'static>> {
        let shader = match brush.into() {
            BrushRef::Solid(c) => Shader::SolidColor(to_color(c)),
            BrushRef::Gradient(g) => {
                let stops = g
                    .stops
                    .iter()
                    .map(|s| GradientStop::new(s.offset, to_color(s.color)))
                    .collect();
                match g.kind {
                    GradientKind::Linear { start, end } => LinearGradient::new(
                        to_point(start),
                        to_point(end),
                        stops,
                        SpreadMode::Pad,
                        Transform::identity(),
                    )?,
                    GradientKind::Radial {
                        start_center,
                        start_radius: _,
                        end_center,
                        end_radius,
                    } => {
                        // FIXME: Doesn't use `start_radius`
                        RadialGradient::new(
                            to_point(start_center),
                            to_point(end_center),
                            end_radius,
                            stops,
                            SpreadMode::Pad,
                            Transform::identity(),
                        )?
                    }
                    GradientKind::Sweep { .. } => return None,
                }
            }
            BrushRef::Image(_) => return None,
        };
        Some(Paint {
            shader,
            ..Default::default()
        })
    }

    /// Transform a `Rect`, applying `self.transform`, into a `tiny_skia::Rect` and
    /// residual transform.
    fn rect(&self, rect: Rect) -> Option<tiny_skia::Rect> {
        tiny_skia::Rect::from_ltrb(
            rect.x0 as f32,
            rect.y0 as f32,
            rect.x1 as f32,
            rect.y1 as f32,
        )
    }

    fn clip_rect(&self, rect: tiny_skia::Rect) -> Option<tiny_skia::Rect> {
        let clip = if let Some(clip) = self.clip {
            clip
        } else {
            return Some(rect);
        };
        let clip = self.rect(clip.scale_from_origin(self.scale))?;
        clip.intersect(&rect)
    }

    /// Renders the pixmap at the position without transforming it.
    fn render_pixmap_direct(&mut self, pixmap: &Pixmap, x: f32, y: f32) {
        let rect = try_ret!(tiny_skia::Rect::from_xywh(
            x,
            y,
            pixmap.width() as f32,
            pixmap.height() as f32,
        ));
        let paint = Paint {
            shader: Pattern::new(
                pixmap.as_ref(),
                SpreadMode::Pad,
                FilterQuality::Nearest,
                1.0,
                Transform::from_translate(x, y),
            ),
            ..Default::default()
        };

        if let Some(rect) = self.clip_rect(rect) {
            self.pixmap
                .fill_rect(rect, &paint, Transform::identity(), None);
        }
    }

    fn render_pixmap_rect(&mut self, pixmap: &Pixmap, rect: tiny_skia::Rect) {
        let paint = Paint {
            shader: Pattern::new(
                pixmap.as_ref(),
                SpreadMode::Pad,
                FilterQuality::Bilinear,
                1.0,
                Transform::from_scale(
                    rect.width() / pixmap.width() as f32,
                    rect.height() / pixmap.height() as f32,
                ),
            ),
            ..Default::default()
        };

        self.pixmap.fill_rect(
            rect,
            &paint,
            self.current_transform(),
            self.clip.is_some().then_some(&self.mask),
        );
    }

    fn render_pixmap_paint(
        &mut self,
        pixmap: &Pixmap,
        rect: tiny_skia::Rect,
        paint: Option<Paint<'static>>,
    ) {
        let paint = if let Some(paint) = paint {
            paint
        } else {
            return self.render_pixmap_rect(pixmap, rect);
        };

        let mut fill = try_ret!(Pixmap::new(pixmap.width(), pixmap.height()));
        fill.fill_rect(
            try_ret!(tiny_skia::Rect::from_xywh(
                0.0,
                0.0,
                pixmap.width() as f32,
                pixmap.height() as f32
            )),
            &paint,
            Transform::identity(),
            None,
        );

        let mask = Mask::from_pixmap(pixmap.as_ref(), MaskType::Alpha);
        fill.apply_mask(&mask);

        self.render_pixmap_rect(&fill, rect);
    }

    fn current_transform(&self) -> Transform {
        let transform = self.transform.as_coeffs();
        let scale = self.scale as f32;
        Transform::from_row(
            transform[0] as f32,
            transform[1] as f32,
            transform[2] as f32,
            transform[3] as f32,
            transform[4] as f32,
            transform[5] as f32,
        )
        .post_scale(scale, scale)
    }

    fn cache_glyph(&mut self, cache_key: CacheKey, color: Color) -> Option<Rc<Glyph>> {
        if let Some((color, glyph)) = self.glyph_cache.get_mut(&(cache_key, color)) {
            *color = self.cache_color;
            return glyph.clone();
        }

        let image = self.swash_scaler.get_image(cache_key)?;

        let result = if image.placement.width == 0 || image.placement.height == 0 {
            // We can't create an empty `Pixmap`
            None
        } else {
            let mut pixmap = Pixmap::new(image.placement.width, image.placement.height)?;

            if image.content == SwashContent::Mask {
                for (a, &alpha) in pixmap.pixels_mut().iter_mut().zip(image.data.iter()) {
                    *a = tiny_skia::Color::from_rgba8(color.r, color.g, color.b, alpha)
                        .premultiply()
                        .to_color_u8();
                }
            } else if image.content == SwashContent::Color {
                for (a, b) in pixmap.pixels_mut().iter_mut().zip(image.data.chunks(4)) {
                    *a = tiny_skia::Color::from_rgba8(b[0], b[1], b[2], b[3])
                        .premultiply()
                        .to_color_u8();
                }
            } else {
                return None;
            }

            Some(Rc::new(Glyph {
                pixmap,
                left: image.placement.left as f32,
                top: image.placement.top as f32,
            }))
        };

        self.glyph_cache
            .insert((cache_key, color), (self.cache_color, result.clone()));

        result
    }
}

impl<W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle> Renderer
    for TinySkiaRenderer<W>
{
    fn begin(&mut self, _capture: bool) {
        self.transform = Affine::IDENTITY;
        self.pixmap.fill(tiny_skia::Color::WHITE);
        self.clip = None;
    }

    fn stroke<'b>(&mut self, shape: &impl Shape, brush: impl Into<BrushRef<'b>>, width: f64) {
        let paint = try_ret!(self.brush_to_paint(brush));
        let path = try_ret!(self.shape_to_path(shape));
        self.pixmap.stroke_path(
            &path,
            &paint,
            &Stroke {
                width: width as f32,
                ..Default::default()
            },
            self.current_transform(),
            self.clip.is_some().then_some(&self.mask),
        );
    }

    fn fill<'b>(&mut self, shape: &impl Shape, brush: impl Into<BrushRef<'b>>, _blur_radius: f64) {
        // FIXME: Handle _blur_radius

        let paint = try_ret!(self.brush_to_paint(brush));
        if let Some(rect) = shape.as_rect() {
            let rect = try_ret!(self.rect(rect));
            self.pixmap
                .fill_rect(rect, &paint, self.current_transform(), None);
        } else {
            let path = try_ret!(self.shape_to_path(shape));
            self.pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                self.current_transform(),
                self.clip.is_some().then_some(&self.mask),
            );
        }
    }

    fn draw_text(&mut self, layout: &TextLayout, pos: impl Into<Point>) {
        let offset = self.transform.translation();
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

                let glyph_x = x * self.scale as f32;
                let glyph_y = (y * self.scale as f32).round();
                let font_size = (glyph_run.font_size * self.scale as f32).round() as u32;
                let (cache_key, new_x, new_y) = CacheKey::new(
                    glyph_run.font_id,
                    glyph_run.glyph_id,
                    font_size as f32,
                    (glyph_x, glyph_y),
                    glyph_run.cache_key_flags,
                );

                let glyph_x = new_x as f32;
                let glyph_y = new_y as f32;

                let color = match glyph_run.color_opt {
                    Some(c) => Color::rgba8(c.r(), c.g(), c.b(), c.a()),
                    None => Color::BLACK,
                };
                let pixmap = self.cache_glyph(cache_key, color);

                if let Some(glyph) = pixmap {
                    self.render_pixmap_direct(
                        &glyph.pixmap,
                        glyph_x + glyph.left,
                        glyph_y - glyph.top,
                    );
                }
            }
        }
    }

    fn draw_img(&mut self, img: Img<'_>, rect: Rect) {
        let rect = try_ret!(self.rect(rect));
        if let Some((color, pixmap)) = self.image_cache.get_mut(img.hash) {
            *color = self.cache_color;
            let pixmap = pixmap.clone();
            self.render_pixmap_rect(&pixmap, rect);
            return;
        }

        let rgba_image = img.img.clone().into_rgba8();
        let mut pixmap = try_ret!(Pixmap::new(rgba_image.width(), rgba_image.height()));
        for (a, &b) in pixmap.pixels_mut().iter_mut().zip(rgba_image.pixels()) {
            *a = tiny_skia::Color::from_rgba8(b.0[0], b.0[1], b.0[2], b.0[3])
                .premultiply()
                .to_color_u8();
        }

        self.render_pixmap_rect(&pixmap, rect);

        self.image_cache
            .insert(img.hash.to_owned(), (self.cache_color, Rc::new(pixmap)));
    }

    fn draw_svg<'b>(
        &mut self,
        svg: floem_renderer::Svg<'b>,
        rect: Rect,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        let width = (rect.width() * self.scale).round() as u32;
        let height = (rect.height() * self.scale).round() as u32;

        let rect = try_ret!(self.rect(rect));

        let paint = brush.and_then(|brush| self.brush_to_paint(brush));

        if let Some((color, pixmap)) = self.image_cache.get_mut(svg.hash) {
            *color = self.cache_color;
            let pixmap = pixmap.clone();
            self.render_pixmap_paint(&pixmap, rect, paint);
            return;
        }

        let mut pixmap = try_ret!(tiny_skia::Pixmap::new(width, height));
        // let rtree = resvg::Tree::from_usvg(svg.tree);
        let svg_transform = tiny_skia::Transform::from_scale(
            width as f32 / svg.tree.size().width(),
            height as f32 / svg.tree.size().height(),
        );
        resvg::render(svg.tree, svg_transform, &mut pixmap.as_mut());

        self.render_pixmap_paint(&pixmap, rect, paint);

        self.image_cache
            .insert(svg.hash.to_owned(), (self.cache_color, Rc::new(pixmap)));
    }

    fn transform(&mut self, transform: Affine) {
        self.transform = transform;
    }

    fn set_z_index(&mut self, _z_index: i32) {
        // FIXME: Remove this method?
    }

    fn clip(&mut self, shape: &impl Shape) {
        let rect = if let Some(rect) = shape.as_rect() {
            rect
        } else if let Some(rect) = shape.as_rounded_rect() {
            rect.rect()
        } else {
            shape.bounding_box()
        };

        let offset = self.transform.translation();
        self.clip = Some(rect + offset);

        self.mask.clear();
        let path = try_ret!(self.shape_to_path(shape));
        self.mask
            .fill_path(&path, FillRule::Winding, false, self.current_transform());
    }

    fn clear_clip(&mut self) {
        self.clip = None;
    }

    fn finish(&mut self) -> Option<DynamicImage> {
        // Remove cache entries which were not accessed.
        self.image_cache.retain(|_, (c, _)| *c == self.cache_color);
        self.glyph_cache.retain(|_, (c, _)| *c == self.cache_color);

        // Swap the cache color.
        self.cache_color = CacheColor(!self.cache_color.0);

        let mut buffer = self
            .surface
            .buffer_mut()
            .expect("failed to get the surface buffer");

        // Copy from `tiny_skia::Pixmap` to the format specified by `softbuffer::Buffer`.
        for (out_pixel, pixel) in (buffer.iter_mut()).zip(self.pixmap.pixels().iter()) {
            *out_pixel =
                (pixel.red() as u32) << 16 | (pixel.green() as u32) << 8 | (pixel.blue() as u32);
        }

        buffer
            .present()
            .expect("failed to present the surface buffer");

        None
    }
}
