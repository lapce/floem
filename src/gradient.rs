//! Floem gradient values.
//!
//! Floem keeps a parallel gradient type so color stops can use Floem units.
//! The value lowers to Peniko only once paint bounds are known.

use std::ops::{Deref, DerefMut};

use peniko::{
    Extend, InterpolationAlphaSpace,
    color::{AlphaColor, ColorSpace, ColorSpaceTag, DynamicColor, HueDirection, OpaqueColor},
    kurbo::{Point, Rect},
};

use crate::unit::{FontSizeCx, Length, Pct};

const DEFAULT_GRADIENT_COLOR_SPACE: ColorSpaceTag = ColorSpaceTag::Srgb;

/// Bounds-resolved point used by Floem gradients.
///
/// Numeric tuple coordinates are absolute Floem points. Use `pct()` or
/// [`Length`] for coordinates relative to the bounds of the painted geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientPoint {
    pub x: Length,
    pub y: Length,
}

impl GradientPoint {
    #[must_use]
    pub fn resolve(self, bounds: Rect, font_size: &FontSizeCx) -> Point {
        Point::new(
            bounds.x0 + self.x.resolve(bounds.width(), font_size),
            bounds.y0 + self.y.resolve(bounds.height(), font_size),
        )
    }
}

impl From<Point> for GradientPoint {
    fn from(value: Point) -> Self {
        Self {
            x: Length::Pt(value.x),
            y: Length::Pt(value.y),
        }
    }
}

impl<X, Y> From<(X, Y)> for GradientPoint
where
    X: Into<Length>,
    Y: Into<Length>,
{
    fn from(value: (X, Y)) -> Self {
        Self {
            x: value.0.into(),
            y: value.1.into(),
        }
    }
}

/// Offset and color of a transition point in a Floem gradient.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorStop {
    pub offset: Length,
    pub color: DynamicColor,
}

impl ColorStop {
    #[must_use]
    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self.color = self.color.with_alpha(alpha);
        self
    }

    #[must_use]
    pub fn multiply_alpha(mut self, alpha: f32) -> Self {
        self.color = self.color.multiply_alpha(alpha);
        self
    }
}

macro_rules! impl_color_stop_tuple {
    ($offset:ty, $to_length:expr) => {
        impl<CS: ColorSpace> From<($offset, AlphaColor<CS>)> for ColorStop {
            fn from(pair: ($offset, AlphaColor<CS>)) -> Self {
                Self {
                    offset: $to_length(pair.0),
                    color: DynamicColor::from_alpha_color(pair.1),
                }
            }
        }

        impl From<($offset, DynamicColor)> for ColorStop {
            fn from(pair: ($offset, DynamicColor)) -> Self {
                Self {
                    offset: $to_length(pair.0),
                    color: pair.1,
                }
            }
        }

        impl<CS: ColorSpace> From<($offset, OpaqueColor<CS>)> for ColorStop {
            fn from(pair: ($offset, OpaqueColor<CS>)) -> Self {
                Self {
                    offset: $to_length(pair.0),
                    color: DynamicColor::from_alpha_color(pair.1.with_alpha(1.0)),
                }
            }
        }
    };
}

impl_color_stop_tuple!(Length, |offset| offset);
impl_color_stop_tuple!(Pct, |offset: Pct| Length::Pct(offset.0));
impl_color_stop_tuple!(f32, |offset: f32| Length::Pct(f64::from(offset) * 100.0));
impl_color_stop_tuple!(f64, |offset: f64| Length::Pct(offset * 100.0));
impl_color_stop_tuple!(i32, |offset: i32| Length::Pct(f64::from(offset) * 100.0));

impl From<peniko::ColorStop> for ColorStop {
    fn from(stop: peniko::ColorStop) -> Self {
        Self {
            offset: Length::Pct(f64::from(stop.offset) * 100.0),
            color: stop.color,
        }
    }
}

/// Collection of Floem gradient stops.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColorStops(pub smallvec::SmallVec<[ColorStop; 4]>);

