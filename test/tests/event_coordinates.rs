//! Tests for event coordinate transformation.
//!
//! These tests verify that pointer event coordinates are correctly transformed
//! to view-local coordinates. Key behaviors:
//!
//! - `event.point()` returns coordinates in the receiving view's local space
//! - Nested views accumulate parent offsets (padding, margins, position)
//! - CSS transforms (scale, rotate, translate) affect coordinate transformation
//! - The inverse of `window_transform` is used to convert window coords to local

use floem::kurbo::Point;
use floem::prelude::*;
use floem::unit::Pct;
use floem_test::prelude::*;
use serial_test::serial;
use std::cell::Cell;
use std::rc::Rc;

// =============================================================================
// Basic coordinate tests
// =============================================================================

#[test]
#[serial]
fn test_event_point_at_origin() {
    // View at origin should receive click coordinates directly
    let received_point = Rc::new(Cell::new(Option::<Point>::None));
    let received_point_clone = received_point.clone();

    let view = Empty::new()
        .style(|s| s.size(100.0, 100.0))
        .on_click_stop(move |e| {
            received_point_clone.set(e.point());
        });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(30.0, 40.0);

    let pt = received_point.get().expect("Should receive click event");
    assert!(
        (pt.x - 30.0).abs() < 0.1 && (pt.y - 40.0).abs() < 0.1,
        "View at origin should receive exact click coords. Expected (30, 40), got ({}, {})",
        pt.x,
        pt.y
    );
}

