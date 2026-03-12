use anyhow::{Result, anyhow};
use floem_renderer::Img;
use floem_renderer::Renderer;
use floem_renderer::text::{Glyph as ParleyGlyph, GlyphRunProps};
use floem_renderer::tiny_skia::{
    self, FillRule, FilterQuality, GradientStop, IntRect, LinearGradient, Mask, MaskType, Paint,
    Path, PathBuilder, Pixmap, PixmapPaint, RadialGradient, Shader, SpreadMode, Stroke,
    Transform,
};
use peniko::kurbo::{PathEl, Size};
use peniko::{BlendMode, Blob, Compose, ImageAlphaType, ImageData, Mix, RadialGradientPosition};
use peniko::{
    BrushRef, Color, GradientKind,
    kurbo::{Affine, Point, Rect, Shape},
};
use resvg::tiny_skia::StrokeDash;
use softbuffer::{Context, Surface};
use std::cell::RefCell;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;
use swash::FontRef;
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::Format;
use tiny_skia::{LineCap, LineJoin};

/// Cache key for rasterized glyphs, replacing cosmic-text's CacheKey.
/// Uses Parley's font blob identity + swash-compatible glyph parameters.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphCacheKey {
    font_blob_id: u64,
    font_index: u32,
    glyph_id: u16,
    font_size_bits: u32,
    x_bin: u8,
    y_bin: u8,
    embolden: bool,
    skew_bits: u32,
}

impl GlyphCacheKey {
    #[allow(clippy::too_many_arguments)]
    fn new(
        font_blob_id: u64,
        font_index: u32,
        glyph_id: u16,
        font_size: f32,
        x: f32,
        y: f32,
        embolden: bool,
        skew: Option<f32>,
    ) -> (Self, f32, f32) {
        let font_size_bits = font_size.to_bits();
        let x_floor = x.floor();
        let y_floor = y.floor();
        let x_fract = x - x_floor;
        let y_fract = y - y_floor;
        // 4 subpixel bins per axis (matching old SubpixelBin behavior)
        let x_bin = (x_fract * 4.0).min(3.0) as u8;
        let y_bin = (y_fract * 4.0).min(3.0) as u8;
        let skew_bits = skew.unwrap_or(0.0).to_bits();

        (
            Self {
                font_blob_id,
                font_index,
                glyph_id,
                font_size_bits,
                x_bin,
                y_bin,
                embolden,
                skew_bits,
            },
            x_floor + (x_bin as f32) / 4.0,
            y_floor + (y_bin as f32) / 4.0,
        )
    }
}

thread_local! {
    #[allow(clippy::type_complexity)]
    static IMAGE_CACHE: RefCell<HashMap<Vec<u8>, (CacheColor, Rc<Pixmap>)>> = RefCell::new(HashMap::new());
    #[allow(clippy::type_complexity)]
    // The `u32` is a color encoded as a u32 so that it is hashable and eq.
    static GLYPH_CACHE: RefCell<HashMap<(GlyphCacheKey, u32), (CacheColor, Option<Rc<Glyph>>)>> = RefCell::new(HashMap::new());
    static SCALE_CONTEXT: RefCell<ScaleContext> = RefCell::new(ScaleContext::new());
}

