use anyhow::{anyhow, Result};
use floem_renderer::swash::SwashScaler;
use floem_renderer::text::{CacheKey, LayoutRun, SwashContent};
use floem_renderer::tiny_skia::{
    self, FillRule, FilterQuality, GradientStop, LinearGradient, Mask, MaskType, Paint, Path,
    PathBuilder, Pattern, Pixmap, RadialGradient, Shader, SpreadMode, Stroke, Transform,
};
use floem_renderer::Img;
use floem_renderer::Renderer;
use peniko::kurbo::{self, PathEl, Size, Vec2};
use peniko::{
    color::palette,
    kurbo::{Affine, Point, Rect, Shape},
    BrushRef, Color, GradientKind,
};
use softbuffer::{Context, Surface};
use std::cell::RefCell;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;
use tiny_skia::{LineCap, LineJoin};

thread_local! {
    static IMAGE_CACHE: RefCell<HashMap<Vec<u8>, (CacheColor, Rc<Pixmap>)>> = RefCell::new(HashMap::new());
    #[allow(clippy::type_complexity)]
    // The `u32` is a color encoded as a u32 so that it is hashable and eq.
    static GLYPH_CACHE: RefCell<HashMap<(CacheKey, u32), (CacheColor, Option<Rc<Glyph>>)>> = RefCell::new(HashMap::new());
    static SWASH_SCALER: RefCell<SwashScaler> = RefCell::new(SwashScaler::default());
}

