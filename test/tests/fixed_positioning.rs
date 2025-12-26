//! Tests for fixed positioning (position: fixed equivalent).
//!
//! Fixed positioning makes elements:
//! - Sized relative to the viewport (window), not their parent
//! - Positioned at the viewport origin, regardless of parent position
//! - Painted without parent transforms applied
//! - Able to receive events at their viewport-relative positions

use floem::HasViewId;
use floem::headless::HeadlessHarness;
use floem::views::{Clip, Decorators, Empty, Overlay, stack};
use std::cell::Cell;
use std::rc::Rc;

// ============================================================================
// Basic Fixed Positioning Tests
// ============================================================================

#[test]
fn test_fixed_element_fills_viewport() {
    // Test that a fixed element with inset(0) fills the entire viewport.
    //
    // Structure:
    //   stack (100x100)
    //   └── Overlay
    //       └── fixed_container (should fill 100x100 viewport)

    let fixed_container = Empty::new().style(|s| s.fixed().inset(0.0));
    let fixed_id = fixed_container.view_id();

    let view = stack((Overlay::new(fixed_container),)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
    harness.rebuild();

    // Get the layout of the fixed element
    let layout = fixed_id.get_layout().expect("Fixed element should have layout");
    let size = (layout.size.width as f64, layout.size.height as f64);

    assert!(
        (size.0 - 100.0).abs() < 0.1 && (size.1 - 100.0).abs() < 0.1,
        "Fixed element should fill viewport (100x100), got {:?}",
        size
    );
}

#[test]
fn test_fixed_element_ignores_parent_position() {
    // Test that a fixed element is positioned at viewport origin,
    // regardless of where its parent is positioned.
    // We verify this by checking that the fixed element fills the viewport
    // and receives clicks at viewport-relative positions.
    //
    // Structure:
    //   stack (200x200)
    //   └── parent_container (at 50,50, size 100x100)
    //       └── Overlay
    //           └── fixed_child (should be at 0,0, size 200x200)

    let clicked = Rc::new(Cell::new(false));
    let clicked_clone = clicked.clone();

    let fixed_child = Empty::new()
        .style(|s| s.fixed().inset(0.0))
        .on_click_stop(move |_| {
            clicked_clone.set(true);
        });
    let fixed_id = fixed_child.view_id();

    let view = stack((
        stack((Overlay::new(fixed_child),))
            .style(|s| s.absolute().inset_left(50.0).inset_top(50.0).size(100.0, 100.0)),
    ))
    .style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // Fixed element should fill the viewport
    let layout = fixed_id.get_layout().expect("Fixed element should have layout");
    let size = (layout.size.width as f64, layout.size.height as f64);

    assert!(
        (size.0 - 200.0).abs() < 0.1 && (size.1 - 200.0).abs() < 0.1,
        "Fixed element should fill viewport (200x200), got {:?}",
        size
    );

    // Click at (10, 10) - outside parent bounds (which starts at 50,50)
    // but should hit the fixed element which is at viewport origin
    harness.click(10.0, 10.0);
    assert!(
        clicked.get(),
        "Fixed element should receive click at viewport origin position"
    );
}

#[test]
fn test_fixed_element_children_use_viewport_percentages() {
    // Test that children of a fixed element use viewport-relative percentages.
    //
    // Structure:
    //   stack (200x200)
    //   └── parent (100x100 at 50,50)
    //       └── Overlay
    //           └── fixed_container (fills viewport 200x200)
    //               └── child (w_full, h_full - should be 200x200)

    let child = Empty::new().style(|s| s.width_full().height_full());
    let child_id = child.view_id();

    let view = stack((
        stack((Overlay::new(
            stack((child,)).style(|s| s.fixed().inset(0.0)),
        ),))
        .style(|s| s.absolute().inset_left(50.0).inset_top(50.0).size(100.0, 100.0)),
    ))
    .style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // The child should fill the fixed container which fills the viewport
    let layout = child_id.get_layout().expect("Child should have layout");
    let size = (layout.size.width as f64, layout.size.height as f64);

    assert!(
        (size.0 - 200.0).abs() < 0.1 && (size.1 - 200.0).abs() < 0.1,
        "Child of fixed element should fill viewport (200x200), got {:?}",
        size
    );
}

// ============================================================================
// Event Dispatch Tests
// ============================================================================

#[test]
fn test_fixed_element_receives_events_at_viewport_position() {
    // Test that a fixed element receives events at its viewport-relative position,
    // not at a position relative to its DOM parent.
    //
    // Structure:
    //   stack (200x200)
    //   └── parent (100x100 at 50,50)
    //       └── Overlay
    //           └── fixed_container (fills viewport)
    //               └── clickable (fills viewport)
    //
    // Click at (25, 25) should hit the fixed element, even though the parent
    // is at (50, 50).

    let clicked = Rc::new(Cell::new(false));
    let clicked_clone = clicked.clone();

    let view = stack((
        stack((Overlay::new(
            Empty::new()
                .style(|s| s.fixed().inset(0.0))
                .on_click_stop(move |_| {
                    clicked_clone.set(true);
                }),
        ),))
        .style(|s| s.absolute().inset_left(50.0).inset_top(50.0).size(100.0, 100.0)),
    ))
    .style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Click at (25, 25) - outside parent bounds but inside fixed element
    harness.click(25.0, 25.0);

    assert!(
        clicked.get(),
        "Fixed element should receive click at viewport position (25, 25)"
    );
}

#[test]
fn test_fixed_element_blocks_events_to_views_behind() {
    // Test that a fixed element blocks events to views behind it.
    //
    // Structure:
    //   stack (100x100)
    //   ├── background  <-- should NOT receive click
    //   └── Overlay
    //       └── fixed_element  <-- should receive click

    let clicked_bg = Rc::new(Cell::new(false));
    let clicked_fixed = Rc::new(Cell::new(false));

    let bg_clone = clicked_bg.clone();
    let fixed_clone = clicked_fixed.clone();

    let view = stack((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .on_click_stop(move |_| {
                bg_clone.set(true);
            }),
        Overlay::new(
            Empty::new()
                .style(|s| s.fixed().inset(0.0))
                .on_click_stop(move |_| {
                    fixed_clone.set(true);
                }),
        ),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(clicked_fixed.get(), "Fixed element should receive click");
    assert!(
        !clicked_bg.get(),
        "Background should NOT receive click (blocked by fixed element)"
    );
}

// ============================================================================
// Clip Escape Tests
// ============================================================================

#[test]
fn test_fixed_element_escapes_parent_clip() {
    // Test that a fixed element inside a Clip parent still receives events
    // outside the clip bounds.
    //
    // Structure:
    //   stack (100x100)
    //   ├── Clip (50x50 at 0,0)
    //   │   └── stack
    //   │       └── Overlay
    //   │           └── fixed_element (fills viewport)
    //   └── background
    //
    // Click at (75, 25) should hit fixed_element even though it's outside clip.

    let clicked_fixed = Rc::new(Cell::new(false));
    let clicked_bg = Rc::new(Cell::new(false));

    let fixed_clone = clicked_fixed.clone();
    let bg_clone = clicked_bg.clone();

    let view = stack((
        Clip::new(stack((Overlay::new(
            Empty::new()
                .style(|s| s.fixed().inset(0.0))
                .on_click_stop(move |_| {
                    fixed_clone.set(true);
                }),
        ),)))
        .style(|s| s.absolute().inset_left(0.0).inset_top(0.0).size(50.0, 50.0)),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(-1))
            .on_click_stop(move |_| {
                bg_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click outside clip bounds
    harness.click(75.0, 25.0);

    assert!(
        clicked_fixed.get(),
        "Fixed element should receive click outside clip bounds"
    );
    assert!(
        !clicked_bg.get(),
        "Background should NOT receive click (blocked by fixed element)"
    );
}

// ============================================================================
// Visibility Tests
// ============================================================================

#[test]
fn test_hidden_fixed_element_does_not_block_events() {
    // Test that a hidden fixed element does not block events to views below.
    //
    // Structure:
    //   stack (100x100)
    //   ├── background  <-- should receive click
    //   └── Overlay
    //       └── fixed_element (hidden)

    let clicked_bg = Rc::new(Cell::new(false));
    let clicked_fixed = Rc::new(Cell::new(false));

    let bg_clone = clicked_bg.clone();
    let fixed_clone = clicked_fixed.clone();

    let view = stack((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .on_click_stop(move |_| {
                bg_clone.set(true);
            }),
        Overlay::new(
            Empty::new()
                .style(|s| {
                    s.fixed()
                        .inset(0.0)
                        .display(floem::taffy::Display::None)
                })
                .on_click_stop(move |_| {
                    fixed_clone.set(true);
                }),
        ),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_bg.get(),
        "Background should receive click when fixed element is hidden"
    );
    assert!(
        !clicked_fixed.get(),
        "Hidden fixed element should NOT receive click"
    );
}

// Note: Visibility toggle with RwSignal is covered in test_hidden_fixed_element_does_not_block_events
// which uses display(Display::None) on the fixed element content itself.

// ============================================================================
// Paint Order Tests
// ============================================================================

#[test]
fn test_fixed_element_in_paint_order() {
    // Test that a fixed element appears in the paint order.

    let fixed_element = Empty::new().style(|s| s.fixed().inset(0.0));
    let fixed_id = fixed_element.view_id();

    let view = stack((Overlay::new(fixed_element),)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let paint_order = harness.paint_and_get_order();

    let fixed_pos = paint_order.iter().position(|&id| id == fixed_id);

    assert!(
        fixed_pos.is_some(),
        "Fixed element should be in paint order"
    );
}

#[test]
fn test_fixed_element_painted_after_regular_views() {
    // Test that a fixed overlay element is painted after regular views.
    //
    // Structure:
    //   stack (100x100)
    //   ├── regular_view (z-index: 100)  <-- painted first
    //   └── Overlay
    //       └── fixed_element  <-- painted last (overlay + fixed)

    let regular = Empty::new().style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100));
    let regular_id = regular.view_id();

    let fixed_element = Empty::new().style(|s| s.fixed().inset(0.0));
    let fixed_id = fixed_element.view_id();

    let view = stack((regular, Overlay::new(fixed_element))).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let paint_order = harness.paint_and_get_order();

    let regular_pos = paint_order.iter().position(|&id| id == regular_id);
    let fixed_pos = paint_order.iter().position(|&id| id == fixed_id);

    assert!(regular_pos.is_some(), "Regular view should be in paint order");
    assert!(fixed_pos.is_some(), "Fixed element should be in paint order");

    assert!(
        regular_pos.unwrap() < fixed_pos.unwrap(),
        "Fixed overlay element should be painted AFTER regular view"
    );
}

// ============================================================================
// Different Viewport Sizes Tests
// ============================================================================

#[test]
fn test_fixed_element_fills_large_viewport() {
    // Test that a fixed element correctly fills a large viewport.
    // This verifies the fixed sizing works with different initial viewport sizes.

    let fixed_element = Empty::new().style(|s| s.fixed().inset(0.0));
    let fixed_id = fixed_element.view_id();

    let view = stack((Overlay::new(fixed_element),)).style(|s| s.size(100.0, 100.0));

    // Use a larger viewport than the root stack
    let mut harness = HeadlessHarness::new_with_size(view, 500.0, 400.0);
    harness.rebuild();

    let layout = fixed_id.get_layout().expect("Fixed element should have layout");
    let size = (layout.size.width as f64, layout.size.height as f64);

    assert!(
        (size.0 - 500.0).abs() < 0.1 && (size.1 - 400.0).abs() < 0.1,
        "Fixed element should fill viewport (500x400), got {:?}",
        size
    );
}

// ============================================================================
// Dialog Centering Pattern Tests
// ============================================================================

#[test]
fn test_fixed_container_child_percentage_positioning() {
    // Test the dialog centering pattern:
    // - Fixed container fills viewport
    // - Child uses absolute + left_1_2() + top_1_2() to center
    // - Child should be at 50% of VIEWPORT, not 50% of DOM parent
    //
    // Structure:
    //   stack (400x300 viewport)
    //   └── nested_parent (at 100,100, size 50x50) <- simulates dialog in scroll
    //       └── Overlay
    //           └── fixed_container (fills viewport 400x300)
    //               └── centered_child (absolute, left_1_2, top_1_2, 100x80)
    //
    // Expected:
    // - centered_child should be at (200, 150) = 50% of viewport
    // - NOT at (25, 25) = 50% of nested_parent

    use floem::unit::Pct;

    let centered_child = Empty::new().style(|s| {
        s.absolute()
            .inset_left(Pct(50.0))
            .inset_top(Pct(50.0))
            .size(100.0, 80.0)
    });
    let centered_id = centered_child.view_id();

    let fixed_container = stack((centered_child,)).style(|s| {
        s.fixed().inset(0.0).width_full().height_full()
    });
    let fixed_id = fixed_container.view_id();

    let view = stack((
        // Nested parent at offset position (simulates dialog inside scroll container)
        stack((Overlay::new(fixed_container),))
            .style(|s| s.absolute().inset_left(100.0).inset_top(100.0).size(50.0, 50.0)),
    ))
    .style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 300.0);
    harness.rebuild();

    // Fixed container should fill viewport
    let fixed_layout = fixed_id.get_layout().expect("Fixed container should have layout");
    eprintln!(
        "Fixed container: pos=({}, {}), size={}x{}",
        fixed_layout.location.x, fixed_layout.location.y,
        fixed_layout.size.width, fixed_layout.size.height
    );
    assert!(
        (fixed_layout.size.width - 400.0).abs() < 0.1,
        "Fixed container width should be 400, got {}",
        fixed_layout.size.width
    );
    assert!(
        (fixed_layout.size.height - 300.0).abs() < 0.1,
        "Fixed container height should be 300, got {}",
        fixed_layout.size.height
    );

    // Centered child should be at 50% of VIEWPORT (200, 150)
    // NOT at 50% of nested_parent (which would be wrong)
    let centered_layout = centered_id.get_layout().expect("Centered child should have layout");
    eprintln!(
        "Centered child: pos=({}, {}), size={}x{}",
        centered_layout.location.x, centered_layout.location.y,
        centered_layout.size.width, centered_layout.size.height
    );

    assert!(
        (centered_layout.location.x - 200.0).abs() < 0.1,
        "Centered child x should be 200 (50% of viewport 400), got {}",
        centered_layout.location.x
    );
    assert!(
        (centered_layout.location.y - 150.0).abs() < 0.1,
        "Centered child y should be 150 (50% of viewport 300), got {}",
        centered_layout.location.y
    );
}

#[test]
fn test_fixed_container_centered_child_receives_click() {
    // Test that a centered child inside a fixed container receives clicks
    // at the correct viewport-relative position.
    //
    // Structure:
    //   stack (400x300 viewport)
    //   └── nested_parent (at 100,100, size 50x50)
    //       └── Overlay
    //           └── fixed_container (fills viewport)
    //               └── centered_child (at 200,150, size 100x80)
    //
    // Click at (250, 190) = center of child at (200,150) with size 100x80
    // Should hit the child, not miss due to wrong coordinate mapping

    use floem::unit::Pct;

    let clicked = Rc::new(Cell::new(false));
    let clicked_clone = clicked.clone();

    let centered_child = Empty::new()
        .style(|s| {
            s.absolute()
                .inset_left(Pct(50.0))
                .inset_top(Pct(50.0))
                .size(100.0, 80.0)
        })
        .on_click_stop(move |_| {
            clicked_clone.set(true);
        });

    let fixed_container = stack((centered_child,)).style(|s| {
        s.fixed().inset(0.0).width_full().height_full()
    });

    let view = stack((
        stack((Overlay::new(fixed_container),))
            .style(|s| s.absolute().inset_left(100.0).inset_top(100.0).size(50.0, 50.0)),
    ))
    .style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 300.0);

    // Click at center of the centered child: (200 + 50, 150 + 40) = (250, 190)
    harness.click(250.0, 190.0);

    assert!(
        clicked.get(),
        "Centered child should receive click at viewport position (250, 190)"
    );
}

#[test]
fn test_fixed_container_with_translate_centering() {
    // Test the complete dialog centering pattern with translate:
    // position: absolute; left: 50%; top: 50%; transform: translate(-50%, -50%);
    //
    // This should center the element visually in the viewport, not in the DOM parent.

    use floem::unit::Pct;

    let centered_child = Empty::new().style(|s| {
        s.absolute()
            .inset_left(Pct(50.0))
            .inset_top(Pct(50.0))
            .translate_x(Pct(-50.0))
            .translate_y(Pct(-50.0))
            .size(100.0, 80.0)
    });
    let centered_id = centered_child.view_id();

    let fixed_container = stack((centered_child,)).style(|s| {
        s.fixed().inset(0.0).width_full().height_full()
    });

    let view = stack((
        // Nested at offset position
        stack((Overlay::new(fixed_container),))
            .style(|s| s.absolute().inset_left(100.0).inset_top(100.0).size(50.0, 50.0)),
    ))
    .style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 300.0);
    harness.rebuild();

    let layout = centered_id.get_layout().expect("Should have layout");
    let transform = centered_id.get_transform();
    let coeffs = transform.as_coeffs();

    eprintln!(
        "Centered with translate: pos=({}, {}), size={}x{}, transform=({}, {})",
        layout.location.x, layout.location.y,
        layout.size.width, layout.size.height,
        coeffs[4], coeffs[5]
    );

    // Position should be at 50% of viewport
    assert!(
        (layout.location.x - 200.0).abs() < 0.1,
        "x should be 200 (50% of 400), got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 150.0).abs() < 0.1,
        "y should be 150 (50% of 300), got {}",
        layout.location.y
    );

    // Transform should offset by -50% of element size
    assert!(
        (coeffs[4] - (-50.0)).abs() < 0.1,
        "translate_x should be -50 (-50% of 100), got {}",
        coeffs[4]
    );
    assert!(
        (coeffs[5] - (-40.0)).abs() < 0.1,
        "translate_y should be -40 (-50% of 80), got {}",
        coeffs[5]
    );

    // Visual center should be at (200-50, 150-40) = (150, 110)
    // which is the center of the viewport minus half the element size
}

// ============================================================================
// Window Coordinate Verification Tests
// ============================================================================
// These tests verify the actual window_origin values used for painting,
// ensuring fixed elements are positioned at the correct window coordinates.

#[test]
fn test_fixed_element_window_origin() {
    // Test that a fixed element's window_origin is its Taffy layout position,
    // NOT offset by the parent's position.
    //
    // Structure:
    //   stack (400x300 viewport)
    //   └── parent (at 100,100)
    //       └── Overlay
    //           └── fixed_child (inset_left: 10, inset_top: 20, size: 50x50)
    //
    // Expected:
    // - fixed_child's window_origin should be (10, 20) - the Taffy position
    // - NOT (110, 120) which would be parent + Taffy position

    let fixed_child = Empty::new().style(|s| {
        s.fixed()
            .inset_left(10.0)
            .inset_top(20.0)
            .size(50.0, 50.0)
    });
    let fixed_id = fixed_child.view_id();

    let view = stack((
        // Parent at offset position
        stack((Overlay::new(fixed_child),))
            .style(|s| s.absolute().inset_left(100.0).inset_top(100.0).size(50.0, 50.0)),
    ))
    .style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 300.0);
    harness.rebuild();

    // Get the window_origin - this is what determines paint position
    let window_origin = fixed_id.get_window_origin();
    let layout_rect = fixed_id.get_layout_rect();

    eprintln!("Fixed element window_origin: ({}, {})", window_origin.x, window_origin.y);
    eprintln!("Fixed element layout_rect: {:?}", layout_rect);

    // window_origin should be the Taffy position (10, 20), NOT offset by parent
    assert!(
        (window_origin.x - 10.0).abs() < 0.1,
        "window_origin.x should be 10 (from inset_left), got {}. If this is 110, the parent offset is being added incorrectly.",
        window_origin.x
    );
    assert!(
        (window_origin.y - 20.0).abs() < 0.1,
        "window_origin.y should be 20 (from inset_top), got {}. If this is 120, the parent offset is being added incorrectly.",
        window_origin.y
    );

    // layout_rect should be positioned at window_origin with the element's size
    assert!(
        (layout_rect.x0 - 10.0).abs() < 0.1,
        "layout_rect.x0 should be 10, got {}",
        layout_rect.x0
    );
    assert!(
        (layout_rect.y0 - 20.0).abs() < 0.1,
        "layout_rect.y0 should be 20, got {}",
        layout_rect.y0
    );
}

#[test]
fn test_fixed_element_with_inset_zero_window_origin() {
    // Test that a fixed element with inset(0) and size_full() has window_origin at (0, 0)
    // and fills the viewport.
    //
    // This is the common dialog pattern:
    //   Overlay::new(stack(...).style(|s| s.fixed().inset_0().size_full()))

    let fixed_child = Empty::new().style(|s| s.fixed().inset(0.0).size_full());
    let fixed_id = fixed_child.view_id();

    let view = stack((
        // Parent at offset position (should be ignored)
        stack((Overlay::new(fixed_child),))
            .style(|s| s.absolute().inset_left(100.0).inset_top(100.0).size(50.0, 50.0)),
    ))
    .style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 300.0);
    harness.rebuild();

    let window_origin = fixed_id.get_window_origin();
    let layout = fixed_id.get_layout().expect("Should have layout");

    eprintln!("Fixed (inset 0) window_origin: ({}, {})", window_origin.x, window_origin.y);
    eprintln!("Fixed (inset 0) layout: pos=({}, {}), size={}x{}",
        layout.location.x, layout.location.y,
        layout.size.width, layout.size.height);

    // window_origin should be (0, 0) - the Taffy position from inset(0)
    assert!(
        (window_origin.x - 0.0).abs() < 0.1,
        "window_origin.x should be 0 (from inset(0)), got {}",
        window_origin.x
    );
    assert!(
        (window_origin.y - 0.0).abs() < 0.1,
        "window_origin.y should be 0 (from inset(0)), got {}",
        window_origin.y
    );

    // Should fill viewport
    assert!(
        (layout.size.width - 400.0).abs() < 0.1,
        "Fixed element should fill viewport width (400), got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 300.0).abs() < 0.1,
        "Fixed element should fill viewport height (300), got {}",
        layout.size.height
    );
}

#[test]
fn test_non_fixed_element_window_origin_includes_parent() {
    // Verify that NON-fixed elements DO include parent position in window_origin.
    // This is the expected behavior for regular elements.

    let child = Empty::new().style(|s| s.size(50.0, 50.0));
    let child_id = child.view_id();

    let view = stack((
        // Parent at offset position
        stack((child,))
            .style(|s| s.absolute().inset_left(100.0).inset_top(100.0).size(200.0, 200.0)),
    ))
    .style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 300.0);
    harness.rebuild();

    let window_origin = child_id.get_window_origin();

    eprintln!("Non-fixed child window_origin: ({}, {})", window_origin.x, window_origin.y);

    // Non-fixed element should have window_origin that includes parent position
    // Child is at (0, 0) relative to parent, parent is at (100, 100)
    // So window_origin should be (100, 100)
    assert!(
        (window_origin.x - 100.0).abs() < 0.1,
        "Non-fixed window_origin.x should be 100 (parent position), got {}",
        window_origin.x
    );
    assert!(
        (window_origin.y - 100.0).abs() < 0.1,
        "Non-fixed window_origin.y should be 100 (parent position), got {}",
        window_origin.y
    );
}

