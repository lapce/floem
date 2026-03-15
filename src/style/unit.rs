//! Unit types and helpers for Floem styles.
//!
//! Floem's length model is intentionally small:
//! - [`Pt`] is the primary absolute length unit exposed by the style API.
//! - [`Pct`] resolves against a property-specific basis such as parent size.
//! - [`Em`] resolves against the current element's computed font size.
//! - [`Lh`] resolves against the current element's computed line height.
//!
//! Floem treats absolute lengths as logical layout points rather than device pixels.
//! The renderer later maps those logical values through the window scale factor, so
//! callers should generally think in view-space measurements, not physical screen
//! pixels.
//!
//! Relative text units are scoped to the current element. Floem does not currently
//! define a separate root-level font-size or line-height unit in this module.
//!
#![allow(deprecated)]
use std::{ops::Neg, time::Duration};

pub use floem_renderer::text::LineHeightValue;
use taffy::style::{Dimension, LengthPercentage, LengthPercentageAuto};

/// An absolute length value in Floem layout points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pt(pub f64);

#[deprecated(note = "use Pt instead")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Px(pub f64);

impl std::ops::Add for Px {
    type Output = Px;

    fn add(self, rhs: Px) -> Px {
        Px(self.0 + rhs.0)
    }
}

impl std::ops::AddAssign for Px {
    fn add_assign(&mut self, rhs: Px) {
        *self = *self + rhs;
    }
}

impl std::ops::Mul<f64> for Px {
    type Output = Px;

    fn mul(self, rhs: f64) -> Px {
        Px(self.0 * rhs)
    }
}

impl std::ops::Mul<f32> for Px {
    type Output = Px;

    fn mul(self, rhs: f32) -> Px {
        Px(self.0 * rhs as f64)
    }
}

impl std::ops::Mul<Px> for f64 {
    type Output = Px;

    fn mul(self, rhs: Px) -> Px {
        Px(self * rhs.0)
    }
}

impl std::ops::Mul<Px> for f32 {
    type Output = Px;

    fn mul(self, rhs: Px) -> Px {
        Px(self as f64 * rhs.0)
    }
}

/// A percent value
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pct(pub f64);

/// A length relative to the current element's computed font size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Em(pub f64);

/// A length relative to the current element's computed line height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lh(pub f64);

impl From<f32> for Pct {
    fn from(value: f32) -> Self {
        Pct(value as f64)
    }
}

impl From<f64> for Pct {
    fn from(value: f64) -> Self {
        Pct(value)
    }
}

impl From<i32> for Pct {
    fn from(value: i32) -> Self {
        Pct(value as f64)
    }
}

macro_rules! impl_scalar_unit_from {
    ($unit:ident) => {
        impl From<f32> for $unit {
            fn from(value: f32) -> Self {
                $unit(value as f64)
            }
        }

        impl From<f64> for $unit {
            fn from(value: f64) -> Self {
                $unit(value)
            }
        }

        impl From<i32> for $unit {
            fn from(value: i32) -> Self {
                $unit(value as f64)
            }
        }

        impl From<u32> for $unit {
            fn from(value: u32) -> Self {
                $unit(value as f64)
            }
        }

        impl From<usize> for $unit {
            fn from(value: usize) -> Self {
                $unit(value as f64)
            }
        }
    };
}

impl_scalar_unit_from!(Em);
impl_scalar_unit_from!(Lh);

/// Used for automatically computed values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Auto;

/// An angle value that can be in degrees or radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Angle {
    /// Degrees (0-360)
    Deg(f64),
    /// Radians (0-2π)
    Rad(f64),
}

impl Angle {
    pub const ZERO: Angle = Angle::Rad(0.0);
    pub const QUARTER_TURN: Angle = Angle::Deg(90.0);
    pub const HALF_TURN: Angle = Angle::Deg(180.0);
    pub const FULL_TURN: Angle = Angle::Deg(360.0);

    /// Convert the angle to radians.
    pub fn to_radians(self) -> f64 {
        match self {
            Angle::Deg(deg) => deg.to_radians(),
            Angle::Rad(rad) => rad,
        }
    }

    /// Convert the angle to degrees.
    pub fn to_degrees(self) -> f64 {
        match self {
            Angle::Deg(deg) => deg,
            Angle::Rad(rad) => rad.to_degrees(),
        }
    }

