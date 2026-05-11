//! `StylePropValue` impls for peniko types (`Color`, `Gradient`, `Brush`,
//! `Stroke`) and kurbo types (`Rect`, `Affine`), plus the `AffineLerp` helper
//! used by `Affine`'s interpolate implementation.

use peniko::color::HueDirection;
use peniko::kurbo::{self, Affine, Stroke, Vec2};
use peniko::{
    Brush, Color, ColorStop, ColorStops, Gradient, InterpolationAlphaSpace,
};

use crate::prop_value::{StylePropValue, hash_f64};

impl StylePropValue for Color {
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        for c in self.components {
            c.to_bits().hash(&mut h);
        }
        h.finish()
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(self.lerp(*other, value as f32, HueDirection::default()))
    }
}

impl StylePropValue for Gradient {
    fn interpolate(&self, _other: &Self, _value: f64) -> Option<Self> {
        None
    }

    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        std::mem::discriminant(&self.kind).hash(&mut h);
        for stop in self.stops.iter() {
            stop.offset.to_bits().hash(&mut h);
            for c in stop.color.components {
                c.to_bits().hash(&mut h);
            }
        }
        h.finish()
    }
}

impl StylePropValue for Stroke {
    fn content_hash(&self) -> u64 {
        hash_f64(self.width)
    }
}

impl StylePropValue for Brush {
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        std::mem::discriminant(self).hash(&mut h);
        match self {
            Brush::Solid(c) => {
                for comp in c.components {
                    comp.to_bits().hash(&mut h);
                }
            }
            Brush::Gradient(g) => g.content_hash().hash(&mut h),
            Brush::Image(_) => {}
        }
        h.finish()
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        match (self, other) {
            (Brush::Solid(color), Brush::Solid(other)) => Some(Self::Solid(color.lerp(
                *other,
                value as f32,
                HueDirection::default(),
            ))),
            (Brush::Gradient(gradient), Brush::Solid(solid)) => {
                let interpolated_stops: Vec<ColorStop> = gradient
                    .stops
                    .iter()
                    .map(|stop| {
                        let interpolated_color = stop.color.to_alpha_color().lerp(
                            *solid,
                            value as f32,
                            HueDirection::default(),
                        );
                        ColorStop::from((stop.offset, interpolated_color))
                    })
                    .collect();
                Some(Brush::Gradient(Gradient {
                    kind: gradient.kind,
                    extend: gradient.extend,
                    interpolation_cs: gradient.interpolation_cs,
                    hue_direction: gradient.hue_direction,
                    stops: ColorStops::from(&*interpolated_stops),
                    interpolation_alpha_space: InterpolationAlphaSpace::Premultiplied,
                }))
            }
            (Brush::Solid(solid), Brush::Gradient(gradient)) => {
                let interpolated_stops: Vec<ColorStop> = gradient
                    .stops
                    .iter()
                    .map(|stop| {
                        let interpolated_color = solid.lerp(
                            stop.color.to_alpha_color(),
                            value as f32,
                            HueDirection::default(),
                        );
                        ColorStop::from((stop.offset, interpolated_color))
                    })
                    .collect();
                Some(Brush::Gradient(Gradient {
                    kind: gradient.kind,
                    extend: gradient.extend,
                    interpolation_cs: gradient.interpolation_cs,
                    hue_direction: gradient.hue_direction,
                    stops: ColorStops::from(&*interpolated_stops),
                    interpolation_alpha_space: InterpolationAlphaSpace::Premultiplied,
                }))
            }

            (Brush::Gradient(gradient1), Brush::Gradient(gradient2)) => {
                gradient1.interpolate(gradient2, value).map(Brush::Gradient)
            }
            _ => None,
        }
    }
}

impl StylePropValue for kurbo::Rect {
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        self.x0.to_bits().hash(&mut h);
        self.y0.to_bits().hash(&mut h);
        self.x1.to_bits().hash(&mut h);
        self.y1.to_bits().hash(&mut h);
        h.finish()
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        let lerp = |a: f64, b: f64| a + (b - a) * value;

        Some(Self {
            x0: lerp(self.x0, other.x0),
            y0: lerp(self.y0, other.y0),
            x1: lerp(self.x1, other.x1),
            y1: lerp(self.y1, other.y1),
        })
    }
}

impl StylePropValue for Affine {
    fn interpolate(&self, other: &Self, t: f64) -> Option<Self> {
        Some(self.lerp(other, t))
    }

    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = rustc_hash::FxHasher::default();

        let coeffs = self.as_coeffs();
        for coeff in coeffs {
            coeff.to_bits().hash(&mut hasher);
        }

        hasher.finish()
    }
}

pub trait AffineLerp {
    fn svd(self) -> (Vec2, f64);

