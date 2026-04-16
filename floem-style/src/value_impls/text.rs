//! `StylePropValue` impls for text-related types (font weight, line height,
//! font style, alignment). These types originate in external crates
//! (`fontique`/`parley`) and are re-exported by `floem_renderer::text` or
//! accessed directly via `parley`.

use floem_renderer::text::{FontStyle, FontWeight, LineHeightValue};
use parley::Alignment;

use crate::prop_value::{StylePropValue, hash_f32, hash_value};

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