#[allow(clippy::too_many_arguments)]
fn cache_glyph(
    cache_color: CacheColor,
    cache_key: GlyphCacheKey,
    color: Color,
    font_ref: &FontRef<'_>,
    font_size: f32,
    normalized_coords: &[i16],
    embolden_strength: f32,
    skew: Option<f32>,
    offset_x: f32,
    offset_y: f32,
) -> Option<Rc<Glyph>> {
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

    let image = SCALE_CONTEXT.with_borrow_mut(|context| {
        let mut scaler = context
            .builder(*font_ref)
            .size(font_size)
            .hint(true)
            .normalized_coords(normalized_coords)
            .build();

        let mut render = Render::new(&[
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ]);
        render
            .format(Format::Alpha)
            .offset(swash::zeno::Vector::new(offset_x.fract(), offset_y.fract()))
            .embolden(embolden_strength);
        if let Some(angle) = skew {
            render.transform(Some(swash::zeno::Transform::skew(
                swash::zeno::Angle::from_degrees(angle),
                swash::zeno::Angle::ZERO,
            )));
        }
        render.render(&mut scaler, cache_key.glyph_id)
    })?;

    let result = if image.placement.width == 0 || image.placement.height == 0 {
        // We can't create an empty `Pixmap`
        None
    } else {
        let mut pixmap = Pixmap::new(image.placement.width, image.placement.height)?;

        match image.content {
            Content::Mask => {
                for (a, &alpha) in pixmap.pixels_mut().iter_mut().zip(image.data.iter()) {
                    *a = tiny_skia::Color::from_rgba8(c.r, c.g, c.b, alpha)
                        .premultiply()
                        .to_color_u8();
                }
            }
            Content::Color => {
                for (a, b) in pixmap.pixels_mut().iter_mut().zip(image.data.chunks(4)) {
                    *a = tiny_skia::Color::from_rgba8(b[0], b[1], b[2], b[3])
                        .premultiply()
                        .to_color_u8();
                }
            }
            _ => return None,
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

#[derive(Clone)]
struct ClipPath {
    path: Path,
    rect: Rect,
    simple_rect: Option<Rect>,
}

#[derive(PartialEq, Clone, Copy)]
struct CacheColor(bool);

struct Layer {
    pixmap: Pixmap,
    base_clip: Option<ClipPath>,
    /// clip is stored with the transform at the time clip is called
    clip: Option<Rect>,
    simple_clip: Option<Rect>,
    draw_bounds: Option<Rect>,
    mask: Mask,
    /// this transform should generally only be used when making a draw call to skia
    transform: Affine,
    blend_mode: BlendMode,
    alpha: f32,
    cache_color: CacheColor,
}
impl Layer {
    fn clip_rect_to_mask_bounds(&self, rect: Rect) -> Option<(usize, usize, usize, usize)> {
        let rect = rect_to_int_rect(rect)?;
        let x0 = rect.x().max(0) as usize;
        let y0 = rect.y().max(0) as usize;
        let x1 = (rect.x() + rect.width() as i32).min(self.mask.width() as i32) as usize;
        let y1 = (rect.y() + rect.height() as i32).min(self.mask.height() as i32) as usize;
        (x0 < x1 && y0 < y1).then_some((x0, y0, x1, y1))
    }

    fn fill_mask_rect(&mut self, rect: Rect) {
        self.mask.clear();
        let Some((x0, y0, x1, y1)) = self.clip_rect_to_mask_bounds(rect) else {
            return;
        };

        let width = self.mask.width() as usize;
        let data = self.mask.data_mut();
        for y in y0..y1 {
            let row = y * width;
            data[row + x0..row + x1].fill(255);
        }
    }

    fn intersect_mask_rect(&mut self, rect: Rect) {
        let Some((x0, y0, x1, y1)) = self.clip_rect_to_mask_bounds(rect) else {
            self.mask.clear();
            return;
        };

        let width = self.mask.width() as usize;
        let height = self.mask.height() as usize;
        let data = self.mask.data_mut();
        for y in 0..height {
            let row = y * width;
            if y < y0 || y >= y1 {
                data[row..row + width].fill(0);
                continue;
            }

            data[row..row + x0].fill(0);
            data[row + x1..row + width].fill(0);
        }
    }

    fn intersect_clip_path(&mut self, clip: &ClipPath) {
        let prior_simple_clip = self
            .simple_clip
            .or(self.base_clip.as_ref().and_then(|clip| clip.simple_rect));
        let clip_rect = self.clip.map(|rect| rect.intersect(clip.rect)).unwrap_or(clip.rect);
        if clip_rect.is_zero_area() {
            self.clip = None;
            self.simple_clip = None;
            self.mask.clear();
            return;
        }

        if self.clip.is_some() {
            if clip.simple_rect.is_some() {
                self.intersect_mask_rect(clip.rect);
            } else {
                self.mask
                    .intersect_path(&clip.path, FillRule::Winding, false, Transform::identity());
            }
        } else {
            if clip.simple_rect.is_some() {
                self.fill_mask_rect(clip.rect);
            } else {
                self.mask.clear();
                self.mask
                    .fill_path(&clip.path, FillRule::Winding, false, Transform::identity());
            }
        }

        self.clip = Some(clip_rect);
        self.simple_clip = match (prior_simple_clip, clip.simple_rect) {
            (Some(current), Some(next)) => {
                let clipped = current.intersect(next);
                (!clipped.is_zero_area()).then_some(clipped)
            }
            (None, Some(next)) if self.base_clip.is_none() && self.clip == Some(clip_rect) => Some(next),
            _ => None,
        };
    }

    fn rebuild_clip_mask(&mut self, clip_stack: &[ClipPath]) {
        self.mask.clear();

        let clips: Vec<ClipPath> = self
            .base_clip
            .iter()
            .cloned()
            .chain(clip_stack.iter().cloned())
            .collect();
        let Some(first) = clips.first() else {
            self.clip = None;
            self.simple_clip = None;
            return;
        };

        let mut clip_rect = first.rect;
        let mut simple_clip = first.simple_rect;
        if first.simple_rect.is_some() {
            self.fill_mask_rect(first.rect);
        } else {
            self.mask
                .fill_path(&first.path, FillRule::Winding, false, Transform::identity());
        }

        for clip in clips.iter().skip(1) {
            clip_rect = clip_rect.intersect(clip.rect);
            simple_clip = match (simple_clip, clip.simple_rect) {
                (Some(current), Some(next)) => {
                    let clipped = current.intersect(next);
                    (!clipped.is_zero_area()).then_some(clipped)
                }
                _ => None,
            };
            if clip.simple_rect.is_some() {
                self.intersect_mask_rect(clip.rect);
            } else {
                self.mask
                    .intersect_path(&clip.path, FillRule::Winding, false, Transform::identity());
            }
        }

        self.clip = (!clip_rect.is_zero_area()).then_some(clip_rect);
        self.simple_clip = self.clip.and(simple_clip);
        if self.clip.is_none() {
            self.mask.clear();
        }
    }

    #[cfg(test)]
    fn set_base_clip(&mut self, clip: Option<ClipPath>) {
        self.base_clip = clip;
        self.rebuild_clip_mask(&[]);
    }

    fn mark_drawn_device_rect(&mut self, rect: Rect) {
        let mut device_rect = rect;
        if let Some(clip) = self.clip {
            device_rect = device_rect.intersect(clip);
        }

        if device_rect.is_zero_area() {
            return;
        }

        self.draw_bounds = Some(
            self.draw_bounds
                .map(|bounds| bounds.union(device_rect))
                .unwrap_or(device_rect),
        );
    }

    fn try_fill_solid_rect_fast(&mut self, rect: Rect, color: Color) -> bool {
        if self.clip.is_some() && self.simple_clip.is_none() {
            return false;
        }

        let coeffs = self.device_transform().as_coeffs();
        if coeffs[0] != 1.0 || coeffs[1] != 0.0 || coeffs[2] != 0.0 || coeffs[3] != 1.0 {
            return false;
        }

        let c = color.to_rgba8();
        if c.a != 255 {
            return false;
        }

        let Some(device_rect) = rect_to_int_rect(self.device_transform().transform_rect_bbox(rect)) else {
            return false;
        };

        let mut device_rect = Rect::new(
            device_rect.x() as f64,
            device_rect.y() as f64,
            (device_rect.x() + device_rect.width() as i32) as f64,
            (device_rect.y() + device_rect.height() as i32) as f64,
        );
        if let Some(simple_clip) = self.simple_clip {
            device_rect = device_rect.intersect(simple_clip);
            if device_rect.is_zero_area() {
                return true;
            }
        }

        let x0 = device_rect.x0.max(0.0) as u32;
        let y0 = device_rect.y0.max(0.0) as u32;
        let x1 = device_rect.x1.min(self.pixmap.width() as f64) as u32;
        let y1 = device_rect.y1.min(self.pixmap.height() as f64) as u32;

        if x0 >= x1 || y0 >= y1 {
            return true;
        }

        self.mark_drawn_device_rect(Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64));

        let fill = tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)
            .premultiply()
            .to_color_u8();
        let width = self.pixmap.width() as usize;
        let pixels = self.pixmap.pixels_mut();
        for y in y0 as usize..y1 as usize {
            let start = y * width + x0 as usize;
            let end = y * width + x1 as usize;
            pixels[start..end].fill(fill);
        }

        true
    }

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

    fn normalized_linear_transform(&self, include_translation: bool) -> Affine {
        let device = self.device_transform().as_coeffs();
        let (scale_x, scale_y, _) = self.scale_components();
        let tx = if include_translation { device[4] } else { 0.0 };
        let ty = if include_translation { device[5] } else { 0.0 };
        Affine::new([
            if scale_x != 0.0 {
                device[0] / scale_x
            } else {
                0.0
            },
            if scale_x != 0.0 {
                device[1] / scale_x
            } else {
                0.0
            },
            if scale_y != 0.0 {
                device[2] / scale_y
            } else {
                0.0
            },
            if scale_y != 0.0 {
                device[3] / scale_y
            } else {
                0.0
            },
            tx,
            ty,
        ])
    }

    fn intersects_clip(&self, img_rect: Rect, transform: Affine) -> bool {
        let device_rect = transform.transform_rect_bbox(img_rect);
        self.clip
            .map(|clip| to_skia_rect(clip.intersect(device_rect)).is_some())
            .unwrap_or(true)
    }

    fn mark_drawn_rect_inflated(&mut self, rect: Rect, transform: Affine, pad: f64) {
        self.mark_drawn_device_rect(transform.transform_rect_bbox(rect).inset(-pad));
    }

    fn mark_stroke_bounds(&mut self, shape: &impl Shape, stroke: &peniko::kurbo::Stroke) {
        if let Some(clip) = self.clip {
            self.mark_drawn_device_rect(clip);
            return;
        }

        let stroke_pad = stroke.width as f64 + stroke.miter_limit.max(1.0) + 4.0;
        self.mark_drawn_rect_inflated(
            shape.bounding_box().inset(-stroke_pad),
            self.device_transform(),
            4.0,
        );
    }

    fn try_draw_pixmap_translate_only(
        &mut self,
        pixmap: &Pixmap,
        x: f64,
        y: f64,
        transform: Affine,
        quality: FilterQuality,
    ) -> bool {
        let coeffs = transform.as_coeffs();
        if coeffs[0] != 1.0 || coeffs[1] != 0.0 || coeffs[2] != 0.0 || coeffs[3] != 1.0 {
            return false;
        }

        let Some(draw_x) = nearly_integral(x + coeffs[4]) else {
            return false;
        };
        let Some(draw_y) = nearly_integral(y + coeffs[5]) else {
            return false;
        };

        let rect = Rect::from_origin_size(
            (draw_x as f64, draw_y as f64),
            (pixmap.width() as f64, pixmap.height() as f64),
        );
        if !self.intersects_clip(rect, Affine::IDENTITY) {
            return true;
        }

        self.mark_drawn_rect_inflated(rect, Affine::IDENTITY, 2.0);
        let paint = PixmapPaint {
            opacity: 1.0,
            blend_mode: tiny_skia::BlendMode::SourceOver,
            quality,
        };
        self.pixmap.draw_pixmap(
            draw_x,
            draw_y,
            pixmap.as_ref(),
            &paint,
            Transform::identity(),
            self.clip.is_some().then_some(&self.mask),
        );
        true
    }

    fn try_fill_rect_with_paint_fast(&mut self, rect: Rect, paint: &Paint<'static>) -> bool {
        if !is_axis_aligned(self.device_transform()) {
            return false;
        }

        let Some(device_rect) = to_skia_rect(self.device_transform().transform_rect_bbox(rect))
        else {
            return false;
        };

        let mut paint = paint.clone();
        paint.shader.transform(self.skia_transform());
        self.pixmap.fill_rect(
            device_rect,
            &paint,
            Transform::identity(),
            self.clip.is_some().then_some(&self.mask),
        );
        true
    }

    /// Renders the pixmap at the position and transforms it with the given transform.
    /// x and y should have already been scaled by the window scale
    fn render_pixmap_direct(&mut self, img_pixmap: &Pixmap, x: f32, y: f32, transform: Affine) {
        if self.try_draw_pixmap_translate_only(
            img_pixmap,
            x as f64,
            y as f64,
            transform,
            FilterQuality::Nearest,
        ) {
            return;
        }

        let img_rect = Rect::from_origin_size(
            (x, y),
            (img_pixmap.width() as f64, img_pixmap.height() as f64),
        );
        if !self.intersects_clip(img_rect, transform) {
            return;
        }
        self.mark_drawn_rect_inflated(img_rect, transform, 2.0);
        let paint = PixmapPaint {
            opacity: 1.0,
            blend_mode: tiny_skia::BlendMode::SourceOver,
            quality: FilterQuality::Nearest,
        };
        let transform = affine_to_skia(transform * Affine::translate((x as f64, y as f64)));
        self.pixmap.draw_pixmap(
            0,
            0,
            img_pixmap.as_ref(),
            &paint,
            transform,
            self.clip.is_some().then_some(&self.mask),
        );
    }

    fn render_pixmap_rect(&mut self, pixmap: &Pixmap, rect: Rect, transform: Affine) {
        if rect.width() == pixmap.width() as f64
            && rect.height() == pixmap.height() as f64
            && self.try_draw_pixmap_translate_only(
                pixmap,
                rect.x0,
                rect.y0,
                transform,
                FilterQuality::Nearest,
            )
        {
            return;
        }

        if !self.intersects_clip(rect, transform) {
            return;
        }
        self.mark_drawn_rect_inflated(rect, transform, 2.0);
        let paint = PixmapPaint {
            opacity: 1.0,
            blend_mode: tiny_skia::BlendMode::SourceOver,
            quality: FilterQuality::Bilinear,
        };
        let local_transform = Affine::translate((rect.x0, rect.y0)).then_scale_non_uniform(
            rect.width() / pixmap.width() as f64,
            rect.height() / pixmap.height() as f64,
        );

        self.pixmap.draw_pixmap(
            0,
            0,
            pixmap.as_ref(),
            &paint,
            affine_to_skia(transform * local_transform),
            self.clip.is_some().then_some(&self.mask),
        );
    }

    fn render_pixmap_with_paint(
        &mut self,
        pixmap: &Pixmap,
        rect: Rect,
        transform: Affine,
        paint: Option<Paint<'static>>,
    ) {
        let paint = if let Some(paint) = paint {
            paint
        } else {
            return self.render_pixmap_rect(pixmap, rect, transform);
        };

        let mut colored_bg = try_ret!(Pixmap::new(pixmap.width(), pixmap.height()));
        colored_bg.fill_rect(
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
        colored_bg.apply_mask(&mask);

        self.render_pixmap_rect(&colored_bg, rect, transform);
    }

    fn skia_transform(&self) -> Transform {
        skia_transform(self.device_transform())
    }
}
impl Layer {
    fn new(
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
        width: u32,
        height: u32,
        cache_color: CacheColor,
    ) -> Result<Self, anyhow::Error> {
        let mut mask = Mask::new(width, height).ok_or_else(|| anyhow!("unable to create mask"))?;
        let base_path = shape_to_path(clip)
            .ok_or_else(|| anyhow!("unable to create clip shape"))?
            .transform(affine_to_skia(transform))
            .ok_or_else(|| anyhow!("unable to transform clip shape"))?;
        mask.fill_path(&base_path, FillRule::Winding, false, Transform::identity());
        Ok(Self {
            pixmap: Pixmap::new(width, height).ok_or_else(|| anyhow!("unable to create pixmap"))?,
            base_clip: Some(ClipPath {
                path: base_path,
                rect: transform.transform_rect_bbox(clip.bounding_box()),
                simple_rect: transformed_axis_aligned_rect(clip, transform),
            }),
            mask,
            clip: Some(transform.transform_rect_bbox(clip.bounding_box())),
            simple_clip: transformed_axis_aligned_rect(clip, transform),
            draw_bounds: None,
            transform,
            blend_mode: blend.into(),
            alpha,
            cache_color,
        })
    }

    fn transform(&mut self, transform: Affine) {
        self.transform *= transform;
    }

    #[cfg(test)]
    fn clip(&mut self, shape: &impl Shape) {
        let path = try_ret!(shape_to_path(shape).and_then(|path| path.transform(self.skia_transform())));
        self.set_base_clip(Some(ClipPath {
            path,
            rect: self
                .device_transform()
                .transform_rect_bbox(shape.bounding_box()),
            simple_rect: transformed_axis_aligned_rect(shape, self.device_transform()),
        }));
    }
    fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s peniko::kurbo::Stroke,
    ) {
        let paint = try_ret!(brush_to_paint(brush));
        let path = try_ret!(shape_to_path(shape));
        self.mark_stroke_bounds(shape, stroke);
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
        let stroke = Stroke {
            width: stroke.width as f32,
            miter_limit: stroke.miter_limit as f32,
            line_cap,
            line_join,
            dash: (!stroke.dash_pattern.is_empty())
                .then_some(StrokeDash::new(
                    stroke.dash_pattern.iter().map(|v| *v as f32).collect(),
                    stroke.dash_offset as f32,
                ))
                .flatten(),
        };
        self.pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            self.skia_transform(),
            self.clip.is_some().then_some(&self.mask),
        );
    }

    fn fill<'b>(&mut self, shape: &impl Shape, brush: impl Into<BrushRef<'b>>, _blur_radius: f64) {
        // FIXME: Handle _blur_radius

        let brush = brush.into();
        if let Some(rect) = shape.as_rect()
            && let BrushRef::Solid(color) = brush
            && self.try_fill_solid_rect_fast(rect, color)
        {
            return;
        }

        let paint = try_ret!(brush_to_paint(brush));
        self.mark_drawn_rect_inflated(shape.bounding_box(), self.device_transform(), 2.0);
        if let Some(rect) = shape.as_rect() {
            if !self.try_fill_rect_with_paint_fast(rect, &paint) {
                let rect = try_ret!(to_skia_rect(rect));
                self.pixmap.fill_rect(
                    rect,
                    &paint,
                    self.skia_transform(),
                    self.clip.is_some().then_some(&self.mask),
                );
            }
        } else {
            let path = try_ret!(shape_to_path(shape));
            self.pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                self.skia_transform(),
                self.clip.is_some().then_some(&self.mask),
            );
        }
    }

    fn draw_glyphs<'a>(
        &mut self,
        origin: Point,
        props: &GlyphRunProps<'a>,
        glyphs: impl Iterator<Item = ParleyGlyph> + 'a,
        font_embolden: f32,
    ) {
        let font = &props.font;
        let clip = self.clip;
        let (_, _, raster_scale) = self.scale_components();
        let coeffs = props.transform.as_coeffs();
        let pos = self.device_transform()
            * (origin + peniko::kurbo::Point::new(coeffs[4], coeffs[5]).to_vec2());
        let transform = self.normalized_linear_transform(false);
        let brush_color = match &props.brush {
            peniko::Brush::Solid(color) => Color::from(*color),
            _ => return,
        };
        let font_ref = match FontRef::from_index(font.data.data(), font.index as usize) {
            Some(f) => f,
            None => return,
        };
        let font_blob_id = font.data.id();
        let skew = props
            .glyph_transform
            .map(|transform| transform.as_coeffs()[0].atan().to_degrees() as f32);

        for glyph in glyphs {
            let glyph_x = pos.x as f32 + glyph.x * raster_scale as f32;
            let glyph_y = pos.y as f32 + glyph.y * raster_scale as f32;

            if let Some(rect) = clip
                && glyph_x as f64 > rect.x1
            {
                break;
            }

            let scaled_font_size = props.font_size * raster_scale as f32;

            let (cache_key, new_x, new_y) = GlyphCacheKey::new(
                font_blob_id,
                font.index,
                glyph.id as u16,
                scaled_font_size,
                glyph_x,
                glyph_y,
                false,
                skew,
            );

            let cached = cache_glyph(
                self.cache_color,
                cache_key,
                brush_color,
                &font_ref,
                scaled_font_size,
                props.normalized_coords,
                font_embolden,
                skew,
                new_x,
                new_y,
            );

            if let Some(cached) = cached {
                self.render_pixmap_direct(
                    &cached.pixmap,
                    new_x.floor() + cached.left,
                    new_y.floor() - cached.top,
                    transform,
                );
            }
        }
    }

    fn draw_svg<'b>(
        &mut self,
        svg: floem_renderer::Svg<'b>,
        rect: Rect,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        let (scale_x, scale_y, _) = self.scale_components();
        let width = (rect.width() * scale_x.abs()).round().max(1.0) as u32;
        let height = (rect.height() * scale_y.abs()).round().max(1.0) as u32;
        let transform = self.device_transform();

        let paint = brush.and_then(|brush| brush_to_paint(brush));

        if IMAGE_CACHE.with_borrow_mut(|ic| {
            if let Some((color, non_colored_svg_pixmap)) = ic.get_mut(svg.hash) {
                *color = self.cache_color;
                let pixmap = non_colored_svg_pixmap.clone();
                self.render_pixmap_with_paint(&pixmap, rect, transform, paint.clone());
                // return
                true
            } else {
                // continue
                false
            }
        }) {
            return;
        };

        let mut non_colored_svg = try_ret!(tiny_skia::Pixmap::new(width, height));
        let svg_transform = tiny_skia::Transform::from_scale(
            width as f32 / svg.tree.size().width(),
            height as f32 / svg.tree.size().height(),
        );
        resvg::render(svg.tree, svg_transform, &mut non_colored_svg.as_mut());

        self.render_pixmap_with_paint(&non_colored_svg, rect, transform, paint);

        IMAGE_CACHE.with_borrow_mut(|ic| {
            ic.insert(
                svg.hash.to_owned(),
                (self.cache_color, Rc::new(non_colored_svg)),
            )
        });
    }

    fn draw_img(&mut self, img: Img<'_>, rect: Rect) {
        let transform = self.device_transform();
        if IMAGE_CACHE.with_borrow_mut(|ic| {
            if let Some((color, pixmap)) = ic.get_mut(img.hash) {
                *color = self.cache_color;
                let pixmap = pixmap.clone();
                self.render_pixmap_rect(&pixmap, rect, transform);
                // return
                true
            } else {
                // continue
                false
            }
        }) {
            return;
        };

        let image_data = img.img.image.data.data();
        let mut pixmap = try_ret!(Pixmap::new(img.img.image.width, img.img.image.height));
        for (a, b) in pixmap
            .pixels_mut()
            .iter_mut()
            .zip(image_data.chunks_exact(4))
        {
            *a = tiny_skia::Color::from_rgba8(b[0], b[1], b[2], b[3])
                .premultiply()
                .to_color_u8();
        }

        self.render_pixmap_rect(&pixmap, rect, transform);

        IMAGE_CACHE.with_borrow_mut(|ic| {
            ic.insert(img.hash.to_owned(), (self.cache_color, Rc::new(pixmap)))
        });
    }
}