fn cache_glyph(cache_color: CacheColor, cache_key: CacheKey, color: Color) -> Option<Rc<Glyph>> {
    let c = color.to_rgba8();

    if let Some(opt_glyph) = GLYPH_CACHE.with_borrow_mut(|gc| {
        if let Some((color, glyph)) = gc.get_mut(&(cache_key, c.to_u32())) {
            *color = cache_color;
            Some(glyph.clone())
        } else {
            None
        }
    }) {
        return opt_glyph;
    };

    let image = SWASH_SCALER.with_borrow_mut(|s| s.get_image(cache_key))?;

    let result = if image.placement.width == 0 || image.placement.height == 0 {
        // We can't create an empty `Pixmap`
        None
    } else {
        let mut pixmap = Pixmap::new(image.placement.width, image.placement.height)?;

        if image.content == SwashContent::Mask {
            for (a, &alpha) in pixmap.pixels_mut().iter_mut().zip(image.data.iter()) {
                *a = tiny_skia::Color::from_rgba8(c.r, c.g, c.b, alpha)
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

    GLYPH_CACHE
        .with_borrow_mut(|gc| gc.insert((cache_key, c.to_u32()), (cache_color, result.clone())));

    result
}

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

struct Layer {
    pixmap: Pixmap,
    mask: Mask,
    clip: Option<Rect>,
    transform: Affine,
    bounds: Rect,
    alpha: f32,
    blend_mode: tiny_skia::BlendMode,
    window_scale: f64,
    cache_color: CacheColor,
}
impl Layer {
    fn current_transform(&self) -> Transform {
        let transform = self.transform.as_coeffs();
        let scale = self.window_scale as f32;
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
}
impl Renderer for Layer {
    fn begin(&mut self, _capture: bool) {
        self.transform = Affine::IDENTITY;
        self.pixmap.fill(tiny_skia::Color::WHITE);
        self.clip = None;
    }

    fn transform(&mut self, transform: Affine) {
        self.transform = transform;
    }

    fn set_z_index(&mut self, _z_index: i32) {
        // Not supported
    }

    fn clip(&mut self, shape: &impl Shape) {
        let rect = if let Some(rect) = shape.as_rect() {
            rect
        } else if let Some(rect) = shape.as_rounded_rect() {
            rect.rect()
        } else {
            shape.bounding_box()
        };

        let p0 = self.transform * Point::new(rect.x0, rect.y0);
        let p1 = self.transform * Point::new(rect.x1, rect.y0);
        let p2 = self.transform * Point::new(rect.x0, rect.y1);
        let p3 = self.transform * Point::new(rect.x1, rect.y1);
        // Find the bounding box of transformed points
        let x0 = p0.x.min(p1.x).min(p2.x).min(p3.x);
        let y0 = p0.y.min(p1.y).min(p2.y).min(p3.y);
        let x1 = p0.x.max(p1.x).max(p2.x).max(p3.x);
        let y1 = p0.y.max(p1.y).max(p2.y).max(p3.y);

        self.clip = Some(Rect::new(x0, y0, x1, y1));

        self.mask.clear();
        let path = try_ret!(shape_to_path(shape));
        self.mask
            .fill_path(&path, FillRule::Winding, false, self.current_transform());
    }

    fn clear_clip(&mut self) {
        self.clip = None;
    }

    fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s peniko::kurbo::Stroke,
    ) {
        let paint = try_ret!(brush_to_paint(brush));
        let path = try_ret!(shape_to_path(shape));
        let line_cap = match stroke.end_cap {
            peniko::kurbo::Cap::Butt => LineCap::Butt,
            peniko::kurbo::Cap::Square => LineCap::Square,
            peniko::kurbo::Cap::Round => LineCap::Round,
        };
        let line_join = match stroke.join {
            peniko::kurbo::Join::Bevel => LineJoin::Bevel,
            peniko::kurbo::Join::Miter => LineJoin::Miter,
            peniko::kurbo::Join::Round => LineJoin::Round,
        };
        // TODO: Finish dash
        let stroke = Stroke {
            width: stroke.width as f32,
            miter_limit: stroke.miter_limit as f32,
            line_cap,
            line_join,
            dash: None,
        };
        self.pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            self.current_transform(),
            self.clip.is_some().then_some(&self.mask),
        );
    }

    fn fill<'b>(&mut self, shape: &impl Shape, brush: impl Into<BrushRef<'b>>, _blur_radius: f64) {
        // FIXME: Handle _blur_radius

        let paint = try_ret!(brush_to_paint(brush));
        if let Some(rect) = shape.as_rect() {
            let rect = try_ret!(to_skia_rect(rect));
            self.pixmap
                .fill_rect(rect, &paint, self.current_transform(), None);
        } else {
            let path = try_ret!(shape_to_path(shape));
            self.pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                self.current_transform(),
                self.clip.is_some().then_some(&self.mask),
            );
        }
    }

    fn push_layer(
        &mut self,
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        todo!()
    }

    fn pop_layer(&mut self) {
        todo!()
    }

    fn draw_text_with_layout<'b>(
        &mut self,
        layout: impl Iterator<Item = LayoutRun<'b>>,
        pos: impl Into<Point>,
    ) {
        let offset = self.transform.translation();
        let pos: Point = pos.into();
        let clip = self.clip;

        let transform = self.transform
            * Affine::translate(Vec2::new(-offset.x, -offset.y))
            * Affine::scale(1.0 / self.window_scale);

        for line in layout {
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

                let glyph_x = x * self.window_scale as f32;
                let glyph_y = (y * self.window_scale as f32).round();
                let font_size = (glyph_run.font_size * self.window_scale as f32).round() as u32;

                let (cache_key, new_x, new_y) = CacheKey::new(
                    glyph_run.font_id,
                    glyph_run.glyph_id,
                    font_size as f32,
                    (glyph_x, glyph_y),
                    glyph_run.cache_key_flags,
                );

                let color = glyph_run.color_opt.map_or(palette::css::BLACK, |c| {
                    Color::from_rgba8(c.r(), c.g(), c.b(), c.a())
                });

                let pixmap = cache_glyph(self.cache_color, cache_key, color);
                if let Some(glyph) = pixmap {
                    render_pixmap_direct(
                        &glyph.pixmap,
                        &mut self.pixmap,
                        new_x as f32 + glyph.left,
                        new_y as f32 - glyph.top,
                        transform,
                        self.clip,
                        self.window_scale,
                    );
                }
            }
        }
    }

    fn draw_svg<'b>(
        &mut self,
        svg: floem_renderer::Svg<'b>,
        rect: Rect,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        let width = (rect.width() * self.window_scale).round() as u32;
        let height = (rect.height() * self.window_scale).round() as u32;

        let rect = try_ret!(to_skia_rect(rect));

        let paint = brush.and_then(|brush| brush_to_paint(brush));

        if IMAGE_CACHE.with_borrow_mut(|ic| {
            if let Some((color, pixmap)) = ic.get_mut(svg.hash) {
                *color = self.cache_color;
                let pixmap = pixmap.clone();
                render_pixmap_paint(
                    &pixmap,
                    &mut self.pixmap,
                    rect,
                    paint.clone(),
                    self.clip,
                    self.window_scale,
                );
                // return
                true
            } else {
                // continue
                false
            }
        }) {
            return;
        };

        let mut pixmap = try_ret!(tiny_skia::Pixmap::new(width, height));
        let svg_transform = tiny_skia::Transform::from_scale(
            width as f32 / svg.tree.size().width(),
            height as f32 / svg.tree.size().height(),
        );
        resvg::render(svg.tree, svg_transform, &mut pixmap.as_mut());

        self.render_pixmap_paint(&pixmap, rect, paint);

        IMAGE_CACHE.with_borrow_mut(|ic| {
            ic.insert(svg.hash.to_owned(), (self.cache_color, Rc::new(pixmap)))
        });
    }

    fn draw_img(&mut self, img: Img<'_>, rect: Rect) {
        todo!()
    }

    fn finish(&mut self) -> Option<peniko::Image> {
        todo!()
    }
}

