//! Stacking context management for z-index ordering in event dispatch and painting.
//!
//! This module implements true CSS stacking context semantics where children of
//! non-stacking-context views participate in their ancestor's stacking context.

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::{cell::RefCell, rc::Rc};

use crate::view::ViewId;

/// Type alias for parent chain storage.
/// Uses SmallVec to avoid heap allocation for shallow nesting (common case).
/// Stored in ancestor-to-parent order (root first, immediate parent last).
/// Iterate with `.iter().rev()` to go from immediate parent up to root.
pub(crate) type ParentChain = SmallVec<[ViewId; 8]>;

/// An item to be painted within a stacking context.
/// Implements true CSS stacking context semantics where children of non-stacking-context
/// views participate in their ancestor's stacking context.
#[derive(Debug, Clone)]
pub(crate) struct StackingContextItem {
    pub view_id: ViewId,
    pub z_index: i32,
    pub dom_order: usize,
    /// If true, this view creates a stacking context; paint it atomically with children
    pub creates_context: bool,
    /// Cached parent chain from this view up to (but not including) the stacking context root.
    /// Stored in ancestor-to-parent order (root first, immediate parent last).
    /// Iterate with `.iter().rev()` to go from immediate parent up to root for event bubbling.
    /// Wrapped in Rc to share among siblings (they have the same parent chain).
    pub parent_chain: Rc<ParentChain>,
}

/// Type alias for stacking context item collection.
/// Uses SmallVec to avoid heap allocation for small numbers of items (common case).
pub(crate) type StackingContextItems = SmallVec<[StackingContextItem; 8]>;

// Thread-local cache for stacking context items.
// Key: ViewId of the stacking context root
// Value: Sorted list of items in that stacking context (Rc to avoid cloning on cache hit)
// Uses FxHashMap for faster hashing of ViewId keys.
thread_local! {
    static STACKING_CONTEXT_CACHE: RefCell<FxHashMap<ViewId, Rc<StackingContextItems>>> =
        RefCell::new(FxHashMap::default());
}

/// Invalidates the stacking context cache for a view and all its ancestors.
/// Call this when z-index, transform, hidden state, or children change.
pub(crate) fn invalidate_stacking_cache(view_id: ViewId) {
    STACKING_CONTEXT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        // Invalidate this view's cache (if it's a stacking context root)
        cache.remove(&view_id);
        // Invalidate all ancestor caches since this view might participate in them
        let mut parent = view_id.parent();
        while let Some(p) = parent {
            cache.remove(&p);
            parent = p.parent();
        }
    });
}

/// Collects all items participating in a stacking context, sorted by z-index.
/// This implements true CSS stacking context semantics:
/// - Views that create stacking contexts are painted atomically (children bounded within)
/// - Views that don't create stacking contexts have their children "escape" and participate
///   in the parent's stacking context
///
/// Results are cached per stacking context root. Call `invalidate_stacking_cache` when
/// z-index, transform, or children change.
///
/// Returns an Rc to avoid cloning the cached items on each call.
pub(crate) fn collect_stacking_context_items(parent_id: ViewId) -> Rc<StackingContextItems> {
    // Check cache first - Rc::clone is cheap (just increments refcount)
    let cached = STACKING_CONTEXT_CACHE.with(|cache| cache.borrow().get(&parent_id).cloned());

    if let Some(items) = cached {
        return items;
    }

    // Cache miss - compute items
    // SmallVec avoids heap allocation for <= 8 items (common case)
    let mut items = StackingContextItems::new();
    let mut dom_order = 0;
    let mut has_non_zero_z = false;

    // Start with empty parent chain (direct children of the stacking context root)
    // Wrap in Rc so siblings can share the same parent chain
    let parent_chain = Rc::new(ParentChain::new());
    for child in parent_id.children() {
        collect_items_recursive(
            child,
            &mut items,
            &mut dom_order,
            &mut has_non_zero_z,
            Rc::clone(&parent_chain),
        );
    }

    // Fast path: skip sorting if all z-indices are zero (already in DOM order)
    if has_non_zero_z {
        items.sort_by(|a, b| {
            a.z_index
                .cmp(&b.z_index)
                .then(a.dom_order.cmp(&b.dom_order))
        });
    }

    // Wrap in Rc and store in cache
    let items = Rc::new(items);
    STACKING_CONTEXT_CACHE.with(|cache| {
        cache.borrow_mut().insert(parent_id, Rc::clone(&items));
    });

    items
}

