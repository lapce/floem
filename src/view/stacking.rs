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

use crate::{VisualId, view::ViewId};

/// An item to be painted within a stacking context (direct child of parent).
#[derive(Debug, Clone)]
pub(crate) struct StackingContextItem {
    pub visual_id: VisualId,
    pub z_index: i32,
    pub dom_order: usize,
}

/// Type alias for stacking context item collection.
/// Uses SmallVec to avoid heap allocation for small numbers of items (common case).
pub(crate) type StackingContextItems = SmallVec<[StackingContextItem; 8]>;

// Thread-local cache for stacking context items.
// Key: VisualId of the parent
// Value: Sorted list of direct children by z-index
thread_local! {
    static STACKING_CONTEXT_CACHE: RefCell<FxHashMap<VisualId, Rc<StackingContextItems>>> =
        RefCell::new(FxHashMap::default());

    // Thread-local cache for overlay order per root.
    // Key: VisualId of the root
    // Value: Sorted list of overlay ViewIds by z-index
    static OVERLAY_ORDER_CACHE: RefCell<FxHashMap<VisualId, SmallVec<[ViewId; 4]>>> =
        RefCell::new(FxHashMap::default());
}

/// Invalidates the stacking context cache for a view and its parent.
/// Call this when z-index or children change.
pub(crate) fn invalidate_stacking_cache(visual_id: VisualId) {
    STACKING_CONTEXT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        // Invalidate this view's cache (its children order)
        cache.remove(&visual_id);
        // Invalidate parent's cache (sibling order)
        let view_id = visual_id.view_id();
        if let Some(parent) = view_id.parent() {
            let parent_visual_id = parent.get_visual_id();
            cache.remove(&parent_visual_id);
        }
    });
}

/// Invalidates the overlay order cache for a root.
/// Call this when overlays are registered/unregistered or their z-index changes.
#[allow(dead_code)] // Kept for targeted cache invalidation when root is known
pub(crate) fn invalidate_overlay_cache(root_visual_id: VisualId) {
    OVERLAY_ORDER_CACHE.with(|cache| {
        cache.borrow_mut().remove(&root_visual_id);
    });
}

/// Invalidates all overlay caches.
/// This is a fallback when the root is not known.
pub(crate) fn invalidate_all_overlay_caches() {
    OVERLAY_ORDER_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
}

/// Clears all stacking context caches.
/// Used during window cleanup to ensure test isolation.
pub(crate) fn clear_all_stacking_caches() {
    STACKING_CONTEXT_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
    OVERLAY_ORDER_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
}

/// Collects direct child visual rectangles (VisualIds) from the box tree, sorted by z-index.
///
/// **Important**: This function iterates through all child VisualIds in the box tree,
/// not just child ViewIds. A single view can have multiple visual rectangles
/// (e.g., scroll view has content area, scrollbars), and all must be properly ordered.
///
/// In the simplified stacking model:
/// - Every view is implicitly a stacking context
/// - z-index only competes with siblings (sibling VisualIds in the box tree)
/// - Children are always bounded within their parent (they cannot "escape")
/// - DOM order (box tree child order) serves as a tiebreaker for equal z-index values
///
/// Results are cached. Call `invalidate_stacking_cache` when z-index or children change.
pub(crate) fn collect_stacking_context_items(
    parent_visual_id: VisualId,
    box_tree: &crate::BoxTree,
) -> Rc<StackingContextItems> {
    // Check cache first
    let cached = STACKING_CONTEXT_CACHE.with(|cache| cache.borrow().get(&parent_visual_id).cloned());

    if let Some(items) = cached {
        return items;
    }

    // Cache miss - collect direct children from box tree
    let mut items = StackingContextItems::new();
    let mut has_non_zero_z = false;

    // Iterate through all child visual rectangles in the box tree
    let box_tree_children = box_tree.children_of(parent_visual_id.0);

    // In normal usage, use box tree children (includes all visual rectangles)
    // In tests, box tree may not be populated, so fall back to view children
    if !box_tree_children.is_empty() {
        for (dom_order, &child_box_id) in box_tree_children.iter().enumerate() {
            // Construct VisualId from box tree node id
            let child_visual_id = VisualId(child_box_id, parent_visual_id.1);

            // Skip overlays - they're painted at root level
            let child_view_id = child_visual_id.view_id();
            if child_view_id.is_overlay() {
                continue;
            }

            // Get z-index from box tree
            let z_index = box_tree.local_z_index(child_box_id)
                .and_then(|opt| opt)
                .unwrap_or(0);

            if z_index != 0 {
                has_non_zero_z = true;
            }

            items.push(StackingContextItem {
                visual_id: child_visual_id,
                z_index,
                dom_order,
            });
        }
    } else {
        // Fallback for tests: use view children when box tree is not populated
        let parent_view_id = parent_visual_id.view_id();
        for (dom_order, child_view_id) in parent_view_id.children().into_iter().enumerate() {
            // Skip overlays - they're painted at root level
            if child_view_id.is_overlay() {
                continue;
            }

            let child_visual_id = child_view_id.get_visual_id();

            // In fallback mode (tests), z-index should be 0 (box tree not populated)
            let z_index = box_tree
                .local_z_index(child_visual_id.0)
                .and_then(|opt| opt)
                .unwrap_or(0);

            if z_index != 0 {
                has_non_zero_z = true;
            }

            items.push(StackingContextItem {
                visual_id: child_visual_id,
                z_index,
                dom_order,
            });
        }
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
        cache.borrow_mut().insert(parent_visual_id, Rc::clone(&items));
    });

    items
}