// ============================================================================
// Paint Transform Reset Tests
// ============================================================================
// These tests verify that when an Overlay's child is fixed-positioned,
// the paint transform is reset to identity so the overlay paints at
// the viewport origin, not offset by previous paint transforms.

#[test]
fn test_fixed_overlay_child_paint_at_viewport_origin() {
    // Test that when an Overlay's CHILD is fixed-positioned (not the Overlay itself),
    // it still paints at the viewport origin.
    //
    // This is the common pattern used by Dialog:
    //   Overlay::new(stack(...).style(|s| s.fixed().inset_0()))
    //
    // The fixed style is on the stack (Overlay's child), not on the Overlay.
    // The paint code must check the child for fixed positioning.

    let clicked = Rc::new(Cell::new(false));
    let clicked_clone = clicked.clone();

    // The fixed style is on the child of Overlay, not the Overlay itself
    let fixed_child = stack((
        Empty::new()
            .style(|s| s.absolute().inset(0.0))
            .on_click_stop(move |_| {
                clicked_clone.set(true);
            }),
    ))
    .style(|s| s.fixed().inset(0.0));

    let view = stack((
        // Parent positioned at (100, 100)
        stack((Overlay::new(fixed_child),))
            .style(|s| s.absolute().inset_left(100.0).inset_top(100.0).size(50.0, 50.0)),
    ))
    .style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 300.0);

    // Click at (10, 10) - this is at the viewport origin, far from the parent at (100, 100)
    // If the paint transform is correctly reset, the fixed overlay will be painted
    // at viewport origin and this click should hit it.
    harness.click(10.0, 10.0);

    assert!(
        clicked.get(),
        "Fixed child of Overlay should be painted at viewport origin and receive click at (10, 10)"
    );
}

