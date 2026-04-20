//! `StylePropValue` impls for container types (`Option`, `Vec`, `SmallVec`) and
//! `std::time::Duration` (or `web_time::Duration` on wasm).

use smallvec::SmallVec;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use crate::prop_value::{StylePropValue, hash_value};

impl<A: smallvec::Array> StylePropValue for SmallVec<A>
where
    <A as smallvec::Array>::Item: StylePropValue,
{
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.iter().zip(other.iter()).try_fold(
            SmallVec::with_capacity(self.len()),
            |mut acc, (v1, v2)| {
                if let Some(interpolated) = v1.interpolate(v2, value) {
                    acc.push(interpolated);
                    Some(acc)
                } else {
                    None
                }
            },
        )
    }

    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        self.len().hash(&mut h);
        for item in self.iter() {
            item.content_hash().hash(&mut h);
        }
        h.finish()
    }
}

impl<T: StylePropValue> StylePropValue for Option<T> {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.as_ref().and_then(|this| {
            other
                .as_ref()
                .and_then(|other| this.interpolate(other, value).map(Some))
        })
    }

    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        match self {
            Some(v) => {
                1u8.hash(&mut h);
                v.content_hash().hash(&mut h);
            }
            None => 0u8.hash(&mut h),
        }
        h.finish()
    }
}

impl<T: StylePropValue + 'static> StylePropValue for Vec<T> {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.iter().zip(other.iter()).try_fold(
            Vec::with_capacity(self.len()),
            |mut acc, (v1, v2)| {
                if let Some(interpolated) = v1.interpolate(v2, value) {
                    acc.push(interpolated);
                    Some(acc)
                } else {
                    None
                }
            },
        )
    }

    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        self.len().hash(&mut h);
        for item in self {
            item.content_hash().hash(&mut h);
        }
        h.finish()
    }
}

impl StylePropValue for Duration {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.as_secs_f64()
            .interpolate(&other.as_secs_f64(), value)
            .map(Duration::from_secs_f64)
    }

    fn content_hash(&self) -> u64 {
        hash_value(self)
    }
}