/// Collects all overlay ViewIds that belong to the given root.
/// Overlays are painted at root level, above all other content.
/// Returns overlays sorted by z-index (lower z-index painted first).
///
/// Results are cached. Call `invalidate_overlay_cache` when overlays are
/// registered/unregistered or their z-index changes.
pub(crate) fn collect_overlays(root_visual_id: VisualId, box_tree: &crate::BoxTree) -> SmallVec<[ViewId; 4]> {
    use super::VIEW_STORAGE;

    // Check cache first
    let cached = OVERLAY_ORDER_CACHE.with(|cache| cache.borrow().get(&root_visual_id).cloned());

    if let Some(overlays) = cached {
        return overlays;
    }

    let root_id = root_visual_id.view_id();

    // Cache miss - collect overlays with their z-indices using a single VIEW_STORAGE borrow
    let mut overlays: SmallVec<[(ViewId, i32); 4]> = VIEW_STORAGE.with_borrow(|s| {
        s.overlays
            .iter()
            .filter_map(|(overlay_id, &stored_root)| {
                // Check if this overlay belongs to our root
                if stored_root != root_id {
                    return None;
                }

                let state = s.states.get(overlay_id)?;
                let state_borrow = state.borrow();
                let visual_id = state_borrow.visual_id;

                // Get z-index from box tree
                let z_index = box_tree.local_z_index(visual_id.0)
                    .and_then(|opt| opt)
                    .unwrap_or(0);

                Some((overlay_id, z_index))
            })
            .collect()
    });

    // Sort by z-index, then DOM order for stability
    overlays.sort_by_key(|(_, z)| *z);

    let result: SmallVec<[ViewId; 4]> = overlays.into_iter().map(|(id, _)| id).collect();

    // Cache and return
    OVERLAY_ORDER_CACHE.with(|cache| {
        cache.borrow_mut().insert(root_visual_id, result.clone());
    });

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BoxTree;

    thread_local! {
        /// Test box tree for unit tests
        static TEST_BOX_TREE: std::cell::RefCell<BoxTree> = std::cell::RefCell::new(
            BoxTree::with_backend(understory_index::backends::GridF64::new(100.))
        );
    }

    /// Helper to create a ViewId and set its z-index in the test box tree
    fn create_view_with_z_index(z_index: i32) -> ViewId {
        let id = ViewId::new();

        // Set z-index in test box tree
        let visual_id = id.get_visual_id();
        TEST_BOX_TREE.with(|bt| {
            let mut bt = bt.borrow_mut();
            bt.set_local_z_index(visual_id.0, Some(z_index));
        });

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
        items.iter().map(|item| item.visual_id.view_id()).collect()
    }

    /// Helper to extract z-indices from stacking context items
    fn get_z_indices_from_items(items: &[StackingContextItem]) -> Vec<i32> {
        items.iter().map(|item| item.z_index).collect()
    }

    /// Helper wrapper to call collect_stacking_context_items with test box tree
    fn test_collect_items(parent: ViewId) -> Rc<StackingContextItems> {
        let visual_id = parent.get_visual_id();
        TEST_BOX_TREE.with(|bt| {
            collect_stacking_context_items(visual_id, &bt.borrow())
        })
    }

    /// Helper wrapper to call collect_overlays with test box tree
    fn test_collect_overlays(root: ViewId) -> SmallVec<[ViewId; 4]> {
        let visual_id = root.get_visual_id();
        TEST_BOX_TREE.with(|bt| {
            collect_overlays(visual_id, &bt.borrow())
        })
    }

    #[test]
    fn test_no_children() {
        let parent = ViewId::new();
        let result = test_collect_items(parent);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_child() {
        let child = create_view_with_z_index(5);
        let parent = setup_parent_with_children(vec![child]);

        let result = test_collect_items(parent);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].visual_id.view_id(), child);
    }

    #[test]
    fn test_children_default_z_index_preserves_dom_order() {
        // All children with default z-index (0) should preserve DOM order
        let child1 = create_view_with_z_index(0);
        let child2 = create_view_with_z_index(0);
        let child3 = create_view_with_z_index(0);
        let parent = setup_parent_with_children(vec![child1, child2, child3]);

        let result = test_collect_items(parent);
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

        let result = test_collect_items(parent);
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

        let result = test_collect_items(parent);
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

        let result = test_collect_items(parent);
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

        let result = test_collect_items(parent);
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

        let paint_order = test_collect_items(parent);
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

        let result = test_collect_items(parent);
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

        let result = test_collect_items(parent);
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

        let result = test_collect_items(root);

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
        let root_result = test_collect_items(root);
        assert_eq!(get_view_ids(&root_result), vec![a, b]);

        // A's direct children
        let a_result = test_collect_items(a);
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
        let parent_result = test_collect_items(parent);
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

        let result1 = test_collect_items(root);
        let result2 = test_collect_items(root);

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

        let result1 = test_collect_items(root);
        assert_eq!(get_view_ids(&result1), vec![a, b]);

        // Change a's z-index to be higher than b
        {
            // Update z-index in box tree
            TEST_BOX_TREE.with(|bt| {
                bt.borrow_mut().set_local_z_index(a.get_visual_id().0, Some(10));
            });

            // Simulate what happens during style computation - invalidate cache
            invalidate_stacking_cache(a.get_visual_id());
        }

        let result2 = test_collect_items(root);
        // Now a should come after b due to higher z-index
        assert_eq!(get_view_ids(&result2), vec![b, a]);
        assert_eq!(get_z_indices_from_items(&result2), vec![2, 10]);
    }

    #[test]
    fn test_stacking_cache_invalidation_on_children_change() {
        // Cache should be invalidated when children are added/removed
        let a = create_view_with_z_index(1);
        let root = setup_parent_with_children(vec![a]);

        let result1 = test_collect_items(root);
        assert_eq!(get_view_ids(&result1), vec![a]);

        // Add a new child
        let b = create_view_with_z_index(2);
        root.set_children_ids(vec![a, b]); // This calls invalidate_stacking_cache

        let result2 = test_collect_items(root);
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

        let result_a = test_collect_items(root_a);
        let result_b = test_collect_items(root_b);

        assert_eq!(get_view_ids(&result_a), vec![a1, a2]);
        assert_eq!(get_view_ids(&result_b), vec![b1, b2]);

        // Invalidate root_a's cache
        invalidate_stacking_cache(a1.get_visual_id());

        // root_b's cache should still be valid (returns same result)
        let result_b2 = test_collect_items(root_b);
        assert_eq!(get_view_ids(&result_b2), vec![b1, b2]);
    }

    #[test]
    fn test_stacking_cache_invalidation_on_child_removal() {
        // Cache should be invalidated when a child is removed
        let a = create_view_with_z_index(1);
        let b = create_view_with_z_index(2);
        let c = create_view_with_z_index(3);
        let root = setup_parent_with_children(vec![a, b, c]);

        let result1 = test_collect_items(root);
        assert_eq!(get_view_ids(&result1), vec![a, b, c]);

        // Remove b from children
        root.set_children_ids(vec![a, c]); // This calls invalidate_stacking_cache

        let result2 = test_collect_items(root);
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

        let result = test_collect_items(root);

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

        let result = test_collect_items(root);

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

        let result = test_collect_items(root);

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
        let result1 = test_collect_overlays(root);

        // Second call should return cached value
        let result2 = test_collect_overlays(root);

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

        let result1 = test_collect_overlays(root);
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
        let result2 = test_collect_overlays(root);
        assert_eq!(result2.len(), 2);

        // Clean up
        VIEW_STORAGE.with_borrow_mut(|s| {
            s.overlays.remove(overlay1);
            s.overlays.remove(overlay2);
        });
    }

    // ========== Bug Fix Tests ==========
    // These tests verify the fixes for stacking cache invalidation bugs

    #[test]
    fn test_set_children_invalidates_stacking_cache() {
        // Bug fix: set_children was not invalidating the stacking cache,
        // causing stale child ViewIds to be returned during painting.
        //
        // This tests that set_children (used by Container::derived) properly
        // invalidates the stacking cache.

        use crate::view::View;
        use crate::views::Empty;

        let parent = ViewId::new();

        // Create initial children using set_children (not set_children_ids)
        let child1 = Empty::new();
        let child1_id = child1.id();
        parent.set_children([child1]);

        // Set parent pointer for child
        child1_id.set_parent(parent);

        // First call - should cache the result
        let result1 = test_collect_items(parent);
        assert_eq!(get_view_ids(&result1), vec![child1_id]);

        // Now replace children using set_children (simulating Container::derived rebuild)
        let child2 = Empty::new();
        let child2_id = child2.id();
        parent.set_children([child2]);
        child2_id.set_parent(parent);

        // The stacking cache should have been invalidated by set_children
        // so this should return the NEW children, not the cached old ones
        let result2 = test_collect_items(parent);
        assert_eq!(
            get_view_ids(&result2),
            vec![child2_id],
            "set_children should invalidate stacking cache"
        );
        assert!(
            !get_view_ids(&result2).contains(&child1_id),
            "Old child should not be in stacking context"
        );
    }

    #[test]
    fn test_set_children_iter_invalidates_stacking_cache() {
        // Same test but for set_children_iter (used by set_children_vec)

        use crate::view::{IntoView, View};
        use crate::views::Empty;

        let parent = ViewId::new();

        // Create initial children using set_children_iter
        let child1 = Empty::new();
        let child1_id = child1.id();
        parent.set_children_iter(std::iter::once(child1.into_any()));
        child1_id.set_parent(parent);

        // First call - should cache the result
        let result1 = test_collect_items(parent);
        assert_eq!(get_view_ids(&result1), vec![child1_id]);

        // Replace children using set_children_iter
        let child2 = Empty::new();
        let child2_id = child2.id();
        parent.set_children_iter(std::iter::once(child2.into_any()));
        child2_id.set_parent(parent);

        // The stacking cache should have been invalidated
        let result2 = test_collect_items(parent);
        assert_eq!(
            get_view_ids(&result2),
            vec![child2_id],
            "set_children_iter should invalidate stacking cache"
        );
    }

    #[test]
    fn test_remove_invalidates_parent_stacking_cache() {
        // Bug fix: id.remove() was not invalidating the parent's stacking cache,
        // causing paint to iterate over removed (stale) ViewIds.
        //
        // This tests that remove() properly invalidates the parent's stacking cache.

        let a = create_view_with_z_index(1);
        let b = create_view_with_z_index(2);
        let c = create_view_with_z_index(3);
        let parent = setup_parent_with_children(vec![a, b, c]);

        // First call - caches the result
        let result1 = test_collect_items(parent);
        assert_eq!(get_view_ids(&result1), vec![a, b, c]);

        // Remove 'b' using id.remove() (which is called by remove_view)
        b.remove();

        // The parent's stacking cache should have been invalidated by remove()
        // so this should return the updated children list
        let result2 = test_collect_items(parent);
        assert_eq!(
            get_view_ids(&result2),
            vec![a, c],
            "remove() should invalidate parent's stacking cache"
        );
        assert!(
            !get_view_ids(&result2).contains(&b),
            "Removed child should not be in stacking context"
        );
    }

    #[test]
    fn test_container_derived_pattern_cache_invalidation() {
        // This test simulates the exact pattern used by Container::derived:
        // 1. Initial children set via set_children
        // 2. Update callback replaces children via set_children
        // 3. Old children are removed via remove()
        //
        // The stacking cache must be properly invalidated at each step.

        use crate::view::View;
        use crate::views::Empty;

        let container_id = ViewId::new();

        // Step 1: Initial child
        let old_child = Empty::new();
        let old_child_id = old_child.id();
        container_id.set_children([old_child]);
        old_child_id.set_parent(container_id);

        // Cache the stacking context
        let result1 = test_collect_items(container_id);
        assert_eq!(get_view_ids(&result1), vec![old_child_id]);

        // Step 2: Simulate Container::derived update
        // - Get old children
        let old_children = container_id.children();
        assert_eq!(old_children, vec![old_child_id]);

        // - Create new child and set it
        let new_child = Empty::new();
        let new_child_id = new_child.id();
        container_id.set_children([new_child]);
        new_child_id.set_parent(container_id);

        // Step 3: Remove old children (simulating update handler)
        for old in old_children {
            old.remove();
        }

        // Verify: stacking context should only contain new child
        let result2 = test_collect_items(container_id);
        assert_eq!(
            get_view_ids(&result2),
            vec![new_child_id],
            "After Container::derived pattern, only new child should be in stacking context"
        );
    }
}
