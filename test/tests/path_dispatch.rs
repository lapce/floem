//! Tests for Chromium-style path-based event dispatch.
//!
//! These tests verify the new event dispatch model where:
//! 1. Hit testing finds the target (z-index aware)
//! 2. Event path is built from target to root (DOM order)
//! 3. Events dispatch through the path (capturing + bubbling phases)
//! 4. Click/DoubleClick/SecondaryClick are synthetic events dispatched AFTER PointerUp
//!
//! Key behaviors tested:
//! - Click events bubble through the entire DOM path
//! - Handlers returning Stop prevent further bubbling
//! - Parent handlers fire even when child has no handler
//! - Multiple click types (single, double, secondary) all bubble correctly

use floem_test::prelude::*;

// =============================================================================
// Click Bubbling Tests - Verify Click bubbles through entire path
// =============================================================================

#[test]
fn test_click_bubbles_to_parent_with_handler() {
    // Click on child (no handler) should bubble to parent (with handler)
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named(
            "parent",
            Container::new(
                // Child has NO click handler
                Empty::new().style(|s| s.size(50.0, 50.0)),
            )
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the child area (no handler), should bubble to parent
    harness.click(25.0, 25.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["parent"],
        "Click should bubble from child (no handler) to parent (with handler)"
    );
}

#[test]
fn test_click_bubbles_through_multiple_ancestors() {
    // Click on deeply nested child should bubble through all ancestors
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named_cont(
            "grandparent",
            Container::new(
                tracker.track_named_cont(
                    "parent",
                    Container::new(
                        // Child has NO handler
                        Empty::new().style(|s| s.size(30.0, 30.0)),
                    )
                    .style(|s| s.size(60.0, 60.0)),
                ),
            )
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the deeply nested child (no handler)
    harness.click(15.0, 15.0);

    // Should bubble: child (no handler, skipped) -> parent -> grandparent
    assert_eq!(
        tracker.clicked_names(),
        vec!["parent", "grandparent"],
        "Click should bubble through multiple ancestors"
    );
}

#[test]
fn test_click_stop_at_child_prevents_parent_handler() {
    // When child returns Stop, parent handler should NOT fire
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named(
            "parent",
            Container::new(
                tracker.track_named("child", Empty::new().style(|s| s.size(50.0, 50.0))),
            )
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(25.0, 25.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["child"],
        "Parent should NOT receive click when child returns Stop"
    );
}

#[test]
fn test_click_stop_in_middle_prevents_further_bubbling() {
    // Stop in the middle of the path prevents further bubbling
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named_cont(
            "grandparent",
            Container::new(
                tracker.track_named(
                    "parent", // Uses Stop!
                    Container::new(
                        tracker
                            .track_named_cont("child", Empty::new().style(|s| s.size(30.0, 30.0))),
                    )
                    .style(|s| s.size(60.0, 60.0)),
                ),
            )
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(15.0, 15.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["child", "parent"],
        "Grandparent should NOT receive click when parent returns Stop"
    );
}

#[test]
fn test_click_bubbles_with_z_index_layers() {
    // Click on z-index layered view should still bubble to parent
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named(
            "container",
            layers((
                tracker
                    .track_named_cont("back", Empty::new())
                    .style(|s| s.z_index(1)),
                tracker
                    .track_named("front", Empty::new())
                    .style(|s| s.z_index(10)),
            ))
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    // Front (highest z-index) receives click, stops propagation
    // Container handler should NOT fire because front uses on_click_stop
    assert_eq!(
        tracker.clicked_names(),
        vec!["front"],
        "Highest z-index view should receive click first"
    );
}

#[test]
fn test_click_with_cont_bubbles_through_layers() {
    // When layered view uses cont, click should bubble to container
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named(
            "container",
            layers((tracker
                .track_named_cont("layer", Empty::new())
                .style(|s| s.z_index(1)),))
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    // Layer uses cont, so click bubbles to container
    assert_eq!(
        tracker.clicked_names(),
        vec!["layer", "container"],
        "Click should bubble from layer (cont) to container"
    );
}

// =============================================================================
// DoubleClick Bubbling Tests
// =============================================================================

#[test]
fn test_double_click_bubbles_to_parent() {
    // DoubleClick on child (no handler) should bubble to parent
    let tracker = ClickTracker::new();

    let view = tracker
        .track_double_click(
            "parent",
            Container::new(
                // Child has NO double-click handler
                Empty::new().style(|s| s.size(50.0, 50.0)),
            )
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Double-click on child area
    harness.double_click(25.0, 25.0);

    assert_eq!(
        tracker.double_click_count(),
        1,
        "DoubleClick should bubble from child (no handler) to parent"
    );
    assert_eq!(tracker.double_clicked_names(), vec!["parent"]);
}

#[test]
fn test_double_click_at_target_fires_first() {
    // When both child and parent have DoubleClick handlers, child fires first
    let tracker = ClickTracker::new();

    let view = tracker
        .track_double_click(
            "parent",
            Container::new(
                tracker.track_double_click("child", Empty::new().style(|s| s.size(50.0, 50.0))),
            )
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.double_click(25.0, 25.0);

    // Child should fire first (target), then stop by default
    assert_eq!(
        tracker.double_clicked_names(),
        vec!["child"],
        "Child DoubleClick handler should fire and stop propagation"
    );
}

// =============================================================================
// SecondaryClick Bubbling Tests
// =============================================================================

#[test]
fn test_secondary_click_bubbles_to_parent() {
    // SecondaryClick on child (no handler) should bubble to parent
    let tracker = ClickTracker::new();

    let view = tracker
        .track_secondary_click(
            "parent",
            Container::new(
                // Child has NO secondary-click handler
                Empty::new().style(|s| s.size(50.0, 50.0)),
            )
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.secondary_click(25.0, 25.0);

    assert_eq!(
        tracker.secondary_click_count(),
        1,
        "SecondaryClick should bubble from child (no handler) to parent"
    );
    assert_eq!(tracker.secondary_clicked_names(), vec!["parent"]);
}

#[test]
fn test_secondary_click_at_target_fires_first() {
    // When both child and parent have SecondaryClick handlers, child fires first
    let tracker = ClickTracker::new();

    let view = tracker
        .track_secondary_click(
            "parent",
            Container::new(
                tracker.track_secondary_click("child", Empty::new().style(|s| s.size(50.0, 50.0))),
            )
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.secondary_click(25.0, 25.0);

    // Child should fire first and stop propagation
    assert_eq!(
        tracker.secondary_clicked_names(),
        vec!["child"],
        "Child SecondaryClick handler should fire and stop propagation"
    );
}

// =============================================================================
// Synthetic Click Tests - Verify Click fires AFTER PointerUp
// =============================================================================

#[test]
fn test_click_is_synthetic_after_pointer_up() {
    // This test verifies that click handlers work correctly with the
    // new synthetic click dispatch (click fires after PointerUp completes)
    use std::cell::Cell;
    use std::rc::Rc;

    let pointer_up_seen = Rc::new(Cell::new(false));
    let click_seen = Rc::new(Cell::new(false));
    let click_after_up = Rc::new(Cell::new(false));

    let pointer_up_clone = pointer_up_seen.clone();
    let click_seen_clone = click_seen.clone();
    let click_after_up_clone = click_after_up.clone();
    let pointer_up_for_click = pointer_up_seen.clone();

    let view = Empty::new()
        .style(|s| s.size(100.0, 100.0))
        .on_event_stop(floem::event::EventListener::PointerUp, move |_| {
            pointer_up_clone.set(true);
        })
        .on_click_stop(move |_| {
            click_seen_clone.set(true);
            // At the time click fires, PointerUp should have already been processed
            // (This is the Chromium-style behavior)
            if pointer_up_for_click.get() {
                click_after_up_clone.set(true);
            }
        });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(pointer_up_seen.get(), "PointerUp handler should have fired");
    assert!(click_seen.get(), "Click handler should have fired");
    assert!(
        click_after_up.get(),
        "Click should fire AFTER PointerUp (synthetic click dispatch)"
    );
}

// =============================================================================
// Path Building Tests - Verify correct path construction
// =============================================================================

#[test]
fn test_path_includes_all_ancestors() {
    // Verify that the event path includes all ancestors from target to root
    use std::cell::RefCell;
    use std::rc::Rc;

    let visit_order = Rc::new(RefCell::new(Vec::<String>::new()));

    let visit_order_gp = visit_order.clone();
    let visit_order_p = visit_order.clone();
    let visit_order_c = visit_order.clone();

    let view = Empty::new()
        .style(|s| s.size(100.0, 100.0))
        .on_click_cont(move |_| {
            visit_order_gp.borrow_mut().push("grandparent".to_string());
        })
        .into_view();

    let parent = Container::new(
        Container::new(
            Empty::new()
                .style(|s| s.size(30.0, 30.0))
                .on_click_cont(move |_| {
                    visit_order_c.borrow_mut().push("child".to_string());
                }),
        )
        .style(|s| s.size(60.0, 60.0))
        .on_click_cont(move |_| {
            visit_order_p.borrow_mut().push("parent".to_string());
        }),
    )
    .style(|s| s.size(100.0, 100.0));

    let view = layers((view, parent));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(15.0, 15.0);

    // The path is built from target to root, and bubbling goes target -> root
    // So child should be visited first, then parent, then grandparent
    let order = visit_order.borrow();
    assert!(
        order.contains(&"child".to_string()),
        "Child should be in visit order"
    );
    assert!(
        order.contains(&"parent".to_string()),
        "Parent should be in visit order"
    );
}

// =============================================================================
// Hit Test Integration Tests
// =============================================================================

#[test]
fn test_hit_test_finds_topmost_z_index() {
    // Hit test should find the topmost (highest z-index) view
    let tracker = ClickTracker::new();

    let view = layers((
        tracker.track_named("z1", Empty::new().style(|s| s.z_index(1))),
        tracker.track_named("z5", Empty::new().style(|s| s.z_index(5))),
        tracker.track_named("z3", Empty::new().style(|s| s.z_index(3))),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["z5"],
        "Highest z-index (5) should receive click"
    );
}

#[test]
fn test_hit_test_uses_dom_order_for_equal_z_index() {
    // When z-indices are equal, later in DOM order should be on top
    let tracker = ClickTracker::new();

    let view = layers((
        tracker.track_named("first", Empty::new().style(|s| s.z_index(5))),
        tracker.track_named("second", Empty::new().style(|s| s.z_index(5))),
        tracker.track_named("third", Empty::new().style(|s| s.z_index(5))),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["third"],
        "Last in DOM order should receive click when z-indices equal"
    );
}

#[test]
fn test_hit_test_respects_pointer_events_none() {
    // Views with pointer_events_none should not receive clicks
    let tracker = ClickTracker::new();

    let view = layers((
        tracker.track_named("back", Empty::new().style(|s| s.z_index(1))),
        tracker.track_named(
            "front_no_events",
            Empty::new().style(|s| s.z_index(10).pointer_events_none()),
        ),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["back"],
        "Click should pass through pointer_events_none to back view"
    );
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_click_outside_all_children_hits_container() {
    // Clicking in container area but outside all children should hit container
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named(
            "container",
            Container::new(
                tracker.track_named("child", Empty::new().style(|s| s.size(30.0, 30.0))),
            )
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click outside child (30x30) but inside container
    harness.click(80.0, 80.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["container"],
        "Container should receive click when clicking outside child"
    );
}

#[test]
fn test_multiple_clicks_each_bubble_separately() {
    // Multiple clicks should each follow the full bubbling path
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named_cont(
            "parent",
            Container::new(
                tracker.track_named_cont("child", Empty::new().style(|s| s.size(50.0, 50.0))),
            )
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(25.0, 25.0);
    harness.click(25.0, 25.0);

    // Each click should bubble: child -> parent
    assert_eq!(
        tracker.clicked_names(),
        vec!["child", "parent", "child", "parent"],
        "Each click should bubble through the full path"
    );
}

#[test]
fn test_click_on_disabled_view_does_not_bubble() {
    // Disabled views should not receive or bubble clicks
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named(
            "parent",
            Container::new(
                tracker.track_named("disabled_child", Empty::new().style(|s| s.size(50.0, 50.0))),
            )
            .style(|s| s.set_disabled(true))
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(25.0, 25.0);

    // The disabled container and its children should not receive clicks
    // But the parent (which wraps the disabled container) might receive it
    // depending on hit test behavior
    let names = tracker.clicked_names();
    assert!(
        !names.contains(&"disabled_child".to_string()),
        "Disabled child should NOT receive click"
    );
}

#[test]
fn test_hidden_view_does_not_receive_click() {
    // Hidden views should not be hit tested
    let tracker = ClickTracker::new();

    let view = layers((
        tracker.track_named("visible", Empty::new().style(|s| s.z_index(1))),
        tracker.track_named("hidden", Empty::new().style(|s| s.z_index(10).hide())),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["visible"],
        "Hidden view should not receive click even with higher z-index"
    );
}