/// Recursively collects items for a stacking context.
/// For views that don't create stacking contexts, their children are collected into the
/// parent's stacking context (they can interleave with siblings based on z-index).
fn collect_items_recursive(
    view_id: ViewId,
    items: &mut StackingContextItems,
    dom_order: &mut usize,
    has_non_zero_z: &mut bool,
    parent_chain: Rc<ParentChain>,
) {
    let info = view_id.state().borrow().stacking_info;

    // Track if any non-zero z-index is encountered
    if info.effective_z_index != 0 {
        *has_non_zero_z = true;
    }

    items.push(StackingContextItem {
        view_id,
        z_index: info.effective_z_index,
        dom_order: *dom_order,
        creates_context: info.creates_context,
        parent_chain: Rc::clone(&parent_chain),
    });
    *dom_order += 1;

    // If this view doesn't create a stacking context, its children participate
    // in the parent's stacking context (they can interleave with uncles/aunts)
    if !info.creates_context {
        // Build the parent chain for children: our parent chain + current view
        // Using push (O(1)) instead of insert(0, ...) (O(n)) for better performance.
        // The chain is stored ancestor-to-parent (root first), iterate with .rev() to bubble.
        // Create a new Rc that all children (siblings) will share.
        let mut child_parent_chain = (*parent_chain).clone();
        child_parent_chain.push(view_id);
        let child_parent_chain = Rc::new(child_parent_chain);
        for child in view_id.children() {
            collect_items_recursive(
                child,
                items,
                dom_order,
                has_non_zero_z,
                Rc::clone(&child_parent_chain),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::StackingInfo;

    /// Helper to create a ViewId and set its z-index
    /// Views with explicit z-index create stacking contexts
    fn create_view_with_z_index(z_index: Option<i32>) -> ViewId {
        let id = ViewId::new();
        let state = id.state();
        state.borrow_mut().stacking_info = StackingInfo {
            creates_context: z_index.is_some(),
            effective_z_index: z_index.unwrap_or(0),
        };
        id
    }

    /// Helper to create a ViewId that does NOT create a stacking context
    /// Its children will participate in the parent's stacking context
    fn create_view_no_stacking_context() -> ViewId {
        let id = ViewId::new();
        let state = id.state();
        state.borrow_mut().stacking_info = StackingInfo {
            creates_context: false,
            effective_z_index: 0,
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
        let child = create_view_with_z_index(Some(5));
        let parent = setup_parent_with_children(vec![child]);

        let result = collect_stacking_context_items(parent);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].view_id, child);
    }

    #[test]
    fn test_children_no_z_index_preserves_dom_order() {
        // All children with default z-index (0) should preserve DOM order
        // Note: children without explicit z-index don't create stacking contexts
        let child1 = create_view_no_stacking_context();
        let child2 = create_view_no_stacking_context();
        let child3 = create_view_no_stacking_context();
        let parent = setup_parent_with_children(vec![child1, child2, child3]);

        let result = collect_stacking_context_items(parent);
        assert_eq!(get_view_ids(&result), vec![child1, child2, child3]);
    }

    #[test]
    fn test_basic_z_index_sorting() {
        // Children with different z-indices should be sorted ascending
        let child_z10 = create_view_with_z_index(Some(10));
        let child_z1 = create_view_with_z_index(Some(1));
        let child_z5 = create_view_with_z_index(Some(5));
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
        let child_pos = create_view_with_z_index(Some(1));
        let child_neg = create_view_with_z_index(Some(-1));
        let child_zero = create_view_with_z_index(Some(0));
        // DOM order: pos, neg, zero
        let parent = setup_parent_with_children(vec![child_pos, child_neg, child_zero]);

        let result = collect_stacking_context_items(parent);
        // Paint order: -1, 0, 1
        assert_eq!(get_z_indices_from_items(&result), vec![-1, 0, 1]);
    }

    #[test]
    fn test_equal_z_index_preserves_dom_order() {
        // Children with same z-index should preserve DOM order (stable sort)
        let child1 = create_view_with_z_index(Some(5));
        let child2 = create_view_with_z_index(Some(5));
        let child3 = create_view_with_z_index(Some(5));
        let parent = setup_parent_with_children(vec![child1, child2, child3]);

        let result = collect_stacking_context_items(parent);
        // Same z-index, so DOM order preserved
        assert_eq!(get_view_ids(&result), vec![child1, child2, child3]);
    }

    #[test]
    fn test_mixed_z_index_and_default() {
        // Mix of explicit z-index and default (None = 0)
        let child_default = create_view_no_stacking_context(); // effective 0, no stacking context
        let child_z5 = create_view_with_z_index(Some(5));
        let child_z_neg = create_view_with_z_index(Some(-1));
        // DOM order: default, z5, z_neg
        let parent = setup_parent_with_children(vec![child_default, child_z5, child_z_neg]);

        let result = collect_stacking_context_items(parent);
        // Paint order: -1, 0, 5
        assert_eq!(get_z_indices_from_items(&result), vec![-1, 0, 5]);
    }

    #[test]
    fn test_large_z_index_values() {
        // Test with large z-index values
        let child_max = create_view_with_z_index(Some(i32::MAX));
        let child_min = create_view_with_z_index(Some(i32::MIN));
        let child_zero = create_view_with_z_index(Some(0));
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
        let child_z1 = create_view_with_z_index(Some(1));
        let child_z10 = create_view_with_z_index(Some(10));
        let child_z5 = create_view_with_z_index(Some(5));
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
            .map(|i| create_view_with_z_index(Some(9 - i))) // z-indices: 9, 8, 7, ..., 0
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
        let child1 = create_view_with_z_index(Some(-5));
        let child2 = create_view_with_z_index(Some(-5));
        let child3 = create_view_with_z_index(Some(-5));
        let parent = setup_parent_with_children(vec![child1, child2, child3]);

        let result = collect_stacking_context_items(parent);
        // All same z-index, DOM order preserved
        assert_eq!(get_view_ids(&result), vec![child1, child2, child3]);
        assert_eq!(get_z_indices_from_items(&result), vec![-5, -5, -5]);
    }

    // ========== True CSS Stacking Context Tests ==========

    #[test]
    fn test_stacking_context_children_escape() {
        // Children of a non-stacking-context view should participate in the
        // parent's stacking context and can interleave with siblings
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context, z=0)
        //   │   ├── A1 (z=5, creates context)
        //   │   └── A2 (z=-1, creates context)
        //   └── B (z=3, creates context)
        //
        // Expected paint order: A2 (z=-1), A (z=0), B (z=3), A1 (z=5)

        let a = create_view_no_stacking_context();
        let a1 = create_view_with_z_index(Some(5));
        let a2 = create_view_with_z_index(Some(-1));
        a.set_children_ids(vec![a1, a2]);

        let b = create_view_with_z_index(Some(3));

        let root = setup_parent_with_children(vec![a, b]);

        let result = collect_stacking_context_items(root);

        // A2 should be first (z=-1), then A (z=0), then B (z=3), then A1 (z=5)
        assert_eq!(get_z_indices_from_items(&result), vec![-1, 0, 3, 5]);
        assert_eq!(get_view_ids(&result), vec![a2, a, b, a1]);
    }

    #[test]
    fn test_stacking_context_bounds_children() {
        // Children of a stacking-context view should NOT escape
        //
        // Structure:
        //   Root
        //   ├── A (z=1, creates stacking context)
        //   │   └── A1 (z=100, creates context) - bounded within A
        //   └── B (z=2, creates context)
        //
        // Expected paint order: A (z=1), B (z=2)
        // A1's z=100 doesn't matter - it's inside A's stacking context

        let a = create_view_with_z_index(Some(1));
        let a1 = create_view_with_z_index(Some(100));
        a.set_children_ids(vec![a1]);

        let b = create_view_with_z_index(Some(2));

        let root = setup_parent_with_children(vec![a, b]);

        let result = collect_stacking_context_items(root);

        // Only A and B should be in root's stacking context
        // A1 is bounded within A's stacking context
        assert_eq!(result.len(), 2);
        assert_eq!(get_z_indices_from_items(&result), vec![1, 2]);
        assert_eq!(get_view_ids(&result), vec![a, b]);
    }

    #[test]
    fn test_deeply_nested_stacking_context_escape() {
        // Deeply nested children should escape multiple levels
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context)
        //   │   └── A1 (no stacking context)
        //   │       └── A1a (z=10, creates context)
        //   └── B (z=5, creates context)
        //
        // Expected paint order: A (z=0), A1 (z=0), B (z=5), A1a (z=10)

        let a = create_view_no_stacking_context();
        let a1 = create_view_no_stacking_context();
        let a1a = create_view_with_z_index(Some(10));
        a1.set_children_ids(vec![a1a]);
        a.set_children_ids(vec![a1]);

        let b = create_view_with_z_index(Some(5));

        let root = setup_parent_with_children(vec![a, b]);

        let result = collect_stacking_context_items(root);

        // A1a escapes through A1 and A to participate in root's stacking context
        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 5, 10]);
        assert_eq!(get_view_ids(&result), vec![a, a1, b, a1a]);
    }

    #[test]
    fn test_ancestor_path_tracking() {
        // Verify that ancestor paths are correctly tracked for nested items
        //
        // Structure:
        //   Root
        //   └── A (no stacking context)
        //       └── A1 (z=5, creates context)

        let a = create_view_no_stacking_context();
        let a1 = create_view_with_z_index(Some(5));
        a.set_children_ids(vec![a1]);

        let root = setup_parent_with_children(vec![a]);

        let result = collect_stacking_context_items(root);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].view_id, a);
        assert_eq!(result[1].view_id, a1);
    }

    #[test]
    fn test_negative_z_index_escapes_and_interleaves() {
        // Negative z-index children should escape and sort before z=0
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context)
        //   │   └── A1 (z=-5)
        //   ├── B (z=-2)
        //   └── C (no stacking context)
        //       └── C1 (z=-10)
        //
        // Expected: C1 (-10), A1 (-5), B (-2), A (0), C (0)

        let a = create_view_no_stacking_context();
        let a1 = create_view_with_z_index(Some(-5));
        a.set_children_ids(vec![a1]);

        let b = create_view_with_z_index(Some(-2));

        let c = create_view_no_stacking_context();
        let c1 = create_view_with_z_index(Some(-10));
        c.set_children_ids(vec![c1]);

        let root = setup_parent_with_children(vec![a, b, c]);
        let result = collect_stacking_context_items(root);

        assert_eq!(get_z_indices_from_items(&result), vec![-10, -5, -2, 0, 0]);
        assert_eq!(get_view_ids(&result), vec![c1, a1, b, a, c]);
    }

    #[test]
    fn test_dom_order_preserved_for_escaped_children_same_z() {
        // When escaped children have the same z-index, DOM order should be preserved
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context)
        //   │   └── A1 (z=5)
        //   ├── B (no stacking context)
        //   │   └── B1 (z=5)
        //   └── C (no stacking context)
        //       └── C1 (z=5)
        //
        // Expected: A (0), B (0), C (0), A1 (5), B1 (5), C1 (5)
        // DOM order: A1 before B1 before C1

        let a = create_view_no_stacking_context();
        let a1 = create_view_with_z_index(Some(5));
        a.set_children_ids(vec![a1]);

        let b = create_view_no_stacking_context();
        let b1 = create_view_with_z_index(Some(5));
        b.set_children_ids(vec![b1]);

        let c = create_view_no_stacking_context();
        let c1 = create_view_with_z_index(Some(5));
        c.set_children_ids(vec![c1]);

        let root = setup_parent_with_children(vec![a, b, c]);
        let result = collect_stacking_context_items(root);

        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 0, 5, 5, 5]);
        // DOM order should be preserved for same z-index
        assert_eq!(get_view_ids(&result), vec![a, b, c, a1, b1, c1]);
    }

    #[test]
    fn test_empty_non_stacking_context_view() {
        // Non-stacking-context views with no children should work correctly
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context, no children)
        //   └── B (z=1)

        let a = create_view_no_stacking_context();
        let b = create_view_with_z_index(Some(1));

        let root = setup_parent_with_children(vec![a, b]);
        let result = collect_stacking_context_items(root);

        assert_eq!(get_z_indices_from_items(&result), vec![0, 1]);
        assert_eq!(get_view_ids(&result), vec![a, b]);
    }

    #[test]
    fn test_creates_context_flag_correctness() {
        // Verify the creates_context flag is set correctly for different view types

        let with_z = create_view_with_z_index(Some(5));
        let without_z = create_view_no_stacking_context();
        let with_z_zero = create_view_with_z_index(Some(0));

        let root = setup_parent_with_children(vec![with_z, without_z, with_z_zero]);
        let result = collect_stacking_context_items(root);

        // View with explicit z-index creates context
        assert!(result[1].creates_context); // with_z (sorted to middle due to z=5)
        // View without z-index doesn't create context
        assert!(!result[0].creates_context); // without_z (z=0)
        // View with explicit z-index: 0 DOES create context (unlike z-index: auto)
        assert!(result[2].creates_context); // with_z_zero (z=0 but explicit)
    }

    #[test]
    fn test_complex_nested_stacking_contexts() {
        // Complex scenario with multiple levels of stacking contexts
        //
        // Structure:
        //   Root
        //   ├── A (z=1, creates context)
        //   │   ├── A1 (no stacking context)
        //   │   │   └── A1a (z=100) -- bounded within A
        //   │   └── A2 (z=50) -- bounded within A
        //   ├── B (no stacking context)
        //   │   └── B1 (z=2, creates context)
        //   │       └── B1a (z=999) -- bounded within B1
        //   └── C (z=3)
        //
        // Root's stacking context: A (1), B (0), B1 (2), C (3)
        // A1a and A2 are in A's stacking context
        // B1a is in B1's stacking context

        let a = create_view_with_z_index(Some(1));
        let a1 = create_view_no_stacking_context();
        let a1a = create_view_with_z_index(Some(100));
        a1.set_children_ids(vec![a1a]);
        let a2 = create_view_with_z_index(Some(50));
        a.set_children_ids(vec![a1, a2]);

        let b = create_view_no_stacking_context();
        let b1 = create_view_with_z_index(Some(2));
        let b1a = create_view_with_z_index(Some(999));
        b1.set_children_ids(vec![b1a]);
        b.set_children_ids(vec![b1]);

        let c = create_view_with_z_index(Some(3));

        let root = setup_parent_with_children(vec![a, b, c]);
        let result = collect_stacking_context_items(root);

        // Root's stacking context should have: B (0), A (1), B1 (2), C (3)
        // Note: B escapes but B1 is in root's context because B doesn't create one
        assert_eq!(result.len(), 4);
        assert_eq!(get_z_indices_from_items(&result), vec![0, 1, 2, 3]);
        assert_eq!(get_view_ids(&result), vec![b, a, b1, c]);
    }

    #[test]
    fn test_siblings_interleave_with_escaped_cousins() {
        // Test that escaped children interleave correctly with their parent's siblings
        //
        // Structure:
        //   Root
        //   ├── A (z=5)
        //   ├── B (no stacking context)
        //   │   ├── B1 (z=3)
        //   │   └── B2 (z=7)
        //   └── C (z=6)
        //
        // Expected order: B (0), B1 (3), A (5), C (6), B2 (7)

        let a = create_view_with_z_index(Some(5));

        let b = create_view_no_stacking_context();
        let b1 = create_view_with_z_index(Some(3));
        let b2 = create_view_with_z_index(Some(7));
        b.set_children_ids(vec![b1, b2]);

        let c = create_view_with_z_index(Some(6));

        let root = setup_parent_with_children(vec![a, b, c]);
        let result = collect_stacking_context_items(root);

        assert_eq!(get_z_indices_from_items(&result), vec![0, 3, 5, 6, 7]);
        assert_eq!(get_view_ids(&result), vec![b, b1, a, c, b2]);
    }

    #[test]
    fn test_all_non_stacking_context_tree() {
        // When no view creates a stacking context, all should be collected with z=0
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context)
        //   │   └── A1 (no stacking context)
        //   │       └── A1a (no stacking context)
        //   └── B (no stacking context)
        //
        // All should be in paint order with z=0, DOM order preserved

        let a = create_view_no_stacking_context();
        let a1 = create_view_no_stacking_context();
        let a1a = create_view_no_stacking_context();
        a1.set_children_ids(vec![a1a]);
        a.set_children_ids(vec![a1]);

        let b = create_view_no_stacking_context();

        let root = setup_parent_with_children(vec![a, b]);
        let result = collect_stacking_context_items(root);

        // All z=0, DOM order: A, A1, A1a, B
        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 0, 0]);
        assert_eq!(get_view_ids(&result), vec![a, a1, a1a, b]);
    }

    #[test]
    fn test_stacking_context_at_leaf() {
        // Stacking context at leaf level (no children)
        //
        // Structure:
        //   Root
        //   └── A (no stacking context)
        //       └── A1 (no stacking context)
        //           └── A1a (z=5, leaf with no children)

        let a = create_view_no_stacking_context();
        let a1 = create_view_no_stacking_context();
        let a1a = create_view_with_z_index(Some(5));
        a1.set_children_ids(vec![a1a]);
        a.set_children_ids(vec![a1]);

        let root = setup_parent_with_children(vec![a]);
        let result = collect_stacking_context_items(root);

        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 5]);
        assert_eq!(get_view_ids(&result), vec![a, a1, a1a]);
    }

    #[test]
    fn test_event_dispatch_order_with_escaping() {
        // Event dispatch should be reverse of paint order, even with escaped children
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context)
        //   │   └── A1 (z=10)
        //   └── B (z=5)
        //
        // Paint order: A (0), B (5), A1 (10)
        // Event order: A1 (10), B (5), A (0)

        let a = create_view_no_stacking_context();
        let a1 = create_view_with_z_index(Some(10));
        a.set_children_ids(vec![a1]);

        let b = create_view_with_z_index(Some(5));

        let root = setup_parent_with_children(vec![a, b]);
        let paint_order = collect_stacking_context_items(root);

        assert_eq!(get_z_indices_from_items(&paint_order), vec![0, 5, 10]);

        // Reverse for event dispatch
        let event_z_indices: Vec<i32> = paint_order.iter().rev().map(|item| item.z_index).collect();
        let event_view_ids: Vec<ViewId> =
            paint_order.iter().rev().map(|item| item.view_id).collect();
        assert_eq!(event_z_indices, vec![10, 5, 0]);
        assert_eq!(event_view_ids, vec![a1, b, a]);
    }

    #[test]
    fn test_multiple_children_escape_same_parent() {
        // Multiple children of a non-stacking-context parent all escape
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context)
        //   │   ├── A1 (z=-1)
        //   │   ├── A2 (z=0)
        //   │   ├── A3 (z=1)
        //   │   └── A4 (z=2)
        //   └── B (z=1)
        //
        // Expected: A1 (-1), A (0), A2 (0), A3 (1), B (1), A4 (2)
        // Note: A3 and B both have z=1, A3 comes first due to DOM order

        let a = create_view_no_stacking_context();
        let a1 = create_view_with_z_index(Some(-1));
        let a2 = create_view_with_z_index(Some(0));
        let a3 = create_view_with_z_index(Some(1));
        let a4 = create_view_with_z_index(Some(2));
        a.set_children_ids(vec![a1, a2, a3, a4]);

        let b = create_view_with_z_index(Some(1));

        let root = setup_parent_with_children(vec![a, b]);
        let result = collect_stacking_context_items(root);

        assert_eq!(get_z_indices_from_items(&result), vec![-1, 0, 0, 1, 1, 2]);
        // A comes before A2 at z=0 because A is the parent (encountered first in DOM)
        // A3 comes before B at z=1 because A3's dom_order is smaller
        assert_eq!(get_view_ids(&result), vec![a1, a, a2, a3, b, a4]);
    }

    // ========== Stacking Context Cache Tests ==========

    #[test]
    fn test_stacking_cache_hit_on_second_call() {
        // Second call should return cached value (same result)
        let a = create_view_with_z_index(Some(1));
        let b = create_view_with_z_index(Some(2));
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
        let a = create_view_with_z_index(Some(1));
        let b = create_view_with_z_index(Some(2));
        let root = setup_parent_with_children(vec![a, b]);

        let result1 = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&result1), vec![a, b]);

        // Change a's z-index to be higher than b
        {
            let state = a.state();
            let old_info = state.borrow().stacking_info;
            state.borrow_mut().stacking_info = StackingInfo {
                creates_context: true,
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
        let a = create_view_with_z_index(Some(1));
        let root = setup_parent_with_children(vec![a]);

        let result1 = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&result1), vec![a]);

        // Add a new child
        let b = create_view_with_z_index(Some(2));
        root.set_children_ids(vec![a, b]); // This calls invalidate_stacking_cache

        let result2 = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&result2), vec![a, b]);
    }

    #[test]
    fn test_stacking_cache_invalidation_propagates_to_ancestors() {
        // Invalidating a child should also invalidate ancestor caches
        //
        // Structure:
        //   Root
        //   └── A (no stacking context)
        //       └── A1 (z=5)
        //
        // When A1's z-index changes, root's cache should also be invalidated

        let a = create_view_no_stacking_context();
        let a1 = create_view_with_z_index(Some(5));
        set_children_with_parents(a, vec![a1]);
        let root = setup_parent_with_children(vec![a]);

        let result1 = collect_stacking_context_items(root);
        assert_eq!(get_z_indices_from_items(&result1), vec![0, 5]);

        // Change A1's z-index
        {
            let state = a1.state();
            state.borrow_mut().stacking_info = StackingInfo {
                creates_context: true,
                effective_z_index: -1,
            };
            invalidate_stacking_cache(a1); // Should invalidate root's cache too
        }

        let result2 = collect_stacking_context_items(root);
        // A1 now has negative z-index, should come first
        assert_eq!(get_z_indices_from_items(&result2), vec![-1, 0]);
        assert_eq!(get_view_ids(&result2), vec![a1, a]);
    }

    #[test]
    fn test_stacking_cache_invalidation_on_creates_context_change() {
        // Cache should be invalidated when creates_context flag changes
        //
        // Structure:
        //   Root
        //   ├── A (initially creates stacking context)
        //   │   └── A1 (z=100)
        //   └── B (z=2)
        //
        // When A stops creating a stacking context, A1 should escape

        let a = create_view_with_z_index(Some(1));
        let a1 = create_view_with_z_index(Some(100));
        a.set_children_ids(vec![a1]);

        let b = create_view_with_z_index(Some(2));
        let root = setup_parent_with_children(vec![a, b]);

        let result1 = collect_stacking_context_items(root);
        // A1 is bounded within A's stacking context
        assert_eq!(result1.len(), 2);
        assert_eq!(get_view_ids(&result1), vec![a, b]);

        // Change A to NOT create a stacking context
        {
            let state = a.state();
            let old_info = state.borrow().stacking_info;
            state.borrow_mut().stacking_info = StackingInfo {
                creates_context: false,
                effective_z_index: 0,
            };
            if old_info.creates_context != false {
                invalidate_stacking_cache(a);
            }
        }

        let result2 = collect_stacking_context_items(root);
        // A1 should now escape and be in root's stacking context
        assert_eq!(result2.len(), 3);
        assert_eq!(get_z_indices_from_items(&result2), vec![0, 2, 100]);
        assert_eq!(get_view_ids(&result2), vec![a, b, a1]);
    }

    #[test]
    fn test_stacking_cache_multiple_roots_independent() {
        // Different stacking context roots should have independent caches
        let a1 = create_view_with_z_index(Some(1));
        let a2 = create_view_with_z_index(Some(2));
        let root_a = setup_parent_with_children(vec![a1, a2]);

        let b1 = create_view_with_z_index(Some(10));
        let b2 = create_view_with_z_index(Some(20));
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
        let a = create_view_with_z_index(Some(1));
        let b = create_view_with_z_index(Some(2));
        let c = create_view_with_z_index(Some(3));
        let root = setup_parent_with_children(vec![a, b, c]);

        let result1 = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&result1), vec![a, b, c]);

        // Remove b from children
        root.set_children_ids(vec![a, c]); // This calls invalidate_stacking_cache

        let result2 = collect_stacking_context_items(root);
        assert_eq!(get_view_ids(&result2), vec![a, c]);
    }

    #[test]
    fn test_stacking_cache_invalidation_nested_escaping_child_change() {
        // When a deeply nested child changes, ancestor caches should be invalidated
        //
        // Structure:
        //   Root
        //   └── A (no stacking context)
        //       └── A1 (no stacking context)
        //           └── A1a (z=10, creates context)
        //
        // When A1a changes, root's cache should be invalidated

        let a = create_view_no_stacking_context();
        let a1 = create_view_no_stacking_context();
        let a1a = create_view_with_z_index(Some(10));
        set_children_with_parents(a1, vec![a1a]);
        set_children_with_parents(a, vec![a1]);
        let root = setup_parent_with_children(vec![a]);

        let result1 = collect_stacking_context_items(root);
        assert_eq!(get_z_indices_from_items(&result1), vec![0, 0, 10]);

        // Change A1a's z-index to negative
        {
            let state = a1a.state();
            state.borrow_mut().stacking_info = StackingInfo {
                creates_context: true,
                effective_z_index: -5,
            };
            invalidate_stacking_cache(a1a);
        }

        let result2 = collect_stacking_context_items(root);
        // A1a should now be first due to negative z-index
        assert_eq!(get_z_indices_from_items(&result2), vec![-5, 0, 0]);
        assert_eq!(get_view_ids(&result2), vec![a1a, a, a1]);
    }

    // ========== Fast Path Tests ==========

    #[test]
    fn test_fast_path_all_zero_z_index_preserves_dom_order() {
        // When all z-indices are zero, items should be in DOM order (no sorting needed)
        let a = create_view_no_stacking_context();
        let b = create_view_no_stacking_context();
        let c = create_view_no_stacking_context();
        let root = setup_parent_with_children(vec![a, b, c]);

        let result = collect_stacking_context_items(root);

        // All z-indices are 0, should be in DOM order
        assert_eq!(get_view_ids(&result), vec![a, b, c]);
        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 0]);
    }

    #[test]
    fn test_fast_path_nested_all_zero_z_index() {
        // Nested structure with all z-indices zero should preserve DOM order
        //
        // Structure:
        //   Root
        //   ├── A (no stacking context)
        //   │   ├── A1 (no stacking context)
        //   │   └── A2 (no stacking context)
        //   └── B (no stacking context)

        let a = create_view_no_stacking_context();
        let a1 = create_view_no_stacking_context();
        let a2 = create_view_no_stacking_context();
        set_children_with_parents(a, vec![a1, a2]);

        let b = create_view_no_stacking_context();

        let root = setup_parent_with_children(vec![a, b]);
        let result = collect_stacking_context_items(root);

        // All z-indices are 0, DOM order: A, A1, A2, B
        assert_eq!(get_view_ids(&result), vec![a, a1, a2, b]);
        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 0, 0]);
    }

    #[test]
    fn test_sorting_triggered_by_single_non_zero_z_index() {
        // Even a single non-zero z-index should trigger sorting
        let a = create_view_no_stacking_context();
        let b = create_view_with_z_index(Some(1)); // Only one with z-index
        let c = create_view_no_stacking_context();
        let root = setup_parent_with_children(vec![a, b, c]);

        let result = collect_stacking_context_items(root);

        // b has z=1, so it should come after a and c (which have z=0)
        assert_eq!(get_view_ids(&result), vec![a, c, b]);
        assert_eq!(get_z_indices_from_items(&result), vec![0, 0, 1]);
    }

    #[test]
    fn test_sorting_triggered_by_negative_z_index() {
        // Negative z-index should also trigger sorting
        let a = create_view_no_stacking_context();
        let b = create_view_with_z_index(Some(-1)); // Negative z-index
        let c = create_view_no_stacking_context();
        let root = setup_parent_with_children(vec![a, b, c]);

        let result = collect_stacking_context_items(root);

        // b has z=-1, so it should come before a and c (which have z=0)
        assert_eq!(get_view_ids(&result), vec![b, a, c]);
        assert_eq!(get_z_indices_from_items(&result), vec![-1, 0, 0]);
    }
}