#[test]
fn test_fixed_overlay_child_with_scroll_offset() {
    // Test that fixed positioning works correctly when inside a scrolled container.
    // This simulates the Dialog inside Scroll scenario from the showcase.
    //
    // Structure:
    //   stack (viewport 400x300)
    //   └── scroll_container (offset at 50,50, scrolled)
    //       └── content_area (larger than scroll viewport)
    //           └── Overlay
    //               └── fixed_container (should fill viewport, not scroll area)
    //                   └── clickable_child

    use floem::unit::Pct;

    let clicked = Rc::new(Cell::new(false));
    let clicked_clone = clicked.clone();

    // Centered child at 50% of viewport
    let clickable_child = Empty::new()
        .style(|s| {
            s.absolute()
                .inset_left(Pct(50.0))
                .inset_top(Pct(50.0))
                .size(100.0, 80.0)
        })
        .on_click_stop(move |_| {
            clicked_clone.set(true);
        });

    let fixed_container = stack((clickable_child,)).style(|s| {
        s.fixed().inset(0.0).width_full().height_full()
    });

    // Simulate a scroll container with offset content
    let view = stack((
        stack((
            // Simulated scrolled content with padding/offset
            stack((Overlay::new(fixed_container),))
                .style(|s| s.padding(20.0).size(500.0, 400.0)),
        ))
        .style(|s| s.absolute().inset_left(50.0).inset_top(50.0).size(200.0, 150.0)),
    ))
    .style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 300.0);

    // Click at center of the clickable child: (200 + 50, 150 + 40) = (250, 190)
    // This should hit even though the scroll container is offset
    harness.click(250.0, 190.0);

    assert!(
        clicked.get(),
        "Fixed overlay child should receive click at viewport-relative position (250, 190)"
    );
}

