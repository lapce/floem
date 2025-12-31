//! Tests for ParentView trait and reactive children (derived_children).
//!
//! These tests verify that:
//! - ParentView::child() and children() work correctly
//! - derived_children() updates when signals change
//! - Old children are properly cleaned up when new children are set

use floem::prelude::*;
use floem::views::{Decorators, Empty, Label, Stem};
use floem_test::prelude::*;

/// Test that Stem::new() creates an empty view with no children.
#[test]
fn test_stem_new_has_no_children() {
    let view = Stem::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let children = id.children();
    assert!(children.is_empty(), "New Stem should have no children");
}

/// Test that ParentView::child() adds a single child.
#[test]
fn test_parent_view_child() {
    let view = Stem::new()
        .child(Empty::new().style(|s| s.size(50.0, 50.0)))
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let children = id.children();
    assert_eq!(children.len(), 1, "Stem should have 1 child after .child()");
}

/// Test that ParentView::children() adds multiple children.
#[test]
fn test_parent_view_children() {
    let view = Stem::new()
        .children((
            Empty::new().style(|s| s.size(30.0, 30.0)),
            Empty::new().style(|s| s.size(30.0, 30.0)),
            Empty::new().style(|s| s.size(30.0, 30.0)),
        ))
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let children = id.children();
    assert_eq!(
        children.len(),
        3,
        "Stem should have 3 children after .children() with tuple"
    );
}

/// Test that multiple .child() calls append children.
#[test]
fn test_parent_view_multiple_child_calls() {
    let view = Stem::new()
        .child(Empty::new())
        .child(Empty::new())
        .child(Empty::new())
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let children = id.children();
    assert_eq!(
        children.len(),
        3,
        "Multiple .child() calls should append children"
    );
}

/// Test that .child() and .children() can be mixed.
#[test]
fn test_parent_view_mixed_child_children() {
    let view = Stem::new()
        .child(Empty::new())
        .children((Empty::new(), Empty::new()))
        .child(Empty::new())
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let children = id.children();
    assert_eq!(
        children.len(),
        4,
        "Mixed .child() and .children() calls should append all children"
    );
}

/// Test that derived_children creates initial children.
#[test]
fn test_derived_children_initial() {
    let items = RwSignal::new(vec!["a", "b", "c"]);

    let view = Stem::new()
        .derived_children(move || {
            items
                .get()
                .into_iter()
                .map(|_| Empty::new())
                .collect::<Vec<_>>()
        })
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let children = id.children();
    assert_eq!(
        children.len(),
        3,
        "derived_children should create initial children from signal"
    );
}

/// Test that derived_children updates when signal changes.
#[test]
fn test_derived_children_updates_on_signal_change() {
    let items = RwSignal::new(vec!["a", "b", "c"]);

    let view = Stem::new()
        .derived_children(move || {
            items
                .get()
                .into_iter()
                .map(|_| Empty::new())
                .collect::<Vec<_>>()
        })
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial: 3 children
    assert_eq!(id.children().len(), 3, "Initial should have 3 children");

    // Update signal to have 5 items
    items.set(vec!["a", "b", "c", "d", "e"]);
    harness.rebuild();

    // After update: 5 children
    assert_eq!(
        id.children().len(),
        5,
        "After signal update should have 5 children"
    );

    // Update signal to have 1 item
    items.set(vec!["x"]);
    harness.rebuild();

    // After update: 1 child
    assert_eq!(
        id.children().len(),
        1,
        "After signal update should have 1 child"
    );
}

/// Test that derived_children cleans up old children when updating.
#[test]
fn test_derived_children_cleans_up_old_children() {
    let items = RwSignal::new(vec!["a", "b", "c"]);

    let view = Stem::new()
        .derived_children(move || {
            items
                .get()
                .into_iter()
                .map(|_| Empty::new())
                .collect::<Vec<_>>()
        })
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Get initial children IDs
    let initial_children: Vec<ViewId> = id.children();
    assert_eq!(initial_children.len(), 3);

    // Update signal
    items.set(vec!["x", "y"]);
    harness.rebuild();

    // Get new children IDs
    let new_children: Vec<ViewId> = id.children();
    assert_eq!(new_children.len(), 2);

    // Verify old children are different from new children
    // (They should be completely new views, not reused)
    for old_id in &initial_children {
        assert!(
            !new_children.contains(old_id),
            "Old child should not be in new children list"
        );
    }
}

