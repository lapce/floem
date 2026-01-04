//! Tests for position consistency between ViewState fields.
//!
//! These tests verify that the various position-related fields in ViewState
//! are consistent with each other:
//!
//! - `visual_origin`: The view's visual position in window coordinates
//! - `visual_transform`: Complete transform from local to window coords
//! - `layout_rect`: Bounding box in window coords (includes children)
//!
//! Key invariants:
//! - visual_origin.x == visual_transform.translation().x (without scale/rotate)
//! - layout_rect.origin() == visual_origin (for views without CSS transforms)
//! - layout_rect union includes all descendants' bounds

use floem::kurbo::Point;
use floem::prelude::*;
use floem::unit::Pct;
use floem_test::prelude::*;
use serial_test::serial;

// =============================================================================
// visual_origin and visual_transform consistency
// =============================================================================

#[test]
#[serial]
fn test_window_origin_matches_transform_translation_simple() {
    // For a view without scale/rotate, window_origin should match transform translation
    let inner = Empty::new().style(|s| s.size(50.0, 50.0));
    let inner_id = inner.view_id();

    let view = Container::new(inner).style(|s| s.padding(30.0).size(110.0, 110.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let window_origin = inner_id.get_visual_origin();
    let transform = inner_id.get_visual_transform();
    let translation = transform.translation();

    // Without transforms, translation should equal window_origin
    assert!(
        (window_origin.x - translation.x).abs() < 0.1,
        "window_origin.x ({}) should equal translation.x ({})",
        window_origin.x,
        translation.x
    );
    assert!(
        (window_origin.y - translation.y).abs() < 0.1,
        "window_origin.y ({}) should equal translation.y ({})",
        window_origin.y,
        translation.y
    );
}

#[test]
#[serial]
fn test_window_origin_matches_transform_with_translate() {
    // CSS translate should be reflected in both window_origin and transform
    let view = Empty::new().style(|s| s.size(50.0, 50.0).translate_x(25.0).translate_y(15.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
    harness.rebuild();

    let window_origin = id.get_visual_origin();
    let transform = id.get_visual_transform();
    let translation = transform.translation();

    // window_origin includes CSS translate
    assert!(
        (window_origin.x - 25.0).abs() < 0.1,
        "window_origin.x ({}) should be 25 (translate)",
        window_origin.x
    );
    assert!(
        (window_origin.y - 15.0).abs() < 0.1,
        "window_origin.y ({}) should be 15 (translate)",
        window_origin.y
    );

    // Translation should match window_origin for translate-only transforms
    assert!(
        (window_origin.x - translation.x).abs() < 0.1,
        "window_origin.x should match translation.x for translate-only"
    );
}

#[test]
#[serial]
fn test_window_origin_equals_translation_with_scale() {
    // visual_origin is derived from visual_transform, so it equals
    // translation() even for views with CSS transforms.
    let view = Empty::new().style(|s| s.size(100.0, 100.0).scale(Pct(200.0)));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let window_origin = id.get_visual_origin();
    let transform = id.get_visual_transform();
    let translation = transform.translation();

    // For 100x100 with 2x scale around center (50, 50):
    // translation = -50 (to move center to origin for scale)
    assert!(
        (translation.x - (-50.0)).abs() < 0.1,
        "translation.x with scale should be -50, got {}",
        translation.x
    );

    // window_origin now equals translation (single source of truth)
    assert!(
        (window_origin.x - translation.x).abs() < 0.1,
        "window_origin.x ({}) should equal translation.x ({})",
        window_origin.x,
        translation.x
    );
    assert!(
        (window_origin.y - translation.y).abs() < 0.1,
        "window_origin.y ({}) should equal translation.y ({})",
        window_origin.y,
        translation.y
    );
}

// =============================================================================
// layout_rect consistency tests
// =============================================================================

#[test]
#[serial]
fn test_layout_rect_origin_at_window_origin() {
    // For simple views, layout_rect origin should be at window_origin
    let inner = Empty::new().style(|s| s.size(40.0, 40.0));
    let inner_id = inner.view_id();

    let view = Container::new(inner).style(|s| s.padding(20.0).size(80.0, 80.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
    harness.rebuild();

    let window_origin = inner_id.get_visual_origin();
    let layout_rect = inner_id.get_layout_rect();

    assert!(
        (layout_rect.x0 - window_origin.x).abs() < 0.1,
        "layout_rect.x0 ({}) should equal window_origin.x ({})",
        layout_rect.x0,
        window_origin.x
    );
    assert!(
        (layout_rect.y0 - window_origin.y).abs() < 0.1,
        "layout_rect.y0 ({}) should equal window_origin.y ({})",
        layout_rect.y0,
        window_origin.y
    );
}

#[test]
#[serial]
fn test_layout_rect_includes_children() {
    // Parent's layout_rect should include children's bounds
    let child = Empty::new().style(|s| s.size(100.0, 100.0));
    let child_id = child.view_id();

    let parent = Container::new(child).style(|s| s.padding(10.0));
    let parent_id = parent.view_id();

    let mut harness = HeadlessHarness::new_with_size(parent, 200.0, 200.0);
    harness.rebuild();

    let parent_rect = parent_id.get_layout_rect();
    let child_rect = child_id.get_layout_rect();

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
fn test_layout_rect_size_matches_view_size() {
    // For leaf views, layout_rect size should match the view's style size
    let view = Empty::new().style(|s| s.size(75.0, 50.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
    harness.rebuild();

    let layout_rect = id.get_layout_rect();

    assert!(
        (layout_rect.width() - 75.0).abs() < 0.1,
        "layout_rect width should be 75, got {}",
        layout_rect.width()
    );
    assert!(
        (layout_rect.height() - 50.0).abs() < 0.1,
        "layout_rect height should be 50, got {}",
        layout_rect.height()
    );
}

// =============================================================================
// Nested view position consistency
// =============================================================================

#[test]
#[serial]
fn test_deeply_nested_positions_accumulate() {
    // Positions should accumulate correctly through nesting
    let deep = Empty::new().style(|s| s.size(20.0, 20.0));
    let deep_id = deep.view_id();

    // Three levels of nesting with different paddings
    let view = Container::new(
        Container::new(Container::new(deep).style(|s| s.padding(15.0))).style(|s| s.padding(25.0)),
    )
    .style(|s| s.padding(10.0).size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
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
    // Sibling views should have independent positions
    let child1 = Empty::new().style(|s| s.size(30.0, 30.0));
    let child1_id = child1.view_id();

    let child2 = Empty::new().style(|s| s.size(30.0, 30.0));
    let child2_id = child2.view_id();

    // Vertical stack: child1 on top, child2 below
    let view = Stack::vertical((child1, child2)).style(|s| s.gap(10.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
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
    // Without CSS transforms, the transform should only have translation
    let view = Empty::new().style(|s| s.size(50.0, 50.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
    harness.rebuild();

    let css_transform = id.get_transform();
    let coeffs = css_transform.as_coeffs();

    // Identity matrix with possible translation: [1, 0, 0, 1, tx, ty]
    assert!(
        (coeffs[0] - 1.0).abs() < 0.01 && (coeffs[3] - 1.0).abs() < 0.01,
        "Scale should be identity (1.0)"
    );
    assert!(
        coeffs[1].abs() < 0.01 && coeffs[2].abs() < 0.01,
        "Rotation/shear should be zero"
    );
}

#[test]
#[serial]
fn test_visual_transform_invertible() {
    // visual_transform should be invertible for coordinate conversion
    let inner = Empty::new().style(|s| s.size(40.0, 40.0));
    let inner_id = inner.view_id();

    let view = Container::new(inner).style(|s| s.padding(30.0).size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
    harness.rebuild();

    let transform = inner_id.get_visual_transform();
    let inverse = transform.inverse();

    // Apply transform then inverse should give back original point
    let local_point = Point::new(10.0, 15.0);
    let window_point = transform * local_point;
    let back_to_local = inverse * window_point;

    assert!(
        (back_to_local.x - local_point.x).abs() < 0.01,
        "Round-trip x should be preserved: {} -> {}",
        local_point.x,
        back_to_local.x
    );
    assert!(
        (back_to_local.y - local_point.y).abs() < 0.01,
        "Round-trip y should be preserved: {} -> {}",
        local_point.y,
        back_to_local.y
    );
}

#[test]
#[serial]
fn test_visual_transform_with_scale_invertible() {
    // Transform with scale should also be invertible
    let view = Empty::new().style(|s| s.size(100.0, 100.0).scale(Pct(150.0)));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let transform = id.get_visual_transform();
    let inverse = transform.inverse();

    let local_point = Point::new(25.0, 75.0);
    let window_point = transform * local_point;
    let back_to_local = inverse * window_point;

    assert!(
        (back_to_local.x - local_point.x).abs() < 0.01,
        "Scaled round-trip x should be preserved"
    );
    assert!(
        (back_to_local.y - local_point.y).abs() < 0.01,
        "Scaled round-trip y should be preserved"
    );
}
