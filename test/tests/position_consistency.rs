//! TODO: should we delete these tests? probably yes.
//! Tests for position consistency between ViewState fields.
//!
//! These tests verify that the various position-related fields in ViewState
//! are consistent with each other:
//!
//! - `visual_origin`: The view's visual position in window coordinates
//! - `visual_transform`: Complete transform from local to window coords
//! - `visual_rect`: Bounding box in window coords (includes children)
//!
//! Key invariants:
//! - visual_origin.x == visual_transform.translation().x (without scale/rotate)
//! - layout_rect.origin() == visual_origin (for views without CSS transforms)
//! - layout_rect union includes all descendants' bounds

use floem::kurbo::{Point, Size};
use floem::prelude::*;
use floem::unit::Pct;
use floem_test::prelude::*;
use floem_test::TestRoot;
use serial_test::serial;

// =============================================================================
// layout_rect consistency tests
// =============================================================================

#[test]
#[serial]
fn test_visual_rect_origin_at_window_origin() {
    let root = TestRoot::new();
    // For simple views, layout_rect origin should be at window_origin
    let inner = Empty::new().style(|s| s.size(40.0, 40.0));
    let inner_id = inner.view_id();

    let view = Container::new(inner).style(|s| s.padding(20.0).size(80.0, 80.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);
    harness.rebuild();

    let window_origin = inner_id.get_visual_origin();
    let visual_rect = inner_id.get_visual_rect();

    assert!(
        (visual_rect.x0 - window_origin.x).abs() < 0.1,
        "layout_rect.x0 ({}) should equal window_origin.x ({})",
        visual_rect.x0,
        window_origin.x
    );
    assert!(
        (visual_rect.y0 - window_origin.y).abs() < 0.1,
        "layout_rect.y0 ({}) should equal window_origin.y ({})",
        visual_rect.y0,
        window_origin.y
    );
}

#[test]
#[serial]
fn test_visual_rect_includes_children() {
    let root = TestRoot::new();
    // Parent's layout_rect should include children's bounds
    let child = Empty::new().style(|s| s.size(100.0, 100.0));
    let child_id = child.view_id();

    let parent = Container::new(child).style(|s| s.padding(10.0));
    let parent_id = parent.view_id();

    let mut harness = HeadlessHarness::new_with_size(root, parent, 200.0, 200.0);
    harness.rebuild();

    let parent_rect = parent_id.get_visual_rect();
    let child_rect = child_id.get_visual_rect();

    // Parent's rect should contain child's rect
    assert!(
        parent_rect.x0 <= child_rect.x0,
        "Parent rect.x0 should be <= child rect.x0"
    );
    assert!(
        parent_rect.y0 <= child_rect.y0,
        "Parent rect.y0 should be <= child rect.y0"
    );
    assert!(
        parent_rect.x1 >= child_rect.x1,
        "Parent rect.x1 should be >= child rect.x1"
    );
    assert!(
        parent_rect.y1 >= child_rect.y1,
        "Parent rect.y1 should be >= child rect.y1"
    );
}

#[test]
#[serial]
fn test_visual_rect_size_matches_view_size() {
    let root = TestRoot::new();
    // For leaf views, layout_rect size should match the view's style size
    let view = Empty::new().style(|s| s.size(75.0, 50.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);
    harness.rebuild();

    let visual_rect = id.get_visual_rect();

    assert!(
        (visual_rect.width() - 75.0).abs() < 0.1,
        "layout_rect width should be 75, got {}",
        visual_rect.width()
    );
    assert!(
        (visual_rect.height() - 50.0).abs() < 0.1,
        "layout_rect height should be 50, got {}",
        visual_rect.height()
    );
}

// =============================================================================
// Nested view position consistency
// =============================================================================

#[test]
#[serial]
fn test_deeply_nested_positions_accumulate() {
    let root = TestRoot::new();
    // Positions should accumulate correctly through nesting
    let deep = Empty::new().style(|s| s.size(20.0, 20.0));
    let deep_id = deep.view_id();

    // Three levels of nesting with different paddings
    let view = Container::new(
        Container::new(Container::new(deep).style(|s| s.padding(15.0))).style(|s| s.padding(25.0)),
    )
    .style(|s| s.padding(10.0).size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 200.0, 200.0);
    harness.rebuild();

    let window_origin = deep_id.get_visual_origin();
    let expected = 10.0 + 25.0 + 15.0; // Total padding

    assert!(
        (window_origin.x - expected).abs() < 0.1,
        "Deeply nested window_origin.x should be {}, got {}",
        expected,
        window_origin.x
    );
    assert!(
        (window_origin.y - expected).abs() < 0.1,
        "Deeply nested window_origin.y should be {}, got {}",
        expected,
        window_origin.y
    );
}

#[test]
#[serial]
fn test_sibling_positions_independent() {
    let root = TestRoot::new();
    // Sibling views should have independent positions
    let child1 = Empty::new().style(|s| s.size(30.0, 30.0));
    let child1_id = child1.view_id();

    let child2 = Empty::new().style(|s| s.size(30.0, 30.0));
    let child2_id = child2.view_id();

    // Vertical stack: child1 on top, child2 below
    let view = Stack::vertical((child1, child2)).style(|s| s.gap(10.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);
    harness.rebuild();

    let pos1 = child1_id.get_visual_origin();
    let pos2 = child2_id.get_visual_origin();

    // child1 should be at y=0
    assert!(
        pos1.y.abs() < 0.1,
        "First child should be at y=0, got {}",
        pos1.y
    );

    // child2 should be at y = 30 (child1 height) + 10 (gap) = 40
    assert!(
        (pos2.y - 40.0).abs() < 0.1,
        "Second child should be at y=40, got {}",
        pos2.y
    );
}

// =============================================================================
// Transform consistency tests
// =============================================================================

#[test]
#[serial]
fn test_transform_is_identity_without_css_transforms() {
    let root = TestRoot::new();
    // Without CSS transforms, the visual position should be at 0, 0
    let size = Size::new(50., 50.);
    let view = Empty::new().style(move |s| s.size(size.width, size.height));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);
    harness.rebuild();

    assert!(id.get_visual_rect() == floem::kurbo::Rect::from_origin_size(Point::ZERO, size));
}
