//! Stacking context management for z-index ordering in event dispatch and painting.
//!
//! This module implements true CSS stacking context semantics where children of
//! non-stacking-context views participate in their ancestor's stacking context.

use smallvec::SmallVec;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::id::ViewId;

/// Type alias for parent chain storage.
/// Uses SmallVec to avoid heap allocation for shallow nesting (common case).
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
    /// Ordered from immediate parent towards root. Used for event bubbling and painting transforms.
    /// Wrapped in Rc to share among siblings (they have the same parent chain).
    pub parent_chain: Rc<ParentChain>,
}

/// Type alias for stacking context item collection.
/// Uses SmallVec to avoid heap allocation for small numbers of items (common case).
pub(crate) type StackingContextItems = SmallVec<[StackingContextItem; 8]>;

// Thread-local cache for stacking context items.
// Key: ViewId of the stacking context root
// Value: Sorted list of items in that stacking context (Rc to avoid cloning on cache hit)
thread_local! {
    static STACKING_CONTEXT_CACHE: RefCell<HashMap<ViewId, Rc<StackingContextItems>>> =
        RefCell::new(HashMap::new());
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
        items.sort_by(|a, b| a.z_index.cmp(&b.z_index).then(a.dom_order.cmp(&b.dom_order)));
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
        // Build the parent chain for children: current view + our parent chain
        // Create a new Rc that all children (siblings) will share
        let mut child_parent_chain = (*parent_chain).clone();
        child_parent_chain.insert(0, view_id);
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

        assert_eq!(
            get_z_indices_from_items(&result),
            vec![0, 0, 0, 5, 5, 5]
        );
        // DOM order should be preserved for same z-index
        assert_eq!(get_view_ids(&result), vec![a, b, c, a1, b1, c1]);
    }
}
