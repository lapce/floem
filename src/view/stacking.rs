//! Stacking context management for z-index ordering in event dispatch and painting.
//!
//! This module implements a simplified stacking model where:
//! - Every view is implicitly a stacking context
//! - z-index only competes with siblings (children never escape parent boundaries)
//! - DOM order is used as a tiebreaker when z-index values are equal
//! - Use overlays to escape z-index constraints when needed

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::{cell::RefCell, rc::Rc};

use crate::view::ViewId;

/// An item to be painted within a stacking context (direct child of parent).
#[derive(Debug, Clone)]
pub(crate) struct StackingContextItem {
    pub view_id: ViewId,
    pub z_index: i32,
    pub dom_order: usize,
}

/// Type alias for stacking context item collection.
/// Uses SmallVec to avoid heap allocation for small numbers of items (common case).
pub(crate) type StackingContextItems = SmallVec<[StackingContextItem; 8]>;

// Thread-local cache for stacking context items.
// Key: ViewId of the parent
// Value: Sorted list of direct children by z-index
thread_local! {
    static STACKING_CONTEXT_CACHE: RefCell<FxHashMap<ViewId, Rc<StackingContextItems>>> =
        RefCell::new(FxHashMap::default());

    // Thread-local cache for overlay order per root.
    // Key: ViewId of the root
    // Value: Sorted list of overlay ViewIds by z-index
    static OVERLAY_ORDER_CACHE: RefCell<FxHashMap<ViewId, SmallVec<[ViewId; 4]>>> =
        RefCell::new(FxHashMap::default());
}

/// Invalidates the stacking context cache for a view and its parent.
/// Call this when z-index or children change.
pub(crate) fn invalidate_stacking_cache(view_id: ViewId) {
    STACKING_CONTEXT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        // Invalidate this view's cache (its children order)
        cache.remove(&view_id);
        // Invalidate parent's cache (sibling order)
        if let Some(parent) = view_id.parent() {
            cache.remove(&parent);
        }
    });
}

/// Invalidates the overlay order cache for a root.
/// Call this when overlays are registered/unregistered or their z-index changes.
pub(crate) fn invalidate_overlay_cache(root_id: ViewId) {
    OVERLAY_ORDER_CACHE.with(|cache| {
        cache.borrow_mut().remove(&root_id);
    });
}

/// Invalidates all overlay caches.
/// This is a fallback when the root is not known.
pub(crate) fn invalidate_all_overlay_caches() {
    OVERLAY_ORDER_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
}

/// Collects direct children of a view, sorted by z-index.
///
/// In the simplified stacking model:
/// - Every view is implicitly a stacking context
/// - z-index only competes with siblings
/// - Children are always bounded within their parent (they cannot "escape")
/// - DOM order serves as a tiebreaker for equal z-index values
///
/// Results are cached. Call `invalidate_stacking_cache` when z-index or children change.
pub(crate) fn collect_stacking_context_items(parent_id: ViewId) -> Rc<StackingContextItems> {
    // Check cache first
    let cached = STACKING_CONTEXT_CACHE.with(|cache| cache.borrow().get(&parent_id).cloned());

    if let Some(items) = cached {
        return items;
    }

    // Cache miss - collect direct children
    let mut items = StackingContextItems::new();
    let mut has_non_zero_z = false;

    for (dom_order, child) in parent_id.children().into_iter().enumerate() {
        // Skip overlays - they're painted at root level
        if child.is_overlay() {
            continue;
        }

        let z_index = child.state().borrow().stacking_info.effective_z_index;
        if z_index != 0 {
            has_non_zero_z = true;
        }

        items.push(StackingContextItem {
            view_id: child,
            z_index,
            dom_order,
        });
    }

    // Sort by z-index, then DOM order
    if has_non_zero_z {
        items.sort_by(|a, b| {
            a.z_index
                .cmp(&b.z_index)
                .then(a.dom_order.cmp(&b.dom_order))
        });
    }

    // Cache and return
    let items = Rc::new(items);
    STACKING_CONTEXT_CACHE.with(|cache| {
        cache.borrow_mut().insert(parent_id, Rc::clone(&items));
    });

    items
}