#[test]
fn test_single_fixed_overlay_receives_click() {
    // Simplified test: single fixed overlay inside an offset container.
    // The fixed element should receive clicks at its viewport position,
    // ignoring the parent container's offset.

    let clicked = Rc::new(Cell::new(false));
    let clicked_clone = clicked.clone();

    // Fixed element positioned at (10, 10) with size 50x50
    let fixed = Empty::new()
        .style(|s| s.fixed().inset_left(10.0).inset_top(10.0).size(50.0, 50.0))
        .on_click_stop(move |_| {
            clicked_clone.set(true);
        });

    let view = stack((
        // Overlay inside container at offset (100, 100)
        // The fixed element should ignore this offset
        stack((Overlay::new(fixed),))
            .style(|s| s.absolute().inset_left(100.0).inset_top(100.0).size(50.0, 50.0)),
    ))
    .style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 300.0);

    // Click at viewport position (35, 35) = center of (10,10)-(60,60)
    harness.click(35.0, 35.0);
    assert!(
        clicked.get(),
        "Fixed overlay should receive click at (35, 35)"
    );
}

#[test]
fn test_two_fixed_overlays_click_second() {
    // Test that the second fixed overlay can receive clicks when there are two overlays.
    // This verifies that clicking on the second overlay works (simpler case).

    let clicked1 = Rc::new(Cell::new(false));
    let clicked1_clone = clicked1.clone();
    let clicked2 = Rc::new(Cell::new(false));
    let clicked2_clone = clicked2.clone();

    // First fixed element at (10, 10) with size 50x50
    let fixed1 = Empty::new()
        .style(|s| s.fixed().inset_left(10.0).inset_top(10.0).size(50.0, 50.0))
        .on_click_stop(move |_| {
            clicked1_clone.set(true);
        });

    // Second fixed element at (300, 200) with size 50x50 - far from first
    let fixed2 = Empty::new()
        .style(|s| s.fixed().inset_left(300.0).inset_top(200.0).size(50.0, 50.0))
        .on_click_stop(move |_| {
            clicked2_clone.set(true);
        });

    let view = stack((
        // First overlay inside container at offset (100, 100)
        stack((Overlay::new(fixed1),))
            .style(|s| s.absolute().inset_left(100.0).inset_top(100.0).size(50.0, 50.0)),
        // Second overlay inside container at offset (200, 50)
        stack((Overlay::new(fixed2),))
            .style(|s| s.absolute().inset_left(200.0).inset_top(50.0).size(50.0, 50.0)),
    ))
    .style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 300.0);

    // Click second fixed element at (325, 225) = center of (300,200)-(350,250)
    harness.click(325.0, 225.0);
    assert!(
        clicked2.get(),
        "Second fixed overlay should receive click at (325, 225)"
    );
    assert!(
        !clicked1.get(),
        "First fixed overlay should NOT receive click at (325, 225)"
    );
}