impl Deref for ColorStops {
    type Target = smallvec::SmallVec<[ColorStop; 4]>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ColorStops {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<&[ColorStop]> for ColorStops {
    fn from(value: &[ColorStop]) -> Self {
        Self(value.into())
    }
}

impl From<peniko::ColorStops> for ColorStops {
    fn from(value: peniko::ColorStops) -> Self {
        Self(value.iter().cloned().map(Into::into).collect())
    }
}

impl From<&peniko::ColorStops> for ColorStops {
    fn from(value: &peniko::ColorStops) -> Self {
        Self(value.iter().cloned().map(Into::into).collect())
    }
}

/// Source of Floem gradient stops.
pub trait ColorStopsSource {
    fn collect_stops(self, stops: &mut ColorStops);
}

impl<T> ColorStopsSource for &'_ [T]
where
    T: Into<ColorStop> + Clone,
{
    fn collect_stops(self, stops: &mut ColorStops) {
        stops.extend(self.iter().cloned().map(Into::into));
    }
}

impl<T, const N: usize> ColorStopsSource for [T; N]
where
    T: Into<ColorStop>,
{
    fn collect_stops(self, stops: &mut ColorStops) {
        stops.extend(self.into_iter().map(Into::into));
    }
}

impl<CS: ColorSpace> ColorStopsSource for &'_ [AlphaColor<CS>] {
    fn collect_stops(self, stops: &mut ColorStops) {
        if !self.is_empty() {
            let denom = (self.len() - 1).max(1) as f32;
            stops.extend(self.iter().enumerate().map(|(i, c)| ColorStop {
                offset: Length::Pct(f64::from((i as f32) / denom) * 100.0),
                color: DynamicColor::from_alpha_color(*c),
            }));
        }
    }
}

impl<CS: ColorSpace, const N: usize> ColorStopsSource for [AlphaColor<CS>; N] {
    fn collect_stops(self, stops: &mut ColorStops) {
        self.as_slice().collect_stops(stops);
    }
}

impl ColorStopsSource for &'_ [DynamicColor] {
    fn collect_stops(self, stops: &mut ColorStops) {
        if !self.is_empty() {
            let denom = (self.len() - 1).max(1) as f32;
            stops.extend(self.iter().enumerate().map(|(i, c)| ColorStop {
                offset: Length::Pct(f64::from((i as f32) / denom) * 100.0),
                color: *c,
            }));
        }
    }
}

impl<const N: usize> ColorStopsSource for [DynamicColor; N] {
    fn collect_stops(self, stops: &mut ColorStops) {
        self.as_slice().collect_stops(stops);
    }
}

impl<CS: ColorSpace> ColorStopsSource for &'_ [OpaqueColor<CS>] {
    fn collect_stops(self, stops: &mut ColorStops) {
        if !self.is_empty() {
            let denom = (self.len() - 1).max(1) as f32;
            stops.extend(self.iter().enumerate().map(|(i, c)| ColorStop {
                offset: Length::Pct(f64::from((i as f32) / denom) * 100.0),
                color: DynamicColor::from_alpha_color(c.with_alpha(1.0)),
            }));
        }
    }
}

impl<CS: ColorSpace, const N: usize> ColorStopsSource for [OpaqueColor<CS>; N] {
    fn collect_stops(self, stops: &mut ColorStops) {
        self.as_slice().collect_stops(stops);
    }
}

