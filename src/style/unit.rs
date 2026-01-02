use std::{ops::Neg, time::Duration};

use taffy::style::{Dimension, LengthPercentage, LengthPercentageAuto};

/// A pixel value
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Px(pub f64);

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

/// An angle value that can be in degrees or radians
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Angle {
    /// Degrees (0-360)
    Deg(f64),
    /// Radians (0-2π)
    Rad(f64),
}

impl Angle {
    /// Convert the angle to radians
    pub fn to_radians(self) -> f64 {
        match self {
            Angle::Deg(deg) => deg.to_radians(),
            Angle::Rad(rad) => rad,
        }
    }

    /// Convert the angle to degrees
    pub fn to_degrees(self) -> f64 {
        match self {
            Angle::Deg(deg) => deg,
            Angle::Rad(rad) => rad.to_degrees(),
        }
    }
}

impl From<f64> for Px {
    fn from(value: f64) -> Self {
        Px(value)
    }
}

impl From<f32> for Px {
    fn from(value: f32) -> Self {
        Px(value as f64)
    }
}

impl From<i32> for Px {
    fn from(value: i32) -> Self {
        Px(value as f64)
    }
}

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
    T: Into<Px>,
{
    fn from(value: T) -> Self {
        PxPct::Px(value.into().0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PxPctAuto {
    Px(f64),
    Pct(f64),
    Auto,
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
    T: Into<Px>,
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
    fn px(self) -> Px;
    fn deg(self) -> Angle;
    fn rad(self) -> Angle;
}

impl UnitExt for f64 {
    fn pct(self) -> Pct {
        Pct(self)
    }

    fn px(self) -> Px {
        Px(self)
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

    fn px(self) -> Px {
        Px(self as f64)
    }

    fn deg(self) -> Angle {
        Angle::Deg(self as f64)
    }

    fn rad(self) -> Angle {
        Angle::Rad(self as f64)
    }
}

impl From<PxPctAuto> for Dimension {
    fn from(value: PxPctAuto) -> Self {
        match value {
            PxPctAuto::Px(v) => Dimension::length(v as f32),
            PxPctAuto::Pct(v) => Dimension::percent(v as f32 / 100.0),
            PxPctAuto::Auto => Dimension::auto(),
        }
    }
}

impl From<PxPct> for LengthPercentage {
    fn from(value: PxPct) -> Self {
        match value {
            PxPct::Px(v) => LengthPercentage::length(v as f32),
            PxPct::Pct(v) => LengthPercentage::percent(v as f32 / 100.0),
        }
    }
}

impl From<PxPctAuto> for LengthPercentageAuto {
    fn from(value: PxPctAuto) -> Self {
        match value {
            PxPctAuto::Px(v) => LengthPercentageAuto::length(v as f32),
            PxPctAuto::Pct(v) => LengthPercentageAuto::percent(v as f32 / 100.0),
            PxPctAuto::Auto => LengthPercentageAuto::auto(),
        }
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