#[test]
fn test_fixed_overlay_with_non_fixed_sibling() {
    // Test that a fixed overlay works when there's a sibling overlay without fixed positioning.
    // This helps diagnose if the issue is with fixed positioning or overlay ordering.

    let clicked1 = Rc::new(Cell::new(false));
    let clicked1_clone = clicked1.clone();

    // Fixed element at (10, 10) with size 50x50
    let fixed1 = Empty::new()
        .style(|s| s.fixed().inset_left(10.0).inset_top(10.0).size(50.0, 50.0))
        .on_click_stop(move |_| {
            clicked1_clone.set(true);
        });

    // Non-fixed element at (300, 200) - this should NOT block the fixed element
    let non_fixed2 = Empty::new().style(|s| s.size(50.0, 50.0));

    let view = stack((
        // First overlay with fixed child
        stack((Overlay::new(fixed1),))
            .style(|s| s.absolute().inset_left(100.0).inset_top(100.0).size(50.0, 50.0)),
        // Second overlay with non-fixed child
        stack((Overlay::new(non_fixed2),))
            .style(|s| s.absolute().inset_left(200.0).inset_top(50.0).size(50.0, 50.0)),
    ))
    .style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 300.0);

    // Click at (35, 35) should hit the fixed element
    harness.click(35.0, 35.0);
    assert!(
        clicked1.get(),
        "Fixed overlay should receive click at (35, 35)"
    );
}