pub struct TinySkiaRenderer<W> {
    #[allow(unused)]
    context: Context<W>,
    surface: Surface<W, W>,
    cache_color: CacheColor,
    transform: Affine,
    clip_stack: Vec<ClipPath>,
    window_scale: f64,
    capture: bool,
    layers: Vec<Layer>,
    last_presented_bounds: Option<Rect>,
    font_embolden: f32,
}

impl<W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle>
    TinySkiaRenderer<W>
{
    fn sync_current_layer_clip(&mut self) {
        if let Some(layer) = self.layers.last_mut() {
            layer.rebuild_clip_mask(&self.clip_stack);
        }
    }

    fn current_clip_path(&self, shape: &impl Shape) -> Option<ClipPath> {
        let path = shape_to_path(shape)?.transform(affine_to_skia(self.transform))?;
        Some(ClipPath {
            path,
            rect: self.transform.transform_rect_bbox(shape.bounding_box()),
            simple_rect: transformed_axis_aligned_rect(shape, self.transform),
        })
    }

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

        let main_layer = Layer {
            pixmap,
            base_clip: None,
            mask,
            clip: None,
            simple_clip: None,
            draw_bounds: None,
            alpha: 1.,
            transform: Affine::IDENTITY,
            blend_mode: Mix::Normal.into(),
            cache_color: CacheColor(false),
        };
        Ok(Self {
            context,
            surface,
            transform: Affine::IDENTITY,
            clip_stack: Vec::new(),
            window_scale: scale,
            capture: false,
            cache_color: CacheColor(false),
            layers: vec![main_layer],
            last_presented_bounds: None,
            font_embolden,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32, scale: f64) {
        if width != self.layers[0].pixmap.width() || height != self.layers[0].pixmap.height() {
            self.surface
                .resize(
                    NonZeroU32::new(width).unwrap_or(NonZeroU32::new(1).unwrap()),
                    NonZeroU32::new(height).unwrap_or(NonZeroU32::new(1).unwrap()),
                )
                .expect("failed to resize surface");
            self.layers[0].pixmap = Pixmap::new(width, height).expect("unable to create pixmap");
            self.layers[0].mask = Mask::new(width, height).expect("unable to create mask");
            self.last_presented_bounds = None;
        }
        self.window_scale = scale;
    }

    pub fn set_scale(&mut self, scale: f64) {
        self.window_scale = scale;
    }

    pub fn size(&self) -> Size {
        Size::new(
            self.layers[0].pixmap.width() as f64,
            self.layers[0].pixmap.height() as f64,
        )
    }
}

fn to_color(color: Color) -> tiny_skia::Color {
    let c = color.to_rgba8();
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

fn to_point(point: Point) -> tiny_skia::Point {
    tiny_skia::Point::from_xy(point.x as f32, point.y as f32)
}

fn is_axis_aligned(transform: Affine) -> bool {
    let coeffs = transform.as_coeffs();
    coeffs[1] == 0.0 && coeffs[2] == 0.0
}

fn transformed_axis_aligned_rect(shape: &impl Shape, transform: Affine) -> Option<Rect> {
    let rect = shape.as_rect()?;
    is_axis_aligned(transform).then(|| transform.transform_rect_bbox(rect))
}

fn nearly_integral(value: f64) -> Option<i32> {
    let rounded = value.round();
    ((value - rounded).abs() <= 1e-6).then_some(rounded as i32)
}

impl<W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle> Renderer
    for TinySkiaRenderer<W>
{
    fn begin(&mut self, capture: bool) {
        self.capture = capture;
        assert!(self.layers.len() == 1);
        self.transform = Affine::IDENTITY;
        self.clip_stack.clear();
        let first_layer = self.layers.last_mut().unwrap();
        first_layer.pixmap.fill(tiny_skia::Color::TRANSPARENT);
        first_layer.base_clip = None;
        first_layer.clip = None;
        first_layer.simple_clip = None;
        first_layer.draw_bounds = None;
        first_layer.transform = Affine::IDENTITY;
        first_layer.mask.clear();
    }

    fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s peniko::kurbo::Stroke,
    ) {
        self.layers.last_mut().unwrap().stroke(shape, brush, stroke);
    }

    fn fill<'b>(&mut self, shape: &impl Shape, brush: impl Into<BrushRef<'b>>, blur_radius: f64) {
        self.layers
            .last_mut()
            .unwrap()
            .fill(shape, brush, blur_radius);
    }

    fn draw_glyphs<'a>(
        &mut self,
        origin: Point,
        props: &GlyphRunProps<'a>,
        glyphs: impl Iterator<Item = ParleyGlyph> + 'a,
    ) {
        self.layers
            .last_mut()
            .unwrap()
            .draw_glyphs(origin, props, glyphs, self.font_embolden);
    }

    fn draw_img(&mut self, img: Img<'_>, rect: Rect) {
        self.layers.last_mut().unwrap().draw_img(img, rect);
    }

    fn draw_svg<'b>(
        &mut self,
        svg: floem_renderer::Svg<'b>,
        rect: Rect,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        self.layers.last_mut().unwrap().draw_svg(svg, rect, brush);
    }

    fn set_transform(&mut self, cumulative_transform: Affine) {
        let uncombined = self.transform.inverse() * cumulative_transform;
        self.transform = cumulative_transform;
        self.layers.last_mut().unwrap().transform(uncombined);
    }

    fn set_z_index(&mut self, _z_index: i32) {
        // FIXME: Remove this method?
    }

    fn clip(&mut self, shape: &impl Shape) {
        if let Some(clip) = self.current_clip_path(shape) {
            self.clip_stack.push(clip);
            if let Some(layer) = self.layers.last_mut() {
                let clip = self.clip_stack.last().unwrap().clone();
                layer.intersect_clip_path(&clip);
            }
        }
    }

    fn clear_clip(&mut self) {
        self.clip_stack.pop();
        self.sync_current_layer_clip();
    }

    fn finish(&mut self) -> Option<peniko::ImageBrush> {
        // Remove cache entries which were not accessed.
        IMAGE_CACHE.with_borrow_mut(|ic| ic.retain(|_, (c, _)| *c == self.cache_color));
        GLYPH_CACHE.with_borrow_mut(|gc| gc.retain(|_, (c, _)| *c == self.cache_color));

        // Swap the cache color.
        self.cache_color = CacheColor(!self.cache_color.0);

        if self.capture {
            let pixmap = &self.layers.last().unwrap().pixmap;
            let data = pixmap.data().to_vec();
            return Some(peniko::ImageBrush::new(ImageData {
                data: Blob::new(Arc::new(data)),
                format: peniko::ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::AlphaPremultiplied,
                width: pixmap.width(),
                height: pixmap.height(),
            }));
        }

        let mut buffer = self
            .surface
            .buffer_mut()
            .expect("failed to get the surface buffer");

        let current_bounds = self.layers.last().unwrap().draw_bounds;
        let full_bounds = Rect::new(
            0.0,
            0.0,
            self.layers.last().unwrap().pixmap.width() as f64,
            self.layers.last().unwrap().pixmap.height() as f64,
        );
        let copy_bounds = if buffer.age() == 0 {
            Some(full_bounds)
        } else {
            match (current_bounds, self.last_presented_bounds) {
                (Some(current), Some(previous)) => Some(current.union(previous)),
                (Some(current), None) => Some(current),
                (None, Some(previous)) => Some(previous),
                (None, None) => None,
            }
        };

        if let Some(copy_bounds) = copy_bounds.and_then(rect_to_int_rect) {
            let x0 = copy_bounds.x().max(0) as u32;
            let y0 = copy_bounds.y().max(0) as u32;
            let x1 = (copy_bounds.x() + copy_bounds.width() as i32)
                .min(self.layers.last().unwrap().pixmap.width() as i32) as u32;
            let y1 = (copy_bounds.y() + copy_bounds.height() as i32)
                .min(self.layers.last().unwrap().pixmap.height() as i32) as u32;

            if x0 < x1 && y0 < y1 {
                let pixmap = &self.layers.last().unwrap().pixmap;
                let width = pixmap.width() as usize;
                for y in y0 as usize..y1 as usize {
                    let row_start = y * width;
                    let src = &pixmap.pixels()[row_start + x0 as usize..row_start + x1 as usize];
                    let dst =
                        &mut buffer[row_start + x0 as usize..row_start + x1 as usize];
                    for (out_pixel, pixel) in dst.iter_mut().zip(src.iter()) {
                        *out_pixel = ((pixel.red() as u32) << 16)
                            | ((pixel.green() as u32) << 8)
                            | (pixel.blue() as u32);
                    }
                }

                let damage = [softbuffer::Rect {
                    x: x0,
                    y: y0,
                    width: NonZeroU32::new(x1 - x0).unwrap(),
                    height: NonZeroU32::new(y1 - y0).unwrap(),
                }];
                buffer
                    .present_with_damage(&damage)
                    .expect("failed to present the surface buffer");
            } else {
                buffer
                    .present()
                    .expect("failed to present the surface buffer");
            }
        } else {
            buffer
                .present()
                .expect("failed to present the surface buffer");
        }

        self.last_presented_bounds = current_bounds;

        None
    }

    fn push_layer(
        &mut self,
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        if let Ok(res) = Layer::new(
            blend,
            alpha,
            self.transform * transform,
            clip,
            self.layers.last().unwrap().pixmap.width(),
            self.layers.last().unwrap().pixmap.height(),
            self.cache_color,
        ) {
            self.layers.push(res);
            self.sync_current_layer_clip();
        }
    }

    fn pop_layer(&mut self) {
        if self.layers.len() <= 1 {
            // Don't pop the main layer
            return;
        }

        let layer = self.layers.pop().unwrap();
        let parent = self.layers.last_mut().unwrap();
        parent.rebuild_clip_mask(&self.clip_stack);

        apply_layer(&layer, parent);
    }

    fn debug_info(&self) -> String {
        "name: tiny_skia".into()
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
                GradientKind::Linear(linear) => LinearGradient::new(
                    to_point(linear.start),
                    to_point(linear.end),
                    stops,
                    SpreadMode::Pad,
                    Transform::identity(),
                )?,
                GradientKind::Radial(RadialGradientPosition {
                    start_center,
                    start_radius: _,
                    end_center,
                    end_radius,
                }) => {
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

fn to_skia_rect(rect: Rect) -> Option<tiny_skia::Rect> {
    tiny_skia::Rect::from_ltrb(
        rect.x0 as f32,
        rect.y0 as f32,
        rect.x1 as f32,
        rect.y1 as f32,
    )
}

fn rect_to_int_rect(rect: Rect) -> Option<IntRect> {
    IntRect::from_ltrb(
        rect.x0.floor() as i32,
        rect.y0.floor() as i32,
        rect.x1.ceil() as i32,
        rect.y1.ceil() as i32,
    )
}

type TinyBlendMode = tiny_skia::BlendMode;

enum BlendStrategy {
    /// Can be directly mapped to a tiny-skia blend mode
    SinglePass(TinyBlendMode),
    /// Requires multiple operations
    MultiPass {
        first_pass: TinyBlendMode,
        second_pass: TinyBlendMode,
    },
}

fn determine_blend_strategy(peniko_mode: &BlendMode) -> BlendStrategy {
    match (peniko_mode.mix, peniko_mode.compose) {
        (Mix::Normal, compose) => BlendStrategy::SinglePass(compose_to_tiny_blend_mode(compose)),

        (mix, Compose::SrcOver) => BlendStrategy::SinglePass(mix_to_tiny_blend_mode(mix)),

        (mix, compose) => BlendStrategy::MultiPass {
            first_pass: compose_to_tiny_blend_mode(compose),
            second_pass: mix_to_tiny_blend_mode(mix),
        },
    }
}

fn compose_to_tiny_blend_mode(compose: Compose) -> TinyBlendMode {
    match compose {
        Compose::Clear => TinyBlendMode::Clear,
        Compose::Copy => TinyBlendMode::Source,
        Compose::Dest => TinyBlendMode::Destination,
        Compose::SrcOver => TinyBlendMode::SourceOver,
        Compose::DestOver => TinyBlendMode::DestinationOver,
        Compose::SrcIn => TinyBlendMode::SourceIn,
        Compose::DestIn => TinyBlendMode::DestinationIn,
        Compose::SrcOut => TinyBlendMode::SourceOut,
        Compose::DestOut => TinyBlendMode::DestinationOut,
        Compose::SrcAtop => TinyBlendMode::SourceAtop,
        Compose::DestAtop => TinyBlendMode::DestinationAtop,
        Compose::Xor => TinyBlendMode::Xor,
        Compose::Plus => TinyBlendMode::Plus,
        Compose::PlusLighter => TinyBlendMode::Plus, // ??
    }
}

fn mix_to_tiny_blend_mode(mix: Mix) -> TinyBlendMode {
    match mix {
        Mix::Normal => TinyBlendMode::SourceOver,
        Mix::Multiply => TinyBlendMode::Multiply,
        Mix::Screen => TinyBlendMode::Screen,
        Mix::Overlay => TinyBlendMode::Overlay,
        Mix::Darken => TinyBlendMode::Darken,
        Mix::Lighten => TinyBlendMode::Lighten,
        Mix::ColorDodge => TinyBlendMode::ColorDodge,
        Mix::ColorBurn => TinyBlendMode::ColorBurn,
        Mix::HardLight => TinyBlendMode::HardLight,
        Mix::SoftLight => TinyBlendMode::SoftLight,
        Mix::Difference => TinyBlendMode::Difference,
        Mix::Exclusion => TinyBlendMode::Exclusion,
        Mix::Hue => TinyBlendMode::Hue,
        Mix::Saturation => TinyBlendMode::Saturation,
        Mix::Color => TinyBlendMode::Color,
        Mix::Luminosity => TinyBlendMode::Luminosity,
    }
}

fn layer_composite_rect(layer: &Layer, parent: &Layer) -> Option<IntRect> {
    let mut rect = Rect::from_origin_size(
        Point::ZERO,
        Size::new(layer.pixmap.width() as f64, layer.pixmap.height() as f64),
    );

    if let Some(draw_bounds) = layer.draw_bounds {
        rect = rect.intersect(draw_bounds);
    } else {
        return None;
    }

    if let Some(layer_clip) = layer.clip {
        rect = rect.intersect(layer_clip);
    }

    if let Some(parent_clip) = parent.clip {
        rect = rect.intersect(parent_clip);
    }

    if rect.is_zero_area() {
        return None;
    }

    rect_to_int_rect(rect)
}

fn draw_layer_pixmap(
    pixmap: &Pixmap,
    x: i32,
    y: i32,
    parent: &mut Layer,
    blend_mode: TinyBlendMode,
    alpha: f32,
) {
    parent.mark_drawn_device_rect(Rect::new(
        x as f64,
        y as f64,
        (x + pixmap.width() as i32) as f64,
        (y + pixmap.height() as i32) as f64,
    ));

    let paint = PixmapPaint {
        opacity: alpha,
        blend_mode,
        quality: FilterQuality::Nearest,
    };

    parent.pixmap.draw_pixmap(
        x,
        y,
        pixmap.as_ref(),
        &paint,
        Transform::identity(),
        parent.clip.is_some().then_some(&parent.mask),
    );
}

fn draw_layer_region(
    parent: &mut Layer,
    pixmap: &Pixmap,
    composite_rect: IntRect,
    blend_mode: TinyBlendMode,
    alpha: f32,
) {
    let Some(cropped) = pixmap.clone_rect(composite_rect) else {
        return;
    };

    draw_layer_pixmap(
        &cropped,
        composite_rect.x(),
        composite_rect.y(),
        parent,
        blend_mode,
        alpha,
    );
}

fn apply_layer(layer: &Layer, parent: &mut Layer) {
    let Some(composite_rect) = layer_composite_rect(layer, parent) else {
        return;
    };

    match determine_blend_strategy(&layer.blend_mode) {
        BlendStrategy::SinglePass(blend_mode) => {
            draw_layer_region(parent, &layer.pixmap, composite_rect, blend_mode, layer.alpha);
        }
        BlendStrategy::MultiPass {
            first_pass,
            second_pass,
        } => {
            let Some(original_parent) = parent.pixmap.clone_rect(composite_rect) else {
                return;
            };

            draw_layer_region(parent, &layer.pixmap, composite_rect, first_pass, 1.0);

            let Some(intermediate) = parent.pixmap.clone_rect(composite_rect) else {
                return;
            };

            draw_layer_pixmap(
                &original_parent,
                composite_rect.x(),
                composite_rect.y(),
                parent,
                TinyBlendMode::Source,
                1.0,
            );

            draw_layer_pixmap(
                &intermediate,
                composite_rect.x(),
                composite_rect.y(),
                parent,
                second_pass,
                1.0,
            );
        }
    }
}

fn affine_to_skia(affine: Affine) -> Transform {
    let transform = affine.as_coeffs();
    Transform::from_row(
        transform[0] as f32,
        transform[1] as f32,
        transform[2] as f32,
        transform[3] as f32,
        transform[4] as f32,
        transform[5] as f32,
    )
}

fn skia_transform(affine: Affine) -> Transform {
    affine_to_skia(affine)
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Creates a `Layer` directly without a window, for offscreen rendering.
    fn make_layer(width: u32, height: u32) -> Layer {
        Layer {
            pixmap: Pixmap::new(width, height).expect("failed to create pixmap"),
            base_clip: None,
            clip: None,
            simple_clip: None,
            draw_bounds: None,
            mask: Mask::new(width, height).expect("failed to create mask"),
            transform: Affine::IDENTITY,
            blend_mode: Mix::Normal.into(),
            alpha: 1.0,
            cache_color: CacheColor(false),
        }
    }

    fn pixel_rgba(layer: &Layer, x: u32, y: u32) -> (u8, u8, u8, u8) {
        let idx = (y * layer.pixmap.width() + x) as usize;
        let pixel = layer.pixmap.pixels()[idx];
        (pixel.red(), pixel.green(), pixel.blue(), pixel.alpha())
    }

    #[test]
    fn render_pixmap_rect_uses_transform_and_mask() {
        let mut layer = make_layer(12, 12);
        layer.transform = Affine::translate((4.0, 0.0));
        layer.clip(&Rect::new(1.0, 0.0, 3.0, 4.0));

        let mut src = Pixmap::new(2, 2).expect("failed to create src pixmap");
        src.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));

        layer.render_pixmap_rect(
            &src,
            Rect::new(0.0, 0.0, 4.0, 4.0),
            layer.device_transform(),
        );

        assert_eq!(pixel_rgba(&layer, 3, 1), (0, 0, 0, 0));
        assert_eq!(pixel_rgba(&layer, 4, 1), (0, 0, 0, 0));
        assert_eq!(pixel_rgba(&layer, 5, 1), (255, 0, 0, 255));
        assert_eq!(pixel_rgba(&layer, 6, 1), (255, 0, 0, 255));
        assert_eq!(pixel_rgba(&layer, 7, 1), (0, 0, 0, 0));
        assert_eq!(pixel_rgba(&layer, 8, 1), (0, 0, 0, 0));
    }

    #[test]
    fn nested_layer_marks_parent_draw_bounds() {
        let mut root = make_layer(8, 8);
        let mut parent = make_layer(8, 8);
        let mut child = make_layer(8, 8);

        child.fill(
            &Rect::new(2.0, 2.0, 4.0, 4.0),
            Color::from_rgb8(255, 0, 0),
            0.0,
        );

        apply_layer(&child, &mut parent);
        assert!(parent.draw_bounds.is_some());

        apply_layer(&parent, &mut root);
        assert_eq!(pixel_rgba(&root, 3, 3), (255, 0, 0, 255));
    }
}
