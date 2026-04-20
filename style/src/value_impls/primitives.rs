//! `StylePropValue` impls for primitive types and `String`.

use crate::prop_value::{StylePropValue, hash_f32, hash_f64, hash_value};

impl StylePropValue for i32 {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some((*self as f64 + (*other as f64 - *self as f64) * value).round() as i32)
    }
    fn content_hash(&self) -> u64 {
        hash_value(self)
    }
}
impl StylePropValue for bool {
    fn content_hash(&self) -> u64 {
        hash_value(self)
    }
}
impl StylePropValue for f32 {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(*self * (1.0 - value as f32) + *other * value as f32)
    }
    fn content_hash(&self) -> u64 {
        hash_f32(*self)
    }
}
impl StylePropValue for u16 {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some((*self as f64 + (*other as f64 - *self as f64) * value).round() as u16)
    }
    fn content_hash(&self) -> u64 {
        hash_value(self)
    }
}
impl StylePropValue for usize {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some((*self as f64 + (*other as f64 - *self as f64) * value).round() as usize)
    }
    fn content_hash(&self) -> u64 {
        hash_value(self)
    }
}
impl StylePropValue for f64 {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(*self * (1.0 - value) + *other * value)
    }
    fn content_hash(&self) -> u64 {
        hash_f64(*self)
    }
}
impl StylePropValue for String {
    fn content_hash(&self) -> u64 {
        hash_value(self)
    }
}