/// Test that derived_children works with empty signal.
#[test]
fn test_derived_children_empty_signal() {
    let items: RwSignal<Vec<&str>> = RwSignal::new(vec![]);

    let view = Stem::new()
        .derived_children(move || {
            items
                .get()
                .into_iter()
                .map(|_| Empty::new())
                .collect::<Vec<_>>()
        })
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial: 0 children
    assert_eq!(id.children().len(), 0, "Initial should have 0 children");

    // Add items
    items.set(vec!["a", "b"]);
    harness.rebuild();

    assert_eq!(
        id.children().len(),
        2,
        "After adding items should have 2 children"
    );

    // Clear items
    items.set(vec![]);
    harness.rebuild();

    assert_eq!(
        id.children().len(),
        0,
        "After clearing items should have 0 children"
    );
}

/// Test that derived_children works with complex view builders.
#[test]
fn test_derived_children_with_styled_children() {
    let count = RwSignal::new(3);

    let view = Stem::new()
        .derived_children(move || {
            (0..count.get())
                .map(|i| {
                    Empty::new().style(move |s| {
                        s.size(20.0, 20.0).background(if i % 2 == 0 {
                            palette::css::RED
                        } else {
                            palette::css::BLUE
                        })
                    })
                })
                .collect::<Vec<_>>()
        })
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial: 3 children
    let children = id.children();
    assert_eq!(children.len(), 3);

    // Verify first child has RED background
    let style = harness.get_computed_style(children[0]);
    let bg = style.get(floem::style::Background);
    assert!(
        matches!(bg, Some(floem::peniko::Brush::Solid(c)) if c == palette::css::RED),
        "First child should be RED"
    );

    // Verify second child has BLUE background
    let style = harness.get_computed_style(children[1]);
    let bg = style.get(floem::style::Background);
    assert!(
        matches!(bg, Some(floem::peniko::Brush::Solid(c)) if c == palette::css::BLUE),
        "Second child should be BLUE"
    );
}

/// Test that derived_children triggers repaint when children change.
#[test]
fn test_derived_children_triggers_repaint() {
    let items = RwSignal::new(vec!["a"]);

    let view = Stem::new()
        .derived_children(move || {
            items
                .get()
                .into_iter()
                .map(|_| Empty::new())
                .collect::<Vec<_>>()
        })
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Change the signal
    items.set(vec!["a", "b", "c"]);

    // Process update and check if repaint is needed
    let needs_repaint = harness.process_update_no_paint();

    assert!(
        needs_repaint,
        "Changing derived_children signal should trigger repaint"
    );
}

/// Test that Stem view is properly created with children scope.
/// (Testing actual scope cleanup is difficult without access to internals,
/// but we verify the view works correctly with reactive children)
#[test]
fn test_stem_with_derived_children_works() {
    let items = RwSignal::new(vec!["a", "b"]);

    let view = Stem::new()
        .derived_children(move || {
            items
                .get()
                .into_iter()
                .map(|_| Empty::new())
                .collect::<Vec<_>>()
        })
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Verify initial children
    let children_before = id.children();
    assert_eq!(children_before.len(), 2, "Should have 2 initial children");

    // Update the signal multiple times to exercise scope management
    items.set(vec!["x"]);
    harness.rebuild();
    assert_eq!(id.children().len(), 1, "Should have 1 child after update");

    items.set(vec!["a", "b", "c", "d"]);
    harness.rebuild();
    assert_eq!(
        id.children().len(),
        4,
        "Should have 4 children after update"
    );

    // The view should still be functional after multiple updates
    items.set(vec![]);
    harness.rebuild();
    assert_eq!(
        id.children().len(),
        0,
        "Should have 0 children after clearing"
    );
}

