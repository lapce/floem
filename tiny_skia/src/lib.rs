use anyhow::{anyhow, Result};
use floem_renderer::text::TextLayout;
use floem_renderer::tiny_skia::{
    self, FillRule, FilterQuality, GradientStop, LinearGradient, Mask, MaskType, Paint, Path,
    PathBuilder, Pattern, Pixmap, RadialGradient, Shader, SpreadMode, Stroke, Transform,
};
use floem_renderer::Img;
use floem_renderer::Renderer;
use parley::layout::PositionedLayoutItem;
use peniko::kurbo::{PathEl, Size};
use peniko::{
    kurbo::{Affine, Point, Rect, Shape},
    BrushRef, Color, GradientKind,
};
use peniko::{BlendMode, Compose, Mix, RadialGradientPosition};
use resvg::tiny_skia::StrokeDash;
use softbuffer::{Context, Surface};
use std::cell::RefCell;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::Format;
use swash::FontRef;
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

#[derive(PartialEq, Clone, Copy)]
struct CacheColor(bool);

struct Layer {
    pixmap: Pixmap,
    /// clip is stored with the transform at the time clip is called
    clip: Option<Rect>,
    mask: Mask,
    /// this transform should generally only be used when making a draw call to skia
    transform: Affine,
    // the transform that the layer was pushed with that will be used when applying the layer
    combine_transform: Affine,
    blend_mode: BlendMode,
    alpha: f32,
    window_scale: f64,
    cache_color: CacheColor,
}
impl Layer {
    fn device_transform(&self) -> Affine {
        Affine::scale(self.window_scale) * self.transform
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
            if scale_x != 0.0 { device[0] / scale_x } else { 0.0 },
            if scale_x != 0.0 { device[1] / scale_x } else { 0.0 },
            if scale_y != 0.0 { device[2] / scale_y } else { 0.0 },
            if scale_y != 0.0 { device[3] / scale_y } else { 0.0 },
            tx,
            ty,
        ])
    }

    /// the img_rect should already be in the correct transformed space along with the window_scale applied
    fn clip_rect(&self, img_rect: Rect) -> Option<tiny_skia::Rect> {
        if let Some(clip) = self.clip {
            let clip = clip.intersect(img_rect);
            to_skia_rect(clip)
        } else {
            to_skia_rect(img_rect)
        }
    }

    /// Renders the pixmap at the position and transforms it with the given transform.
    /// x and y should have already been scaled by the window scale
    fn render_pixmap_direct(&mut self, img_pixmap: &Pixmap, x: f32, y: f32, transform: Affine) {
        let img_rect = Rect::from_origin_size(
            (x, y),
            (img_pixmap.width() as f64, img_pixmap.height() as f64),
        );
        let paint = Paint {
            shader: Pattern::new(
                img_pixmap.as_ref(),
                SpreadMode::Pad,
                FilterQuality::Nearest,
                1.0,
                Transform::from_translate(x, y),
            ),
            ..Default::default()
        };

        let transform = transform.as_coeffs();
        let transform = Transform::from_row(
            transform[0] as f32,
            transform[1] as f32,
            transform[2] as f32,
            transform[3] as f32,
            transform[4] as f32,
            transform[5] as f32,
        );
        if let Some(rect) = self.clip_rect(img_rect) {
            self.pixmap.fill_rect(rect, &paint, transform, None);
        }
    }

    fn render_pixmap_rect(
        &mut self,
        pixmap: &Pixmap,
        rect: tiny_skia::Rect,
        transform: Transform,
    ) {
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
            transform,
            self.clip.is_some().then_some(&self.mask),
        );
    }

    fn render_pixmap_with_paint(
        &mut self,
        pixmap: &Pixmap,
        rect: tiny_skia::Rect,
        transform: Transform,
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
    /// The combine transform should be the transform that the layer is pushed with without combining with the previous transform. It will be used when combining layers to offset/transform this layer into the parent with the parent transform
    fn new(
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        combine_transform: Affine,
        clip: &impl Shape,
        window_scale: f64,
        cache_color: CacheColor,
    ) -> Result<Self, anyhow::Error> {
        let transform = Affine::IDENTITY;
        let bbox = clip.bounding_box();
        let scaled_box = Affine::scale(window_scale).transform_rect_bbox(bbox);
        let width = scaled_box.width() as u32;
        let height = scaled_box.height() as u32;
        let mut mask = Mask::new(width, height).ok_or_else(|| anyhow!("unable to create mask"))?;
        mask.fill_path(
            &shape_to_path(clip).ok_or_else(|| anyhow!("unable to create clip shape"))?,
            FillRule::Winding,
            false,
            Transform::from_scale(window_scale as f32, window_scale as f32),
        );
        Ok(Self {
            pixmap: Pixmap::new(width, height).ok_or_else(|| anyhow!("unable to create pixmap"))?,
            mask,
            clip: Some(bbox),
            transform,
            combine_transform,
            blend_mode: blend.into(),
            alpha,
            window_scale,
            cache_color,
        })
    }

    fn transform(&mut self, transform: Affine) {
        self.transform *= transform;
    }

    fn clip(&mut self, shape: &impl Shape) {
        self.clip = Some(self.device_transform().transform_rect_bbox(shape.bounding_box()));
        let path = try_ret!(shape_to_path(shape));
        self.mask.clear();
        self.mask
            .fill_path(&path, FillRule::Winding, false, self.skia_transform());
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

        let paint = try_ret!(brush_to_paint(brush));
        if let Some(rect) = shape.as_rect() {
            let rect = try_ret!(to_skia_rect(rect));
            self.pixmap.fill_rect(
                rect,
                &paint,
                self.skia_transform(),
                self.clip.is_some().then_some(&self.mask),
            );
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

    fn draw_text(&mut self, text_layout: &TextLayout, pos: impl Into<Point>, font_embolden: f32) {
        let pos: Point = pos.into();
        let clip = self.clip;
        let (_, _, raster_scale) = self.scale_components();
        let pos = self.device_transform() * pos;
        let transform = self.normalized_linear_transform(false);

        let layout = text_layout.parley_layout();

        for line in layout.lines() {
            let metrics = line.metrics();
            if let Some(rect) = clip {
                let y = pos.y + metrics.baseline as f64 * raster_scale;
                if y + (metrics.line_height as f64) < rect.y0 {
                    continue;
                }
                if y - (metrics.line_height as f64) > rect.y1 {
                    break;
                }
            }

            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };

                let run = glyph_run.run();
                let font = run.font();
                let font_size = run.font_size();
                let synthesis = run.synthesis();
                let normalized_coords = run.normalized_coords();
                let style = glyph_run.style();
                let brush_color: Color = style.brush.0;

                let font_ref = match FontRef::from_index(font.data.data(), font.index as usize) {
                    Some(f) => f,
                    None => continue,
                };

                let font_blob_id = font.data.id();
                // Extra embolden strength when Parley requests synthetic bold
                // (font lacks a native bold variant). Additive so it's always
                // distinguishable from the base `font_embolden` weight.
                const SYNTHESIS_EMBOLDEN_STRENGTH: f32 = 0.02;
                let embolden_strength = font_embolden
                    + if synthesis.embolden() {
                        SYNTHESIS_EMBOLDEN_STRENGTH
                    } else {
                        0.0
                    };
                let skew = synthesis.skew();

                for glyph in glyph_run.positioned_glyphs() {
                    let glyph_x = pos.x as f32 + glyph.x * raster_scale as f32;
                    let glyph_y = pos.y as f32 + glyph.y * raster_scale as f32;

                    if let Some(rect) = clip {
                        if glyph_x as f64 > rect.x1 {
                            break;
                        }
                    }

                    let scaled_font_size = font_size * raster_scale as f32;

                    let (cache_key, new_x, new_y) = GlyphCacheKey::new(
                        font_blob_id,
                        font.index,
                        glyph.id as u16,
                        scaled_font_size,
                        glyph_x,
                        glyph_y,
                        synthesis.embolden(),
                        skew,
                    );

                    let cached = cache_glyph(
                        self.cache_color,
                        cache_key,
                        brush_color,
                        &font_ref,
                        scaled_font_size,
                        normalized_coords,
                        embolden_strength,
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
        }
    }

    fn draw_svg<'b>(
        &mut self,
        svg: floem_renderer::Svg<'b>,
        rect: Rect,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        let (scale_x, scale_y, _) = self.scale_components();
        let device_transform = self.device_transform();
        let normalized_transform = affine_to_skia(self.normalized_linear_transform(false));
        let origin = device_transform * rect.origin();
        let width = (rect.width() * scale_x.abs()).round().max(1.0) as u32;
        let height = (rect.height() * scale_y.abs()).round().max(1.0) as u32;
        let rect = try_ret!(tiny_skia::Rect::from_xywh(
            origin.x as f32,
            origin.y as f32,
            width as f32,
            height as f32,
        ));

        let paint = brush.and_then(|brush| brush_to_paint(brush));

        if IMAGE_CACHE.with_borrow_mut(|ic| {
            if let Some((color, non_colored_svg_pixmap)) = ic.get_mut(svg.hash) {
                *color = self.cache_color;
                let pixmap = non_colored_svg_pixmap.clone();
                self.render_pixmap_with_paint(&pixmap, rect, normalized_transform, paint.clone());
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

        self.render_pixmap_with_paint(&non_colored_svg, rect, normalized_transform, paint);

        IMAGE_CACHE.with_borrow_mut(|ic| {
            ic.insert(
                svg.hash.to_owned(),
                (self.cache_color, Rc::new(non_colored_svg)),
            )
        });
    }

    fn draw_img(&mut self, img: Img<'_>, rect: Rect) {
        let (scale_x, scale_y, _) = self.scale_components();
        let device_transform = self.device_transform();
        let normalized_transform = affine_to_skia(self.normalized_linear_transform(false));
        let origin = device_transform * rect.origin();
        let rect = try_ret!(tiny_skia::Rect::from_xywh(
            origin.x as f32,
            origin.y as f32,
            (rect.width() * scale_x.abs()).max(1.0) as f32,
            (rect.height() * scale_y.abs()).max(1.0) as f32,
        ));
        if IMAGE_CACHE.with_borrow_mut(|ic| {
            if let Some((color, pixmap)) = ic.get_mut(img.hash) {
                *color = self.cache_color;
                let pixmap = pixmap.clone();
                self.render_pixmap_rect(&pixmap, rect, normalized_transform);
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

        self.render_pixmap_rect(&pixmap, rect, normalized_transform);

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
    window_scale: f64,
    layers: Vec<Layer>,
    font_embolden: f32,
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

        let main_layer = Layer {
            pixmap,
            mask,
            clip: None,
            alpha: 1.,
            transform: Affine::IDENTITY,
            combine_transform: Affine::IDENTITY,
            blend_mode: Mix::Normal.into(),
            window_scale: scale,
            cache_color: CacheColor(false),
        };
        Ok(Self {
            context,
            surface,
            transform: Affine::IDENTITY,
            window_scale: scale,
            cache_color: CacheColor(false),
            layers: vec![main_layer],
            font_embolden,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32, scale: f64) {
        if width != self.layers[0].pixmap.width() || height != self.layers[0].pixmap.width() {
            self.surface
                .resize(
                    NonZeroU32::new(width).unwrap_or(NonZeroU32::new(1).unwrap()),
                    NonZeroU32::new(height).unwrap_or(NonZeroU32::new(1).unwrap()),
                )
                .expect("failed to resize surface");
            self.layers[0].pixmap = Pixmap::new(width, height).expect("unable to create pixmap");
            self.layers[0].mask = Mask::new(width, height).expect("unable to create mask");
        }
        self.layers[0].window_scale = scale;
        self.window_scale = scale;
    }

    pub fn set_scale(&mut self, scale: f64) {
        self.layers[0].window_scale = scale;
        self.window_scale = scale;
    }

    pub fn scale(&self) -> f64 {
        self.window_scale
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

impl<W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle> Renderer
    for TinySkiaRenderer<W>
{
    fn begin(&mut self, _capture: bool) {
        assert!(self.layers.len() == 1);
        let first_layer = self.layers.last_mut().unwrap();
        first_layer.pixmap.fill(tiny_skia::Color::WHITE);
        first_layer.clip = None;
        first_layer.transform = Affine::IDENTITY;
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

    fn draw_text(&mut self, layout: &TextLayout, pos: impl Into<Point>) {
        self.layers
            .last_mut()
            .unwrap()
            .draw_text(layout, pos, self.font_embolden);
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
        self.layers.iter_mut().for_each(|l| l.clip(shape));
    }

    fn clear_clip(&mut self) {
        self.layers.iter_mut().for_each(|l| l.clear_clip());
    }

    fn finish(&mut self) -> Option<peniko::ImageBrush> {
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
        for (out_pixel, pixel) in
            (buffer.iter_mut()).zip(self.layers.last().unwrap().pixmap.pixels().iter())
        {
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
        if let Ok(res) = Layer::new(
            blend,
            alpha,
            transform,
            clip,
            self.window_scale,
            self.cache_color,
        ) {
            self.layers.push(res);
        }
    }

    fn pop_layer(&mut self) {
        if self.layers.len() <= 1 {
            // Don't pop the main layer
            return;
        }

        let layer = self.layers.pop().unwrap();
        let parent = self.layers.last_mut().unwrap();

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

fn apply_layer(layer: &Layer, parent: &mut Layer) {
    match determine_blend_strategy(&layer.blend_mode) {
        BlendStrategy::SinglePass(blend_mode) => {
            let mut paint = Paint {
                blend_mode,
                anti_alias: true,
                ..Default::default()
            };

            let transform = skia_transform_with_scaled_translation(
                parent.transform * layer.combine_transform,
                layer.window_scale as f32,
                1.,
            );

            let layer_pattern = Pattern::new(
                layer.pixmap.as_ref(),
                SpreadMode::Pad,
                FilterQuality::Bilinear,
                layer.alpha,
                Transform::identity(),
            );

            paint.shader = layer_pattern;

            let layer_rect = try_ret!(tiny_skia::Rect::from_xywh(
                0.0,
                0.0,
                layer.pixmap.width() as f32,
                layer.pixmap.height() as f32,
            ));

            parent.pixmap.fill_rect(
                layer_rect,
                &paint,
                transform,
                parent.clip.is_some().then_some(&parent.mask),
            );
        }
        BlendStrategy::MultiPass {
            first_pass,
            second_pass,
        } => {
            let original_parent = parent.pixmap.clone();

            let mut paint = Paint {
                blend_mode: first_pass,
                anti_alias: true,
                ..Default::default()
            };

            let transform = skia_transform_with_scaled_translation(
                parent.transform * layer.combine_transform,
                layer.window_scale as f32,
                1.,
            );
            let layer_pattern = Pattern::new(
                layer.pixmap.as_ref(),
                SpreadMode::Pad,
                FilterQuality::Bilinear,
                1.0,
                Transform::identity(),
            );

            paint.shader = layer_pattern;

            let layer_rect = try_ret!(tiny_skia::Rect::from_xywh(
                0.0,
                0.0,
                layer.pixmap.width() as f32,
                layer.pixmap.height() as f32,
            ));

            parent.pixmap.fill_rect(
                layer_rect,
                &paint,
                transform,
                parent.clip.is_some().then_some(&parent.mask),
            );

            let intermediate = parent.pixmap.clone();

            parent.pixmap = original_parent;

            let mut paint = Paint {
                blend_mode: second_pass,
                anti_alias: true,
                ..Default::default()
            };

            let intermediate_pattern = Pattern::new(
                intermediate.as_ref(),
                SpreadMode::Pad,
                FilterQuality::Bilinear,
                1.0,
                Transform::identity(),
            );

            paint.shader = intermediate_pattern;

            parent.pixmap.fill_rect(
                layer_rect,
                &paint,
                transform,
                parent.clip.is_some().then_some(&parent.mask),
            )
        }
    }
    parent.transform *= layer.transform;
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

fn skia_transform_with_scaled_translation(
    affine: Affine,
    translation_scale: f32,
    render_scale: f32,
) -> Transform {
    let transform = affine.as_coeffs();
    Transform::from_row(
        transform[0] as f32,
        transform[1] as f32,
        transform[2] as f32,
        transform[3] as f32,
        transform[4] as f32 * translation_scale,
        transform[5] as f32 * translation_scale,
    )
    .post_scale(render_scale, render_scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use floem_renderer::text::{Attrs, AttrsList, FamilyOwned, FONT_CONTEXT};
    use std::sync::Once;

    const DEJAVU_SERIF: &[u8] = include_bytes!("../../examples/webgpu/fonts/DejaVuSerif.ttf");

    static FONT_INIT: Once = Once::new();

    fn ensure_font() {
        FONT_INIT.call_once(|| {
            let mut font_cx = FONT_CONTEXT.lock();
            font_cx
                .collection
                .register_fonts(DEJAVU_SERIF.to_vec().into(), None);
        });
    }

    /// Creates a `Layer` directly without a window, for offscreen rendering.
    fn make_layer(width: u32, height: u32) -> Layer {
        Layer {
            pixmap: Pixmap::new(width, height).expect("failed to create pixmap"),
            clip: None,
            mask: Mask::new(width, height).expect("failed to create mask"),
            transform: Affine::IDENTITY,
            combine_transform: Affine::IDENTITY,
            blend_mode: Mix::Normal.into(),
            alpha: 1.0,
            window_scale: 1.0,
            cache_color: CacheColor(false),
        }
    }

    fn make_attrs(size: f32) -> AttrsList {
        let family = vec![FamilyOwned::Name("DejaVu Serif".into())];
        AttrsList::new(Attrs::new().font_size(size).family(&family))
    }

    /// Clears the thread-local glyph cache so that subsequent draws at a
    /// different embolden strength are not served stale rasterizations.
    /// Needed because the cache key stores `embolden: bool` (synthesis flag)
    /// but not the float strength value.
    fn clear_glyph_cache() {
        GLYPH_CACHE.with_borrow_mut(|gc| gc.clear());
    }

    /// Visual test: renders text at three embolden strengths onto a single PNG.
    ///
    /// Run with:
    /// ```text
    /// cargo test -p floem_tiny_skia_renderer -- --ignored visual_embolden
    /// ```
    /// Output: `target/test_embolden.png`
    #[test]
    #[ignore]
    fn visual_embolden() {
        ensure_font();

        let width = 550u32;
        let height = 250u32;
        let mut layer = make_layer(width, height);
        layer.pixmap.fill(tiny_skia::Color::WHITE);

        let font_size = 28.0;
        let attrs = make_attrs(font_size);
        let sample = "The quick brown fox jumps";

        // Row 1: normal (no embolden)
        let label = TextLayout::new_with_text("Normal:", make_attrs(14.0), None);
        layer.draw_text(&label, Point::new(10.0, 10.0), 0.0);
        let text = TextLayout::new_with_text(sample, attrs.clone(), None);
        layer.draw_text(&text, Point::new(10.0, 35.0), 0.0);

        clear_glyph_cache();

        // Row 2: moderate embolden (0.05)
        let label = TextLayout::new_with_text("Emboldened (0.05):", make_attrs(14.0), None);
        layer.draw_text(&label, Point::new(10.0, 100.0), 0.0);
        let text = TextLayout::new_with_text(sample, attrs.clone(), None);
        layer.draw_text(&text, Point::new(10.0, 125.0), 0.05);

        clear_glyph_cache();

        // Row 3: strong embolden (0.10)
        let label = TextLayout::new_with_text("Emboldened (0.10):", make_attrs(14.0), None);
        layer.draw_text(&label, Point::new(10.0, 190.0), 0.0);
        let text = TextLayout::new_with_text(sample, attrs, None);
        layer.draw_text(&text, Point::new(10.0, 215.0), 0.10);

        // Save to workspace target directory.
        let out_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("target")
            .join("test_embolden.png");
        layer
            .pixmap
            .save_png(&out_path)
            .expect("failed to save PNG");
        eprintln!("Saved embolden visual test to: {}", out_path.display());
    }
}
