//! Tests for z-index event dispatch behavior.
//!
//! These tests verify that events are dispatched to views based on their
//! visual stacking order (z-index), not just DOM order.

use floem_test::prelude::*;

#[test]
fn test_higher_z_index_receives_click_first() {
    let tracker = ClickTracker::new();

    // Create two overlapping views - higher z-index should receive click
    let view = layers((
        tracker.track_named("z1", Empty::new().style(|s| s.z_index(1))),
        tracker.track_named("z10", Empty::new().style(|s| s.z_index(10))),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);
    harness.click(50.0, 50.0);

    // z-index 10 should have been clicked, z-index 1 should not
    assert_eq!(
        tracker.clicked_names(),
        vec!["z10"],
        "Only z-index 10 should receive the click"
    );
}

#[test]
fn test_negative_z_index_receives_click_last() {
    let tracker = ClickTracker::new();

    // Views with negative z-index should be behind those with z-index 0
    let view = layers((
        tracker.track_named("neg", Empty::new().style(|s| s.z_index(-1))),
        tracker.track_named("zero", Empty::new().style(|s| s.z_index(0))),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);
    harness.click(50.0, 50.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["zero"],
        "z-index 0 should receive click, not negative z-index"
    );
}

#[test]
fn test_dom_order_when_z_index_equal() {
    let tracker = ClickTracker::new();

    // When z-index is equal, last child in DOM order receives events first
    let view = layers((
        tracker.track_named("first", Empty::new().style(|s| s.z_index(5))),
        tracker.track_named("second", Empty::new().style(|s| s.z_index(5))),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);
    harness.click(50.0, 50.0);

    // Last child in DOM order should receive the click
    assert_eq!(
        tracker.clicked_names(),
        vec!["second"],
        "Second view (last in DOM) should receive the click when z-index is equal"
    );
}

#[test]
fn test_stacking_context_boundary() {
    // Test that grandchild's z-index doesn't escape parent's stacking context
    //
    // Structure:
    //   layers
    //   ├── container (z-index: 1)
    //   │   └── grandchild (z-index: 1000) <- should NOT escape parent
    //   └── sibling (z-index: 2) <- should receive click
    //
    let tracker = ClickTracker::new();

    let view = layers((
        Container::new(
            tracker.track_named(
                "grandchild",
                Empty::new().style(|s| s.size(100.0, 100.0).z_index(1000)),
            ),
        )
        .style(|s| s.z_index(1)),
        tracker.track_named("sibling", Empty::new().style(|s| s.z_index(2))),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);
    harness.click(50.0, 50.0);

    // The sibling with z-index 2 should receive the click,
    // NOT the grandchild with z-index 1000 (it's inside z-index 1 container)
    assert_eq!(
        tracker.clicked_names(),
        vec!["sibling"],
        "Sibling with z-index 2 should receive click, not grandchild with z-index 1000"
    );
}

#[test]
fn test_hit_test_respects_z_index() {
    // Test the hit_test function directly
    let view = layers((
        Empty::new().style(|s| s.z_index(1)),
        Empty::new().style(|s| s.z_index(10)),
    ))
    .style(|s| s.size(100.0, 100.0));

    let harness = TestHarness::new_with_size(view, 100.0, 100.0);

    // The view at (50, 50) should be the one with z-index 10
    let hit = harness.view_at(50.0, 50.0);
    assert!(hit.is_some(), "Should hit a view at (50, 50)");
}

#[test]
fn test_click_tracker_reset() {
    let tracker = ClickTracker::new();

    let view = tracker.track(Empty::new().style(|s| s.size(100.0, 100.0)));

    let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);
    assert_eq!(tracker.click_count(), 1);

    harness.click(50.0, 50.0);
    assert_eq!(tracker.click_count(), 2);

    tracker.reset();
    assert_eq!(tracker.click_count(), 0);
    assert!(!tracker.was_clicked());
}

#[test]
fn test_multiple_z_index_layers() {
    // Test sorting with more than 2 elements including negative, zero, and positive
    let tracker = ClickTracker::new();

    let view = layers((
        tracker.track_named("neg5", Empty::new().style(|s| s.z_index(-5))),
        tracker.track_named("pos100", Empty::new().style(|s| s.z_index(100))),
        tracker.track_named("zero", Empty::new().style(|s| s.z_index(0))),
        tracker.track_named("pos10", Empty::new().style(|s| s.z_index(10))),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);
    harness.click(50.0, 50.0);

    // Highest z-index (100) should receive the click
    assert_eq!(
        tracker.clicked_names(),
        vec!["pos100"],
        "z-index 100 should receive click over all others"
    );
}

#[test]
fn test_partial_overlap_click_non_overlapping_region() {
    // Test clicking on a region where only one view exists
    //
    // Layout:
    //   +--------+
    //   | left   |
    //   |   +----+----+
    //   |   |overlap  |
    //   +---+----+    |
    //       |  right  |
    //       +---------+
    //
    let tracker = ClickTracker::new();

    let view = stack((
        // Left view: 0-60 x, 0-60 y
        tracker
            .track_named("left", Empty::new())
            .style(|s| s.absolute().inset_left(0.0).inset_top(0.0).size(60.0, 60.0).z_index(1)),
        // Right view: 40-100 x, 40-100 y (overlaps with left in 40-60 x 40-60 region)
        tracker
            .track_named("right", Empty::new())
            .style(|s| s.absolute().inset_left(40.0).inset_top(40.0).size(60.0, 60.0).z_index(10)),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

    // Click in left-only region (20, 20)
    harness.click(20.0, 20.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["left"],
        "Click at (20,20) should hit only 'left'"
    );

    tracker.reset();

    // Click in right-only region (80, 80)
    harness.click(80.0, 80.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["right"],
        "Click at (80,80) should hit only 'right'"
    );

    tracker.reset();

    // Click in overlap region (50, 50) - right has higher z-index
    harness.click(50.0, 50.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["right"],
        "Click at (50,50) overlap should hit 'right' (higher z-index)"
    );
}

#[test]
fn test_click_outside_all_views() {
    // Clicking outside all views should not trigger any handlers
    let tracker = ClickTracker::new();

    let view = stack((tracker
        .track_named("small", Empty::new())
        .style(|s| s.absolute().inset_left(10.0).inset_top(10.0).size(30.0, 30.0)),))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

    // Click outside the small view
    harness.click(80.0, 80.0);

    assert_eq!(
        tracker.click_count(),
        0,
        "Clicking outside all views should not trigger any handlers"
    );
}

#[test]
fn test_extreme_z_index_values() {
    // Test with very large z-index values
    let tracker = ClickTracker::new();

    let view = layers((
        tracker.track_named("min", Empty::new().style(|s| s.z_index(i32::MIN))),
        tracker.track_named("max", Empty::new().style(|s| s.z_index(i32::MAX))),
        tracker.track_named("zero", Empty::new().style(|s| s.z_index(0))),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);
    harness.click(50.0, 50.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["max"],
        "i32::MAX z-index should receive click"
    );
}