/// Floem gradient definition.
///
/// This is intentionally separate from [`peniko::Gradient`]. Peniko gradients
/// are renderer-ready values with concrete geometry and normalized color-stop
/// offsets. Floem gradients can keep geometry and stops as [`Length`] values,
/// then resolve them against the bounds of the fill or stroke geometry while
/// lowering the display list. For a background, that geometry is the full view
/// bounds; for explicit painter fills/strokes, it is the painted shape's bounds.
///
/// Numeric stop offsets keep Peniko semantics: `0.0` is the start and `1.0` is
/// the end. Use `pct()`/`Length::Pct` when the stop should be expressed as a
/// Floem length:
///
/// ```
/// # use floem::{Gradient, kurbo::Point, peniko::Color, prelude::*};
/// # let red = Color::from_rgb8(255, 0, 0);
/// # let blue = Color::from_rgb8(0, 0, 255);
/// let gradient = Gradient::new_linear((0.0.pct(), 0.0.pct()), (100.0.pct(), 0.0.pct()))
///     .with_stops([
///         (0.0.pct(), red),
///         (100.0.pct(), blue),
///     ]);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub kind: peniko::GradientKind,
    pub extend: Extend,
    pub interpolation_cs: ColorSpaceTag,
    pub hue_direction: HueDirection,
    pub interpolation_alpha_space: InterpolationAlphaSpace,
    pub stops: ColorStops,
    pub(crate) geometry: GradientGeometry,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum GradientGeometry {
    #[default]
    Absolute,
    Linear {
        start: GradientPoint,
        end: GradientPoint,
    },
    Radial {
        center: GradientPoint,
    },
    TwoPointRadial {
        start_center: GradientPoint,
        end_center: GradientPoint,
    },
    Sweep {
        center: GradientPoint,
    },
}

impl Default for Gradient {
    fn default() -> Self {
        Self {
            kind: peniko::LinearGradientPosition {
                start: Point::default(),
                end: Point::default(),
            }
            .into(),
            extend: Extend::default(),
            interpolation_cs: DEFAULT_GRADIENT_COLOR_SPACE,
            hue_direction: HueDirection::default(),
            interpolation_alpha_space: InterpolationAlphaSpace::default(),
            stops: ColorStops::default(),
            geometry: GradientGeometry::Absolute,
        }
    }
}

impl Gradient {
    pub fn new_linear(start: impl Into<GradientPoint>, end: impl Into<GradientPoint>) -> Self {
        let start = start.into();
        let end = end.into();
        let kind = peniko::LinearGradientPosition::new(
            start.resolve(Rect::ZERO, &FontSizeCx::new(14.0, 16.0)),
            end.resolve(Rect::ZERO, &FontSizeCx::new(14.0, 16.0)),
        )
        .into();
        Self {
            kind,
            geometry: GradientGeometry::Linear { start, end },
            ..Self::default()
        }
    }

    pub fn new_radial(center: impl Into<GradientPoint>, radius: f32) -> Self {
        let center = center.into();
        Self {
            kind: peniko::RadialGradientPosition::new(
                center.resolve(Rect::ZERO, &FontSizeCx::new(14.0, 16.0)),
                radius,
            )
            .into(),
            geometry: GradientGeometry::Radial { center },
            ..Self::default()
        }
    }

    pub fn new_two_point_radial(
        start_center: impl Into<GradientPoint>,
        start_radius: f32,
        end_center: impl Into<GradientPoint>,
        end_radius: f32,
    ) -> Self {
        let start_center = start_center.into();
        let end_center = end_center.into();
        Self {
            kind: peniko::RadialGradientPosition::new_two_point(
                start_center.resolve(Rect::ZERO, &FontSizeCx::new(14.0, 16.0)),
                start_radius,
                end_center.resolve(Rect::ZERO, &FontSizeCx::new(14.0, 16.0)),
                end_radius,
            )
            .into(),
            geometry: GradientGeometry::TwoPointRadial {
                start_center,
                end_center,
            },
            ..Self::default()
        }
    }