/// Test derived_children with click interaction.
#[test]
fn test_derived_children_with_click_interaction() {
    let count = RwSignal::new(1);
    let tracker = ClickTracker::new();

    let view = Stem::new()
        .derived_children({
            let tracker = tracker.clone();
            move || {
                let tracker = tracker.clone();
                (0..count.get())
                    .map({
                        let tracker = tracker.clone();
                        move |i| {
                            tracker.track_named(
                                &format!("child_{}", i),
                                Empty::new().style(|s| s.size(50.0, 50.0)),
                            )
                        }
                    })
                    .collect::<Vec<_>>()
            }
        })
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the child
    harness.click(25.0, 25.0);

    assert!(tracker.was_clicked(), "Child should receive click");
    assert_eq!(
        tracker.clicked_names(),
        vec!["child_0"],
        "First child should be clicked"
    );

    // Reset tracker and add more children
    tracker.reset();
    count.set(3);
    harness.rebuild();

    // Click should still work on new children
    harness.click(25.0, 25.0);

    assert!(tracker.was_clicked(), "New child should receive click");
}

// =============================================================================
// keyed_children tests
// =============================================================================

/// Test that keyed_children creates initial children.
#[test]
fn test_keyed_children_initial() {
    let items = RwSignal::new(vec!["a", "b", "c"]);

    let view = Stem::new()
        .keyed_children(
            move || items.get(),
            |item| *item,
            |_item| Empty::new().style(move |s| s.size(30.0, 30.0)),
        )
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let children = id.children();
    assert_eq!(
        children.len(),
        3,
        "keyed_children should create initial children from signal"
    );
}

/// Test that keyed_children updates when signal changes.
#[test]
fn test_keyed_children_updates_on_signal_change() {
    let items = RwSignal::new(vec!["a", "b", "c"]);

    let view = Stem::new()
        .keyed_children(move || items.get(), |item| *item, |_item| Empty::new())
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial: 3 children
    assert_eq!(id.children().len(), 3, "Initial should have 3 children");

    // Update signal to have 5 items
    items.set(vec!["a", "b", "c", "d", "e"]);
    harness.rebuild();

    // After update: 5 children
    assert_eq!(
        id.children().len(),
        5,
        "After signal update should have 5 children"
    );

    // Update signal to have 1 item
    items.set(vec!["x"]);
    harness.rebuild();

    // After update: 1 child
    assert_eq!(
        id.children().len(),
        1,
        "After signal update should have 1 child"
    );
}

/// Test that keyed_children reuses views for unchanged keys.
#[test]
fn test_keyed_children_reuses_unchanged_views() {
    let items = RwSignal::new(vec!["a", "b", "c"]);

    let view = Stem::new()
        .keyed_children(move || items.get(), |item| *item, |_item| Empty::new())
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Get initial children IDs
    let initial_children: Vec<ViewId> = id.children();
    assert_eq!(initial_children.len(), 3);

    // Update signal - keep "a" and "c", add "d"
    items.set(vec!["a", "c", "d"]);
    harness.rebuild();

    // Get new children IDs
    let new_children: Vec<ViewId> = id.children();
    assert_eq!(new_children.len(), 3);

    // "a" should be reused (same ViewId)
    assert_eq!(
        initial_children[0], new_children[0],
        "View for 'a' should be reused"
    );

    // "c" should be reused (same ViewId, but at different position)
    assert_eq!(
        initial_children[2], new_children[1],
        "View for 'c' should be reused"
    );

    // "d" is new, so it should have a different ViewId
    assert!(
        !initial_children.contains(&new_children[2]),
        "View for 'd' should be new"
    );
}

/// Test that keyed_children works with empty signal.
#[test]
fn test_keyed_children_empty_signal() {
    let items: RwSignal<Vec<&str>> = RwSignal::new(vec![]);

    let view = Stem::new()
        .keyed_children(move || items.get(), |item| *item, |_item| Empty::new())
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial: 0 children
    assert_eq!(id.children().len(), 0, "Initial should have 0 children");

    // Add items
    items.set(vec!["a", "b"]);
    harness.rebuild();

    assert_eq!(
        id.children().len(),
        2,
        "After adding items should have 2 children"
    );

    // Clear items
    items.set(vec![]);
    harness.rebuild();

    assert_eq!(
        id.children().len(),
        0,
        "After clearing items should have 0 children"
    );
}

