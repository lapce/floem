mod recording;

use anyhow::{Result, anyhow};
use floem_renderer::Img;
use floem_renderer::Renderer;
use floem_renderer::text::{Glyph as ParleyGlyph, GlyphRunProps};
use floem_renderer::tiny_skia::{
    self, FillRule, FilterQuality, GradientStop, IntRect, LinearGradient, Mask, MaskType, Paint,
    Path, PathBuilder, Pixmap, PixmapPaint, PremultipliedColorU8, RadialGradient, Shader,
    SpreadMode, Stroke, Transform,
};
use peniko::kurbo::{PathEl, Size};
use peniko::{
    BlendMode, Blob, Compose, ImageAlphaType, ImageData, ImageQuality, Mix, RadialGradientPosition,
};
use peniko::{
    BrushRef, Color, GradientKind,
    kurbo::{Affine, Point, Rect, Shape},
};
use recording::{RecordedCommand, RecordedLayer, Recording};
use resvg::tiny_skia::StrokeDash;
use rustc_hash::FxHashMap;
use softbuffer::{Context, Surface};
use std::cell::RefCell;
use std::num::NonZeroU32;
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
    hint: bool,
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
        hint: bool,
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
                hint,
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
    static IMAGE_CACHE: RefCell<FxHashMap<Vec<u8>, (CacheColor, Arc<Pixmap>)>> = RefCell::new(FxHashMap::default());
    #[allow(clippy::type_complexity)]
    static SCALED_IMAGE_CACHE: RefCell<FxHashMap<ScaledImageCacheKey, (CacheColor, Arc<Pixmap>)>> = RefCell::new(FxHashMap::default());
    #[allow(clippy::type_complexity)]
    // The `u32` is a color encoded as a u32 so that it is hashable and eq.
    static GLYPH_CACHE: RefCell<FxHashMap<(GlyphCacheKey, u32), (CacheColor, Option<Arc<Glyph>>)>> = RefCell::new(FxHashMap::default());
    static SCALE_CONTEXT: RefCell<ScaleContext> = RefCell::new(ScaleContext::new());
}