    /// Normalize to [0, 2π) in radians.
    pub fn normalized(self) -> Angle {
        let rad = self.to_radians().rem_euclid(std::f64::consts::TAU);
        Angle::Rad(rad)
    }

    /// Sine of the angle.
    pub fn sin(self) -> f64 {
        self.to_radians().sin()
    }

    /// Cosine of the angle.
    pub fn cos(self) -> f64 {
        self.to_radians().cos()
    }

    /// Tangent of the angle.
    pub fn tan(self) -> f64 {
        self.to_radians().tan()
    }

    /// Returns (sin, cos) efficiently.
    pub fn sin_cos(self) -> (f64, f64) {
        self.to_radians().sin_cos()
    }

    /// Linear interpolation between two angles.
    pub fn lerp(self, other: &Angle, t: f64) -> Angle {
        Angle::Rad(self.to_radians() * (1.0 - t) + other.to_radians() * t)
    }
}

impl Default for Angle {
    fn default() -> Self {
        Angle::ZERO
    }
}

// --- Arithmetic ops (always produce Rad) ---

impl std::ops::Add for Angle {
    type Output = Angle;
    fn add(self, rhs: Angle) -> Angle {
        Angle::Rad(self.to_radians() + rhs.to_radians())
    }
}

impl std::ops::AddAssign for Angle {
    fn add_assign(&mut self, rhs: Angle) {
        *self = *self + rhs;
    }
}

impl std::ops::Sub for Angle {
    type Output = Angle;
    fn sub(self, rhs: Angle) -> Angle {
        Angle::Rad(self.to_radians() - rhs.to_radians())
    }
}

impl std::ops::SubAssign for Angle {
    fn sub_assign(&mut self, rhs: Angle) {
        *self = *self - rhs;
    }
}

impl std::ops::Neg for Angle {
    type Output = Angle;
    fn neg(self) -> Angle {
        Angle::Rad(-self.to_radians())
    }
}

impl std::ops::Mul<f64> for Angle {
    type Output = Angle;
    fn mul(self, rhs: f64) -> Angle {
        Angle::Rad(self.to_radians() * rhs)
    }
}

impl std::ops::Mul<Angle> for f64 {
    type Output = Angle;
    fn mul(self, rhs: Angle) -> Angle {
        Angle::Rad(self * rhs.to_radians())
    }
}

impl std::ops::MulAssign<f64> for Angle {
    fn mul_assign(&mut self, rhs: f64) {
        *self = *self * rhs;
    }
}

impl std::ops::Div<f64> for Angle {
    type Output = Angle;
    fn div(self, rhs: f64) -> Angle {
        Angle::Rad(self.to_radians() / rhs)
    }
}

impl std::ops::DivAssign<f64> for Angle {
    fn div_assign(&mut self, rhs: f64) {
        *self = *self / rhs;
    }
}

/// Dividing two angles gives a dimensionless ratio.
impl std::ops::Div<Angle> for Angle {
    type Output = f64;
    fn div(self, rhs: Angle) -> f64 {
        self.to_radians() / rhs.to_radians()
    }
}

// --- Comparisons (compare by radians value) ---

impl PartialOrd for Angle {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.to_radians().partial_cmp(&other.to_radians())
    }
}

// --- Display ---

impl std::fmt::Display for Angle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Angle::Deg(deg) => write!(f, "{}°", deg),
            Angle::Rad(rad) => write!(f, "{}rad", rad),
        }
    }
}

// --- From conversions ---

impl From<f64> for Angle {
    /// Converts from radians (same convention as std trig functions).
    fn from(radians: f64) -> Self {
        Angle::Rad(radians)
    }
}

impl From<f64> for Pt {
    fn from(value: f64) -> Self {
        Pt(value)
    }
}

impl From<f64> for Px {
    fn from(value: f64) -> Self {
        Px(value)
    }
}

impl From<f32> for Pt {
    fn from(value: f32) -> Self {
        Pt(value as f64)
    }
}

impl From<f32> for Px {
    fn from(value: f32) -> Self {
        Px(value as f64)
    }
}

impl From<i32> for Pt {
    fn from(value: i32) -> Self {
        Pt(value as f64)
    }
}

impl From<i32> for Px {
    fn from(value: i32) -> Self {
        Px(value as f64)
    }
}

impl From<Px> for LineHeightValue {
    fn from(value: Px) -> Self {
        LineHeightValue::Pt(value.0 as f32)
    }
}