/// Test that keyed_children handles reordering correctly.
#[test]
fn test_keyed_children_reordering() {
    let items = RwSignal::new(vec!["a", "b", "c"]);

    let view = Stem::new()
        .keyed_children(move || items.get(), |item| *item, |_item| Empty::new())
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Get initial children IDs
    let initial_children: Vec<ViewId> = id.children();
    let (id_a, id_b, id_c) = (
        initial_children[0],
        initial_children[1],
        initial_children[2],
    );

    // Reverse the order
    items.set(vec!["c", "b", "a"]);
    harness.rebuild();

    // Get new children IDs
    let new_children: Vec<ViewId> = id.children();
    assert_eq!(new_children.len(), 3);

    // Views should be reused but in different order
    assert_eq!(new_children[0], id_c, "First should now be 'c'");
    assert_eq!(new_children[1], id_b, "Second should remain 'b'");
    assert_eq!(new_children[2], id_a, "Third should now be 'a'");
}

/// Test that keyed_children works with multiple updates.
#[test]
fn test_keyed_children_multiple_updates() {
    let items = RwSignal::new(vec!["a", "b"]);

    let view = Stem::new()
        .keyed_children(move || items.get(), |item| *item, |_item| Empty::new())
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Verify initial children
    assert_eq!(id.children().len(), 2, "Should have 2 initial children");

    // Update multiple times
    items.set(vec!["x"]);
    harness.rebuild();
    assert_eq!(id.children().len(), 1, "Should have 1 child after update");

    items.set(vec!["a", "b", "c", "d"]);
    harness.rebuild();
    assert_eq!(
        id.children().len(),
        4,
        "Should have 4 children after update"
    );

    items.set(vec![]);
    harness.rebuild();
    assert_eq!(
        id.children().len(),
        0,
        "Should have 0 children after clearing"
    );

    items.set(vec!["new"]);
    harness.rebuild();
    assert_eq!(
        id.children().len(),
        1,
        "Should have 1 child after re-adding"
    );
}

// =============================================================================
// derived_child tests
// =============================================================================

/// Test that derived_child creates initial child.
#[test]
fn test_derived_child_initial() {
    let state = RwSignal::new(1);

    let view = Stem::new()
        .derived_child(move || state.get(), |_value| Empty::new())
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let children = id.children();
    assert_eq!(
        children.len(),
        1,
        "derived_child should create exactly one child"
    );
}

/// Test that derived_child updates when signal changes.
#[test]
fn test_derived_child_updates_on_signal_change() {
    let state = RwSignal::new(1);

    let view = Stem::new()
        .derived_child(
            move || state.get(),
            |_value| Empty::new().style(move |s| s.size(30.0, 30.0)),
        )
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Get initial child ID
    let initial_children: Vec<ViewId> = id.children();
    assert_eq!(initial_children.len(), 1, "Should have 1 child initially");
    let initial_child_id = initial_children[0];

    // Update signal
    state.set(2);
    harness.rebuild();

    // Get new child ID
    let new_children: Vec<ViewId> = id.children();
    assert_eq!(new_children.len(), 1, "Should still have 1 child");
    let new_child_id = new_children[0];

    // Child should be replaced (new ViewId)
    assert_ne!(
        initial_child_id, new_child_id,
        "Child should be recreated when signal changes"
    );
}