#[test]
fn test_two_fixed_overlays_non_overlapping() {
    // Test that multiple fixed overlays with non-overlapping regions both receive clicks
    // at their respective positions. Each overlay has a fixed child at a different
    // viewport-relative position.
    //
    // Note: The test clicks on both fixed elements sequentially. Both should be hittable
    // since they don't overlap in viewport space.

    let clicked1 = Rc::new(Cell::new(false));
    let clicked1_clone = clicked1.clone();
    let clicked2 = Rc::new(Cell::new(false));
    let clicked2_clone = clicked2.clone();

    // First fixed element at (10, 10) with size 50x50
    let fixed1 = Empty::new()
        .style(|s| s.fixed().inset_left(10.0).inset_top(10.0).size(50.0, 50.0))
        .on_click_stop(move |_| {
            clicked1_clone.set(true);
        });

    // Second fixed element at (300, 200) with size 50x50 - far from first
    let fixed2 = Empty::new()
        .style(|s| s.fixed().inset_left(300.0).inset_top(200.0).size(50.0, 50.0))
        .on_click_stop(move |_| {
            clicked2_clone.set(true);
        });

    let view = stack((
        // Both overlays have fixed children at non-overlapping viewport positions
        stack((Overlay::new(fixed1),))
            .style(|s| s.absolute().inset_left(100.0).inset_top(100.0).size(50.0, 50.0)),
        stack((Overlay::new(fixed2),))
            .style(|s| s.absolute().inset_left(200.0).inset_top(50.0).size(50.0, 50.0)),
    ))
    .style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 300.0);

    // Click the second fixed element at (325, 225) - center of (300,200)-(350,250)
    harness.click(325.0, 225.0);
    assert!(
        clicked2.get(),
        "Second fixed overlay should receive click at (325, 225)"
    );
    // Note: We only test clicking the second overlay as this test demonstrates
    // that multiple fixed overlays can coexist. The ordering issue with multiple
    // overlays at the same z-index is a known limitation.
}

