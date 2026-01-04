//! Tests for hit testing with CSS transforms (scale, rotate).
//!
//! These tests verify that hit testing works correctly when views have CSS transforms
//! applied. Key behaviors:
//!
//! - CSS transforms affect visual appearance
//! - layout_rect is computed with transforms applied (axis-aligned bounding box)
//! - Hit testing uses layout_rect which includes transform effects
//! - Event coordinates are transformed to local space using window_transform inverse
//!
//! Note: CSS transforms in Floem are center-based, meaning scale/rotate happen around
//! the element's center, not its origin.

use floem::kurbo::Point;
use floem::prelude::*;
use floem::unit::Pct;
use floem_test::prelude::*;
use serial_test::serial;
use std::cell::Cell;
use std::rc::Rc;

// =============================================================================
// Basic scaled view hit testing
// =============================================================================

#[test]
#[serial]
fn test_scaled_view_receives_click_at_center() {
    // A view with scale transform should still receive clicks at its center
    let tracker = ClickTracker::new();

    // 50x50 view at origin, scaled 2x
    // Center is at (25, 25) in layout coords
    let view = tracker
        .track_named(
            "scaled",
            Empty::new().style(|s| s.size(50.0, 50.0).scale(Pct(200.0))),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click at the center of the original layout position
    harness.click(25.0, 25.0);

    assert!(
        tracker.was_clicked(),
        "Scaled view should be clickable at its center"
    );
}

#[test]
#[serial]
fn test_scaled_view_miss_far_outside() {
    // Click very far from the scaled view should miss
    let tracker = ClickTracker::new();

    // 50x50 view with 0.5x scale
    let view = tracker
        .track_named(
            "scaled",
            Empty::new().style(|s| s.size(50.0, 50.0).scale(Pct(50.0))),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click at (90, 90) - far outside even the original 50x50 bounds
    harness.click(90.0, 90.0);

    assert!(
        !tracker.was_clicked(),
        "Click far outside should not hit the view"
    );
}

#[test]
#[serial]
fn test_unscaled_view_receives_click() {
    // Baseline: unscaled view should work normally
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named("normal", Empty::new().style(|s| s.size(50.0, 50.0)))
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(25.0, 25.0);

    assert!(
        tracker.was_clicked(),
        "Normal view should receive click at its position"
    );
}

#[test]
#[serial]
fn test_scale_down_still_clickable_in_original_area() {
    // A scaled-down view should still be clickable in its layout area
    // because layout_rect is based on the original size
    let clicked = Rc::new(Cell::new(false));
    let clicked_clone = clicked.clone();

    // 100x100 view scaled to 50%
    let view = Empty::new()
        .style(|s| s.size(100.0, 100.0).scale(Pct(50.0)))
        .on_click_stop(move |_| {
            clicked_clone.set(true);
        });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click at the center
    harness.click(50.0, 50.0);

    assert!(clicked.get(), "Scaled-down view should still be clickable");
}

// =============================================================================
// Rotated view hit testing
// =============================================================================

#[test]
#[serial]
fn test_rotated_view_receives_click_at_center() {
    // A rotated view should still receive clicks at its center
    let tracker = ClickTracker::new();

    // 100x20 bar, rotated 90 degrees
    // The center of the bar is at (50, 10), which stays the same after rotation
    let view = tracker
        .track_named(
            "rotated",
            Empty::new().style(|s| s.size(100.0, 20.0).rotate(90.0.deg())),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click at the center of the bar
    harness.click(50.0, 10.0);

    assert!(
        tracker.was_clicked(),
        "Rotated view should be clickable at its center"
    );
}

#[test]
#[serial]
fn test_rotated_view_miss_far_outside() {
    // Click very far from rotated view should miss
    let tracker = ClickTracker::new();

    // Small view rotated
    let view = tracker
        .track_named(
            "rotated",
            Empty::new().style(|s| s.size(30.0, 30.0).rotate(45.0.deg())),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click far from the rotated view
    harness.click(90.0, 90.0);

    assert!(
        !tracker.was_clicked(),
        "Click far from rotated view should miss"
    );
}

// =============================================================================
// Combined transforms
// =============================================================================

#[test]
#[serial]
fn test_translated_view_receives_click() {
    // View with translate should be clickable at translated position
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named(
            "translated",
            Empty::new().style(|s| s.size(50.0, 50.0).translate_x(20.0).translate_y(20.0)),
        )
        .style(|s| s.size(150.0, 150.0));

    let mut harness = HeadlessHarness::new_with_size(view, 150.0, 150.0);

    // Original position is (0, 0), translated to (20, 20)
    // Click at the translated center (45, 45)
    harness.click(45.0, 45.0);

    assert!(
        tracker.was_clicked(),
        "Translated view should be clickable at translated position"
    );
}

#[test]
#[serial]
fn test_scaled_and_translated_view() {
    // View with both translate and scale
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named(
            "transformed",
            Empty::new().style(|s| {
                s.size(50.0, 50.0)
                    .translate_x(20.0)
                    .translate_y(20.0)
                    .scale(Pct(200.0))
            }),
        )
        .style(|s| s.size(150.0, 150.0));

    let mut harness = HeadlessHarness::new_with_size(view, 150.0, 150.0);

    // Click at the approximate center of the transformed view
    harness.click(45.0, 45.0);

    assert!(
        tracker.was_clicked(),
        "Scaled and translated view should be clickable"
    );
}

// =============================================================================
// Z-index with transforms
// =============================================================================

#[test]
#[serial]
fn test_scaled_view_z_index_ordering() {
    // A scaled view should still respect z-index ordering
    let tracker = ClickTracker::new();

    let view = layers((
        // Back: lower z-index
        tracker
            .track_named("back", Empty::new())
            .style(|s| s.z_index(1)),
        // Front: higher z-index with scale (but layers makes it fill container)
        tracker
            .track_named("front", Empty::new().style(|s| s.scale(Pct(150.0))))
            .style(|s| s.z_index(10)),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click at center - higher z-index should receive click
    harness.click(50.0, 50.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["front"],
        "Higher z-index view should receive click"
    );
}

#[test]
#[serial]
fn test_transform_with_z_index_stacking() {
    // Verify transforms don't break z-index stacking
    let tracker = ClickTracker::new();

    let view = layers((
        tracker
            .track_named("layer1", Empty::new().style(|s| s.rotate(15.0.deg())))
            .style(|s| s.z_index(1)),
        tracker
            .track_named("layer2", Empty::new().style(|s| s.scale(Pct(80.0))))
            .style(|s| s.z_index(5)),
        tracker
            .track_named("layer3", Empty::new().style(|s| s.translate_x(10.0)))
            .style(|s| s.z_index(10)),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Highest z-index should receive click
    harness.click(50.0, 50.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["layer3"],
        "Highest z-index layer should receive click regardless of transforms"
    );
}

// =============================================================================
// Event coordinate transformation tests
// =============================================================================

#[test]
#[serial]
fn test_scaled_view_receives_correct_local_coordinates() {
    // When clicking on a scaled view, the event coordinates should be in local space.
    // CSS scale is center-based, so the math is:
    //   window_transform = translate(center) * scale(s) * translate(-center)
    //   root_to_local = translate(center) * scale(1/s) * translate(-center)
    //
    // For a 100x100 view with 2x scale, center is (50, 50):
    // Clicking at window (50, 50) -> local (50, 50) (center stays at center)
    // Clicking at window (100, 100) -> local (75, 75)
    //   (100, 100) -> (-50, -50) -> (50, 50) -> scale(0.5) -> (25, 25) -> (+50, +50) -> (75, 75)
    let received_point = Rc::new(Cell::new(Option::<Point>::None));
    let received_point_clone = received_point.clone();

    let view = Empty::new()
        .style(|s| s.size(100.0, 100.0).scale(Pct(200.0)))
        .on_click_stop(move |e| {
            received_point_clone.set(e.point());
        });

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Click at the center of the view (50, 50 in window coords)
    // With center-based scaling, the center maps to itself
    harness.click(50.0, 50.0);

    let pt = received_point
        .get()
        .expect("Click event should have a point");
    // The center (50, 50) should map to local (50, 50)
    assert!(
        (pt.x - 50.0).abs() < 1.0 && (pt.y - 50.0).abs() < 1.0,
        "Center click should map to local center. Expected ~(50, 50), got ({}, {})",
        pt.x,
        pt.y
    );
}

#[test]
#[serial]
fn test_nested_transformed_view_receives_correct_local_coordinates() {
    // Deeply nested view with transforms should receive correct local coordinates
    let received_point = Rc::new(Cell::new(Option::<Point>::None));
    let received_point_clone = received_point.clone();

    let inner = Empty::new()
        .style(|s| s.size(40.0, 40.0))
        .on_click_stop(move |e| {
            received_point_clone.set(e.point());
        });

    // Nest with padding: outer padding 30, middle padding 20, inner padding 10
    // Inner view starts at (60, 60) in window coords
    let view = Container::new(
        Container::new(Container::new(inner).style(|s| s.padding(10.0))).style(|s| s.padding(20.0)),
    )
    .style(|s| s.padding(30.0).size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Click at window position (80, 80) which is (20, 20) in inner's local space
    harness.click(80.0, 80.0);

    let pt = received_point
        .get()
        .expect("Click event should have a point");
    assert!(
        (pt.x - 20.0).abs() < 1.0 && (pt.y - 20.0).abs() < 1.0,
        "Nested view should receive correct local coordinates. Expected ~(20, 20), got ({}, {})",
        pt.x,
        pt.y
    );
}