/// Test that derived_child passes state to child function.
#[test]
fn test_derived_child_passes_state() {
    let tracker = ClickTracker::new();
    let state = RwSignal::new("child_a");

    let tracker_clone = tracker.clone();
    let view = Stem::new()
        .derived_child(
            move || state.get(),
            move |value| {
                tracker_clone.track_named(value, Empty::new().style(|s| s.size(100.0, 100.0)))
            },
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click and verify the child name
    harness.click(50.0, 50.0);
    assert_eq!(tracker.clicked_names(), vec!["child_a"]);

    // Update state
    tracker.reset();
    state.set("child_b");
    harness.rebuild();

    // Click again and verify new child name
    harness.click(50.0, 50.0);
    assert_eq!(tracker.clicked_names(), vec!["child_b"]);
}

/// Test that derived_child cleans up old children.
#[test]
fn test_derived_child_cleans_up_old_children() {
    let state = RwSignal::new(1);

    let view = Stem::new()
        .derived_child(move || state.get(), |_value| Empty::new())
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Collect initial child IDs
    let mut all_child_ids: Vec<ViewId> = id.children();
    assert_eq!(all_child_ids.len(), 1);

    // Update multiple times
    for i in 2..=5 {
        state.set(i);
        harness.rebuild();
        let new_children = id.children();
        assert_eq!(new_children.len(), 1, "Should always have exactly 1 child");
        all_child_ids.extend(new_children);
    }

    // All child IDs should be unique (old ones were cleaned up and replaced)
    let unique_ids: std::collections::HashSet<ViewId> = all_child_ids.iter().copied().collect();
    assert_eq!(
        unique_ids.len(),
        all_child_ids.len(),
        "All child IDs should be unique (children were replaced)"
    );
}

/// Test that derived_child works with enum states.
#[test]
fn test_derived_child_with_enum() {
    #[derive(Clone, Copy)]
    enum ViewType {
        Empty,
        Label,
    }

    let view_type = RwSignal::new(ViewType::Empty);

    let view = Stem::new()
        .derived_child(
            move || view_type.get(),
            |vt| match vt {
                ViewType::Empty => Empty::new().into_any(),
                ViewType::Label => Label::new("Hello").into_any(),
            },
        )
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial: Empty view
    let initial_children = id.children();
    assert_eq!(initial_children.len(), 1);
    let initial_child_id = initial_children[0];

    // Switch to Label
    view_type.set(ViewType::Label);
    harness.rebuild();

    let new_children = id.children();
    assert_eq!(new_children.len(), 1);
    let new_child_id = new_children[0];

    // Child should have changed
    assert_ne!(initial_child_id, new_child_id);
}

/// Test behavior when mixing static children with derived_child.
///
/// With fully deferred child construction, both `.children()` and `.derived_child()`
/// queue messages that are processed in order. When `derived_child` sets up,
/// it calls `set_children_ids` which replaces any previously added children.
///
/// Note: Mixing static `.children()` with reactive `.derived_child()` is not
/// a recommended pattern. Use one or the other for predictable behavior.
#[test]
fn test_derived_child_with_static_children() {
    let dynamic_state = RwSignal::new(1);

    let view = Stem::new()
        .children([Empty::new(), Empty::new()])
        .derived_child(move || dynamic_state.get(), |_| Empty::new())
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Both .children() and .derived_child() are now deferred.
    // Messages are processed in order:
    // 1. AddChildren adds 2 children
    // 2. SetupReactiveChildren runs derived_child setup which calls set_children_ids,
    //    replacing the 2 static children with 1 dynamic child.
    // Result: only 1 child from derived_child
    assert_eq!(
        id.children().len(),
        1,
        "derived_child's set_children_ids replaces static children"
    );

    // Update dynamic state - derived_child recreates its child
    dynamic_state.set(2);
    harness.rebuild();

    // Still only derived_child's child
    assert_eq!(
        id.children().len(),
        1,
        "After update, still only derived_child's child"
    );
}

// =============================================================================
// ParentView::scope() tests - Context propagation from parent to children
// =============================================================================

use floem::reactive::Scope;
use floem::view::ParentView;
use std::cell::RefCell;
use std::rc::Rc;

/// A container view that provides context to its children via scope.
///
/// This demonstrates the pattern for using ParentView::scope():
/// 1. Create a child scope in the constructor
/// 2. Provide context in that scope
/// 3. Override scope() to return that scope
struct ContextProvider<T: Clone + 'static> {
    id: ViewId,
    scope: Scope,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Clone + 'static> ContextProvider<T> {
    fn new(value: T) -> Self {
        let id = ViewId::new();
        // Create a child scope and provide context in it
        let scope = Scope::current().create_child();
        scope.provide_context(value);
        Self {
            id,
            scope,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T: Clone + 'static> floem::View for ContextProvider<T> {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "ContextProvider".into()
    }
}

impl<T: Clone + 'static> ParentView for ContextProvider<T> {
    fn scope(&self) -> Option<Scope> {
        Some(self.scope)
    }
}

