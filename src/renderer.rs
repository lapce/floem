use std::sync::Arc;

use anyrender::*;
use peniko::{
    kurbo::{Affine, Rect, Shape, Stroke},
    *,
};

macro_rules! define_renderer_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $enum_name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident($renderer_type:ty) with feature $feature:literal,
            )*
        }
    ) => {
        $(#[$meta])*
        $vis enum $enum_name {
            $(
                $(#[$variant_meta])*
                #[cfg(feature = $feature)]
                $variant($renderer_type),
            )*
        }

        $vis enum AnyScenePainter<'a> {
            $(
                #[cfg(feature = $feature)]
                $variant(&'a mut <$renderer_type as WindowRenderer>::ScenePainter<'a>),
            )*
        }

        impl WindowRenderer for $enum_name {
            type ScenePainter<'a> = AnyScenePainter<'a>
            where
                Self: 'a;

            fn resume(&mut self, window: Arc<dyn WindowHandle>, width: u32, height: u32) {
                match self {
                    $(
                        #[cfg(feature = $feature)]
                        Self::$variant(r) => r.resume(window, width, height),
                    )*
                }
            }

            fn suspend(&mut self) {
                match self {
                    $(
                        #[cfg(feature = $feature)]
                        Self::$variant(r) => r.suspend(),
                    )*
                }
            }

            fn is_active(&self) -> bool {
                match self {
                    $(
                        #[cfg(feature = $feature)]
                        Self::$variant(r) => r.is_active(),
                    )*
                }
            }

            fn set_size(&mut self, width: u32, height: u32) {
                match self {
                    $(
                        #[cfg(feature = $feature)]
                        Self::$variant(r) => r.set_size(width, height),
                    )*
                }
            }

            fn render<F: FnOnce(&mut Self::ScenePainter<'_>)>(&mut self, draw_fn: F) {
                match self {
                    $(
                        #[cfg(feature = $feature)]
                        Self::$variant(r) => r.render(|p| {
                            let mut wrapper = AnyScenePainter::$variant(p);
                            draw_fn(&mut wrapper);
                        }),
                    )*
                }
            }
        }

        impl<'a> PaintScene for AnyScenePainter<'a> {
            fn reset(&mut self) {
                match self {
                    $(
                        #[cfg(feature = $feature)]
                        Self::$variant(p) => p.reset(),
                    )*
                }
            }

            fn push_layer(
                &mut self,
                blend: impl Into<BlendMode>,
                alpha: f32,
                transform: Affine,
                clip: &impl Shape,
            ) {
                match self {
                    $(
                        #[cfg(feature = $feature)]
                        Self::$variant(p) => p.push_layer(blend, alpha, transform, clip),
                    )*
                }
            }

            fn push_clip_layer(&mut self, transform: Affine, clip: &impl Shape) {
                match self {
                    $(
                        #[cfg(feature = $feature)]
                        Self::$variant(p) => p.push_clip_layer(transform, clip),
                    )*
                }
            }

            fn pop_layer(&mut self) {
                match self {
                    $(
                        #[cfg(feature = $feature)]
                        Self::$variant(p) => p.pop_layer(),
                    )*
                }
            }

            fn stroke<'b>(
                &mut self,
                style: &Stroke,
                transform: Affine,
                brush: impl Into<PaintRef<'b>>,
                brush_transform: Option<Affine>,
                shape: &impl Shape,
            ) {
                match self {
                    $(
                        #[cfg(feature = $feature)]
                        Self::$variant(p) => p.stroke(style, transform, brush, brush_transform, shape),
                    )*
                }
            }

            fn fill<'b>(
                &mut self,
                style: Fill,
                transform: Affine,
                brush: impl Into<PaintRef<'b>>,
                brush_transform: Option<Affine>,
                shape: &impl Shape,
            ) {
                match self {
                    $(
                        #[cfg(feature = $feature)]
                        Self::$variant(p) => p.fill(style, transform, brush, brush_transform, shape),
                    )*
                }
            }

            fn draw_glyphs<'b, 's: 'b>(
                &'s mut self,
                font: &'b FontData,
                font_size: f32,
                hint: bool,
                normalized_coords: &'b [NormalizedCoord],
                embolden: kurbo::Vec2,
                style: impl Into<StyleRef<'b>>,
                brush: impl Into<PaintRef<'b>>,
                brush_alpha: f32,
                transform: Affine,
                glyph_transform: Option<Affine>,
                glyphs: impl Iterator<Item = Glyph>,
            ) {
                match self {
                    $(
                        #[cfg(feature = $feature)]
                        Self::$variant(p) => p.draw_glyphs(
                            font,
                            font_size,
                            hint,
                            normalized_coords,
                            embolden,
                            style,
                            brush,
                            brush_alpha,
                            transform,
                            glyph_transform,
                            glyphs,
                        ),
                    )*
                }
            }

            fn draw_box_shadow(
                &mut self,
                transform: Affine,
                rect: Rect,
                brush: Color,
                radius: f64,
                std_dev: f64,
            ) {
                match self {
                    $(
                        #[cfg(feature = $feature)]
                        Self::$variant(p) => p.draw_box_shadow(transform, rect, brush, radius, std_dev),
                    )*
                }
            }

            fn draw_image(&mut self, image: ImageBrushRef, transform: Affine) {
                match self {
                    $(
                        #[cfg(feature = $feature)]
                        Self::$variant(p) => p.draw_image(image, transform),
                    )*
                }
            }
        }
    };
}

