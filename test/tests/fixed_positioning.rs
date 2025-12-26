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
