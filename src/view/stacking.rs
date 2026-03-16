//! Stacking context management for z-index ordering in event dispatch and painting.
//!
//! This module implements a simplified stacking model where:
//! - Every view is implicitly a stacking context
//! - z-index only competes with siblings (children never escape parent boundaries)
//! - DOM order is used as a tiebreaker when z-index values are equal

use crate::ElementId;

/// An item to be painted within a stacking context (direct child of parent).
#[derive(Debug, Clone)]
pub(crate) struct StackingContextItem {
    pub element_id: ElementId,
    pub z_index: i32,
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
pub(crate) fn collect_stacking_context_items_into(
    parent_element_id: ElementId,
    box_tree: &crate::BoxTree,
    scratch: &mut Vec<StackingContextItem>,
) {
    // Iterate through all child visual rectangles in the box tree
    let box_tree_children = box_tree.children_of(parent_element_id.0);
    scratch.clear();
    scratch.reserve(box_tree_children.len());
    let mut prev_z = i32::MIN;
    let mut needs_sort = false;

    // use box tree children (includes all visual rectangles)
    for &child_box_id in box_tree_children {
        // Construct VisualId from box tree node id
        let child_element_id = box_tree.element_id_of(child_box_id).unwrap();

        // Get z-index from box tree
        let z_index = box_tree.z_index(child_box_id).unwrap_or(0);
        needs_sort |= z_index < prev_z;
        prev_z = z_index;

        scratch.push(StackingContextItem {
            element_id: child_element_id,
            z_index,
        });
    }

    // Stable sort keeps DOM order for equal z-index values.
    if needs_sort {
        scratch.sort_by_key(|item| item.z_index);
    }
}