    pub fn new_sweep(center: impl Into<GradientPoint>, start_angle: f32, end_angle: f32) -> Self {
        let center = center.into();
        Self {
            kind: peniko::SweepGradientPosition::new(
                center.resolve(Rect::ZERO, &FontSizeCx::new(14.0, 16.0)),
                start_angle,
                end_angle,
            )
            .into(),
            geometry: GradientGeometry::Sweep { center },
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_extend(mut self, mode: Extend) -> Self {
        self.extend = mode;
        self
    }

    #[must_use]
    pub fn with_interpolation_cs(mut self, interpolation_cs: ColorSpaceTag) -> Self {
        self.interpolation_cs = interpolation_cs;
        self
    }

    #[must_use]
    pub fn with_interpolation_alpha_space(
        mut self,
        interpolation_alpha_space: InterpolationAlphaSpace,
    ) -> Self {
        self.interpolation_alpha_space = interpolation_alpha_space;
        self
    }

    #[must_use]
    pub fn with_hue_direction(mut self, hue_direction: HueDirection) -> Self {
        self.hue_direction = hue_direction;
        self
    }

    #[must_use]
    pub fn with_stops(mut self, stops: impl ColorStopsSource) -> Self {
        self.stops.clear();
        stops.collect_stops(&mut self.stops);
        self
    }

    #[must_use]
    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self.stops
            .iter_mut()
            .for_each(|stop| *stop = stop.clone().with_alpha(alpha));
        self
    }

    #[must_use]
    pub fn multiply_alpha(mut self, alpha: f32) -> Self {
        self.stops
            .iter_mut()
            .for_each(|stop| *stop = stop.clone().multiply_alpha(alpha));
        self
    }

    pub(crate) fn to_peniko(&self, bounds: Rect, font_size: &FontSizeCx) -> peniko::Gradient {
        let kind = match (self.kind, self.geometry) {
            (peniko::GradientKind::Linear(_), GradientGeometry::Linear { start, end }) => {
                peniko::LinearGradientPosition::new(
                    start.resolve(bounds, font_size),
                    end.resolve(bounds, font_size),
                )
                .into()
            }
            (peniko::GradientKind::Radial(position), GradientGeometry::Radial { center }) => {
                peniko::RadialGradientPosition::new(
                    center.resolve(bounds, font_size),
                    position.end_radius,
                )
                .into()
            }
            (
                peniko::GradientKind::Radial(position),
                GradientGeometry::TwoPointRadial {
                    start_center,
                    end_center,
                },
            ) => peniko::RadialGradientPosition::new_two_point(
                start_center.resolve(bounds, font_size),
                position.start_radius,
                end_center.resolve(bounds, font_size),
                position.end_radius,
            )
            .into(),
            (peniko::GradientKind::Sweep(position), GradientGeometry::Sweep { center }) => {
                peniko::SweepGradientPosition::new(
                    center.resolve(bounds, font_size),
                    position.start_angle,
                    position.end_angle,
                )
                .into()
            }
            (kind, _) => kind,
        };
        let reference = match kind {
            peniko::GradientKind::Linear(position) => position.start.distance(position.end),
            peniko::GradientKind::Radial(position) => f64::from(position.end_radius.max(1.0)),
            peniko::GradientKind::Sweep(_) => bounds.size().min_side().max(1.0),
        };
        let stops = self
            .stops
            .iter()
            .map(|stop| peniko::ColorStop {
                offset: if reference <= f64::EPSILON {
                    0.0
                } else {
                    (stop.offset.resolve(reference, font_size) / reference) as f32
                },
                color: stop.color,
            })
            .collect::<smallvec::SmallVec<[peniko::ColorStop; 4]>>();
        peniko::Gradient {
            kind,
            extend: self.extend,
            interpolation_cs: self.interpolation_cs,
            hue_direction: self.hue_direction,
            interpolation_alpha_space: self.interpolation_alpha_space,
            stops: peniko::ColorStops(stops),
        }
    }
}

impl From<peniko::Gradient> for Gradient {
    fn from(value: peniko::Gradient) -> Self {
        Self {
            kind: value.kind,
            extend: value.extend,
            interpolation_cs: value.interpolation_cs,
            hue_direction: value.hue_direction,
            interpolation_alpha_space: value.interpolation_alpha_space,
            stops: ColorStops::from(value.stops),
            geometry: GradientGeometry::Absolute,
        }
    }
}