#[test]
#[serial]
fn test_event_point_with_padding() {
    // View inside padded container should receive offset coordinates
    let received_point = Rc::new(Cell::new(Option::<Point>::None));
    let received_point_clone = received_point.clone();

    let inner = Empty::new()
        .style(|s| s.size(50.0, 50.0))
        .on_click_stop(move |e| {
            received_point_clone.set(e.point());
        });

    let view = Container::new(inner).style(|s| s.padding(25.0).size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click at window (40, 45) -> inner local (15, 20)
    harness.click(40.0, 45.0);

    let pt = received_point.get().expect("Should receive click event");
    assert!(
        (pt.x - 15.0).abs() < 0.1 && (pt.y - 20.0).abs() < 0.1,
        "Padded view should receive offset coords. Expected (15, 20), got ({}, {})",
        pt.x,
        pt.y
    );
}

#[test]
#[serial]
fn test_event_point_nested_padding() {
    // Deeply nested view should accumulate all padding offsets
    let received_point = Rc::new(Cell::new(Option::<Point>::None));
    let received_point_clone = received_point.clone();

    let inner = Empty::new()
        .style(|s| s.size(40.0, 40.0))
        .on_click_stop(move |e| {
            received_point_clone.set(e.point());
        });

    // 10 + 20 + 30 = 60 total padding
    let view = Container::new(
        Container::new(Container::new(inner).style(|s| s.padding(30.0))).style(|s| s.padding(20.0)),
    )
    .style(|s| s.padding(10.0).size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Click at window (80, 85) -> inner local (20, 25)
    harness.click(80.0, 85.0);

    let pt = received_point.get().expect("Should receive click event");
    assert!(
        (pt.x - 20.0).abs() < 0.1 && (pt.y - 25.0).abs() < 0.1,
        "Nested view should receive offset coords. Expected (20, 25), got ({}, {})",
        pt.x,
        pt.y
    );
}

// =============================================================================
// CSS translate tests
// =============================================================================

#[test]
#[serial]
fn test_event_point_with_translate() {
    // CSS translate should offset event coordinates
    let received_point = Rc::new(Cell::new(Option::<Point>::None));
    let received_point_clone = received_point.clone();

    let view = Empty::new()
        .style(|s| s.size(50.0, 50.0).translate_x(30.0).translate_y(20.0))
        .on_click_stop(move |e| {
            received_point_clone.set(e.point());
        });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // View is translated, so clicking at window (45, 35) hits the translated view
    // Local coordinate should be (15, 15)
    harness.click(45.0, 35.0);

    let pt = received_point.get().expect("Should receive click event");
    assert!(
        (pt.x - 15.0).abs() < 0.1 && (pt.y - 15.0).abs() < 0.1,
        "Translated view should receive offset coords. Expected (15, 15), got ({}, {})",
        pt.x,
        pt.y
    );
}

// =============================================================================
// CSS scale tests
// =============================================================================

#[test]
#[serial]
fn test_event_point_with_scale_at_center() {
    // CSS scale is center-based, so clicking at center returns center coords
    let received_point = Rc::new(Cell::new(Option::<Point>::None));
    let received_point_clone = received_point.clone();

    let view = Empty::new()
        .style(|s| s.size(100.0, 100.0).scale(Pct(200.0)))
        .on_click_stop(move |e| {
            received_point_clone.set(e.point());
        });

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Click at center (50, 50) - center stays at center with scale
    harness.click(50.0, 50.0);

    let pt = received_point.get().expect("Should receive click event");
    assert!(
        (pt.x - 50.0).abs() < 0.1 && (pt.y - 50.0).abs() < 0.1,
        "Scaled view center click should map to local center. Expected (50, 50), got ({}, {})",
        pt.x,
        pt.y
    );
}

#[test]
#[serial]
fn test_event_point_with_scale_offset() {
    // CSS scale transforms points differently based on distance from center
    let received_point = Rc::new(Cell::new(Option::<Point>::None));
    let received_point_clone = received_point.clone();

    // 100x100 view with 2x scale
    // Center is (50, 50), which stays fixed during scale
    let view = Empty::new()
        .style(|s| s.size(100.0, 100.0).scale(Pct(200.0)))
        .on_click_stop(move |e| {
            received_point_clone.set(e.point());
        });

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Click at window (0, 0)
    // With 2x scale around center (50, 50):
    // inverse = translate(50, 50) * scale(0.5) * translate(-50, -50)
    // (0, 0) -> (-50, -50) -> scale(0.5) -> (-25, -25) -> (+50, +50) -> (25, 25)
    harness.click(0.0, 0.0);

    let pt = received_point.get().expect("Should receive click event");
    assert!(
        (pt.x - 25.0).abs() < 0.1 && (pt.y - 25.0).abs() < 0.1,
        "Scaled view corner click should map correctly. Expected (25, 25), got ({}, {})",
        pt.x,
        pt.y
    );
}

#[test]
#[serial]
fn test_event_point_with_scale_down() {
    // Scale down (50%) should also transform coordinates correctly
    let received_point = Rc::new(Cell::new(Option::<Point>::None));
    let received_point_clone = received_point.clone();

    // 100x100 view with 0.5x scale
    let view = Empty::new()
        .style(|s| s.size(100.0, 100.0).scale(Pct(50.0)))
        .on_click_stop(move |e| {
            received_point_clone.set(e.point());
        });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click at center - should still map to local center
    harness.click(50.0, 50.0);

    let pt = received_point.get().expect("Should receive click event");
    assert!(
        (pt.x - 50.0).abs() < 0.1 && (pt.y - 50.0).abs() < 0.1,
        "Scaled-down center should map to local center. Expected (50, 50), got ({}, {})",
        pt.x,
        pt.y
    );
}

// =============================================================================
// CSS rotate tests
// =============================================================================

#[test]
#[serial]
fn test_event_point_with_rotate_at_center() {
    // Rotation is also center-based, so center stays at center
    let received_point = Rc::new(Cell::new(Option::<Point>::None));
    let received_point_clone = received_point.clone();

    let view = Empty::new()
        .style(|s| s.size(100.0, 100.0).rotate(45.0.deg()))
        .on_click_stop(move |e| {
            received_point_clone.set(e.point());
        });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click at center
    harness.click(50.0, 50.0);

    let pt = received_point.get().expect("Should receive click event");
    assert!(
        (pt.x - 50.0).abs() < 0.1 && (pt.y - 50.0).abs() < 0.1,
        "Rotated view center should map to local center. Expected (50, 50), got ({}, {})",
        pt.x,
        pt.y
    );
}

// =============================================================================
// Combined transform tests
// =============================================================================

#[test]
#[serial]
fn test_event_point_with_nested_transforms() {
    // Nested views with transforms should compound correctly
    let received_point = Rc::new(Cell::new(Option::<Point>::None));
    let received_point_clone = received_point.clone();

    let inner = Empty::new()
        .style(|s| s.size(40.0, 40.0).translate_x(5.0))
        .on_click_stop(move |e| {
            received_point_clone.set(e.point());
        });

    let view = Container::new(inner).style(|s| s.padding(20.0).size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Inner is at x = 20 (padding) + 5 (translate) = 25
    // Click at window (35, 30) -> inner local (10, 10)
    harness.click(35.0, 30.0);

    let pt = received_point.get().expect("Should receive click event");
    assert!(
        (pt.x - 10.0).abs() < 0.1 && (pt.y - 10.0).abs() < 0.1,
        "Nested transforms should compound. Expected (10, 10), got ({}, {})",
        pt.x,
        pt.y
    );
}

#[test]
#[serial]
fn test_event_point_padding_plus_scale() {
    // Padding + scale combination
    let received_point = Rc::new(Cell::new(Option::<Point>::None));
    let received_point_clone = received_point.clone();

    let inner = Empty::new()
        .style(|s| s.size(40.0, 40.0).scale(Pct(200.0)))
        .on_click_stop(move |e| {
            received_point_clone.set(e.point());
        });

    let view = Container::new(inner).style(|s| s.padding(30.0).size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Inner starts at (30, 30) with center at (50, 50)
    // Click at the center of the inner view's position
    harness.click(50.0, 50.0);

    let pt = received_point.get().expect("Should receive click event");
    // Center of 40x40 view is (20, 20) in local coords
    assert!(
        (pt.x - 20.0).abs() < 0.1 && (pt.y - 20.0).abs() < 0.1,
        "Padding + scale center should map correctly. Expected (20, 20), got ({}, {})",
        pt.x,
        pt.y
    );
}

// =============================================================================
// Pointer move coordinate tests
// =============================================================================

#[test]
#[serial]
fn test_pointer_move_coordinates() {
    // Pointer move events should also have correct local coordinates
    let received_points = Rc::new(std::cell::RefCell::new(Vec::<Point>::new()));
    let received_points_clone = received_points.clone();

    let view = Empty::new().style(|s| s.size(100.0, 100.0)).on_event_cont(
        floem::event::EventListener::PointerMove,
        move |e| {
            if let Some(pt) = e.point() {
                received_points_clone.borrow_mut().push(pt);
            }
        },
    );

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.pointer_move(25.0, 30.0);
    harness.pointer_move(75.0, 80.0);

    let points = received_points.borrow();
    assert!(
        points.len() >= 2,
        "Should have received at least 2 move events"
    );

    // Check first move
    let first = points.first().unwrap();
    assert!(
        (first.x - 25.0).abs() < 0.1 && (first.y - 30.0).abs() < 0.1,
        "First move should be at (25, 30), got ({}, {})",
        first.x,
        first.y
    );

    // Check last move
    let last = points.last().unwrap();
    assert!(
        (last.x - 75.0).abs() < 0.1 && (last.y - 80.0).abs() < 0.1,
        "Last move should be at (75, 80), got ({}, {})",
        last.x,
        last.y
    );
}
