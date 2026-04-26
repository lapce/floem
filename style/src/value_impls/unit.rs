//! `StylePropValue` impls for floem_style's own unit types.

#![allow(deprecated)]

use crate::prop_value::{StylePropValue, hash_f64};
use crate::unit::{
    AnchorAbout, Angle, Length, LengthAuto, Pct, Pt, Px, PxPct, PxPctAuto,
};

impl StylePropValue for Pt {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.0.interpolate(&other.0, value).map(Pt)
    }
    fn content_hash(&self) -> u64 {
        hash_f64(self.0)
    }
}

impl StylePropValue for Px {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.0.interpolate(&other.0, value).map(Px)
    }
    fn content_hash(&self) -> u64 {
        hash_f64(self.0)
    }
}

impl StylePropValue for Pct {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.0.interpolate(&other.0, value).map(Pct)
    }
    fn content_hash(&self) -> u64 {
        hash_f64(self.0)
    }
}

impl StylePropValue for LengthAuto {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        match (self, other) {
            (Self::Pt(v1), Self::Pt(v2)) => Some(Self::Pt(v1 + (v2 - v1) * value)),
            (Self::Pct(v1), Self::Pct(v2)) => Some(Self::Pct(v1 + (v2 - v1) * value)),
            (Self::Em(v1), Self::Em(v2)) => Some(Self::Em(v1 + (v2 - v1) * value)),
            (Self::Lh(v1), Self::Lh(v2)) => Some(Self::Lh(v1 + (v2 - v1) * value)),
            (Self::Auto, Self::Auto) => Some(Self::Auto),
            // TODO: Figure out some way to get in the relevant layout information in order to interpolate between pixels and percent
            _ => None,
        }
    }
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        std::mem::discriminant(self).hash(&mut h);
        match self {
            Self::Pt(v) | Self::Pct(v) | Self::Em(v) | Self::Lh(v) => v.to_bits().hash(&mut h),
            Self::Auto => {}
        }
        h.finish()
    }
}

impl StylePropValue for PxPctAuto {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        match (self, other) {
            (Self::Px(v1), Self::Px(v2)) => Some(Self::Px(v1 + (v2 - v1) * value)),
            (Self::Pct(v1), Self::Pct(v2)) => Some(Self::Pct(v1 + (v2 - v1) * value)),
            (Self::Auto, Self::Auto) => Some(Self::Auto),
            _ => None,
        }
    }
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        std::mem::discriminant(self).hash(&mut h);
        match self {
            Self::Px(v) | Self::Pct(v) => v.to_bits().hash(&mut h),
            Self::Auto => {}
        }
        h.finish()
    }
}

impl StylePropValue for Length {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        match (self, other) {
            (Self::Pt(v1), Self::Pt(v2)) => Some(Self::Pt(v1 + (v2 - v1) * value)),
            (Self::Pct(v1), Self::Pct(v2)) => Some(Self::Pct(v1 + (v2 - v1) * value)),
            (Self::Em(v1), Self::Em(v2)) => Some(Self::Em(v1 + (v2 - v1) * value)),
            (Self::Lh(v1), Self::Lh(v2)) => Some(Self::Lh(v1 + (v2 - v1) * value)),
            // TODO: Figure out some way to get in the relevant layout information in order to interpolate between pixels and percent
            _ => None,
        }
    }
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        std::mem::discriminant(self).hash(&mut h);
        match self {
            Self::Pt(v) | Self::Pct(v) | Self::Em(v) | Self::Lh(v) => v.to_bits().hash(&mut h),
        }
        h.finish()
    }
}

impl StylePropValue for PxPct {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        match (self, other) {
            (Self::Px(v1), Self::Px(v2)) => Some(Self::Px(v1 + (v2 - v1) * value)),
            (Self::Pct(v1), Self::Pct(v2)) => Some(Self::Pct(v1 + (v2 - v1) * value)),
            _ => None,
        }
    }
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        std::mem::discriminant(self).hash(&mut h);
        match self {
            Self::Px(v) | Self::Pct(v) => v.to_bits().hash(&mut h),
        }
        h.finish()
    }
}

impl StylePropValue for Angle {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(self.lerp(other, value))
    }
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        std::mem::discriminant(self).hash(&mut h);
        match self {
            Angle::Deg(v) | Angle::Rad(v) => v.to_bits().hash(&mut h),
        }
        h.finish()
    }
}

impl StylePropValue for AnchorAbout {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            x: self.x + (other.x - self.x) * value,
            y: self.y + (other.y - self.y) * value,
        })
    }
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        self.x.to_bits().hash(&mut h);
        self.y.to_bits().hash(&mut h);
        h.finish()
    }
}
