//! Merge-id machinery for tracking style-map identity across cascades.
//!
//! Each mutation to a [`crate::Style`] increments the global counter; downstream
//! consumers compare merge ids to know when to recompute derived state.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::props::StyleKeyInfo;
use crate::StyleKey;

static NEXT_STYLE_MERGE_ID: AtomicU64 = AtomicU64::new(1);
const MERGE_MIX_CONST: u64 = 0x9E3779B97F4A7C15;

/// Static key referring to the deferred-effects slot in the style map.
pub static DEFERRED_EFFECTS_INFO: StyleKeyInfo = StyleKeyInfo::DeferredEffects;
pub const DEFERRED_EFFECTS_KEY: StyleKey = StyleKey {
    info: &DEFERRED_EFFECTS_INFO,
};

/// Allocate a fresh merge id. Guaranteed unique for the lifetime of the
/// process.
#[inline]
pub fn next_style_merge_id() -> u64 {
    NEXT_STYLE_MERGE_ID.fetch_add(1, Ordering::Relaxed)
}

/// Combine two merge ids. Order-dependent so `combine(a, b) != combine(b, a)`.
#[inline]
pub fn combine_merge_ids(a: u64, b: u64) -> u64 {
    a.rotate_left(13) ^ b.wrapping_mul(MERGE_MIX_CONST)
}