/// Collects all overlay ViewIds that belong to the given root.
/// Overlays are painted at root level, above all other content.
/// Returns overlays sorted by z-index (lower z-index painted first).
///
/// Results are cached. Call `invalidate_overlay_cache` when overlays are
/// registered/unregistered or their z-index changes.
pub(crate) fn collect_overlays(root_id: ViewId) -> SmallVec<[ViewId; 4]> {
    use super::VIEW_STORAGE;

    // Check cache first
    let cached = OVERLAY_ORDER_CACHE.with(|cache| cache.borrow().get(&root_id).cloned());

    if let Some(overlays) = cached {
        return overlays;
    }

    // Cache miss - collect overlay IDs that belong to root_id
    // We compute the actual root dynamically since the root may not be known at registration time
    let overlay_ids: SmallVec<[ViewId; 4]> = VIEW_STORAGE.with_borrow(|s| {
        s.overlays
            .keys()
            .filter(|&overlay_id| {
                // Compute actual root by walking up the parent chain
                let actual_root = s.root_view_id(overlay_id);
                actual_root == Some(root_id)
            })
            .collect()
    });

    // Get z-indices outside VIEW_STORAGE borrow to avoid RefCell conflict
    let mut overlays: SmallVec<[(ViewId, i32); 4]> = overlay_ids
        .into_iter()
        .map(|id| {
            let z_index = id.state().borrow().stacking_info.effective_z_index;
            (id, z_index)
        })
        .collect();

    // Sort by z-index, then DOM order for stability
    overlays.sort_by_key(|(_, z)| *z);

    let result: SmallVec<[ViewId; 4]> = overlays.into_iter().map(|(id, _)| id).collect();

    // Cache and return
    OVERLAY_ORDER_CACHE.with(|cache| {
        cache.borrow_mut().insert(root_id, result.clone());
    });

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::StackingInfo;

    /// Helper to create a ViewId and set its z-index
    fn create_view_with_z_index(z_index: i32) -> ViewId {
        let id = ViewId::new();
        let state = id.state();
        state.borrow_mut().stacking_info = StackingInfo {
            effective_z_index: z_index,
        };
        id
    }

    /// Helper to set up parent with children (also sets parent pointers)
    fn setup_parent_with_children(children: Vec<ViewId>) -> ViewId {
        let parent = ViewId::new();
        set_children_with_parents(parent, children);
        parent
    }

    /// Helper to set children AND parent pointers (for test purposes)
    fn set_children_with_parents(parent: ViewId, children: Vec<ViewId>) {
        for child in &children {
            child.set_parent(parent);
        }
        parent.set_children_ids(children);
    }

    /// Helper to extract view IDs from stacking context items
    fn get_view_ids(items: &[StackingContextItem]) -> Vec<ViewId> {
        items.iter().map(|item| item.view_id).collect()
    }

    /// Helper to extract z-indices from stacking context items
    fn get_z_indices_from_items(items: &[StackingContextItem]) -> Vec<i32> {
        items.iter().map(|item| item.z_index).collect()
    }

    #[test]
    fn test_no_children() {
        let parent = ViewId::new();
        let result = collect_stacking_context_items(parent);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_child() {
        let child = create_view_with_z_index(5);
        let parent = setup_parent_with_children(vec![child]);

        let result = collect_stacking_context_items(parent);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].view_id, child);
    }

    #[test]
    fn test_children_default_z_index_preserves_dom_order() {
        // All children with default z-index (0) should preserve DOM order
        let child1 = create_view_with_z_index(0);
        let child2 = create_view_with_z_index(0);
        let child3 = create_view_with_z_index(0);
        let parent = setup_parent_with_children(vec![child1, child2, child3]);

        let result = collect_stacking_context_items(parent);
        assert_eq!(get_view_ids(&result), vec![child1, child2, child3]);
    }

    #[test]
    fn test_basic_z_index_sorting() {
        // Children with different z-indices should be sorted ascending
        let child_z10 = create_view_with_z_index(10);
        let child_z1 = create_view_with_z_index(1);
        let child_z5 = create_view_with_z_index(5);
        // DOM order: z10, z1, z5
        let parent = setup_parent_with_children(vec![child_z10, child_z1, child_z5]);

        let result = collect_stacking_context_items(parent);
        // Paint order should be: z1, z5, z10 (ascending)
        assert_eq!(get_z_indices_from_items(&result), vec![1, 5, 10]);
        assert_eq!(get_view_ids(&result), vec![child_z1, child_z5, child_z10]);
    }

    #[test]
    fn test_negative_z_index() {
        // Negative z-index should sort before positive
        let child_pos = create_view_with_z_index(1);
        let child_neg = create_view_with_z_index(-1);
        let child_zero = create_view_with_z_index(0);
        // DOM order: pos, neg, zero
        let parent = setup_parent_with_children(vec![child_pos, child_neg, child_zero]);

        let result = collect_stacking_context_items(parent);
        // Paint order: -1, 0, 1
        assert_eq!(get_z_indices_from_items(&result), vec![-1, 0, 1]);
    }

    #[test]
    fn test_equal_z_index_preserves_dom_order() {
        // Children with same z-index should preserve DOM order (stable sort)
        let child1 = create_view_with_z_index(5);
        let child2 = create_view_with_z_index(5);
        let child3 = create_view_with_z_index(5);
        let parent = setup_parent_with_children(vec![child1, child2, child3]);

        let result = collect_stacking_context_items(parent);
        // Same z-index, so DOM order preserved
        assert_eq!(get_view_ids(&result), vec![child1, child2, child3]);
    }

    #[test]
    fn test_large_z_index_values() {
        // Test with large z-index values
        let child_max = create_view_with_z_index(i32::MAX);
        let child_min = create_view_with_z_index(i32::MIN);
        let child_zero = create_view_with_z_index(0);
        let parent = setup_parent_with_children(vec![child_max, child_min, child_zero]);

        let result = collect_stacking_context_items(parent);
        assert_eq!(
            get_z_indices_from_items(&result),
            vec![i32::MIN, 0, i32::MAX]
        );
    }

    #[test]
    fn test_event_dispatch_order_is_reverse_of_paint() {
        // Event dispatch iterates in reverse, so highest z-index receives events first
        let child_z1 = create_view_with_z_index(1);
        let child_z10 = create_view_with_z_index(10);
        let child_z5 = create_view_with_z_index(5);
        let parent = setup_parent_with_children(vec![child_z1, child_z10, child_z5]);

        let paint_order = collect_stacking_context_items(parent);
        // Paint order: 1, 5, 10 (ascending)
        assert_eq!(get_z_indices_from_items(&paint_order), vec![1, 5, 10]);

        // Event dispatch order (reverse): 10, 5, 1
        let event_order: Vec<i32> = paint_order.iter().rev().map(|item| item.z_index).collect();
        assert_eq!(event_order, vec![10, 5, 1]);
    }

    #[test]
    fn test_many_children_sorting() {
        // Test with many children to ensure sorting is stable and correct
        let children: Vec<_> = (0..10)
            .map(|i| create_view_with_z_index(9 - i)) // z-indices: 9, 8, 7, ..., 0
            .collect();
        let parent = setup_parent_with_children(children.clone());

        let result = collect_stacking_context_items(parent);
        // Should be sorted ascending: 0, 1, 2, ..., 9
        let z_indices = get_z_indices_from_items(&result);
        assert_eq!(z_indices, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn test_all_same_nonzero_z_index_preserves_dom_order() {
        // When all children have the same non-zero z-index, DOM order should be preserved
        let child1 = create_view_with_z_index(-5);
        let child2 = create_view_with_z_index(-5);
        let child3 = create_view_with_z_index(-5);
        let parent = setup_parent_with_children(vec![child1, child2, child3]);

        let result = collect_stacking_context_items(parent);
        // All same z-index, DOM order preserved
        assert_eq!(get_view_ids(&result), vec![child1, child2, child3]);
        assert_eq!(get_z_indices_from_items(&result), vec![-5, -5, -5]);
    }

    // ========== Simplified Stacking Model Tests ==========
    // In the simplified model, every view is a stacking context and z-index only
    // competes with siblings. Children are always bounded within their parent.

    #[test]
    fn test_stacking_model_children_bounded() {
        // Children are bounded within their parent's stacking context
        // A child with high z-index does NOT escape to compete with parent's siblings
        //
        // Structure:
        //   Root
        //   ├── A (z=1)
        //   │   └── A1 (z=100) - bounded within A
        //   └── B (z=2)
        //
        // collect_stacking_context_items(root) returns only direct children: A, B
        // A1's z=100 doesn't matter at root level - it's inside A

        let a = create_view_with_z_index(1);
        let a1 = create_view_with_z_index(100);
        a.set_children_ids(vec![a1]);

        let b = create_view_with_z_index(2);

        let root = setup_parent_with_children(vec![a, b]);

        let result = collect_stacking_context_items(root);

        // Only A and B should be in root's direct children list
        assert_eq!(result.len(), 2);
        assert_eq!(get_z_indices_from_items(&result), vec![1, 2]);
        assert_eq!(get_view_ids(&result), vec![a, b]);
    }

    #[test]
    fn test_stacking_model_only_direct_children() {
        // collect_stacking_context_items only returns direct children
        //
        // Structure:
        //   Root
        //   ├── A (z=1)
        //   │   ├── A1 (z=10)
        //   │   └── A2 (z=20)
        //   └── B (z=2)

        let a = create_view_with_z_index(1);
        let a1 = create_view_with_z_index(10);
        let a2 = create_view_with_z_index(20);
        set_children_with_parents(a, vec![a1, a2]);

        let b = create_view_with_z_index(2);

        let root = setup_parent_with_children(vec![a, b]);

        // Root's direct children
        let root_result = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&root_result), vec![a, b]);

        // A's direct children
        let a_result = collect_stacking_context_items(a);
        assert_eq!(get_view_ids(&a_result), vec![a1, a2]);
    }

    #[test]
    fn test_stacking_model_nested_z_index_competition() {
        // z-index only competes with siblings at each level
        //
        // Structure:
        //   Root
        //   └── Parent (z=0)
        //       ├── Child1 (z=10)
        //       ├── Child2 (z=5)
        //       └── Child3 (z=15)
        //
        // Children of Parent compete among themselves, not with Root's siblings

        let parent = create_view_with_z_index(0);
        let child1 = create_view_with_z_index(10);
        let child2 = create_view_with_z_index(5);
        let child3 = create_view_with_z_index(15);
        set_children_with_parents(parent, vec![child1, child2, child3]);

        let _root = setup_parent_with_children(vec![parent]);

        // Parent level: only direct children sorted by z-index
        let parent_result = collect_stacking_context_items(parent);
        assert_eq!(get_z_indices_from_items(&parent_result), vec![5, 10, 15]);
        assert_eq!(get_view_ids(&parent_result), vec![child2, child1, child3]);
    }

    // ========== Stacking Context Cache Tests ==========

    #[test]
    fn test_stacking_cache_hit_on_second_call() {
        // Second call should return cached value (same result)
        let a = create_view_with_z_index(1);
        let b = create_view_with_z_index(2);
        let root = setup_parent_with_children(vec![a, b]);

        let result1 = collect_stacking_context_items(root);
        let result2 = collect_stacking_context_items(root);

        // Results should be identical
        assert_eq!(get_view_ids(&result1), get_view_ids(&result2));
        assert_eq!(
            get_z_indices_from_items(&result1),
            get_z_indices_from_items(&result2)
        );
    }

    #[test]
    fn test_stacking_cache_invalidation_on_z_index_change() {
        // Cache should be invalidated when z-index changes
        let a = create_view_with_z_index(1);
        let b = create_view_with_z_index(2);
        let root = setup_parent_with_children(vec![a, b]);

        let result1 = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&result1), vec![a, b]);

        // Change a's z-index to be higher than b
        {
            let state = a.state();
            let old_info = state.borrow().stacking_info;
            state.borrow_mut().stacking_info = StackingInfo {
                effective_z_index: 10,
            };
            // Simulate what happens during style computation
            if old_info.effective_z_index != 10 {
                invalidate_stacking_cache(a);
            }
        }

        let result2 = collect_stacking_context_items(root);
        // Now a should come after b due to higher z-index
        assert_eq!(get_view_ids(&result2), vec![b, a]);
        assert_eq!(get_z_indices_from_items(&result2), vec![2, 10]);
    }

    #[test]
    fn test_stacking_cache_invalidation_on_children_change() {
        // Cache should be invalidated when children are added/removed
        let a = create_view_with_z_index(1);
        let root = setup_parent_with_children(vec![a]);

        let result1 = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&result1), vec![a]);

        // Add a new child
        let b = create_view_with_z_index(2);
        root.set_children_ids(vec![a, b]); // This calls invalidate_stacking_cache

        let result2 = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&result2), vec![a, b]);
    }

    #[test]
    fn test_stacking_cache_multiple_roots_independent() {
        // Different stacking context roots should have independent caches
        let a1 = create_view_with_z_index(1);
        let a2 = create_view_with_z_index(2);
        let root_a = setup_parent_with_children(vec![a1, a2]);

        let b1 = create_view_with_z_index(10);
        let b2 = create_view_with_z_index(20);
        let root_b = setup_parent_with_children(vec![b1, b2]);

        let result_a = collect_stacking_context_items(root_a);
        let result_b = collect_stacking_context_items(root_b);

        assert_eq!(get_view_ids(&result_a), vec![a1, a2]);
        assert_eq!(get_view_ids(&result_b), vec![b1, b2]);

        // Invalidate root_a's cache
        invalidate_stacking_cache(a1);

        // root_b's cache should still be valid (returns same result)
        let result_b2 = collect_stacking_context_items(root_b);
        assert_eq!(get_view_ids(&result_b2), vec![b1, b2]);
    }

    #[test]
    fn test_stacking_cache_invalidation_on_child_removal() {
        // Cache should be invalidated when a child is removed
        let a = create_view_with_z_index(1);
        let b = create_view_with_z_index(2);
        let c = create_view_with_z_index(3);
        let root = setup_parent_with_children(vec![a, b, c]);

        let result1 = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&result1), vec![a, b, c]);

        // Remove b from children
        root.set_children_ids(vec![a, c]); // This calls invalidate_stacking_cache

        let result2 = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&result2), vec![a, c]);
    }

    // ========== Fast Path Tests ==========

    #[test]
    fn test_fast_path_all_zero_z_index_preserves_dom_order() {
        // When all z-indices are zero, items should be in DOM order (no sorting needed)
        let a = create_view_with_z_index(0);
        let b = create_view_with_z_index(0);
        let c = create_view_with_z_index(0);
        let root = setup_parent_with_children(vec![a, b, c]);

        let result = collect_stacking_context_items(root);

        // All z-indices are 0, should be in DOM order
        assert_eq!(get_view_ids(&result), vec![a, b, c]);
        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 0]);
    }

    #[test]
    fn test_sorting_triggered_by_single_non_zero_z_index() {
        // Even a single non-zero z-index should trigger sorting
        let a = create_view_with_z_index(0);
        let b = create_view_with_z_index(1); // Only one with z-index
        let c = create_view_with_z_index(0);
        let root = setup_parent_with_children(vec![a, b, c]);

        let result = collect_stacking_context_items(root);

        // b has z=1, so it should come after a and c (which have z=0)
        assert_eq!(get_view_ids(&result), vec![a, c, b]);
        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 1]);
    }

    #[test]
    fn test_sorting_triggered_by_negative_z_index() {
        // Negative z-index should also trigger sorting
        let a = create_view_with_z_index(0);
        let b = create_view_with_z_index(-1); // Negative z-index
        let c = create_view_with_z_index(0);
        let root = setup_parent_with_children(vec![a, b, c]);

        let result = collect_stacking_context_items(root);

        // b has z=-1, so it should come before a and c (which have z=0)
        assert_eq!(get_view_ids(&result), vec![b, a, c]);
        assert_eq!(get_z_indices_from_items(&result), vec![-1, 0, 0]);
    }

    // ========== Overlay Cache Tests ==========

    #[test]
    fn test_overlay_cache_hit_on_second_call() {
        use crate::view::VIEW_STORAGE;

        // Create a root and register overlays
        let root = ViewId::new();
        let overlay1 = create_view_with_z_index(1);
        let overlay2 = create_view_with_z_index(2);

        // Set up parent chain so overlays belong to root
        overlay1.set_parent(root);
        overlay2.set_parent(root);
        root.set_children_ids(vec![overlay1, overlay2]);

        // Register overlays
        VIEW_STORAGE.with_borrow_mut(|s| {
            s.overlays.insert(overlay1, root);
            s.overlays.insert(overlay2, root);
        });

        // First call should populate cache
        let result1 = collect_overlays(root);

        // Second call should return cached value
        let result2 = collect_overlays(root);

        assert_eq!(result1, result2);
        assert_eq!(result1.len(), 2);

        // Clean up
        VIEW_STORAGE.with_borrow_mut(|s| {
            s.overlays.remove(overlay1);
            s.overlays.remove(overlay2);
        });
    }

    #[test]
    fn test_overlay_cache_invalidation() {
        use crate::view::VIEW_STORAGE;

        // Create a root and register an overlay
        let root = ViewId::new();
        let overlay1 = create_view_with_z_index(1);

        overlay1.set_parent(root);
        root.set_children_ids(vec![overlay1]);

        VIEW_STORAGE.with_borrow_mut(|s| {
            s.overlays.insert(overlay1, root);
        });

        let result1 = collect_overlays(root);
        assert_eq!(result1.len(), 1);

        // Invalidate cache
        invalidate_all_overlay_caches();

        // Add another overlay
        let overlay2 = create_view_with_z_index(2);
        overlay2.set_parent(root);
        VIEW_STORAGE.with_borrow_mut(|s| {
            s.overlays.insert(overlay2, root);
        });

        // Should get fresh result with both overlays
        let result2 = collect_overlays(root);
        assert_eq!(result2.len(), 2);

        // Clean up
        VIEW_STORAGE.with_borrow_mut(|s| {
            s.overlays.remove(overlay1);
            s.overlays.remove(overlay2);
        });
    }
}