pub enum AnyWindowRenderer {
    VelloClassic(anyrender_vello::VelloWindowRenderer),
}
pub enum AnyScenePainter<'a> {
    VelloClassic(
        &'a mut <anyrender_vello::VelloWindowRenderer as WindowRenderer>::ScenePainter<'a>,
    ),
}
impl WindowRenderer for AnyWindowRenderer {
    type ScenePainter<'a>
        = AnyScenePainter<'a>
    where
        Self: 'a;
    fn resume(&mut self, window: Arc<dyn WindowHandle>, width: u32, height: u32) {
        match self {
            Self::VelloClassic(r) => r.resume(window, width, height),
        }
    }
    fn suspend(&mut self) {
        match self {
            Self::VelloClassic(r) => r.suspend(),
        }
    }
    fn is_active(&self) -> bool {
        match self {
            Self::VelloClassic(r) => r.is_active(),
        }
    }
    fn set_size(&mut self, width: u32, height: u32) {
        match self {
            Self::VelloClassic(r) => r.set_size(width, height),
        }
    }
    fn render<F: FnOnce(&mut Self::ScenePainter<'_>)>(&mut self, draw_fn: F) {
        match self {
            Self::VelloClassic(r) => r.render(|p| {
                let mut wrapper = AnyScenePainter::VelloClassic(p);
                draw_fn(&mut wrapper);
            }),
        }
    }
}
impl<'a> PaintScene for AnyScenePainter<'a> {
    fn reset(&mut self) {
        match self {
            Self::VelloClassic(p) => p.reset(),
        }
    }
    fn push_layer(
        &mut self,
        blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        match self {
            Self::VelloClassic(p) => p.push_layer(blend, alpha, transform, clip),
        }
    }
    fn push_clip_layer(&mut self, transform: Affine, clip: &impl Shape) {
        match self {
            Self::VelloClassic(p) => p.push_clip_layer(transform, clip),
        }
    }
    fn pop_layer(&mut self) {
        match self {
            Self::VelloClassic(p) => p.pop_layer(),
        }
    }
    fn stroke<'b>(
        &mut self,
        style: &Stroke,
        transform: Affine,
        brush: impl Into<PaintRef<'b>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        match self {
            Self::VelloClassic(p) => p.stroke(style, transform, brush, brush_transform, shape),
        }
    }
    fn fill<'b>(
        &mut self,
        style: Fill,
        transform: Affine,
        brush: impl Into<PaintRef<'b>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        match self {
            Self::VelloClassic(p) => p.fill(style, transform, brush, brush_transform, shape),
        }
    }
    fn draw_glyphs<'b, 's: 'b>(
        &'s mut self,
        font: &'b FontData,
        font_size: f32,
        hint: bool,
        normalized_coords: &'b [NormalizedCoord],
        embolden: kurbo::Vec2,
        style: impl Into<StyleRef<'b>>,
        brush: impl Into<PaintRef<'b>>,
        brush_alpha: f32,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: impl Iterator<Item = Glyph>,
    ) {
        match self {
            Self::VelloClassic(p) => p.draw_glyphs(
                font,
                font_size,
                hint,
                normalized_coords,
                embolden,
                style,
                brush,
                brush_alpha,
                transform,
                glyph_transform,
                glyphs,
            ),
        }
    }
    fn draw_box_shadow(
        &mut self,
        transform: Affine,
        rect: Rect,
        brush: Color,
        radius: f64,
        std_dev: f64,
    ) {
        match self {
            Self::VelloClassic(p) => p.draw_box_shadow(transform, rect, brush, radius, std_dev),
        }
    }
    fn draw_image(&mut self, image: ImageBrushRef, transform: Affine) {
        match self {
            Self::VelloClassic(p) => p.draw_image(image, transform),
        }
    }
}
