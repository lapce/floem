use anyhow::Context;
use floem_renderer::text::{LayoutRun, FONT_SYSTEM};
use floem_renderer::{Img, Renderer, Svg};
use gl::types::GLint;
use glutin::config::{ConfigTemplateBuilder, GetGlConfig};
use glutin::context::{ContextAttributesBuilder, PossiblyCurrentContext};
use glutin::display::{Display, DisplayApiPreference, GetGlDisplay};
use glutin::prelude::{
    GlConfig, GlDisplay, GlSurface, NotCurrentGlContext, PossiblyCurrentGlContext,
};
use glutin::surface::{SurfaceAttributesBuilder, WindowSurface};
use peniko::kurbo::{Affine, PathEl, Point, Rect, Shape, Size, Stroke};
use peniko::{kurbo, BlendMode, BrushRef, Compose, Image, Mix};
use skia_safe::canvas::{GlyphPositions, SaveLayerRec, SetMatrix};
use skia_safe::gpu::gl::FramebufferInfo;
use skia_safe::gpu::{backend_render_targets, SurfaceOrigin};
use skia_safe::image_filters::{blur, CropRect};
use std::collections::HashMap;
use std::ffi::CString;
use std::num::NonZeroU32;

pub struct SkiaRenderer<W> {
    surface: skia_safe::Surface,
    gl_surface: glutin::surface::Surface<WindowSurface>,
    gr_context: skia_safe::gpu::DirectContext,
    gl_context: PossiblyCurrentContext,
    fb_info: FramebufferInfo,
    font_mgr: skia_safe::FontMgr,
    typeface_cache: HashMap<floem_renderer::text::fontdb::ID, skia_safe::Typeface>,
    window: W,
}

impl<W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle> SkiaRenderer<W> {
    pub fn new(window: W, width: u32, height: u32, scale: f64) -> anyhow::Result<Self> {
        let raw_display_handle = window.display_handle()?.as_raw();
        let raw_window_handle = window.window_handle()?.as_raw();

        let gl_display = unsafe { Display::new(raw_display_handle, DisplayApiPreference::Egl)? };

        let gl_config_template = ConfigTemplateBuilder::new().with_transparency(true).build();
        let gl_config = unsafe {
            gl_display
                .find_configs(gl_config_template)?
                .reduce(|accum, config| {
                    let transparency_check = config.supports_transparency().unwrap_or(false)
                        & !accum.supports_transparency().unwrap_or(false);

                    if transparency_check || config.num_samples() < accum.num_samples() {
                        config
                    } else {
                        accum
                    }
                })
                .with_context(|| "Could not find a matching GL config")?
        };

        let gl_context_attrs = ContextAttributesBuilder::new().build(Some(raw_window_handle));
        let gl_surface_attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            raw_window_handle,
            NonZeroU32::new(width).with_context(|| "Width should be a positive value")?,
            NonZeroU32::new(height).with_context(|| "Height should be a positive value")?,
        );

        let gl_not_current_context =
            unsafe { gl_display.create_context(&gl_config, &gl_context_attrs)? };

        let gl_surface = unsafe {
            gl_config
                .display()
                .create_window_surface(&gl_config, &gl_surface_attrs)
        }?;

        let gl_context = gl_not_current_context
            .make_current(&gl_surface)
            .with_context(|| "Could not make GL context current when setting up skia renderer")?;

        gl::load_with(|s| {
            gl_config
                .display()
                .get_proc_address(CString::new(s).unwrap().as_c_str())
        });

        let interface = skia_safe::gpu::gl::Interface::new_load_with(|name| {
            if name == "eglGetCurrentDisplay" {
                return std::ptr::null();
            }
            gl_config
                .display()
                .get_proc_address(CString::new(name).unwrap().as_c_str())
        })
        .with_context(|| "Could not create interface")?;

        let mut gr_context = skia_safe::gpu::direct_contexts::make_gl(interface, None)
            .with_context(|| "Could not create direct context")?;

        let fb_info = {
            let mut fboid: GLint = 0;
            unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };

            FramebufferInfo {
                fboid: fboid.try_into()?,
                format: skia_safe::gpu::gl::Format::RGBA8.into(),
                ..Default::default()
            }
        };

        let num_samples = gl_context.config().num_samples() as usize;
        let stencil_size = gl_context.config().stencil_size() as usize;

        let surface = Self::create_surface(
            width as i32,
            height as i32,
            fb_info,
            &mut gr_context,
            num_samples,
            stencil_size,
        );

        Ok(SkiaRenderer {
            surface,
            gl_surface,
            gr_context,
            gl_context,
            fb_info,
            font_mgr: skia_safe::FontMgr::new(),
            typeface_cache: HashMap::new(),
            window,
        })
    }

    fn create_surface(
        width: i32,
        height: i32,
        fb_info: FramebufferInfo,
        gr_context: &mut skia_safe::gpu::DirectContext,
        num_samples: usize,
        stencil_size: usize,
    ) -> skia_safe::Surface {
        let backend_render_target =
            backend_render_targets::make_gl((width, height), num_samples, stencil_size, fb_info);

        skia_safe::gpu::surfaces::wrap_backend_render_target(
            gr_context,
            &backend_render_target,
            SurfaceOrigin::BottomLeft,
            skia_safe::ColorType::RGBA8888,
            None,
            None,
        )
        .expect("Could not create skia surface")
    }

    pub fn resize(&mut self, width: u32, height: u32, scale: f64) {
        self.surface = Self::create_surface(
            width as i32,
            height as i32,
            self.fb_info,
            &mut self.gr_context,
            self.gl_context.config().num_samples() as usize,
            self.gl_context.config().stencil_size() as usize,
        );
        self.gl_surface.resize(
            &self.gl_context,
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
        );
    }

    pub fn set_scale(&mut self, scale: f64) {
        // ToDo: implement scale
    }

    pub fn scale(&self) -> f64 {
        // ToDo: implement scale
        1.0
    }

    pub fn size(&self) -> Size {
        Size::new(0f64, 0f64)
    }

    pub fn debug_info(&self) -> String {
        // ToDo: implement debug info
        String::new()
    }
}

impl<W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle> Renderer
    for SkiaRenderer<W>
{
    fn begin(&mut self, _capture: bool) {
        self.surface.canvas().restore_to_count(1);
        self.surface.canvas().clear(skia_safe::Color::WHITE);
    }

    fn set_transform(&mut self, transform: Affine) {
        self.surface
            .canvas()
            .set_matrix(&skia_safe::M44::from(kurbo_affine_to_skia_matrix(
                transform,
            )));
    }

    fn set_z_index(&mut self, z_index: i32) {
        // ToDo: implement z index
    }

    fn clip(&mut self, shape: &impl Shape) {
        self.surface.canvas().save();
        self.surface.canvas().clip_path(
            &kurbo_shape_to_skia_path(shape),
            skia_safe::ClipOp::Intersect,
            None,
        );
    }

    fn clear_clip(&mut self) {
        self.surface.canvas().restore_to_count(1);
    }

    fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s Stroke,
    ) {
        let mut paint = peniko_brush_to_skia_paint(brush);
        paint.set_anti_alias(true);
        paint.set_style(skia_safe::paint::Style::Stroke);
        paint.set_stroke_width(stroke.width as f32);
        draw_kurbo_shape_to_skia_canvas(self.surface.canvas(), shape, &paint);
    }

    fn fill<'b>(&mut self, path: &impl Shape, brush: impl Into<BrushRef<'b>>, blur_radius: f64) {
        let mut paint = peniko_brush_to_skia_paint(brush);
        paint.set_anti_alias(true);
        paint.set_style(skia_safe::paint::Style::Fill);
        if blur_radius > 0.0 {
            let image_filter = blur(
                (blur_radius as f32, blur_radius as f32),
                None,
                None,
                CropRect::default(),
            );
            paint.set_image_filter(image_filter);
        }

        draw_kurbo_shape_to_skia_canvas(self.surface.canvas(), path, &paint);
    }

    fn push_layer(
        &mut self,
        blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        self.surface.canvas().save();

        let clip_path: skia_safe::Path = kurbo_shape_to_skia_path(clip);
        self.surface.canvas().clip_path(&clip_path, None, None);

        let blend = blend.into();

        let mut paint = skia_safe::Paint::new(skia_safe::Color4f::new(1.0, 1.0, 1.0, alpha), None);
        paint.set_anti_alias(true);
        paint.set_blend_mode(peniko_blend_to_skia_blend(blend));

        let rec = SaveLayerRec::default().paint(&paint);

        let matrix = kurbo_affine_to_skia_matrix(transform);
        self.surface.canvas().concat(&matrix);

        self.surface.canvas().save_layer(&rec);
    }

    fn pop_layer(&mut self) {
        self.surface.canvas().restore();
    }

    fn draw_text_with_layout<'b>(
        &mut self,
        layout: impl Iterator<Item = LayoutRun<'b>>,
        pos: impl Into<Point>,
    ) {
        let origin = pos.into();
        let mut paint = skia_safe::Paint::new(skia_safe::colors::WHITE, None);
        paint.set_anti_alias(true);

        for run in layout {
            if run.glyphs.is_empty() {
                continue;
            }

            let line_pos = skia_safe::Point::new(origin.x as f32, origin.y as f32 + run.line_y);

            for glyph in run.glyphs {
                let typeface_cached = self.typeface_cache.get(&glyph.font_id);
                let typeface = match typeface_cached {
                    Some(it) => it.clone(),
                    None => {
                        let typeface = FONT_SYSTEM
                            .lock()
                            .db()
                            .with_face_data(glyph.font_id, |it, index| {
                                self.font_mgr.new_from_data(it, index as usize).unwrap()
                            })
                            .unwrap()
                            .clone();
                        self.typeface_cache.insert(glyph.font_id, typeface.clone());
                        typeface
                    }
                };

                let color = match glyph.color_opt {
                    Some(it) => skia_safe::Color4f::from(it.0),
                    None => skia_safe::colors::BLACK,
                };
                paint.set_color4f(color, None);

                self.surface.canvas().draw_glyphs_at(
                    &[skia_safe::GlyphId::from(glyph.glyph_id)],
                    GlyphPositions::Points(&[skia_safe::Point::new(glyph.x, glyph.y)]),
                    line_pos,
                    &skia_safe::Font::from_typeface(typeface, glyph.font_size),
                    &paint,
                )
            }
        }
    }

    fn draw_svg<'b>(&mut self, svg: Svg<'b>, rect: Rect, brush: Option<impl Into<BrushRef<'b>>>) {
        // ToDo: implement svg
    }

    fn draw_img(&mut self, img: Img<'_>, rect: Rect) {
        // ToDo: implement img
    }

    fn finish(&mut self) -> Option<Image> {
        self.gr_context.flush_and_submit();
        self.gl_surface.swap_buffers(&self.gl_context).unwrap();
        self.surface.canvas().discard();
        None
    }
}

