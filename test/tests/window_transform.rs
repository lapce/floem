//! Tests for window_transform correctness.
//!
//! The `window_transform` is the complete coordinate transformation from
//! a view's local coordinate space to window (root) coordinates. It should equal:
//!
//!   translate(window_origin) * scale_rotation
//!
//! These tests verify that this transform is computed correctly for various
//! scenarios including nesting, CSS transforms (scale, rotate), and combinations.

use floem::kurbo::Point;
use floem::prelude::*;
use floem::unit::Pct;
use floem_test::prelude::*;
use serial_test::serial;

// =============================================================================
// Basic transform tests
// =============================================================================

#[test]
#[serial]
fn test_window_transform_at_origin() {
    // A view at the origin with no transforms should have identity transform
    let view = Empty::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let transform = id.get_visual_transform();
    let window_origin = id.get_visual_origin();

    // At origin, transform should be identity (or just the small offset if any)
    assert!(
        (window_origin.x).abs() < 0.1 && (window_origin.y).abs() < 0.1,
        "View at origin should have window_origin near (0,0), got ({}, {})",
        window_origin.x,
        window_origin.y
    );

    // Transform translation should match window_origin
    let translation = transform.translation();
    assert!(
        (translation.x - window_origin.x).abs() < 0.1,
        "Transform translation.x should match window_origin.x"
    );
    assert!(
        (translation.y - window_origin.y).abs() < 0.1,
        "Transform translation.y should match window_origin.y"
    );
}

