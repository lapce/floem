use cosmic_text::{CacheKey, CacheKeyFlags, SwashImage};
use swash::{
    scale::{Render, ScaleContext, Source, StrikeWith},
    zeno::{Angle, Format, Transform, Vector},
};

use crate::text::FONT_SYSTEM;

const IS_MACOS: bool = cfg!(target_os = "macos");

pub struct SwashScaler {
    context: ScaleContext,
    font_embolden: f32,
}

impl Default for SwashScaler {
    fn default() -> Self {
        Self {
            context: ScaleContext::new(),
            font_embolden: 0.,
        }
    }
}

impl SwashScaler {
    pub fn new(font_embolden: f32) -> Self {
        Self {
            context: ScaleContext::new(),
            font_embolden,
        }
    }

    pub fn get_image(&mut self, cache_key: CacheKey) -> Option<SwashImage> {
        let font = match FONT_SYSTEM.lock().get_font(cache_key.font_id) {
            Some(some) => some,
            None => {
                return None;
            }
        };

        // Build the scaler
        let mut scaler = self
            .context
            .builder(font.as_swash())
            .size(f32::from_bits(cache_key.font_size_bits))
            .hint(!IS_MACOS)
            .build();

        // Compute the fractional offset-- you'll likely want to quantize this
        // in a real renderer
        let offset = Vector::new(cache_key.x_bin.as_float(), cache_key.y_bin.as_float());

        // Select our source order
        Render::new(&[
            // Color outline with the first palette
            Source::ColorOutline(0),
            // Color bitmap with best fit selection mode
            Source::ColorBitmap(StrikeWith::BestFit),
            // Standard scalable outline
            Source::Outline,
        ])
        // Select a subpixel format
        .format(Format::Alpha)
        // Apply the fractional offset
        .offset(offset)
        .embolden(self.font_embolden)
        .transform(if cache_key.flags.contains(CacheKeyFlags::FAKE_ITALIC) {
            Some(Transform::skew(
                Angle::from_degrees(14.0),
                Angle::from_degrees(0.0),
            ))
        } else {
            None
        })
        // Render the image
        .render(&mut scaler, cache_key.glyph_id)
    }
}
