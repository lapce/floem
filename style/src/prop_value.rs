//! The [`StylePropValue`] trait — the common interface the engine uses for
//! every value stored on a [`crate::Style`] map.
//!
//! Hosts typically don't implement this directly; instead the `prop!` macro
//! in floem proper registers property types that already carry this bound
//! on their value type.

use std::fmt::Debug;

pub trait StylePropValue: Clone + PartialEq + Debug {
    fn interpolate(&self, _other: &Self, _value: f64) -> Option<Self> {
        None
    }

    /// Compute a content-based hash for this value.
    ///
    /// This hash is used for style caching — identical values should produce
    /// identical hashes. The default implementation uses the `Debug`
    /// representation, which works for most types but allocates. Types should
    /// override this for better performance.
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = rustc_hash::FxHasher::default();
        let debug_str = format!("{:?}", self);
        debug_str.hash(&mut hasher);
        hasher.finish()
    }
}

/// Hash a type that implements `Hash` using FxHasher.
#[inline]
pub fn hash_value<T: std::hash::Hash>(val: &T) -> u64 {
    use std::hash::Hasher;
    let mut h = rustc_hash::FxHasher::default();
    val.hash(&mut h);
    h.finish()
}

/// Hash an `f32` by its bit representation.
#[inline]
pub fn hash_f32(v: f32) -> u64 {
    hash_value(&v.to_bits())
}

/// Hash an `f64` by its bit representation.
#[inline]
pub fn hash_f64(v: f64) -> u64 {
    hash_value(&v.to_bits())
}