/// Test that child() defers view conversion but not construction.
///
/// When a view is passed to child(), the view itself is already constructed.
/// The deferred part only runs `into_any()` inside the scope.
/// For View types, `into_any()` just boxes the already-constructed view.
///
/// For true lazy construction inside the scope, use reactive methods like
/// `derived_child`, `derived_children`, or `keyed_children` which take closures.
#[test]
fn test_scope_context_with_child_limitation() {
    // Track whether context was accessed
    let context_value = Rc::new(RefCell::new(None::<i32>));
    let context_value_clone = context_value.clone();

    let view = ContextProvider::new(42i32)
        .child({
            // This view is constructed HERE, outside the scope
            // The .child() call only defers the into_any() conversion
            ContextCapturingView::new(context_value_clone)
        })
        .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Context is NOT accessible because the view was constructed
    // before child() was called
    assert_eq!(
        *context_value.borrow(),
        None,
        "View construction happens before child(), so context is not accessible"
    );
}

/// A view that captures context when constructed.
struct ContextCapturingView {
    id: ViewId,
}

impl ContextCapturingView {
    fn new(context_holder: Rc<RefCell<Option<i32>>>) -> Self {
        let id = ViewId::new();
        // This will be executed during deferred construction, inside parent's scope
        *context_holder.borrow_mut() = Scope::current().get_context::<i32>();
        Self { id }
    }
}

impl floem::View for ContextCapturingView {
    fn id(&self) -> ViewId {
        self.id
    }
}

/// Test that children() has the same limitation as child().
///
/// Views passed to children() are already constructed before the call.
/// For true lazy construction, use reactive methods.
#[test]
fn test_scope_context_with_children_limitation() {
    let context_values = Rc::new(RefCell::new(Vec::<Option<String>>::new()));
    let cv1 = context_values.clone();
    let cv2 = context_values.clone();

    let view = ContextProvider::new("hello".to_string())
        .children([
            // These views are constructed HERE, outside the scope
            ContextCapturingStringView::new(cv1),
            ContextCapturingStringView::new(cv2),
        ])
        .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let values = context_values.borrow();
    assert_eq!(values.len(), 2, "Should have 2 context captures");
    // Context is NOT accessible for directly passed views
    assert_eq!(
        values[0], None,
        "View construction happens before children(), no context"
    );
    assert_eq!(
        values[1], None,
        "View construction happens before children(), no context"
    );
}

/// A view that captures String context when constructed.
struct ContextCapturingStringView {
    id: ViewId,
}

impl ContextCapturingStringView {
    fn new(context_holder: Rc<RefCell<Vec<Option<String>>>>) -> Self {
        let id = ViewId::new();
        let value = Scope::current().get_context::<String>();
        context_holder.borrow_mut().push(value);
        Self { id }
    }
}

impl floem::View for ContextCapturingStringView {
    fn id(&self) -> ViewId {
        self.id
    }
}

/// Test that derived_children can access parent's context.
#[test]
fn test_scope_context_with_derived_children() {
    let items = RwSignal::new(vec![1, 2, 3]);
    let context_values = Rc::new(RefCell::new(Vec::<Option<i32>>::new()));
    let context_values_clone = context_values.clone();

    let view = ContextProvider::new(100i32)
        .derived_children(move || {
            // Access context during each derived_children rebuild
            let value = Scope::current().get_context::<i32>();
            context_values_clone.borrow_mut().push(value);
            items
                .get()
                .into_iter()
                .map(|_| Empty::new().style(|s| s.size(30.0, 30.0)))
                .collect::<Vec<_>>()
        })
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial: 3 children
    assert_eq!(id.children().len(), 3);

    // Update to trigger rebuild
    items.set(vec![1, 2]);
    harness.rebuild();

    assert_eq!(id.children().len(), 2);

    // Check context was accessible in all rebuilds
    let values = context_values.borrow();
    assert!(values.len() >= 2, "Should have at least 2 context accesses");
    for (i, value) in values.iter().enumerate() {
        assert_eq!(
            *value,
            Some(100),
            "Context should be accessible in derived_children rebuild {}",
            i
        );
    }
}

