//! Hit test caching for event dispatch.
//!
//! Provides a 2-entry cache for hit test results, inspired by Chromium's Blink engine.
//! This exploits the common pattern where multiple events occur at the same location
//! (e.g., mousedown, mouseup, click all at the same point).

use std::{cell::RefCell, rc::Rc};

use peniko::kurbo::Point;
use understory_box_tree::NodeFlags;

use crate::{VisualId, view::ViewId};

// ============================================================================
// Hit Test Result Cache
// ============================================================================
//
// A small 2-entry cache for hit test results, inspired by Chromium's Blink engine.
// This exploits the common pattern where multiple events occur at the same location
// (e.g., mousedown, mouseup, click all at the same point).
//
// The cache size of 2 is chosen because:
// 1. It handles the ping-pong pattern of alternating event types
// 2. It's cheap to store and search (O(2) lookup)
// 3. Matches Blink's proven design: HIT_TEST_CACHE_SIZE = 2

/// Cache entry for hit test results.
#[derive(Clone)]
struct HitTestCacheEntry {
    /// The root visual ID for this hit test
    root_id: crate::VisualId,
    /// The point that was tested (in window coordinates)
    point: Point,
    /// The result of the hit test: (target node, full path from root to target)
    result: Option<(crate::VisualId, Rc<[crate::VisualId]>)>,
}

/// 2-entry hit test result cache.
struct HitTestCache {
    entries: [Option<HitTestCacheEntry>; 2],
    /// Index of next slot to write (round-robin)
    next_slot: usize,
}

impl HitTestCache {
    const fn new() -> Self {
        Self {
            entries: [None, None],
            next_slot: 0,
        }
    }

    /// Look up a cached hit test result.
    /// Returns Some(result) on cache hit, None on cache miss.
    /// Result is (target node, full path from root to target).
    #[inline]
    fn lookup(
        &self,
        root_id: crate::VisualId,
        point: Point,
    ) -> Option<Option<(crate::VisualId, Rc<[crate::VisualId]>)>> {
        for e in self.entries.iter().flatten() {
            // Use bitwise comparison for Point (exact match like Blink)
            if e.root_id == root_id
                && e.point.x.to_bits() == point.x.to_bits()
                && e.point.y.to_bits() == point.y.to_bits()
            {
                return Some(e.result.clone());
            }
        }
        None
    }

    /// Add a hit test result to the cache.
    #[inline]
    fn insert(
        &mut self,
        root_id: crate::VisualId,
        point: Point,
        result: Option<(crate::VisualId, Rc<[crate::VisualId]>)>,
    ) {
        self.entries[self.next_slot] = Some(HitTestCacheEntry {
            root_id,
            point,
            result,
        });
        self.next_slot = (self.next_slot + 1) % 2;
    }

    /// Clear the cache. Call this when layout or view tree changes.
    #[inline]
    fn clear(&mut self) {
        self.entries = [None, None];
    }
}

thread_local! {
    static HIT_TEST_CACHE: RefCell<HitTestCache> = const { RefCell::new(HitTestCache::new()) };
}

/// Clear the hit test result cache.
/// Call this when layout changes, view tree changes, or at the start of a new frame.
pub fn clear_hit_test_cache() {
    HIT_TEST_CACHE.with(|cache| cache.borrow_mut().clear());
}

/// Perform hit testing to find the target view under a point.
///
/// This walks the stacking context in reverse z-order (highest z-index first),
/// recursively checking children of stacking context items. Returns the target
/// and the full path from root to target.
///
/// Results are cached in a 2-entry cache to optimize repeated hit tests
/// at the same location (common during event sequences like click).
///
/// # Arguments
/// * `root_id` - The root view to start hit testing from
/// * `point` - The point in absolute (window) coordinates
///
/// # Returns
/// Optional tuple of (target VisualId, path from root to target as Rc<[VisualId]>)
pub fn hit_test(root_id: ViewId, point: Point) -> Option<(VisualId, Rc<[VisualId]>)> {
    let root_visual_id = root_id.get_visual_id();

    // Check cache first
    if let Some(cached_result) =
        HIT_TEST_CACHE.with(|cache| cache.borrow().lookup(root_visual_id, point))
    {
        // dbg!(&cached_result);
        return cached_result;
    }

    // Cache miss - query Understory
    let box_tree = root_id.box_tree();
    // dbg!(box_tree.borrow());
    let hit = box_tree.borrow().hit_test_point(
        point,
        understory_box_tree::QueryFilter {
            required_flags: NodeFlags::VISIBLE | NodeFlags::PICKABLE,
        },
    );

    let result = hit.map(|h| {
        use crate::visual_id::CastIds;
        let target = crate::VisualId(h.node, root_id);
        let path: Vec<_> = h.path.cast_ids(root_id);
        (target, path.into())
    });

    // Cache the result
    HIT_TEST_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .insert(root_visual_id, point, result.clone())
    });

    // dbg!(&result);
    result
}
