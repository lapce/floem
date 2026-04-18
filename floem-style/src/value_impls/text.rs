//! `StylePropValue` impls for text-related types (font weight, line height,
//! font style, alignment). Font types originate in `fontique` and are
//! re-exported by `parley`; `LineHeightValue` is defined in this crate
//! (see [`crate::unit::LineHeightValue`]).

use parley::{Alignment, FontStyle, FontWeight};

use crate::prop_value::{StylePropValue, hash_f32, hash_value};
use crate::unit::LineHeightValue;

impl StylePropValue for FontWeight {
    fn content_hash(&self) -> u64 {
        hash_f32(self.value())
    }
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.value()
            .interpolate(&other.value(), value)
            .map(FontWeight::new)
    }
}
impl StylePropValue for FontStyle {
    fn content_hash(&self) -> u64 {
        hash_value(&std::mem::discriminant(self))
    }
}
impl StylePropValue for Alignment {
    fn content_hash(&self) -> u64 {
        hash_value(&std::mem::discriminant(self))
    }
}
impl StylePropValue for LineHeightValue {
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        std::mem::discriminant(self).hash(&mut h);
        match self {
            LineHeightValue::Normal(v) => v.to_bits().hash(&mut h),
            LineHeightValue::Pt(v) => v.to_bits().hash(&mut h),
        }
        h.finish()
    }
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        match (self, other) {
            (LineHeightValue::Normal(v1), LineHeightValue::Normal(v2)) => {
                v1.interpolate(v2, value).map(LineHeightValue::Normal)
            }
            (LineHeightValue::Pt(v1), LineHeightValue::Pt(v2)) => {
                v1.interpolate(v2, value).map(LineHeightValue::Pt)
            }
            _ => None,
        }
    }
}