    /// Linearly interpolate between two affine transforms.
    ///
    /// This implements the CSS Transforms interpolation algorithm:
    /// - Decompose both transforms into translation, rotation, and scale components
    /// - Interpolate each component separately
    /// - Recompose the result
    ///
    /// `t` should be in the range [0.0, 1.0] where:
    /// - t = 0.0 returns `self`
    /// - t = 1.0 returns `other`
    /// - t = 0.5 returns the midpoint
    fn lerp(&self, other: &Affine, t: f64) -> Affine;
}

impl AffineLerp for Affine {
    fn svd(self) -> (Vec2, f64) {
        let [a, b, c, d, _, _] = self.as_coeffs();
        let a2 = a * a;
        let b2 = b * b;
        let c2 = c * c;
        let d2 = d * d;
        let ab = a * b;
        let cd = c * d;
        let angle = 0.5 * (2.0 * (ab + cd)).atan2(a2 - b2 + c2 - d2);
        let s1 = a2 + b2 + c2 + d2;
        let s2 = ((a2 - b2 + c2 - d2).powi(2) + 4.0 * (ab + cd).powi(2)).sqrt();
        (
            Vec2 {
                x: (0.5 * (s1 + s2)).sqrt(),
                y: (0.5 * (s1 - s2)).sqrt(),
            },
            angle,
        )
    }

    fn lerp(&self, other: &Affine, t: f64) -> Affine {
        // Extract translations
        let trans_a = self.translation();
        let trans_b = other.translation();

        // Remove translations to get the linear parts
        let linear_a = self.with_translation(Vec2::ZERO);
        let linear_b = other.with_translation(Vec2::ZERO);

        // Decompose into scale and rotation using SVD
        let (scale_a, rotation_a) = linear_a.svd();
        let (scale_b, rotation_b) = linear_b.svd();

        // Interpolate translation
        let trans = Vec2 {
            x: trans_a.x + (trans_b.x - trans_a.x) * t,
            y: trans_a.y + (trans_b.y - trans_a.y) * t,
        };

        // Interpolate scale
        let scale = Vec2 {
            x: scale_a.x + (scale_b.x - scale_a.x) * t,
            y: scale_a.y + (scale_b.y - scale_a.y) * t,
        };

        // Interpolate rotation (taking the shorter path)
        let mut angle_diff = rotation_b - rotation_a;
        // Normalize to [-π, π] to take the shorter rotation path
        while angle_diff > std::f64::consts::PI {
            angle_diff -= 2.0 * std::f64::consts::PI;
        }
        while angle_diff < -std::f64::consts::PI {
            angle_diff += 2.0 * std::f64::consts::PI;
        }
        let rotation = rotation_a + angle_diff * t;

        // Recompose: rotate -> scale -> translate
        Affine::rotate(rotation)
            .then_scale_non_uniform(scale.x, scale.y)
            .then_translate(trans)
    }
}

#[cfg(test)]
mod affine_lerp_tests {
    use super::*;
    use peniko::kurbo::Point;

    #[test]
    fn test_lerp_identity() {
        let a = Affine::IDENTITY;
        let b = Affine::translate(Vec2::new(100.0, 50.0));

        let result = a.lerp(&b, 0.0);
        assert_eq!(result.as_coeffs(), a.as_coeffs());

        let result = a.lerp(&b, 1.0);
        assert_eq!(result.as_coeffs(), b.as_coeffs());
    }

    #[test]
    fn test_lerp_translation() {
        let a = Affine::translate(Vec2::new(0.0, 0.0));
        let b = Affine::translate(Vec2::new(100.0, 50.0));

        let result = a.lerp(&b, 0.5);
        let trans = result.translation();
        assert!((trans.x - 50.0).abs() < 1e-10);
        assert!((trans.y - 25.0).abs() < 1e-10);
    }

    #[test]
    fn test_lerp_rotation() {
        let a = Affine::rotate(0.0);
        let b = Affine::rotate(std::f64::consts::PI / 2.0);

        let result = a.lerp(&b, 0.5);
        // Should be rotated by π/4
        let point = result * Point::new(1.0, 0.0);
        let expected_angle = std::f64::consts::PI / 4.0;
        assert!((point.x - expected_angle.cos()).abs() < 1e-10);
        assert!((point.y - expected_angle.sin()).abs() < 1e-10);
    }

    #[test]
    fn test_lerp_scale() {
        let a = Affine::scale(1.0);
        let b = Affine::scale(2.0);

        let result = a.lerp(&b, 0.5);
        let point = result * Point::new(1.0, 1.0);
        assert!((point.x - 1.5).abs() < 1e-10);
        assert!((point.y - 1.5).abs() < 1e-10);
    }
}
