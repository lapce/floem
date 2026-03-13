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
        LineHeightValue::Px(value.0 as f32)
    }
}

impl From<Pt> for LineHeightValue {
    fn from(value: Px) -> Self {
        LineHeightValue::Px(value.0 as f32)
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LengthAuto {
    Pt(f64),
    Pct(f64),
    Auto,
}

impl LengthAuto {
    pub fn as_pt(&self) -> Option<f64> {
        match self {
            LengthAuto::Pt(v) => Some(*v),
            _ => None,
        }
    }

    pub fn resolve(&self, reference: f64) -> Option<f64> {
        match self {
            LengthAuto::Pt(v) => Some(*v),
            LengthAuto::Pct(v) => Some(reference * v / 100.0),
            LengthAuto::Auto => None,
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

impl From<Length> for LengthAuto {
    fn from(value: Length) -> Self {
        match value {
            Length::Pct(pct) => LengthAuto::Pct(pct),
            Length::Pt(pt) => LengthAuto::Pt(pt),
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
    fn pct(self) -> Pct;
    fn pt(self) -> Pt;
    #[deprecated(note = "use .pt() instead")]
    fn px(self) -> Pt;
    fn deg(self) -> Angle;
    fn rad(self) -> Angle;
}

impl UnitExt for f64 {
    fn pct(self) -> Pct {
        Pct(self)
    }

    fn pt(self) -> Pt {
        Pt(self)
    }

    fn px(self) -> Pt {
        Pt(self)
    }

    fn deg(self) -> Angle {
        Angle::Deg(self)
    }

    fn rad(self) -> Angle {
        Angle::Rad(self)
    }
}

impl UnitExt for i32 {
    fn pct(self) -> Pct {
        Pct(self as f64)
    }

    fn pt(self) -> Pt {
        Pt(self as f64)
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

impl From<LengthAuto> for Dimension {
    fn from(value: LengthAuto) -> Self {
        match value {
            LengthAuto::Pt(v) => Dimension::length(v as f32),
            LengthAuto::Pct(v) => Dimension::percent(v as f32 / 100.0),
            LengthAuto::Auto => Dimension::auto(),
        }
    }
}

impl From<PxPctAuto> for Dimension {
    fn from(value: PxPctAuto) -> Self {
        LengthAuto::from(value).into()
    }
}

impl From<Length> for LengthPercentage {
    fn from(value: Length) -> Self {
        match value {
            Length::Pt(v) => LengthPercentage::length(v as f32),
            Length::Pct(v) => LengthPercentage::percent(v as f32 / 100.0),
        }
    }
}

impl From<PxPct> for LengthPercentage {
    fn from(value: PxPct) -> Self {
        Length::from(value).into()
    }
}

impl From<LengthAuto> for LengthPercentageAuto {
    fn from(value: LengthAuto) -> Self {
        match value {
            LengthAuto::Pt(v) => LengthPercentageAuto::length(v as f32),
            LengthAuto::Pct(v) => LengthPercentageAuto::percent(v as f32 / 100.0),
            LengthAuto::Auto => LengthPercentageAuto::auto(),
        }
    }
}

impl From<PxPctAuto> for LengthPercentageAuto {
    fn from(value: PxPctAuto) -> Self {
        LengthAuto::from(value).into()
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