#[allow(clippy::too_many_arguments)]
fn cache_glyph(
    cache_color: CacheColor,
    cache_key: GlyphCacheKey,
    color: Color,
    font_ref: &FontRef<'_>,
    font_size: f32,
    hint: bool,
    normalized_coords: &[i16],
    embolden_strength: f32,
    skew: Option<f32>,
    offset_x: f32,
    offset_y: f32,
) -> Option<Arc<Glyph>> {
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
            .hint(hint)
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

        Some(Arc::new(Glyph {
            pixmap: Arc::new(pixmap),
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
    pixmap: Arc<Pixmap>,
    left: f32,
    top: f32,
}

#[derive(Clone)]
pub(crate) struct ClipPath {
    path: Path,
    rect: Rect,
    simple_rect: Option<Rect>,
}

#[derive(PartialEq, Clone, Copy)]
struct CacheColor(bool);

#[derive(Hash, PartialEq, Eq)]
struct ScaledImageCacheKey {
    image_id: u64,
    width: u32,
    height: u32,
    quality: u8,
}

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
}
impl Layer {
    fn new_root(width: u32, height: u32) -> Result<Self, anyhow::Error> {
        Ok(Self {
            pixmap: Pixmap::new(width, height).ok_or_else(|| anyhow!("unable to create pixmap"))?,
            base_clip: None,
            clip: None,
            simple_clip: None,
            draw_bounds: None,
            mask: Mask::new(width, height).ok_or_else(|| anyhow!("unable to create mask"))?,
            transform: Affine::IDENTITY,
            blend_mode: Mix::Normal.into(),
            alpha: 1.0,
        })
    }

    fn new_with_base_clip(
        blend_mode: BlendMode,
        alpha: f32,
        clip: ClipPath,
        width: u32,
        height: u32,
    ) -> Result<Self, anyhow::Error> {
        let mut layer = Self {
            pixmap: Pixmap::new(width, height).ok_or_else(|| anyhow!("unable to create pixmap"))?,
            base_clip: Some(clip),
            clip: None,
            simple_clip: None,
            draw_bounds: None,
            mask: Mask::new(width, height).ok_or_else(|| anyhow!("unable to create mask"))?,
            transform: Affine::IDENTITY,
            blend_mode,
            alpha,
        };
        layer.rebuild_clip_mask(&[]);
        Ok(layer)
    }

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
        let clip_rect = self
            .clip
            .map(|rect| rect.intersect(clip.rect))
            .unwrap_or(clip.rect);
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
                self.mask.intersect_path(
                    &clip.path,
                    FillRule::Winding,
                    false,
                    Transform::identity(),
                );
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
            (None, Some(next)) if self.base_clip.is_none() && self.clip == Some(clip_rect) => {
                Some(next)
            }
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
                self.mask.intersect_path(
                    &clip.path,
                    FillRule::Winding,
                    false,
                    Transform::identity(),
                );
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

        let Some(device_rect) = rect_to_int_rect(self.device_transform().transform_rect_bbox(rect))
        else {
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

        let stroke_pad = stroke.width + stroke.miter_limit.max(1.0) + 4.0;
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
        let Some((draw_x, draw_y)) = integer_translation(transform, x, y) else {
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
        if quality == FilterQuality::Nearest && self.blit_pixmap_source_over(pixmap, draw_x, draw_y)
        {
            return true;
        }

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

    fn blit_pixmap_source_over(&mut self, pixmap: &Pixmap, draw_x: i32, draw_y: i32) -> bool {
        let Some((x0, y0, x1, y1)) = self.blit_bounds(pixmap, draw_x, draw_y) else {
            return true;
        };

        let src_width = pixmap.width() as usize;
        let dst_width = self.pixmap.width() as usize;
        let mask_width = self.mask.width() as usize;
        let src_pixels = pixmap.pixels();
        let mask = self.clip.is_some().then_some(self.mask.data());
        let dst_pixels = self.pixmap.pixels_mut();

        for dst_y in y0 as usize..y1 as usize {
            let src_y = (dst_y as i32 - draw_y) as usize;
            let dst_row = dst_y * dst_width;
            let src_row = src_y * src_width;
            let mask_row = dst_y * mask_width;

            for dst_x in x0 as usize..x1 as usize {
                let src_x = (dst_x as i32 - draw_x) as usize;
                let src = src_pixels[src_row + src_x];
                let coverage = mask.map_or(255, |mask| mask[mask_row + dst_x]);
                if coverage == 0 || src.alpha() == 0 {
                    continue;
                }

                let src = scale_premultiplied_color(src, coverage);
                let dst = dst_pixels[dst_row + dst_x];
                dst_pixels[dst_row + dst_x] = blend_source_over(src, dst);
            }
        }

        true
    }

    fn blit_bounds(
        &self,
        pixmap: &Pixmap,
        draw_x: i32,
        draw_y: i32,
    ) -> Option<(i32, i32, i32, i32)> {
        let mut x0 = draw_x.max(0);
        let mut y0 = draw_y.max(0);
        let mut x1 = (draw_x + pixmap.width() as i32).min(self.pixmap.width() as i32);
        let mut y1 = (draw_y + pixmap.height() as i32).min(self.pixmap.height() as i32);

        if let Some(simple_clip) = self.simple_clip {
            let clip_rect = rect_to_int_rect(simple_clip)?;
            x0 = x0.max(clip_rect.x());
            y0 = y0.max(clip_rect.y());
            x1 = x1.min(clip_rect.x() + clip_rect.width() as i32);
            y1 = y1.min(clip_rect.y() + clip_rect.height() as i32);
        }

        (x0 < x1 && y0 < y1).then_some((x0, y0, x1, y1))
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
    fn render_pixmap_direct(
        &mut self,
        img_pixmap: &Pixmap,
        x: f32,
        y: f32,
        transform: Affine,
        quality: FilterQuality,
    ) {
        if self.try_draw_pixmap_translate_only(img_pixmap, x as f64, y as f64, transform, quality) {
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
            quality,
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

    fn render_pixmap_rect(
        &mut self,
        pixmap: &Pixmap,
        rect: Rect,
        transform: Affine,
        quality: ImageQuality,
    ) {
        let filter_quality = image_quality_to_filter_quality(quality);
        let local_transform = Affine::translate((rect.x0, rect.y0)).then_scale_non_uniform(
            rect.width() / pixmap.width() as f64,
            rect.height() / pixmap.height() as f64,
        );
        let composite_transform = transform * local_transform;

        if self.try_draw_pixmap_translate_only(
            pixmap,
            0.0,
            0.0,
            composite_transform,
            filter_quality,
        ) {
            return;
        }

        if !self.intersects_clip(rect, transform) {
            return;
        }
        self.mark_drawn_rect_inflated(rect, transform, 2.0);
        let paint = PixmapPaint {
            opacity: 1.0,
            blend_mode: tiny_skia::BlendMode::SourceOver,
            quality: filter_quality,
        };

        self.pixmap.draw_pixmap(
            0,
            0,
            pixmap.as_ref(),
            &paint,
            affine_to_skia(composite_transform),
            self.clip.is_some().then_some(&self.mask),
        );
    }

    fn skia_transform(&self) -> Transform {
        skia_transform(self.device_transform())
    }
}
impl Layer {
    #[cfg(test)]
    fn clip(&mut self, shape: &impl Shape) {
        let path =
            try_ret!(shape_to_path(shape).and_then(|path| path.transform(self.skia_transform())));
        self.set_base_clip(Some(ClipPath {
            path,
            rect: self
                .device_transform()
                .transform_rect_bbox(shape.bounding_box()),
            simple_rect: transformed_axis_aligned_rect(shape, self.device_transform()),
        }));
    }
    fn stroke_recorded_path<'b, 's>(
        &mut self,
        path: &Path,
        bounds: Rect,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s peniko::kurbo::Stroke,
    ) {
        let paint = try_ret!(brush_to_paint(brush));
        self.mark_stroke_bounds(&bounds, stroke);
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
            path,
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

    fn fill_recorded_path<'b>(
        &mut self,
        path: &Path,
        bounds: Rect,
        brush: impl Into<BrushRef<'b>>,
        _blur_radius: f64,
    ) {
        let paint = try_ret!(brush_to_paint(brush));
        self.mark_drawn_rect_inflated(bounds, self.device_transform(), 2.0);
        self.pixmap.fill_path(
            path,
            &paint,
            FillRule::Winding,
            self.skia_transform(),
            self.clip.is_some().then_some(&self.mask),
        );
    }
}

pub struct TinySkiaRenderer<W> {
    #[allow(unused)]
    context: Context<W>,
    surface: Surface<W, W>,
    cache_color: CacheColor,
    recording: Recording,
    transform: Affine,
    window_scale: f64,
    capture: bool,
    layers: Vec<Layer>,
    last_presented_bounds: Option<Rect>,
    font_embolden: f32,
}

impl<W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle>
    TinySkiaRenderer<W>
{
    fn current_clip_path(&self, shape: &impl Shape) -> Option<ClipPath> {
        let path = shape_to_path(shape)?.transform(affine_to_skia(self.transform))?;
        Some(ClipPath {
            path,
            rect: self.transform.transform_rect_bbox(shape.bounding_box()),
            simple_rect: transformed_axis_aligned_rect(shape, self.transform),
        })
    }

    fn clear_root_layer(&mut self) {
        let first_layer = &mut self.layers[0];
        first_layer.pixmap.fill(tiny_skia::Color::TRANSPARENT);
        first_layer.base_clip = None;
        first_layer.clip = None;
        first_layer.simple_clip = None;
        first_layer.draw_bounds = None;
        first_layer.transform = Affine::IDENTITY;
        first_layer.mask.clear();
    }

    fn brush_to_owned<'b>(&self, brush: impl Into<BrushRef<'b>>) -> Option<peniko::Brush> {
        match brush.into() {
            BrushRef::Solid(color) => Some(peniko::Brush::Solid(color)),
            BrushRef::Gradient(gradient) => Some(peniko::Brush::Gradient(gradient.clone())),
            BrushRef::Image(_) => None,
        }
    }

    fn colorize_pixmap<'b>(
        &self,
        pixmap: &Pixmap,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) -> Option<Arc<Pixmap>> {
        let paint = brush.and_then(|brush| brush_to_paint(brush))?;
        let mut colored_bg = Pixmap::new(pixmap.width(), pixmap.height())?;
        colored_bg.fill_rect(
            tiny_skia::Rect::from_xywh(0.0, 0.0, pixmap.width() as f32, pixmap.height() as f32)?,
            &paint,
            Transform::identity(),
            None,
        );

        let mask = Mask::from_pixmap(pixmap.as_ref(), MaskType::Alpha);
        colored_bg.apply_mask(&mask);
        Some(Arc::new(colored_bg))
    }

    fn replay_layer(
        recorded: &RecordedLayer,
        raster: &mut Layer,
        inherited_clips: &[ClipPath],
        width: u32,
        height: u32,
    ) {
        let mut active_clips = inherited_clips.to_vec();

        for command in &recorded.commands {
            match command {
                RecordedCommand::PushClip(clip) => {
                    active_clips.push(clip.clone());
                    raster.intersect_clip_path(clip);
                }
                RecordedCommand::PopClip => {
                    active_clips.pop();
                    raster.rebuild_clip_mask(&active_clips);
                }
                RecordedCommand::FillRect {
                    rect,
                    brush,
                    transform,
                    blur_radius,
                } => {
                    raster.transform = *transform;
                    raster.fill(rect, brush, *blur_radius);
                }
                RecordedCommand::FillPath {
                    path,
                    bounds,
                    brush,
                    transform,
                    blur_radius,
                } => {
                    raster.transform = *transform;
                    raster.fill_recorded_path(path, *bounds, brush, *blur_radius);
                }
                RecordedCommand::StrokePath {
                    path,
                    bounds,
                    brush,
                    stroke,
                    transform,
                } => {
                    raster.transform = *transform;
                    raster.stroke_recorded_path(path, *bounds, brush, stroke);
                }
                RecordedCommand::DrawPixmapDirect {
                    pixmap,
                    x,
                    y,
                    transform,
                    quality,
                } => {
                    raster.render_pixmap_direct(pixmap, *x, *y, *transform, *quality);
                }
                RecordedCommand::DrawPixmapRect {
                    pixmap,
                    rect,
                    transform,
                    quality,
                } => {
                    raster.render_pixmap_rect(pixmap, *rect, *transform, *quality);
                }
                RecordedCommand::Layer(layer) => {
                    let Some(clip) = layer.clip.clone() else {
                        continue;
                    };
                    let Ok(mut child) = Layer::new_with_base_clip(
                        layer.blend_mode,
                        layer.alpha,
                        clip,
                        width,
                        height,
                    ) else {
                        continue;
                    };
                    child.rebuild_clip_mask(&active_clips);
                    Self::replay_layer(layer, &mut child, &active_clips, width, height);
                    apply_layer(&child, raster);
                }
            }
        }
    }

    fn replay_recording(&mut self) {
        self.clear_root_layer();
        let width = self.layers[0].pixmap.width();
        let height = self.layers[0].pixmap.height();
        let raster = &mut self.layers[0];
        Self::replay_layer(self.recording.root(), raster, &[], width, height);
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
        let main_layer = Layer::new_root(width, height)?;
        Ok(Self {
            context,
            surface,
            recording: Recording::new(),
            transform: Affine::IDENTITY,
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
            self.layers[0] = Layer::new_root(width, height).expect("unable to create layer");
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

fn affine_scale_components(transform: Affine) -> (f64, f64, f64) {
    let coeffs = transform.as_coeffs();
    let scale_x = coeffs[0].hypot(coeffs[1]);
    let scale_y = coeffs[2].hypot(coeffs[3]);
    let uniform = (scale_x + scale_y) * 0.5;
    (scale_x, scale_y, uniform)
}

fn scaled_embolden_strength(font_embolden: f32, raster_scale: f64) -> f32 {
    font_embolden * raster_scale as f32
}

fn normalize_affine(transform: Affine, include_translation: bool) -> Affine {
    let coeffs = transform.as_coeffs();
    let (scale_x, scale_y, _) = affine_scale_components(transform);
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

fn transformed_axis_aligned_rect(shape: &impl Shape, transform: Affine) -> Option<Rect> {
    let rect = shape.as_rect()?;
    is_axis_aligned(transform).then(|| transform.transform_rect_bbox(rect))
}

fn nearly_integral(value: f64) -> Option<i32> {
    let rounded = value.round();
    ((value - rounded).abs() <= 1e-6).then_some(rounded as i32)
}

fn integer_translation(transform: Affine, x: f64, y: f64) -> Option<(i32, i32)> {
    let coeffs = transform.as_coeffs();
    (coeffs[0] == 1.0 && coeffs[1] == 0.0 && coeffs[2] == 0.0 && coeffs[3] == 1.0).then_some((
        nearly_integral(x + coeffs[4])?,
        nearly_integral(y + coeffs[5])?,
    ))
}

fn image_quality_to_filter_quality(quality: ImageQuality) -> FilterQuality {
    match quality {
        ImageQuality::Low => FilterQuality::Nearest,
        ImageQuality::Medium | ImageQuality::High => FilterQuality::Bilinear,
    }
}

fn axis_aligned_device_placement(rect: Rect, transform: Affine) -> Option<(f32, f32, u32, u32)> {
    if !is_axis_aligned(transform) {
        return None;
    }

    let device_rect = transform.transform_rect_bbox(rect);
    let width = nearly_integral(device_rect.width())?;
    let height = nearly_integral(device_rect.height())?;
    (width > 0 && height > 0).then_some((
        device_rect.x0 as f32,
        device_rect.y0 as f32,
        width as u32,
        height as u32,
    ))
}

fn cache_scaled_pixmap(
    cache_color: CacheColor,
    cache_key: ScaledImageCacheKey,
    pixmap: &Pixmap,
    quality: ImageQuality,
) -> Option<Arc<Pixmap>> {
    if let Some(cached) = SCALED_IMAGE_CACHE.with_borrow_mut(|cache| {
        cache.get_mut(&cache_key).map(|(color, pixmap)| {
            *color = cache_color;
            pixmap.clone()
        })
    }) {
        return Some(cached);
    }

    let mut scaled = Pixmap::new(cache_key.width, cache_key.height)?;
    let paint = PixmapPaint {
        opacity: 1.0,
        blend_mode: tiny_skia::BlendMode::SourceOver,
        quality: image_quality_to_filter_quality(quality),
    };
    let transform = Transform::from_scale(
        cache_key.width as f32 / pixmap.width() as f32,
        cache_key.height as f32 / pixmap.height() as f32,
    );
    scaled.draw_pixmap(0, 0, pixmap.as_ref(), &paint, transform, None);

    let scaled = Arc::new(scaled);
    SCALED_IMAGE_CACHE.with_borrow_mut(|cache| {
        cache.insert(cache_key, (cache_color, scaled.clone()));
    });
    Some(scaled)
}

fn mul_div_255(value: u8, factor: u8) -> u8 {
    (((value as u16 * factor as u16) + 127) / 255) as u8
}

fn scale_premultiplied_color(color: PremultipliedColorU8, alpha: u8) -> PremultipliedColorU8 {
    if alpha == 255 {
        return color;
    }

    PremultipliedColorU8::from_rgba(
        mul_div_255(color.red(), alpha),
        mul_div_255(color.green(), alpha),
        mul_div_255(color.blue(), alpha),
        mul_div_255(color.alpha(), alpha),
    )
    .expect("scaled premultiplied color must remain premultiplied")
}

fn blend_source_over(src: PremultipliedColorU8, dst: PremultipliedColorU8) -> PremultipliedColorU8 {
    if src.alpha() == 255 {
        return src;
    }
    if src.alpha() == 0 {
        return dst;
    }

    let inv_alpha = 255 - src.alpha();
    PremultipliedColorU8::from_rgba(
        src.red().saturating_add(mul_div_255(dst.red(), inv_alpha)),
        src.green()
            .saturating_add(mul_div_255(dst.green(), inv_alpha)),
        src.blue()
            .saturating_add(mul_div_255(dst.blue(), inv_alpha)),
        src.alpha()
            .saturating_add(mul_div_255(dst.alpha(), inv_alpha)),
    )
    .expect("source-over premultiplied blend must remain premultiplied")
}

impl<W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle> Renderer
    for TinySkiaRenderer<W>
{
    fn begin(&mut self, capture: bool) {
        self.capture = capture;
        assert!(self.layers.len() == 1);
        self.transform = Affine::IDENTITY;
        self.recording.clear();
        self.clear_root_layer();
    }

    fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s peniko::kurbo::Stroke,
    ) {
        let Some(brush) = self.brush_to_owned(brush) else {
            return;
        };
        let Some(path) = shape_to_path(shape) else {
            return;
        };
        self.recording.stroke_path(
            path,
            shape.bounding_box(),
            brush,
            stroke.clone(),
            self.transform,
        );
    }

    fn fill<'b>(&mut self, shape: &impl Shape, brush: impl Into<BrushRef<'b>>, blur_radius: f64) {
        let Some(brush) = self.brush_to_owned(brush) else {
            return;
        };
        if let Some(rect) = shape.as_rect() {
            self.recording
                .fill_rect(rect, brush, self.transform, blur_radius);
        } else if let Some(path) = shape_to_path(shape) {
            self.recording.fill_path(
                path,
                shape.bounding_box(),
                brush,
                self.transform,
                blur_radius,
            );
        }
    }

    fn draw_glyphs<'a>(
        &mut self,
        origin: Point,
        props: &GlyphRunProps<'a>,
        glyphs: impl Iterator<Item = ParleyGlyph> + 'a,
    ) {
        let font = &props.font;
        let text_transform = self.transform * props.transform;
        let (_, _, raster_scale) = affine_scale_components(text_transform);
        let transform = normalize_affine(text_transform, false);
        let raster_origin = transform.inverse() * (text_transform * origin);
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
            let glyph_x = (raster_origin.x + glyph.x as f64 * raster_scale) as f32;
            let glyph_y = (raster_origin.y + glyph.y as f64 * raster_scale) as f32;
            let scaled_font_size = props.font_size * raster_scale as f32;
            let scaled_embolden = scaled_embolden_strength(self.font_embolden, raster_scale);
            let (cache_key, new_x, new_y) = GlyphCacheKey::new(
                font_blob_id,
                font.index,
                glyph.id as u16,
                scaled_font_size,
                glyph_x,
                glyph_y,
                props.hint,
                false,
                skew,
            );

            let cached = cache_glyph(
                self.cache_color,
                cache_key,
                brush_color,
                &font_ref,
                scaled_font_size,
                props.hint,
                props.normalized_coords,
                scaled_embolden,
                skew,
                new_x,
                new_y,
            );

            if let Some(cached) = cached {
                self.recording.draw_pixmap_direct(
                    cached.pixmap.clone(),
                    new_x.floor() + cached.left,
                    new_y.floor() - cached.top,
                    transform,
                    FilterQuality::Nearest,
                );
            }
        }
    }

    fn draw_img(&mut self, img: Img<'_>, rect: Rect) {
        let transform = self.transform;
        let pixmap = if let Some(pixmap) = IMAGE_CACHE.with_borrow_mut(|ic| {
            ic.get_mut(img.hash).map(|(color, pixmap)| {
                *color = self.cache_color;
                pixmap.clone()
            })
        }) {
            pixmap
        } else {
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

            let pixmap = Arc::new(pixmap);
            IMAGE_CACHE.with_borrow_mut(|ic| {
                ic.insert(img.hash.to_owned(), (self.cache_color, pixmap.clone()));
            });
            pixmap
        };

        let quality = img.img.sampler.quality;
        if let Some((draw_x, draw_y, width, height)) =
            axis_aligned_device_placement(rect, transform)
        {
            let filter_quality = image_quality_to_filter_quality(quality);
            let device_pixmap = if width == pixmap.width() && height == pixmap.height() {
                pixmap.clone()
            } else {
                let cache_key = ScaledImageCacheKey {
                    image_id: img.img.image.data.id(),
                    width,
                    height,
                    quality: quality as u8,
                };
                try_ret!(cache_scaled_pixmap(
                    self.cache_color,
                    cache_key,
                    &pixmap,
                    quality
                ))
            };
            self.recording.draw_pixmap_direct(
                device_pixmap,
                draw_x,
                draw_y,
                Affine::IDENTITY,
                filter_quality,
            );
            return;
        }

        self.recording
            .draw_pixmap_rect(pixmap, rect, transform, quality);
    }

    fn draw_svg<'b>(
        &mut self,
        svg: floem_renderer::Svg<'b>,
        rect: Rect,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        let coeffs = self.transform.as_coeffs();
        let scale_x = coeffs[0].hypot(coeffs[1]);
        let scale_y = coeffs[2].hypot(coeffs[3]);
        let width = (rect.width() * scale_x.abs()).round().max(1.0) as u32;
        let height = (rect.height() * scale_y.abs()).round().max(1.0) as u32;
        let transform = self.transform;

        if let Some(pixmap) = IMAGE_CACHE.with_borrow_mut(|ic| {
            ic.get_mut(svg.hash).map(|(color, pixmap)| {
                *color = self.cache_color;
                pixmap.clone()
            })
        }) {
            let final_pixmap = self.colorize_pixmap(&pixmap, brush).unwrap_or(pixmap);
            self.recording
                .draw_pixmap_rect(final_pixmap, rect, transform, ImageQuality::High);
            return;
        }

        let mut non_colored_svg = try_ret!(tiny_skia::Pixmap::new(width, height));
        let svg_transform = tiny_skia::Transform::from_scale(
            width as f32 / svg.tree.size().width(),
            height as f32 / svg.tree.size().height(),
        );
        resvg::render(svg.tree, svg_transform, &mut non_colored_svg.as_mut());

        let non_colored_svg = Arc::new(non_colored_svg);
        let final_pixmap = self
            .colorize_pixmap(&non_colored_svg, brush)
            .unwrap_or_else(|| non_colored_svg.clone());
        self.recording
            .draw_pixmap_rect(final_pixmap, rect, transform, ImageQuality::High);

        IMAGE_CACHE.with_borrow_mut(|ic| {
            ic.insert(svg.hash.to_owned(), (self.cache_color, non_colored_svg));
        });
    }

    fn set_transform(&mut self, cumulative_transform: Affine) {
        self.transform = cumulative_transform;
    }

    fn set_z_index(&mut self, _z_index: i32) {
        // FIXME: Remove this method?
    }

    fn clip(&mut self, shape: &impl Shape) {
        if let Some(clip) = self.current_clip_path(shape) {
            self.recording.push_clip(clip);
        }
    }

    fn clear_clip(&mut self) {
        self.recording.pop_clip();
    }

    fn finish(&mut self) -> Option<peniko::ImageBrush> {
        // Remove cache entries which were not accessed.
        IMAGE_CACHE.with_borrow_mut(|ic| ic.retain(|_, (c, _)| *c == self.cache_color));
        SCALED_IMAGE_CACHE.with_borrow_mut(|ic| ic.retain(|_, (c, _)| *c == self.cache_color));
        GLYPH_CACHE.with_borrow_mut(|gc| gc.retain(|_, (c, _)| *c == self.cache_color));

        // Swap the cache color.
        self.cache_color = CacheColor(!self.cache_color.0);

        self.replay_recording();

        if self.capture {
            let pixmap = &self.layers[0].pixmap;
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

        let current_bounds = self.layers[0].draw_bounds;
        let full_bounds = Rect::new(
            0.0,
            0.0,
            self.layers[0].pixmap.width() as f64,
            self.layers[0].pixmap.height() as f64,
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
                .min(self.layers[0].pixmap.width() as i32) as u32;
            let y1 = (copy_bounds.y() + copy_bounds.height() as i32)
                .min(self.layers[0].pixmap.height() as i32) as u32;

            if x0 < x1 && y0 < y1 {
                let pixmap = &self.layers[0].pixmap;
                let width = pixmap.width() as usize;
                for y in y0 as usize..y1 as usize {
                    let row_start = y * width;
                    let src = &pixmap.pixels()[row_start + x0 as usize..row_start + x1 as usize];
                    let dst = &mut buffer[row_start + x0 as usize..row_start + x1 as usize];
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
        let layer_transform = self.transform * transform;
        let Some(path) =
            shape_to_path(clip).and_then(|path| path.transform(affine_to_skia(layer_transform)))
        else {
            return;
        };
        self.recording.push_layer(
            blend.into(),
            alpha,
            ClipPath {
                path,
                rect: layer_transform.transform_rect_bbox(clip.bounding_box()),
                simple_rect: transformed_axis_aligned_rect(clip, layer_transform),
            },
        );
    }

    fn pop_layer(&mut self) {
        self.recording.pop_layer();
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
            draw_layer_region(
                parent,
                &layer.pixmap,
                composite_rect,
                blend_mode,
                layer.alpha,
            );
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
            ImageQuality::Medium,
        );

        assert_eq!(pixel_rgba(&layer, 3, 1), (0, 0, 0, 0));
        assert_eq!(pixel_rgba(&layer, 4, 1), (0, 0, 0, 0));
        assert_eq!(pixel_rgba(&layer, 5, 1), (255, 0, 0, 255));
        assert_eq!(pixel_rgba(&layer, 6, 1), (255, 0, 0, 255));
        assert_eq!(pixel_rgba(&layer, 7, 1), (0, 0, 0, 0));
        assert_eq!(pixel_rgba(&layer, 8, 1), (0, 0, 0, 0));
    }

    #[test]
    fn render_pixmap_rect_detects_exact_device_blit_when_scale_cancels() {
        let rect = Rect::new(1.0, 2.0, 2.0, 3.0);
        let pixmap = Pixmap::new(2, 2).expect("failed to create src pixmap");
        let local_transform = Affine::translate((rect.x0, rect.y0)).then_scale_non_uniform(
            rect.width() / pixmap.width() as f64,
            rect.height() / pixmap.height() as f64,
        );
        let composite_transform = Affine::scale(2.0) * local_transform;

        assert_eq!(
            integer_translation(composite_transform, 0.0, 0.0),
            Some((1, 2))
        );
    }

    #[test]
    fn image_quality_low_maps_to_nearest_filtering() {
        assert_eq!(
            image_quality_to_filter_quality(ImageQuality::Low),
            FilterQuality::Nearest
        );
        assert_eq!(
            image_quality_to_filter_quality(ImageQuality::Medium),
            FilterQuality::Bilinear
        );
        assert_eq!(
            image_quality_to_filter_quality(ImageQuality::High),
            FilterQuality::Bilinear
        );
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

    #[test]
    fn render_pixmap_direct_blends_premultiplied_pixels() {
        let mut layer = make_layer(4, 4);
        layer
            .pixmap
            .fill(tiny_skia::Color::from_rgba8(0, 0, 255, 255));

        let mut src = Pixmap::new(1, 1).expect("failed to create src pixmap");
        src.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 128));

        layer.render_pixmap_direct(&src, 1.0, 1.0, Affine::IDENTITY, FilterQuality::Nearest);

        assert_eq!(pixel_rgba(&layer, 1, 1), (128, 0, 127, 255));
    }

    #[test]
    fn normalized_text_transform_keeps_translation_and_rotation_separate() {
        let transform = Affine::translate((30.0, 20.0))
            * Affine::rotate(std::f64::consts::FRAC_PI_2)
            * Affine::scale(2.0);

        let normalized = normalize_affine(transform, true);
        let (_, _, raster_scale) = affine_scale_components(transform);
        let device_origin = normalized * Point::new(5.0 * raster_scale, 0.0);

        assert!((device_origin.x - 30.0).abs() < 1e-6);
        assert!((device_origin.y - 30.0).abs() < 1e-6);
    }

    #[test]
    fn embolden_strength_scales_with_raster_scale() {
        assert!((scaled_embolden_strength(0.2, 1.5) - 0.3).abs() < f32::EPSILON);
        assert_eq!(scaled_embolden_strength(0.2, 0.0), 0.0);
    }
}