impl From<Pt> for LineHeightValue {
    fn from(value: Pt) -> Self {
        LineHeightValue::Pt(value.0 as f32)
    }
}

impl From<Px> for Pt {
    fn from(value: Px) -> Self {
        Pt(value.0)
    }
}

impl From<Pt> for Px {
    fn from(value: Pt) -> Self {
        Px(value.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Pt(f64),
    Pct(f64),
    Em(f64),
    Lh(f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontSizeCx {
    pub font_size: f64,
    pub line_height: f64,
}

impl FontSizeCx {
    pub fn new(font_size: f64, line_height: f64) -> Self {
        Self {
            font_size,
            line_height,
        }
    }
}

impl From<Pct> for Length {
    fn from(value: Pct) -> Self {
        Length::Pct(value.0)
    }
}

impl<T> From<T> for Length
where
    T: Into<Pt>,
{
    fn from(value: T) -> Self {
        Length::Pt(value.into().0)
    }
}

impl From<Em> for Length {
    fn from(value: Em) -> Self {
        Length::Em(value.0)
    }
}

impl From<Lh> for Length {
    fn from(value: Lh) -> Self {
        Length::Lh(value.0)
    }
}

impl Length {
    pub fn resolve(&self, reference: f64, cx: &FontSizeCx) -> f64 {
        match self {
            Length::Pt(v) => *v,
            Length::Pct(v) => reference * v / 100.0,
            Length::Em(v) => cx.font_size * v,
            Length::Lh(v) => cx.line_height * v,
        }
    }

    pub fn to_taffy(&self, cx: &FontSizeCx) -> LengthPercentage {
        match self {
            Length::Pt(v) => LengthPercentage::length(*v as f32),
            Length::Pct(v) => LengthPercentage::percent(*v as f32 / 100.0),
            Length::Em(_) | Length::Lh(_) => LengthPercentage::length(self.resolve(0.0, cx) as f32),
        }
    }
}

#[deprecated(note = "use Length instead")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PxPct {
    Px(f64),
    Pct(f64),
}

impl From<Pct> for PxPct {
    fn from(value: Pct) -> Self {
        PxPct::Pct(value.0)
    }
}

impl<T> From<T> for PxPct
where
    T: Into<Pt>,
{
    fn from(value: T) -> Self {
        PxPct::Px(value.into().0)
    }
}

impl From<PxPct> for Length {
    fn from(value: PxPct) -> Self {
        match value {
            PxPct::Px(v) => Length::Pt(v),
            PxPct::Pct(v) => Length::Pct(v),
        }
    }
}

impl From<Length> for PxPct {
    fn from(value: Length) -> Self {
        match value {
            Length::Pt(v) => PxPct::Px(v),
            Length::Pct(v) => PxPct::Pct(v),
            Length::Em(v) => PxPct::Px(v * 14.),
            Length::Lh(v) => PxPct::Px(v * 14.),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LengthAuto {
    Pt(f64),
    Pct(f64),
    Em(f64),
    Lh(f64),
    Auto,
}

impl LengthAuto {
    pub fn as_pt(&self) -> Option<f64> {
        match self {
            LengthAuto::Pt(v) => Some(*v),
            _ => None,
        }
    }

    pub fn resolve(&self, reference: f64, cx: &FontSizeCx) -> Option<f64> {
        match self {
            LengthAuto::Pt(v) => Some(*v),
            LengthAuto::Pct(v) => Some(reference * v / 100.0),
            LengthAuto::Em(v) => Some(cx.font_size * v),
            LengthAuto::Lh(v) => Some(cx.line_height * v),
            LengthAuto::Auto => None,
        }
    }

    pub fn to_taffy_dim(&self, cx: &FontSizeCx) -> Dimension {
        match self {
            LengthAuto::Pt(v) => Dimension::length(*v as f32),
            LengthAuto::Pct(v) => Dimension::percent(*v as f32 / 100.0),
            LengthAuto::Em(_) | LengthAuto::Lh(_) => {
                Dimension::length(self.resolve(0.0, cx).unwrap_or_default() as f32)
            }
            LengthAuto::Auto => Dimension::auto(),
        }
    }

    pub fn to_taffy_len_perc_auto(&self, cx: &FontSizeCx) -> LengthPercentageAuto {
        match self {
            LengthAuto::Pt(v) => LengthPercentageAuto::length(*v as f32),
            LengthAuto::Pct(v) => LengthPercentageAuto::percent(*v as f32 / 100.0),
            LengthAuto::Em(_) | LengthAuto::Lh(_) => {
                LengthPercentageAuto::length(self.resolve(0.0, cx).unwrap_or_default() as f32)
            }
            LengthAuto::Auto => LengthPercentageAuto::auto(),
        }
    }
}

impl From<Pct> for LengthAuto {
    fn from(value: Pct) -> Self {
        LengthAuto::Pct(value.0)
    }
}

impl From<Auto> for LengthAuto {
    fn from(_: Auto) -> Self {
        LengthAuto::Auto
    }
}

impl Neg for Length {
    type Output = Self;

    fn neg(self) -> Self::Output {
        match self {
            Length::Pt(pt) => Length::Pt(-pt),
            Length::Pct(pct) => Length::Pct(-pct),
            Length::Em(em) => Length::Em(-em),
            Length::Lh(lh) => Length::Lh(-lh),
        }
    }
}

impl<T> From<T> for LengthAuto
where
    T: Into<Pt>,
{
    fn from(value: T) -> Self {
        LengthAuto::Pt(value.into().0)
    }
}

impl From<Em> for LengthAuto {
    fn from(value: Em) -> Self {
        LengthAuto::Em(value.0)
    }
}

impl From<Lh> for LengthAuto {
    fn from(value: Lh) -> Self {
        LengthAuto::Lh(value.0)
    }
}

impl From<Length> for LengthAuto {
    fn from(value: Length) -> Self {
        match value {
            Length::Pct(pct) => LengthAuto::Pct(pct),
            Length::Pt(pt) => LengthAuto::Pt(pt),
            Length::Em(em) => LengthAuto::Em(em),
            Length::Lh(lh) => LengthAuto::Lh(lh),
        }
    }
}

#[deprecated(note = "use LengthAuto instead")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PxPctAuto {
    Px(f64),
    Pct(f64),
    Auto,
}

impl PxPctAuto {
    pub fn as_px(&self) -> Option<f64> {
        match self {
            PxPctAuto::Px(v) => Some(*v),
            _ => None,
        }
    }

    pub fn resolve(&self, reference: f64) -> Option<f64> {
        match self {
            PxPctAuto::Px(v) => Some(*v),
            PxPctAuto::Pct(v) => Some(reference * v / 100.0),
            PxPctAuto::Auto => None,
        }
    }
}

impl From<Pct> for PxPctAuto {
    fn from(value: Pct) -> Self {
        PxPctAuto::Pct(value.0)
    }
}

impl From<Auto> for PxPctAuto {
    fn from(_: Auto) -> Self {
        PxPctAuto::Auto
    }
}

impl Neg for PxPct {
    type Output = Self;

    fn neg(self) -> Self::Output {
        match self {
            PxPct::Px(px) => PxPct::Px(-px),
            PxPct::Pct(pct) => PxPct::Pct(-pct),
        }
    }
}

impl<T> From<T> for PxPctAuto
where
    T: Into<Pt>,
{
    fn from(value: T) -> Self {
        PxPctAuto::Px(value.into().0)
    }
}

impl From<PxPct> for PxPctAuto {
    fn from(value: PxPct) -> Self {
        match value {
            PxPct::Pct(pct) => PxPctAuto::Pct(pct),
            PxPct::Px(px) => PxPctAuto::Px(px),
        }
    }
}

impl From<PxPctAuto> for LengthAuto {
    fn from(value: PxPctAuto) -> Self {
        match value {
            PxPctAuto::Px(v) => LengthAuto::Pt(v),
            PxPctAuto::Pct(v) => LengthAuto::Pct(v),
            PxPctAuto::Auto => LengthAuto::Auto,
        }
    }
}

impl From<LengthAuto> for PxPctAuto {
    fn from(value: LengthAuto) -> Self {
        match value {
            LengthAuto::Pt(v) => PxPctAuto::Px(v),
            LengthAuto::Pct(v) => PxPctAuto::Pct(v),
            LengthAuto::Em(v) => PxPctAuto::Px(v * 14.),
            LengthAuto::Lh(v) => PxPctAuto::Px(v * 14.),
            LengthAuto::Auto => PxPctAuto::Auto,
        }
    }
}

pub trait DurationUnitExt {
    fn minutes(self) -> Duration;
    fn seconds(self) -> Duration;
    fn millis(self) -> Duration;
}
impl DurationUnitExt for u64 {
    /// # Panics
    ///
    /// Panics if `self` minutes would overflow a `Duration` when converted to
    /// seconds.
    fn minutes(self) -> Duration {
        const SECS_PER_MINUTE: u64 = 60;

        if self > u64::MAX / SECS_PER_MINUTE {
            panic!("overflow in DurationUnitExt::minutes for u64");
        }

        Duration::from_secs(self * SECS_PER_MINUTE)
    }

    fn seconds(self) -> Duration {
        Duration::from_secs(self)
    }

    fn millis(self) -> Duration {
        Duration::from_millis(self)
    }
}

pub trait UnitExt {
    /// Create a percentage unit.
    ///
    /// The final basis depends on the property being styled.
    fn pct(self) -> Pct;

    /// Create an absolute Floem point unit.
    ///
    /// Floem resolves points in logical view space. The renderer applies the
    /// window scale factor later when drawing to the target surface.
    fn pt(self) -> Pt;

    /// Create a unit relative to the current element's computed font size.
    fn em(self) -> Em;

    /// Create a unit relative to the current element's computed line height.
    fn lh(self) -> Lh;

    #[deprecated(note = "use .pt() instead")]
    /// Deprecated alias for [`UnitExt::pt`].
    fn px(self) -> Pt;

    /// Create an angle expressed in degrees.
    fn deg(self) -> Angle;

    /// Create an angle expressed in radians.
    fn rad(self) -> Angle;
}

macro_rules! impl_unit_ext {
    ($ty:ty) => {
        impl UnitExt for $ty {
            fn pct(self) -> Pct {
                Pct(self as f64)
            }

            fn pt(self) -> Pt {
                Pt(self as f64)
            }

            fn em(self) -> Em {
                Em(self as f64)
            }

            fn lh(self) -> Lh {
                Lh(self as f64)
            }

            fn px(self) -> Pt {
                Pt(self as f64)
            }

            fn deg(self) -> Angle {
                Angle::Deg(self as f64)
            }

            fn rad(self) -> Angle {
                Angle::Rad(self as f64)
            }
        }
    };
}

impl_unit_ext!(f32);
impl_unit_ext!(f64);
impl_unit_ext!(i32);
impl_unit_ext!(u32);
impl_unit_ext!(usize);

impl From<PxPctAuto> for Dimension {
    fn from(value: PxPctAuto) -> Self {
        // PxPctAuto cannot encode em/lh-relative units, so a zeroed font context is
        // semantically neutral here and cannot affect the resolved result.
        LengthAuto::from(value).to_taffy_dim(&FontSizeCx::new(0.0, 0.0))
    }
}

impl From<PxPct> for LengthPercentage {
    fn from(value: PxPct) -> Self {
        Length::from(value).to_taffy(&FontSizeCx::new(0., 0.))
    }
}

impl From<PxPctAuto> for LengthPercentageAuto {
    fn from(value: PxPctAuto) -> Self {
        // PxPctAuto cannot encode em/lh-relative units, so a zeroed font context is
        // semantically neutral here and cannot affect the resolved result.
        LengthAuto::from(value).to_taffy_len_perc_auto(&FontSizeCx::new(0.0, 0.0))
    }
}

/// Anchor point for transform-origin, used with rotate and scale transformations.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AnchorAbout {
    /// X coordinate as percentage (0.0 = left, 0.5 = center, 1.0 = right)
    pub x: f64,
    /// Y coordinate as percentage (0.0 = top, 0.5 = center, 1.0 = bottom)
    pub y: f64,
}

impl AnchorAbout {
    /// Center anchor point (default)
    pub const CENTER: Self = Self { x: 0.5, y: 0.5 };
    /// Top-left corner
    pub const TOP_LEFT: Self = Self { x: 0.0, y: 0.0 };
    /// Top-right corner
    pub const TOP_RIGHT: Self = Self { x: 1.0, y: 0.0 };
    /// Bottom-left corner
    pub const BOTTOM_LEFT: Self = Self { x: 0.0, y: 1.0 };
    /// Bottom-right corner
    pub const BOTTOM_RIGHT: Self = Self { x: 1.0, y: 1.0 };

    /// Returns the anchor point as fractions (0.0 to 1.0)
    pub fn as_fractions(&self) -> (f64, f64) {
        (self.x, self.y)
    }
}