pub struct TinySkiaRenderer<W> {
    #[allow(unused)]
    context: Context<W>,
    surface: Surface<W, W>,
    pixmap: Pixmap,
    mask: Mask,
    window_scale: f64,
    transform: Affine,
    clip: Option<Rect>,

    /// The cache color value set for cache entries accessed this frame.
    cache_color: CacheColor,
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
            window_scale: scale,
            transform: Affine::IDENTITY,
            clip: None,
            cache_color: CacheColor(false),
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
        self.window_scale = scale;
    }

    pub fn set_scale(&mut self, scale: f64) {
        self.window_scale = scale;
    }

    pub const fn scale(&self) -> f64 {
        self.window_scale
    }

    pub fn size(&self) -> Size {
        Size::new(self.pixmap.width() as f64, self.pixmap.height() as f64)
    }
}

fn to_color(color: Color) -> tiny_skia::Color {
    let c = color.to_rgba8();
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

fn to_point(point: Point) -> tiny_skia::Point {
    tiny_skia::Point::from_xy(point.x as f32, point.y as f32)
}

impl<W> TinySkiaRenderer<W> {
    fn current_transform(&self) -> Transform {
        let transform = self.transform.as_coeffs();
        let scale = self.window_scale as f32;
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
}

impl<W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle> Renderer
    for TinySkiaRenderer<W>
{
    fn begin(&mut self, _capture: bool) {
        self.transform = Affine::IDENTITY;
        self.pixmap.fill(tiny_skia::Color::WHITE);
        self.clip = None;
    }

    fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s peniko::kurbo::Stroke,
    ) {
        let paint = try_ret!(brush_to_paint(brush));
        let path = try_ret!(shape_to_path(shape));
        let line_cap = match stroke.end_cap {
            peniko::kurbo::Cap::Butt => LineCap::Butt,
            peniko::kurbo::Cap::Square => LineCap::Square,
            peniko::kurbo::Cap::Round => LineCap::Round,
        };
        let line_join = match stroke.join {
            peniko::kurbo::Join::Bevel => LineJoin::Bevel,
            peniko::kurbo::Join::Miter => LineJoin::Miter,
            peniko::kurbo::Join::Round => LineJoin::Round,
        };
        // TODO: Finish dash
        let stroke = Stroke {
            width: stroke.width as f32,
            miter_limit: stroke.miter_limit as f32,
            line_cap,
            line_join,
            dash: None,
        };
        self.pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            self.current_transform(),
            self.clip.is_some().then_some(&self.mask),
        );
    }

    fn fill<'b>(&mut self, shape: &impl Shape, brush: impl Into<BrushRef<'b>>, _blur_radius: f64) {
        // FIXME: Handle _blur_radius

        let paint = try_ret!(brush_to_paint(brush));
        if let Some(rect) = shape.as_rect() {
            let rect = try_ret!(to_skia_rect(rect));
            self.pixmap
                .fill_rect(rect, &paint, self.current_transform(), None);
        } else {
            let path = try_ret!(shape_to_path(shape));
            self.pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                self.current_transform(),
                self.clip.is_some().then_some(&self.mask),
            );
        }
    }

    fn draw_text_with_layout<'b>(
        &mut self,
        layout: impl Iterator<Item = LayoutRun<'b>>,
        pos: impl Into<Point>,
    ) {
        let offset = self.transform.translation();
        let pos: Point = pos.into();
        let clip = self.clip;

        let transform = self.transform
            * Affine::translate(Vec2::new(-offset.x, -offset.y))
            * Affine::scale(1.0 / self.window_scale);

        for line in layout {
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

                let glyph_x = x * self.window_scale as f32;
                let glyph_y = (y * self.window_scale as f32).round();
                let font_size = (glyph_run.font_size * self.window_scale as f32).round() as u32;

                let (cache_key, new_x, new_y) = CacheKey::new(
                    glyph_run.font_id,
                    glyph_run.glyph_id,
                    font_size as f32,
                    (glyph_x, glyph_y),
                    glyph_run.cache_key_flags,
                );

                let color = glyph_run.color_opt.map_or(palette::css::BLACK, |c| {
                    Color::from_rgba8(c.r(), c.g(), c.b(), c.a())
                });

                let pixmap = cache_glyph(self.cache_color, cache_key, color);
                if let Some(glyph) = pixmap {
                    self.render_pixmap_direct(
                        &glyph.pixmap,
                        new_x as f32 + glyph.left,
                        new_y as f32 - glyph.top,
                        transform,
                    );
                }
            }
        }
    }

    fn draw_img(&mut self, img: Img<'_>, rect: Rect) {
        let rect = try_ret!(to_skia_rect(rect));
        if IMAGE_CACHE.with_borrow_mut(|ic| {
            if let Some((color, pixmap)) = ic.get_mut(img.hash) {
                *color = self.cache_color;
                let pixmap = pixmap.clone();
                self.render_pixmap_rect(&pixmap, rect);
                // return
                true
            } else {
                // continue
                false
            }
        }) {
            return;
        };

        let image_data = img.img.data.data();
        let mut pixmap = try_ret!(Pixmap::new(img.img.width, img.img.height));
        for (a, b) in pixmap
            .pixels_mut()
            .iter_mut()
            .zip(image_data.chunks_exact(4))
        {
            *a = tiny_skia::Color::from_rgba8(b[0], b[1], b[2], b[3])
                .premultiply()
                .to_color_u8();
        }

        self.render_pixmap_rect(&pixmap, rect);

        IMAGE_CACHE.with_borrow_mut(|ic| {
            ic.insert(img.hash.to_owned(), (self.cache_color, Rc::new(pixmap)))
        });
    }

    fn draw_svg<'b>(
        &mut self,
        svg: floem_renderer::Svg<'b>,
        rect: Rect,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        let width = (rect.width() * self.window_scale).round() as u32;
        let height = (rect.height() * self.window_scale).round() as u32;

        let rect = try_ret!(to_skia_rect(rect));

        let paint = brush.and_then(|brush| brush_to_paint(brush));

        if IMAGE_CACHE.with_borrow_mut(|ic| {
            if let Some((color, pixmap)) = ic.get_mut(svg.hash) {
                *color = self.cache_color;
                let pixmap = pixmap.clone();
                self.render_pixmap_paint(&pixmap, rect, paint.clone());
                // return
                true
            } else {
                // continue
                false
            }
        }) {
            return;
        };

        let mut pixmap = try_ret!(tiny_skia::Pixmap::new(width, height));
        let svg_transform = tiny_skia::Transform::from_scale(
            width as f32 / svg.tree.size().width(),
            height as f32 / svg.tree.size().height(),
        );
        resvg::render(svg.tree, svg_transform, &mut pixmap.as_mut());

        self.render_pixmap_paint(&pixmap, rect, paint);

        IMAGE_CACHE.with_borrow_mut(|ic| {
            ic.insert(svg.hash.to_owned(), (self.cache_color, Rc::new(pixmap)))
        });
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

        let p0 = self.transform * Point::new(rect.x0, rect.y0);
        let p1 = self.transform * Point::new(rect.x1, rect.y0);
        let p2 = self.transform * Point::new(rect.x0, rect.y1);
        let p3 = self.transform * Point::new(rect.x1, rect.y1);
        // Find the bounding box of transformed points
        let x0 = p0.x.min(p1.x).min(p2.x).min(p3.x);
        let y0 = p0.y.min(p1.y).min(p2.y).min(p3.y);
        let x1 = p0.x.max(p1.x).max(p2.x).max(p3.x);
        let y1 = p0.y.max(p1.y).max(p2.y).max(p3.y);

        self.clip = Some(Rect::new(x0, y0, x1, y1));

        self.mask.clear();
        let path = try_ret!(shape_to_path(shape));
        self.mask
            .fill_path(&path, FillRule::Winding, false, self.current_transform());
    }

    fn clear_clip(&mut self) {
        self.clip = None;
    }

    fn finish(&mut self) -> Option<peniko::Image> {
        // Remove cache entries which were not accessed.
        IMAGE_CACHE.with_borrow_mut(|ic| ic.retain(|_, (c, _)| *c == self.cache_color));
        GLYPH_CACHE.with_borrow_mut(|gc| gc.retain(|_, (c, _)| *c == self.cache_color));

        // Swap the cache color.
        self.cache_color = CacheColor(!self.cache_color.0);

        let mut buffer = self
            .surface
            .buffer_mut()
            .expect("failed to get the surface buffer");

        // Copy from `tiny_skia::Pixmap` to the format specified by `softbuffer::Buffer`.
        for (out_pixel, pixel) in (buffer.iter_mut()).zip(self.pixmap.pixels().iter()) {
            *out_pixel = ((pixel.red() as u32) << 16)
                | ((pixel.green() as u32) << 8)
                | (pixel.blue() as u32);
        }

        buffer
            .present()
            .expect("failed to present the surface buffer");

        None
    }

    fn push_layer(
        &mut self,
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        todo!()
    }

    fn pop_layer(&mut self) {
        todo!()
    }
}

