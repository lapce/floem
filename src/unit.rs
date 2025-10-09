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

impl From<i32> for Pct {
    fn from(value: i32) -> Self {
        Pct(value as f64)
    }
}

/// Used for automatically computed values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Auto;

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
    fn minutes(self) -> Duration {
        Duration::from_secs(self)
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
}

impl UnitExt for f64 {
    fn pct(self) -> Pct {
        Pct(self)
    }

    fn px(self) -> Px {
        Px(self)
    }
}

impl UnitExt for i32 {
    fn pct(self) -> Pct {
        Pct(self as f64)
    }

    fn px(self) -> Px {
        Px(self as f64)
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