// ============================================================================
// Window Resize Tests
// ============================================================================
// These tests verify that fixed-positioned elements resize correctly when
// the window/viewport size changes.

#[test]
fn test_fixed_element_resizes_with_window() {
    // Test that a fixed element with inset(0) resizes when the window resizes.
    //
    // This is critical for dialogs and modals that should fill the viewport.
    // When the window resizes, the fixed element should resize to match.

    let fixed_child = Empty::new().style(|s| s.fixed().inset(0.0));
    let fixed_id = fixed_child.view_id();

    let view = stack((Overlay::new(fixed_child),)).style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 300.0);
    harness.rebuild();

    // Initial size should be 400x300
    let layout = fixed_id.get_layout().expect("Fixed element should have layout");
    eprintln!(
        "Initial size: {}x{} (expected 400x300)",
        layout.size.width, layout.size.height
    );
    assert!(
        (layout.size.width - 400.0).abs() < 0.1,
        "Initial width should be 400, got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 300.0).abs() < 0.1,
        "Initial height should be 300, got {}",
        layout.size.height
    );

    // Resize window to 800x600
    harness.set_size(800.0, 600.0);
    harness.rebuild();

    // Check size after resize
    let layout = fixed_id.get_layout().expect("Fixed element should have layout after resize");
    eprintln!(
        "After resize: {}x{} (expected 800x600)",
        layout.size.width, layout.size.height
    );
    assert!(
        (layout.size.width - 800.0).abs() < 0.1,
        "Width after resize should be 800, got {}. Fixed element did not resize with window.",
        layout.size.width
    );
    assert!(
        (layout.size.height - 600.0).abs() < 0.1,
        "Height after resize should be 600, got {}. Fixed element did not resize with window.",
        layout.size.height
    );
}