/// Converts a peniko BlendMode to a tiny_skia BlendMode
fn convert_blend_mode(blend: impl Into<peniko::BlendMode>) -> tiny_skia::BlendMode {
    let peniko_blend = blend.into();

    match (peniko_blend.mix, peniko_blend.compose) {
        // Map standard Porter-Duff compositing operations
        (peniko::Mix::Normal, peniko::Compose::Clear) => tiny_skia::BlendMode::Clear,
        (peniko::Mix::Normal, peniko::Compose::Copy) => tiny_skia::BlendMode::Source,
        (peniko::Mix::Normal, peniko::Compose::Dest) => tiny_skia::BlendMode::Destination,
        (peniko::Mix::Normal, peniko::Compose::SrcOver) => tiny_skia::BlendMode::SourceOver,
        (peniko::Mix::Clip, peniko::Compose::SrcOver) => tiny_skia::BlendMode::SourceOver,
        (peniko::Mix::Normal, peniko::Compose::DestOver) => tiny_skia::BlendMode::DestinationOver,
        (peniko::Mix::Normal, peniko::Compose::SrcIn) => tiny_skia::BlendMode::SourceIn,
        (peniko::Mix::Normal, peniko::Compose::DestIn) => tiny_skia::BlendMode::DestinationIn,
        (peniko::Mix::Normal, peniko::Compose::SrcOut) => tiny_skia::BlendMode::SourceOut,
        (peniko::Mix::Normal, peniko::Compose::DestOut) => tiny_skia::BlendMode::DestinationOut,
        (peniko::Mix::Normal, peniko::Compose::SrcAtop) => tiny_skia::BlendMode::SourceAtop,
        (peniko::Mix::Normal, peniko::Compose::DestAtop) => tiny_skia::BlendMode::DestinationAtop,
        (peniko::Mix::Normal, peniko::Compose::Xor) => tiny_skia::BlendMode::Xor,
        (peniko::Mix::Normal, peniko::Compose::Plus) => tiny_skia::BlendMode::Plus,

        // Map blend modes with SrcOver composition
        (peniko::Mix::Multiply, peniko::Compose::SrcOver) => tiny_skia::BlendMode::Multiply,
        (peniko::Mix::Screen, peniko::Compose::SrcOver) => tiny_skia::BlendMode::Screen,
        (peniko::Mix::Overlay, peniko::Compose::SrcOver) => tiny_skia::BlendMode::Overlay,
        (peniko::Mix::Darken, peniko::Compose::SrcOver) => tiny_skia::BlendMode::Darken,
        (peniko::Mix::Lighten, peniko::Compose::SrcOver) => tiny_skia::BlendMode::Lighten,
        (peniko::Mix::ColorDodge, peniko::Compose::SrcOver) => tiny_skia::BlendMode::ColorDodge,
        (peniko::Mix::ColorBurn, peniko::Compose::SrcOver) => tiny_skia::BlendMode::ColorBurn,
        (peniko::Mix::HardLight, peniko::Compose::SrcOver) => tiny_skia::BlendMode::HardLight,
        (peniko::Mix::SoftLight, peniko::Compose::SrcOver) => tiny_skia::BlendMode::SoftLight,
        (peniko::Mix::Difference, peniko::Compose::SrcOver) => tiny_skia::BlendMode::Difference,
        (peniko::Mix::Exclusion, peniko::Compose::SrcOver) => tiny_skia::BlendMode::Exclusion,
        (peniko::Mix::Hue, peniko::Compose::SrcOver) => tiny_skia::BlendMode::Hue,
        (peniko::Mix::Saturation, peniko::Compose::SrcOver) => tiny_skia::BlendMode::Saturation,
        (peniko::Mix::Color, peniko::Compose::SrcOver) => tiny_skia::BlendMode::Color,
        (peniko::Mix::Luminosity, peniko::Compose::SrcOver) => tiny_skia::BlendMode::Luminosity,

        // Special cases
        (_, peniko::Compose::PlusLighter) => tiny_skia::BlendMode::Plus,

        // Default to SourceOver for unmatched combinations
        _ => tiny_skia::BlendMode::SourceOver,
    }
}