fn draw_kurbo_shape_to_skia_canvas(
    canvas: &skia_safe::Canvas,
    shape: &impl Shape,
    paint: &skia_safe::Paint,
) {
    if let Some(rect) = shape.as_rect() {
        canvas.draw_rect(
            skia_safe::Rect::from_xywh(
                rect.x0 as f32,
                rect.y0 as f32,
                rect.x1 as f32,
                rect.y1 as f32,
            ),
            paint,
        );
    } else if let Some(rrect) = shape.as_rounded_rect() {
        let rect = skia_safe::Rect::from_xywh(
            rrect.rect().x0 as f32,
            rrect.rect().y0 as f32,
            rrect.rect().x1 as f32,
            rrect.rect().y1 as f32,
        );
        canvas.draw_rrect(
            skia_safe::RRect::new_nine_patch(
                rect,
                rrect.radii().bottom_left as f32,
                rrect.radii().top_left as f32,
                rrect.radii().top_right as f32,
                rrect.radii().bottom_right as f32,
            ),
            paint,
        );
    } else if let Some(line) = shape.as_line() {
        canvas.draw_line(
            (line.p0.x as f32, line.p1.x as f32),
            (line.p1.x as f32, line.p1.x as f32),
            paint,
        );
    } else if let Some(circle) = shape.as_circle() {
        canvas.draw_circle(
            (circle.center.x as f32, circle.center.y as f32),
            circle.radius as f32,
            paint,
        );
    } else if let Some(path_els) = shape.as_path_slice() {
        canvas.draw_path(&bezpath_els_to_skia_path(path_els), paint);
    } else {
        canvas.draw_path(&kurbo_shape_to_skia_path(shape), paint);
    }
}

fn kurbo_affine_to_skia_matrix(affine: Affine) -> skia_safe::Matrix {
    let m = affine.as_coeffs();
    let scale_x = m[0] as f32;
    let shear_y = m[1] as f32;
    let shear_x = m[2] as f32;
    let scale_y = m[3] as f32;
    let translate_x = m[4] as f32;
    let translate_y = m[5] as f32;

    skia_safe::Matrix::new_all(
        scale_x,
        shear_x,
        translate_x,
        shear_y,
        scale_y,
        translate_y,
        0.0,
        0.0,
        1.0,
    )
}