#[test]
#[serial]
fn test_window_transform_with_padding() {
    // A view inside a container with padding should have window_origin offset
    let inner = Empty::new().style(|s| s.size(50.0, 50.0));
    let inner_id = inner.view_id();

    let view = Container::new(inner).style(|s| s.padding(30.0).size(110.0, 110.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let transform = inner_id.get_visual_transform();
    let window_origin = inner_id.get_visual_origin();

    // Inner view should be at (30, 30) due to parent padding
    assert!(
        (window_origin.x - 30.0).abs() < 0.1,
        "window_origin.x should be 30 (padding), got {}",
        window_origin.x
    );
    assert!(
        (window_origin.y - 30.0).abs() < 0.1,
        "window_origin.y should be 30 (padding), got {}",
        window_origin.y
    );

    // Transform translation should match
    let translation = transform.translation();
    assert!(
        (translation.x - 30.0).abs() < 0.1,
        "Transform translation.x should be 30, got {}",
        translation.x
    );
    assert!(
        (translation.y - 30.0).abs() < 0.1,
        "Transform translation.y should be 30, got {}",
        translation.y
    );
}

#[test]
#[serial]
fn test_window_transform_nested_accumulates() {
    // Deeply nested views should accumulate all parent offsets
    let deep = Empty::new().style(|s| s.size(20.0, 20.0));
    let deep_id = deep.view_id();

    // Nest 3 levels deep: 10 + 20 + 30 = 60 padding total
    let view = Container::new(
        Container::new(Container::new(deep).style(|s| s.padding(30.0))).style(|s| s.padding(20.0)),
    )
    .style(|s| s.padding(10.0).size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let transform = deep_id.get_visual_transform();
    let window_origin = deep_id.get_visual_origin();

    // Total offset: 10 + 20 + 30 = 60
    assert!(
        (window_origin.x - 60.0).abs() < 0.1,
        "Nested window_origin.x should be 60, got {}",
        window_origin.x
    );

    let translation = transform.translation();
    assert!(
        (translation.x - 60.0).abs() < 0.1,
        "Nested transform translation.x should be 60, got {}",
        translation.x
    );
}

// =============================================================================
// CSS Transform tests
// =============================================================================

#[test]
#[serial]
fn test_window_transform_with_scale() {
    // A view with CSS scale transform
    let view = Empty::new().style(|s| s.size(100.0, 100.0).scale(Pct(200.0))); // 2x scale
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 300.0, 300.0);
    harness.rebuild();

    let transform = id.get_visual_transform();
    let css_transform = id.get_transform();

    // CSS transform should have scale
    let css_coeffs = css_transform.as_coeffs();
    assert!(
        (css_coeffs[0] - 2.0).abs() < 0.1, // a coefficient (scale x)
        "CSS transform should have scale 2.0, got {}",
        css_coeffs[0]
    );

    // window_transform should incorporate the scale
    let coeffs = transform.as_coeffs();
    assert!(
        (coeffs[0] - 2.0).abs() < 0.1,
        "window_transform should have scale 2.0, got {}",
        coeffs[0]
    );
}

#[test]
#[serial]
fn test_window_transform_with_translate() {
    // A view with CSS translate
    let view = Empty::new().style(|s| s.size(50.0, 50.0).translate_x(20.0).translate_y(10.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let transform = id.get_visual_transform();
    let window_origin = id.get_visual_origin();

    // CSS translate affects window_origin
    // window_origin should include the translate
    assert!(
        (window_origin.x - 20.0).abs() < 0.1,
        "window_origin.x should include translate (20), got {}",
        window_origin.x
    );
    assert!(
        (window_origin.y - 10.0).abs() < 0.1,
        "window_origin.y should include translate (10), got {}",
        window_origin.y
    );

    // Transform translation should match
    let translation = transform.translation();
    assert!(
        (translation.x - 20.0).abs() < 0.1,
        "Transform translation.x should be 20, got {}",
        translation.x
    );
}

#[test]
#[serial]
fn test_window_transform_with_rotation() {
    // A view with CSS rotation (90 degrees)
    let view = Empty::new().style(|s| {
        s.size(50.0, 50.0).rotate(90.0.deg()) // 90 degrees
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let transform = id.get_visual_transform();
    let css_transform = id.get_transform();

    // CSS transform should have rotation
    // For 90 degree rotation: [cos, sin, -sin, cos, tx, ty] = [0, 1, -1, 0, tx, ty]
    let css_coeffs = css_transform.as_coeffs();
    assert!(
        (css_coeffs[0]).abs() < 0.1 && (css_coeffs[1] - 1.0).abs() < 0.1,
        "CSS transform should be 90 degree rotation, got [{}, {}, {}, {}]",
        css_coeffs[0],
        css_coeffs[1],
        css_coeffs[2],
        css_coeffs[3]
    );

    // window_transform should incorporate the rotation
    let coeffs = transform.as_coeffs();
    assert!(
        (coeffs[0]).abs() < 0.1 && (coeffs[1] - 1.0).abs() < 0.1,
        "window_transform should have 90 degree rotation"
    );
}

#[test]
#[serial]
fn test_window_transform_combined_transforms() {
    // A view with position + translate + scale.
    // window_origin is derived from window_transform, so it equals
    // the transform's translation component.
    let inner = Empty::new().style(|s| {
        s.size(40.0, 40.0).translate_x(10.0).scale(Pct(150.0)) // 1.5x scale
    });
    let inner_id = inner.view_id();

    let view = Container::new(inner).style(|s| s.padding(20.0).size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let transform = inner_id.get_visual_transform();
    let window_origin = inner_id.get_visual_origin();

    // Transform should have scale 1.5
    let coeffs = transform.as_coeffs();
    assert!(
        (coeffs[0] - 1.5).abs() < 0.1,
        "Transform scale should be 1.5, got {}",
        coeffs[0]
    );

    // For a 40x40 element at position (20,20) with translate(10) and 1.5x scale:
    // - Layout position = 20 (padding)
    // - CSS translate = 10
    // - Center = (20, 20)
    // - scale_rotation = T(center) * S(1.5) * T(-center)
    // - This transforms (0,0) to (-10,-10) before position translation
    // - So translation.x = 20 + 10 + (-10) = 20
    assert!(
        (coeffs[4] - 20.0).abs() < 0.1,
        "Transform translation.x should be 20, got {}",
        coeffs[4]
    );

    // window_origin now equals translation (single source of truth)
    let translation = transform.translation();
    assert!(
        (window_origin.x - translation.x).abs() < 0.1,
        "window_origin.x ({}) should equal translation.x ({})",
        window_origin.x,
        translation.x
    );

    // Verify point conversion: local (0,0) maps to x=20
    let local_origin = Point::new(0.0, 0.0);
    let window_point = transform * local_origin;
    assert!(
        (window_point.x - 20.0).abs() < 0.1,
        "Local (0,0) should map to x=20, got {}",
        window_point.x
    );
}

// =============================================================================
// Coordinate conversion tests
// =============================================================================

#[test]
#[serial]
fn test_window_transform_point_conversion() {
    // Test that we can convert points correctly using the transform
    let inner = Empty::new().style(|s| s.size(100.0, 100.0));
    let inner_id = inner.view_id();

    let view = Container::new(inner).style(|s| s.padding(50.0).size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let transform = inner_id.get_visual_transform();

    // Local point (0, 0) should map to window point (50, 50)
    let local_origin = Point::new(0.0, 0.0);
    let window_point = transform * local_origin;
    assert!(
        (window_point.x - 50.0).abs() < 0.1,
        "Local (0,0) should map to window x=50, got {}",
        window_point.x
    );
    assert!(
        (window_point.y - 50.0).abs() < 0.1,
        "Local (0,0) should map to window y=50, got {}",
        window_point.y
    );

    // Local point (25, 25) should map to window point (75, 75)
    let local_center = Point::new(25.0, 25.0);
    let window_center = transform * local_center;
    assert!(
        (window_center.x - 75.0).abs() < 0.1,
        "Local (25,25) should map to window x=75, got {}",
        window_center.x
    );
}

#[test]
#[serial]
fn test_root_to_local_point_conversion() {
    // Test inverse transform: window to local
    let inner = Empty::new().style(|s| s.size(100.0, 100.0));
    let inner_id = inner.view_id();

    let view = Container::new(inner).style(|s| s.padding(50.0).size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let transform = inner_id.get_visual_transform();
    let inverse = transform.inverse();

    // Window point (50, 50) should map to local (0, 0)
    let window_origin = Point::new(50.0, 50.0);
    let local_point = inverse * window_origin;
    assert!(
        (local_point.x).abs() < 0.1,
        "Window (50,50) should map to local x=0, got {}",
        local_point.x
    );
    assert!(
        (local_point.y).abs() < 0.1,
        "Window (50,50) should map to local y=0, got {}",
        local_point.y
    );

    // Window point (100, 100) should map to local (50, 50)
    let window_center = Point::new(100.0, 100.0);
    let local_center = inverse * window_center;
    assert!(
        (local_center.x - 50.0).abs() < 0.1,
        "Window (100,100) should map to local x=50, got {}",
        local_center.x
    );
}

#[test]
#[serial]
fn test_point_conversion_with_scale() {
    // Test point conversion with scale transform
    let view = Empty::new().style(|s| s.size(50.0, 50.0).scale(Pct(200.0))); // 2x scale
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let transform = id.get_visual_transform();

    // With 2x scale, local (10, 10) should map to window (20, 20) + any offset
    let local_point = Point::new(10.0, 10.0);
    let window_point = transform * local_point;

    // The window point should be 2x the local point (plus any translation)
    let translation = transform.translation();
    let expected_x = translation.x + 10.0 * 2.0;
    let expected_y = translation.y + 10.0 * 2.0;

    assert!(
        (window_point.x - expected_x).abs() < 0.1,
        "With 2x scale, local (10,10) x should scale to {}, got {}",
        expected_x,
        window_point.x
    );
    assert!(
        (window_point.y - expected_y).abs() < 0.1,
        "With 2x scale, local (10,10) y should scale to {}, got {}",
        expected_y,
        window_point.y
    );
}

// =============================================================================
// Nested transform tests (parent and child both have transforms)
// =============================================================================

#[test]
#[serial]
fn test_nested_transforms_parent_rotation_child_scale() {
    // Parent has rotation, child has scale
    // The window_transform should include both transforms
    let inner = Empty::new().style(|s| s.size(40.0, 40.0).scale(Pct(200.0)));
    let inner_id = inner.view_id();

    let outer =
        Container::new(inner).style(|s| s.padding(20.0).size(100.0, 100.0).rotate(45.0.deg()));
    let outer_id = outer.view_id();

    let mut harness = HeadlessHarness::new_with_size(outer, 200.0, 200.0);
    harness.rebuild();

    let outer_transform = outer_id.get_visual_transform();
    let inner_transform = inner_id.get_visual_transform();

    // Outer's transform should have rotation (45 degrees)
    let outer_coeffs = outer_transform.as_coeffs();
    // cos(45°) ≈ 0.707, sin(45°) ≈ 0.707
    assert!(
        (outer_coeffs[0] - 0.707).abs() < 0.01,
        "Outer should have cos(45°) in [0], got {}",
        outer_coeffs[0]
    );

    // Inner's transform should include BOTH parent's rotation AND its own scale
    // Expected: parent_rotation * child_scale = rotation(45°) * scale(2)
    // This means: a = cos(45°) * 2 ≈ 1.414, b = sin(45°) * 2 ≈ 1.414
    let inner_coeffs = inner_transform.as_coeffs();

    // Check for rotation component - if [1] (b) is near zero, parent rotation is missing
    let has_rotation = inner_coeffs[1].abs() > 0.1;
    assert!(
        has_rotation,
        "Inner's window_transform should include parent's rotation. \
         Coeffs: {:?}. Expected non-zero b (coeffs[1]) for rotation.",
        inner_coeffs
    );
}