fn shape_to_path(shape: &impl Shape) -> Option<Path> {
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

fn brush_to_paint<'b>(brush: impl Into<BrushRef<'b>>) -> Option<Paint<'static>> {
    let shader = match brush.into() {
        BrushRef::Solid(c) => Shader::SolidColor(to_color(c)),
        BrushRef::Gradient(g) => {
            let stops = g
                .stops
                .iter()
                .map(|s| GradientStop::new(s.offset, to_color(s.color.to_alpha_color())))
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
fn to_skia_rect(rect: Rect) -> Option<tiny_skia::Rect> {
    tiny_skia::Rect::from_ltrb(
        rect.x0 as f32,
        rect.y0 as f32,
        rect.x1 as f32,
        rect.y1 as f32,
    )
}

fn clip_rect(
    clip: Option<Rect>,
    rect: tiny_skia::Rect,
    window_scale: f64,
) -> Option<tiny_skia::Rect> {
    let clip = if let Some(clip) = clip {
        clip
    } else {
        return Some(rect);
    };
    let clip = to_skia_rect(clip.scale_from_origin(window_scale))?;
    clip.intersect(&rect)
}

/// Renders the pixmap at the position and transforms it with the given transform.
fn render_pixmap_direct(
    from_pixmap: &Pixmap,
    into_pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    transform: kurbo::Affine,
    clip: Option<Rect>,
    window_scale: f64,
) {
    let rect = try_ret!(tiny_skia::Rect::from_xywh(
        x,
        y,
        from_pixmap.width() as f32,
        from_pixmap.height() as f32,
    ));
    let paint = Paint {
        shader: Pattern::new(
            from_pixmap.as_ref(),
            SpreadMode::Pad,
            FilterQuality::Nearest,
            1.0,
            Transform::from_translate(x, y),
        ),
        ..Default::default()
    };

    let transform = transform.as_coeffs();
    let scale = window_scale as f32;
    let transform = Transform::from_row(
        transform[0] as f32,
        transform[1] as f32,
        transform[2] as f32,
        transform[3] as f32,
        transform[4] as f32,
        transform[5] as f32,
    )
    .post_scale(scale, scale);
    if let Some(rect) = clip_rect(clip, rect, window_scale) {
        into_pixmap.fill_rect(rect, &paint, transform, None);
    }
}

fn render_pixmap_paint(
    &mut self,
    from_pixmap: &Pixmap,
    into_pixmap: &mut Pixmap,
    rect: tiny_skia::Rect,
    paint: Option<Paint<'static>>,
) {
    let paint = if let Some(paint) = paint {
        paint
    } else {
        return self.render_pixmap_rect(from_pixmap, rect);
    };

    let mut fill = try_ret!(Pixmap::new(from_pixmap.width(), from_pixmap.height()));
    fill.fill_rect(
        try_ret!(tiny_skia::Rect::from_xywh(
            0.0,
            0.0,
            from_pixmap.width() as f32,
            from_pixmap.height() as f32
        )),
        &paint,
        Transform::identity(),
        None,
    );

    let mask = Mask::from_pixmap(from_pixmap.as_ref(), MaskType::Alpha);
    fill.apply_mask(&mask);

    self.render_pixmap_rect(&fill, rect);
}

fn render_pixmap_rect(from_pixmap: &Pixmap, into_pixmap: &mut Pixmap, rect: tiny_skia::Rect) {
    let paint = Paint {
        shader: Pattern::new(
            from_pixmap.as_ref(),
            SpreadMode::Pad,
            FilterQuality::Bilinear,
            1.0,
            Transform::from_scale(
                rect.width() / from_pixmap.width() as f32,
                rect.height() / from_pixmap.height() as f32,
            ),
        ),
        ..Default::default()
    };

    into_pixmap.fill_rect(
        rect,
        &paint,
        self.current_transform(),
        self.clip.is_some().then_some(&self.mask),
    );
}