fn peniko_blend_to_skia_blend(blend_mode: BlendMode) -> skia_safe::BlendMode {
    if blend_mode.mix == Mix::Normal || blend_mode.mix == Mix::Clip {
        match blend_mode.compose {
            Compose::Clear => skia_safe::BlendMode::Clear,
            Compose::Copy => skia_safe::BlendMode::Src,
            Compose::Dest => skia_safe::BlendMode::Dst,
            Compose::SrcOver => skia_safe::BlendMode::SrcOver,
            Compose::DestOver => skia_safe::BlendMode::DstOver,
            Compose::SrcIn => skia_safe::BlendMode::SrcIn,
            Compose::DestIn => skia_safe::BlendMode::DstIn,
            Compose::SrcOut => skia_safe::BlendMode::SrcOut,
            Compose::DestOut => skia_safe::BlendMode::DstOut,
            Compose::SrcAtop => skia_safe::BlendMode::SrcATop,
            Compose::DestAtop => skia_safe::BlendMode::DstATop,
            Compose::Xor => skia_safe::BlendMode::Xor,
            Compose::Plus => skia_safe::BlendMode::Plus,
            Compose::PlusLighter => skia_safe::BlendMode::Plus,
        }
    } else {
        match blend_mode.mix {
            Mix::Normal => unreachable!(), // Handled above
            Mix::Multiply => skia_safe::BlendMode::Multiply,
            Mix::Screen => skia_safe::BlendMode::Screen,
            Mix::Overlay => skia_safe::BlendMode::Overlay,
            Mix::Darken => skia_safe::BlendMode::Darken,
            Mix::Lighten => skia_safe::BlendMode::Lighten,
            Mix::ColorDodge => skia_safe::BlendMode::ColorDodge,
            Mix::ColorBurn => skia_safe::BlendMode::ColorBurn,
            Mix::HardLight => skia_safe::BlendMode::HardLight,
            Mix::SoftLight => skia_safe::BlendMode::SoftLight,
            Mix::Difference => skia_safe::BlendMode::Difference,
            Mix::Exclusion => skia_safe::BlendMode::Exclusion,
            Mix::Hue => skia_safe::BlendMode::Hue,
            Mix::Saturation => skia_safe::BlendMode::Saturation,
            Mix::Color => skia_safe::BlendMode::Color,
            Mix::Luminosity => skia_safe::BlendMode::Luminosity,
            Mix::Clip => unreachable!(), // Handled above
        }
    }
}

fn peniko_brush_to_skia_paint<'b>(brush: impl Into<BrushRef<'b>>) -> skia_safe::Paint {
    let brush = brush.into();
    match brush {
        BrushRef::Solid(color) => {
            let mut paint = skia_safe::Paint::new(
                skia_safe::Color4f::new(
                    color.components[0],
                    color.components[1],
                    color.components[2],
                    color.components[3],
                ),
                None,
            );
            paint.set_style(skia_safe::paint::Style::Fill);
            paint
        }
        BrushRef::Gradient(gradient) => skia_safe::Paint::default(), // ToDo: implement gradient using paint shader
        BrushRef::Image(image) => skia_safe::Paint::default(), // ToDo: implement image using paint texture
    }
}

fn kurbo_shape_to_skia_path(shape: &impl kurbo::Shape) -> skia_safe::Path {
    let mut sk_path = skia_safe::Path::new();
    for el in shape.path_elements(0.1) {
        add_bezpath_el_to_skia_path(&el, &mut sk_path);
    }
    sk_path
}

fn bezpath_to_skia_path(path: &kurbo::BezPath) -> skia_safe::Path {
    bezpath_els_to_skia_path(path.elements())
}

fn bezpath_els_to_skia_path(path: &[PathEl]) -> skia_safe::Path {
    let mut sk_path = skia_safe::Path::new();
    for el in path {
        add_bezpath_el_to_skia_path(&el, &mut sk_path);
    }
    sk_path
}

fn add_bezpath_el_to_skia_path(path_el: &PathEl, skia_path: &mut skia_safe::Path) {
    match path_el {
        PathEl::MoveTo(p) => _ = skia_path.move_to(skpt(*p)),
        PathEl::LineTo(p) => _ = skia_path.line_to(skpt(*p)),
        PathEl::QuadTo(p1, p2) => _ = skia_path.quad_to(skpt(*p1), skpt(*p2)),
        PathEl::CurveTo(p1, p2, p3) => _ = skia_path.cubic_to(skpt(*p1), skpt(*p2), skpt(*p3)),
        PathEl::ClosePath => _ = skia_path.close(),
    };
}

fn skpt(p: Point) -> skia_safe::Point {
    (p.x as f32, p.y as f32).into()
}