/// Test that keyed_children can access parent's context.
#[test]
fn test_scope_context_with_keyed_children() {
    let items = RwSignal::new(vec!["a", "b"]);
    let context_values = Rc::new(RefCell::new(Vec::<Option<String>>::new()));
    let context_values_clone = context_values.clone();

    let view = ContextProvider::new("keyed_context".to_string())
        .keyed_children(move || items.get(), |item| *item, {
            let context_values = context_values_clone.clone();
            move |_item| {
                // Access context during view creation
                let value = Scope::current().get_context::<String>();
                context_values.borrow_mut().push(value);
                Empty::new().style(|s| s.size(30.0, 30.0))
            }
        })
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial: 2 children
    assert_eq!(id.children().len(), 2);

    // Add a new item to trigger creation of new child
    items.set(vec!["a", "b", "c"]);
    harness.rebuild();

    assert_eq!(id.children().len(), 3);

    // Check context was accessible for all created children
    let values = context_values.borrow();
    assert!(values.len() >= 3, "Should have created at least 3 children");
    for (i, value) in values.iter().enumerate() {
        assert_eq!(
            *value,
            Some("keyed_context".to_string()),
            "Context should be accessible for keyed child {}",
            i
        );
    }
}

/// Test that derived_child can access parent's context.
#[test]
fn test_scope_context_with_derived_child() {
    let state = RwSignal::new(1);
    let context_values = Rc::new(RefCell::new(Vec::<Option<f64>>::new()));
    let context_values_clone = context_values.clone();

    let view = ContextProvider::new(3.14f64)
        .derived_child(move || state.get(), {
            let context_values = context_values_clone.clone();
            move |_value| {
                // Access context during child creation
                let ctx = Scope::current().get_context::<f64>();
                context_values.borrow_mut().push(ctx);
                Empty::new().style(|s| s.size(50.0, 50.0))
            }
        })
        .style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    assert_eq!(id.children().len(), 1);

    // Update to trigger rebuild
    state.set(2);
    harness.rebuild();

    assert_eq!(id.children().len(), 1);

    // Check context was accessible in all creations
    let values = context_values.borrow();
    assert!(values.len() >= 2, "Should have at least 2 child creations");
    for (i, value) in values.iter().enumerate() {
        assert_eq!(
            *value,
            Some(3.14),
            "Context should be accessible in derived_child creation {}",
            i
        );
    }
}

/// Test context shadowing with derived_child (which does support context access).
///
/// Unlike child()/children(), derived_child's closure IS called inside the scope,
/// so context is accessible there.
#[test]
fn test_scope_context_shadowing_with_derived_child() {
    let outer_value = Rc::new(RefCell::new(None::<i32>));
    let inner_value = Rc::new(RefCell::new(None::<i32>));
    let outer_clone = outer_value.clone();
    let inner_clone = inner_value.clone();

    let view = ContextProvider::new(1i32)
        .derived_child(
            || (),
            move |_| {
                // This closure runs inside the outer scope
                *outer_clone.borrow_mut() = Scope::current().get_context::<i32>();

                // Inner provider with its own scope
                ContextProvider::new(2i32).derived_child(|| (), {
                    let inner_clone = inner_clone.clone();
                    move |_| {
                        // This closure runs inside the inner scope
                        *inner_clone.borrow_mut() = Scope::current().get_context::<i32>();
                        Empty::new()
                    }
                })
            },
        )
        .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    assert_eq!(
        *outer_value.borrow(),
        Some(1),
        "Outer derived_child closure should see outer context"
    );
    assert_eq!(
        *inner_value.borrow(),
        Some(2),
        "Inner derived_child closure should see inner (shadowed) context"
    );
}

/// A wrapper view that captures context and contains a child.
struct ContextCapturingWrapper {
    id: ViewId,
}

impl ContextCapturingWrapper {
    fn new(context_holder: Rc<RefCell<Option<i32>>>, child: impl floem::View + 'static) -> Self {
        let id = ViewId::new();
        *context_holder.borrow_mut() = Scope::current().get_context::<i32>();
        let child_id = child.id();
        child_id.set_parent(id);
        child_id.set_view(Box::new(child));
        id.set_children_ids(vec![child_id]);
        Self { id }
    }
}

impl floem::View for ContextCapturingWrapper {
    fn id(&self) -> ViewId {
        self.id
    }
}

/// Test that children without a scope-providing parent don't have context.
#[test]
fn test_no_scope_no_context() {
    let context_value = Rc::new(RefCell::new(None::<i32>));
    let context_value_clone = context_value.clone();

    // Use regular Stem which doesn't provide a scope
    let view = Stem::new()
        .child(ContextCapturingView::new(context_value_clone))
        .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    assert_eq!(
        *context_value.borrow(),
        None,
        "Without scope provider, context should be None"
    );
}