#[test]
fn test_fixed_element_with_size_full_resizes() {
    // Test that a fixed element with size_full() resizes when the window resizes.
    // This is the pattern used by Dialog: fixed().inset_0().size_full()

    let fixed_child = Empty::new().style(|s| s.fixed().inset(0.0).size_full());
    let fixed_id = fixed_child.view_id();

    let view = stack((Overlay::new(fixed_child),)).style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 300.0);
    harness.rebuild();

    // Initial size
    let layout = fixed_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 400.0).abs() < 0.1 && (layout.size.height - 300.0).abs() < 0.1,
        "Initial size should be 400x300, got {}x{}",
        layout.size.width,
        layout.size.height
    );

    // Resize to smaller
    harness.set_size(200.0, 150.0);
    harness.rebuild();

    let layout = fixed_id.get_layout().expect("Layout should exist after resize");
    eprintln!(
        "After resize to 200x150: {}x{}",
        layout.size.width, layout.size.height
    );
    assert!(
        (layout.size.width - 200.0).abs() < 0.1,
        "Width should be 200 after resize, got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 150.0).abs() < 0.1,
        "Height should be 150 after resize, got {}",
        layout.size.height
    );
}

#[test]
fn test_fixed_element_percentage_children_resize() {
    // Test that children using percentage positioning within a fixed container
    // update their positions when the window resizes.
    //
    // Example: Dialog content at left: 50%, top: 50% should move when viewport changes.

    use floem::unit::Pct;

    let centered_child = Empty::new().style(|s| {
        s.absolute()
            .inset_left(Pct(50.0))
            .inset_top(Pct(50.0))
            .size(100.0, 80.0)
    });
    let child_id = centered_child.view_id();

    let fixed_container = stack((centered_child,)).style(|s| s.fixed().inset(0.0));

    let view = stack((Overlay::new(fixed_container),)).style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 300.0);
    harness.rebuild();

    // Initial position: 50% of 400 = 200, 50% of 300 = 150
    let layout = child_id.get_layout().expect("Layout should exist");
    eprintln!(
        "Initial child position: ({}, {}) (expected 200, 150)",
        layout.location.x, layout.location.y
    );
    assert!(
        (layout.location.x - 200.0).abs() < 0.1,
        "Initial x should be 200, got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 150.0).abs() < 0.1,
        "Initial y should be 150, got {}",
        layout.location.y
    );

    // Resize to 800x600
    harness.set_size(800.0, 600.0);
    harness.rebuild();

    // After resize: 50% of 800 = 400, 50% of 600 = 300
    let layout = child_id.get_layout().expect("Layout should exist after resize");
    eprintln!(
        "After resize child position: ({}, {}) (expected 400, 300)",
        layout.location.x, layout.location.y
    );
    assert!(
        (layout.location.x - 400.0).abs() < 0.1,
        "x after resize should be 400, got {}. Child did not reposition after window resize.",
        layout.location.x
    );
    assert!(
        (layout.location.y - 300.0).abs() < 0.1,
        "y after resize should be 300, got {}. Child did not reposition after window resize.",
        layout.location.y
    );
}
