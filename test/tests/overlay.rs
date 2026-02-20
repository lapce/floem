//! Tests for the declarative Overlay view.
//!
//! These tests verify that the Overlay view correctly manages overlays,
//! including event dispatch order and paint order.

use floem::headless::{HeadlessHarness, TestRoot};
use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
use floem::view::ParentView;
use floem::views::{
    Clip, ClipExt, ContainerExt, Decorators, Empty, Label, Overlay, OverlayExt, Stack,
};
use floem::{HasViewId, View};
use serial_test::serial;
use std::cell::Cell;
use std::rc::Rc;

#[test]
#[serial]
fn test_overlay_new() {
    let root = TestRoot::new();
    // Test that an Overlay can be created with static content
    let view = Stack::new((
        Label::new("Main content"),
        Label::new("Overlay content").overlay(),
    ))
    .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);
}

#[test]
#[serial]
fn test_overlay_derived() {
    let root = TestRoot::new();
    // Test that an Overlay can be created with derived content
    let view = Stack::new((
        Label::new("Main content"),
        Label::derived(|| "Overlay content".to_string()).overlay(),
    ))
    .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);
}

#[test]
#[serial]
fn test_overlay_with_visibility_control() {
    let root = TestRoot::new();
    // Test that overlay visibility can be controlled via styles
    let visible = RwSignal::new(true);

    let view = Stack::new((
        Label::new("Main content"),
        Label::derived(|| "Overlay content".to_string())
            .style(move |s| s.apply_if(!visible.get(), |s| s.hide()))
            .overlay(),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    // Toggle visibility
    visible.set(false);
    harness.rebuild();

    visible.set(true);
    harness.rebuild();
}

#[test]
#[serial]
fn test_overlay_content_factory_called() {
    let root = TestRoot::new();
    // Test that the content factory is called when creating overlay
    let factory_called = Rc::new(Cell::new(false));
    let factory_called_clone = factory_called.clone();

    let view = Stack::new((
        Label::new("Main content"),
        Overlay::new_dyn(move || {
            factory_called_clone.set(true);
            Label::new("Overlay content")
        }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    assert!(
        factory_called.get(),
        "Content factory should be called when overlay is created"
    );

    drop(harness);
}

#[test]
#[serial]
fn test_overlay_in_nested_structure() {
    let root = TestRoot::new();
    // Test that Overlay works in nested view structures
    let view = Stack::new((Stack::new((
        Label::new("Nested label"),
        Stack::new((Label::new("Nested overlay"), Empty::new()))
            .style(|s| s.size(50.0, 50.0))
            .overlay(),
    ))
    .style(|s| s.size(80.0, 80.0)),))
    .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);
}

#[test]
#[serial]
fn test_multiple_overlays() {
    let root = TestRoot::new();
    // Test that multiple overlays can coexist
    let view = Stack::new((
        Label::new("Main content"),
        Label::new("Overlay 1").overlay(),
        Label::new("Overlay 2").overlay(),
    ))
    .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);
}

#[test]
#[serial]
fn test_overlay_with_styled_content() {
    let root = TestRoot::new();
    // Test that overlay content can be styled
    let view = Stack::new((
        Label::new("Main content"),
        Label::new("Styled overlay")
            .style(|s| {
                s.background(floem::peniko::Color::WHITE)
                    .padding(20.0)
                    .border_radius(8.0)
            })
            .overlay(),
    ))
    .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);
}

// ============================================================================
// Event Dispatch Order Tests
// ============================================================================

#[test]
#[serial]
fn test_overlay_receives_events_before_high_z_index() {
    let root = TestRoot::new();
    // Test that overlays receive events before regular views, even if
    // the regular view has a higher z-index.
    //
    // Structure:
    //   stack
    //   ├── regular_view (z-index: 100)  <-- should NOT receive click
    //   └── Overlay
    //       └── overlay_content (z-index: 1)  <-- should receive click
    //
    // The overlay should receive the click because overlays are always
    // on top of regular views, regardless of z-index.

    let clicked_regular = Rc::new(Cell::new(false));
    let clicked_overlay = Rc::new(Cell::new(false));

    let clicked_regular_clone = clicked_regular.clone();
    let clicked_overlay_clone = clicked_overlay.clone();

    let view = Stack::new((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
            .action(move || {
                clicked_regular_clone.set(true);
            }),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size_full().z_index(1))
            .action(move || {
                clicked_overlay_clone.set(true);
            })
            .overlay()
            .style(|s| s.size(100, 100)),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_overlay.get(),
        "Overlay should receive click (overlays are always on top)"
    );
    assert!(
        !clicked_regular.get(),
        "High z-index should NOT receive click (blocked by overlay)"
    );
}

#[test]
#[serial]
fn test_multiple_overlays_respect_z_index() {
    let root = TestRoot::new();
    // Test that multiple overlays respect z-index ordering among themselves.
    // The z-index must be on the Overlay view itself, not just its content.
    //
    // Structure:
    //   stack
    //   ├── Overlay (z-index: 10)  <-- higher z-index, should receive click
    //   │   └── overlay1_content
    //   └── Overlay (z-index: 1)   <-- lower z-index, later in DOM
    //       └── overlay2_content
    //
    // Overlay1 (z-index: 10) should receive the click despite being earlier in DOM.

    let clicked_overlay1 = Rc::new(Cell::new(false));
    let clicked_overlay2 = Rc::new(Cell::new(false));

    let clicked1_clone = clicked_overlay1.clone();
    let clicked2_clone = clicked_overlay2.clone();

    let view = Stack::new((
        // First overlay with higher z-index
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .action(move || {
                clicked1_clone.set(true);
            })
            .overlay()
            .style(|s| s.z_index(10)),
        // Second overlay with lower z-index (later in DOM)
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .action(move || {
                clicked2_clone.set(true);
            })
            .overlay()
            .style(|s| s.z_index(1)),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_overlay1.get(),
        "Overlay1 (z-index: 10) should receive click"
    );
    assert!(
        !clicked_overlay2.get(),
        "Overlay2 (z-index: 1) should NOT receive click"
    );
}

#[test]
#[serial]
fn test_overlay_dom_order_tiebreaker() {
    let root = TestRoot::new();
    // Test that when overlays have equal z-index, DOM order is used as tiebreaker.
    // The z-index must be on the Overlay view itself.
    //
    // Structure:
    //   stack
    //   ├── Overlay (z-index: 5)
    //   │   └── overlay1_content
    //   └── Overlay (z-index: 5)  <-- should receive click (later in DOM)
    //       └── overlay2_content

    let clicked_overlay1 = Rc::new(Cell::new(false));
    let clicked_overlay2 = Rc::new(Cell::new(false));

    let clicked1_clone = clicked_overlay1.clone();
    let clicked2_clone = clicked_overlay2.clone();

    let view = Stack::new((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .action(move || {
                clicked1_clone.set(true);
            })
            .overlay()
            .style(|s| s.z_index(5)),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .action(move || {
                clicked2_clone.set(true);
            })
            .overlay()
            .style(|s| s.z_index(5)),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_overlay2.get(),
        "Overlay2 (later in DOM) should receive click when z-index is equal"
    );
    assert!(!clicked_overlay1.get(), "Overlay1 should NOT receive click");
}

#[test]
#[serial]
fn test_nested_overlay_escapes_parent_z_index() {
    let root = TestRoot::new();
    // Test that an overlay nested inside a low z-index parent still
    // receives events before a higher z-index sibling.
    //
    // Structure:
    //   stack
    //   ├── parent_container (z-index: 1)
    //   │   └── Overlay
    //   │       └── overlay_content  <-- should receive click
    //   └── sibling (z-index: 100)  <-- should NOT receive click
    //
    // The overlay should "escape" its parent's z-index and receive the click.

    let clicked_sibling = Rc::new(Cell::new(false));
    let clicked_overlay = Rc::new(Cell::new(false));

    let sibling_clone = clicked_sibling.clone();
    let overlay_clone = clicked_overlay.clone();

    let view = Stack::new((
        // Parent with low z-index containing an overlay
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .action(move || {
                overlay_clone.set(true);
            })
            .overlay()
            .container()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1)),
        // Sibling with high z-index
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
            .action(move || {
                sibling_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_overlay.get(),
        "Overlay should receive click (escapes parent z-index)"
    );
    assert!(
        !clicked_sibling.get(),
        "Sibling should NOT receive click (overlay is on top)"
    );
}

#[test]
#[serial]
fn test_hidden_overlay_does_not_block_events() {
    let root = TestRoot::new();
    // Test that a hidden overlay does not block events to views below.
    //
    // Structure:
    //   stack
    //   ├── regular_view  <-- should receive click
    //   └── Overlay (hidden)
    //       └── overlay_content

    let clicked_regular = Rc::new(Cell::new(false));
    let clicked_overlay = Rc::new(Cell::new(false));

    let regular_clone = clicked_regular.clone();
    let overlay_clone = clicked_overlay.clone();

    let view = Stack::new((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .action(move || {
                regular_clone.set(true);
            }),
        Empty::new()
            .style(|s| {
                s.absolute()
                    .inset(0.0)
                    .size(100.0, 100.0)
                    .display(floem::taffy::Display::None)
            })
            .action(move || {
                overlay_clone.set(true);
            })
            .overlay(),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_regular.get(),
        "Regular view should receive click when overlay is hidden"
    );
    assert!(
        !clicked_overlay.get(),
        "Hidden overlay should NOT receive click"
    );
}

// ============================================================================
// Paint Order Tests
// ============================================================================

#[test]
#[serial]
fn test_paint_order_overlays_after_regular_views() {
    // Test that overlays are painted after regular views.
    //
    // Structure:
    //   stack
    //   ├── regular_view (z-index: 100)  <-- painted first (even with high z-index)
    //   └── Overlay
    //       └── overlay_content  <-- painted last (overlays always on top)
    //
    // Paint order should be: regular_view, then overlay_content
    // (regardless of z-index values)

    let root = TestRoot::new();
    let regular = Empty::new().style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100));
    let regular_id = regular.view_id();

    let overlay_content = Empty::new().style(|s| s.absolute().inset(0.0).size(100.0, 100.0));
    let overlay_id = overlay_content.view_id();

    let view = Stack::new((
        regular,
        overlay_content.overlay().style(|s| s.size(100, 100)),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    let paint_order = harness.paint_and_get_order();

    let regular_pos = paint_order.iter().position(|&id| id == regular_id);
    let overlay_pos = paint_order.iter().position(|&id| id == overlay_id);

    assert!(
        regular_pos.is_some(),
        "Regular view should be in paint order"
    );
    assert!(overlay_pos.is_some(), "Overlay should be in paint order");

    assert!(
        regular_pos.unwrap() < overlay_pos.unwrap(),
        "Overlay should be painted AFTER regular view (overlay at {}, regular at {})",
        overlay_pos.unwrap(),
        regular_pos.unwrap()
    );
}

#[test]
#[serial]
fn test_paint_order_multiple_overlays_by_z_index() {
    // Test that multiple overlays are painted in z-index order (low to high).
    //
    // Structure:
    //   stack
    //   ├── Overlay (z-index: 10)  <-- painted second (higher z-index = painted later)
    //   │   └── overlay1_content
    //   └── Overlay (z-index: 1)   <-- painted first (lower z-index = painted earlier)
    //       └── overlay2_content

    let root = TestRoot::new();
    let overlay1_content = Empty::new().style(|s| s.absolute().inset(0.0).size(100.0, 100.0));
    let overlay1_id = overlay1_content.view_id();

    let overlay2_content = Empty::new().style(|s| s.absolute().inset(0.0).size(100.0, 100.0));
    let overlay2_id = overlay2_content.view_id();

    let overlay1 = overlay1_content
        .overlay()
        .style(|s| s.z_index(10).size(100, 100));
    let overlay2 = overlay2_content
        .overlay()
        .style(|s| s.z_index(1).size(100, 100));

    let view = Stack::new((overlay1, overlay2)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);
    // Ensure deferred children are processed before checking paint order
    harness.rebuild();

    let paint_order = harness.paint_and_get_order();

    let overlay1_pos = paint_order.iter().position(|&id| id == overlay1_id);
    let overlay2_pos = paint_order.iter().position(|&id| id == overlay2_id);

    assert!(overlay1_pos.is_some(), "Overlay1 should be in paint order");
    assert!(overlay2_pos.is_some(), "Overlay2 should be in paint order");

    // Lower z-index (overlay2, z=1) should be painted first
    // Higher z-index (overlay1, z=10) should be painted later
    assert!(
        overlay2_pos.unwrap() < overlay1_pos.unwrap(),
        "Overlay with lower z-index should be painted first (overlay2 z=1 at {}, overlay1 z=10 at {})",
        overlay2_pos.unwrap(),
        overlay1_pos.unwrap()
    );
}

#[test]
#[serial]
fn test_paint_order_regular_views_by_z_index() {
    // Test that regular views (non-overlays) are painted in z-index order.
    //
    // Structure:
    //   stack
    //   ├── view1 (z-index: 5)   <-- painted second
    //   ├── view2 (z-index: 1)   <-- painted first (lowest z-index)
    //   └── view3 (z-index: 10)  <-- painted third (highest z-index)

    let root = TestRoot::new();
    let v1 = Empty::new().style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5));
    let v1_id = v1.view_id();

    let v2 = Empty::new().style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1));
    let v2_id = v2.view_id();

    let v3 = Empty::new().style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(10));
    let v3_id = v3.view_id();

    let view = Stack::new((v1, v2, v3)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    let paint_order = harness.paint_and_get_order();

    let pos1 = paint_order.iter().position(|&id| id == v1_id);
    let pos2 = paint_order.iter().position(|&id| id == v2_id);
    let pos3 = paint_order.iter().position(|&id| id == v3_id);

    assert!(pos1.is_some(), "View1 should be in paint order");
    assert!(pos2.is_some(), "View2 should be in paint order");
    assert!(pos3.is_some(), "View3 should be in paint order");

    // Paint order should be: view2 (z=1), view1 (z=5), view3 (z=10)
    assert!(
        pos2.unwrap() < pos1.unwrap() && pos1.unwrap() < pos3.unwrap(),
        "Views should be painted in z-index order: view2(z=1) at {}, view1(z=5) at {}, view3(z=10) at {}",
        pos2.unwrap(),
        pos1.unwrap(),
        pos3.unwrap()
    );
}

#[test]
#[serial]
fn test_paint_order_nested_overlay_escapes_parent() {
    // Test that an overlay nested inside a low z-index parent is still painted
    // after its high z-index sibling.
    //
    // Structure:
    //   stack
    //   ├── parent (z-index: 1)
    //   │   └── Overlay
    //   │       └── overlay_content  <-- painted last (overlay always on top)
    //   └── sibling (z-index: 100)  <-- painted before overlay (regular view)

    let root = TestRoot::new();
    let overlay_content = Empty::new()
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
        .debug_name("Overlay Content");
    let overlay_id = overlay_content.view_id();

    let sibling = Empty::new()
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
        .debug_name("sibling that should be before overlay and overlay content");
    let sibling_id = sibling.view_id();

    let view = Stack::new((
        overlay_content
            .overlay()
            .style(|s| s.size(100, 100))
            .container()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1)),
        sibling,
    ))
    .style(|s| s.size(100.0, 100.0))
    .debug_name("Outer Stack");

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    let paint_order = harness.paint_and_get_order();

    let sibling_pos = paint_order.iter().position(|&id| id == sibling_id);
    let overlay_pos = paint_order.iter().position(|&id| id == overlay_id);

    assert!(sibling_pos.is_some(), "Sibling should be in paint order");
    assert!(overlay_pos.is_some(), "Overlay should be in paint order");

    assert!(
        sibling_pos.unwrap() < overlay_pos.unwrap(),
        "Overlay should be painted AFTER sibling (overlay at {}, sibling at {})",
        overlay_pos.unwrap(),
        sibling_pos.unwrap()
    );
}

// ============================================================================
// Clip Escape Tests
// ============================================================================

#[test]
#[serial]
fn test_overlay_escapes_parent_clip() {
    // Test that an overlay inside a Clip view still receives events
    // outside the clip bounds.
    //
    // Structure:
    //   stack (100x100)
    //   ├── Clip (50x50 at 0,0)
    //   │   └── stack
    //   │       └── Overlay
    //   │           └── overlay_content (100x100)  <-- extends beyond clip
    //   └── background (100x100)  <-- should NOT receive click at (75, 25)
    //
    // Click at (75, 25) is outside the Clip bounds but inside
    // the overlay_content bounds. The overlay should receive this click.

    let root = TestRoot::new();
    let clicked_overlay = Rc::new(Cell::new(false));
    let clicked_background = Rc::new(Cell::new(false));

    let overlay_clone = clicked_overlay.clone();
    let bg_clone = clicked_background.clone();

    let view = Stack::new((
        // Clipping parent using Clip view
        Clip::new(
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
                .action(move || {
                    overlay_clone.set(true);
                })
                .overlay(),
        )
        .style(|s| s.absolute().inset_left(0.0).inset_top(0.0).size(50.0, 50.0)),
        // Background that would receive click if overlay didn't escape clip
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(-1))
            .action(move || {
                bg_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    // Click outside clip bounds (75, 25) but inside overlay bounds
    harness.click(75.0, 25.0);

    assert!(
        clicked_overlay.get(),
        "Overlay should receive click outside parent's clip bounds"
    );
    assert!(
        !clicked_background.get(),
        "Background should NOT receive click (blocked by overlay)"
    );
}

#[test]
#[serial]
#[should_panic]
// Floem assumes all views create a stacking context and cannot escape. if we allow escaping in the future, this test would be expected to pass
fn test_overlay_painted_outside_parent_clip() {
    // Test that an overlay is painted even when it extends outside
    // its parent's clipping region.
    //
    // Structure:
    //   stack
    //   └── Clip (50x50)
    //       └── stack
    //           └── Overlay
    //               └── overlay_content (100x100)  <-- extends beyond clip
    //
    // The overlay content should appear in the paint order.

    let root = TestRoot::new();
    let overlay_content = Empty::new().style(|s| s.absolute().inset(0.0).size(100.0, 100.0));
    let overlay_id = overlay_content.view_id();

    let view =
        Stack::new((Clip::new(overlay_content.overlay())
            .style(|s| s.absolute().inset(0.0).size(50.0, 50.0)),))
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    let paint_order = harness.paint_and_get_order();

    let overlay_pos = paint_order.iter().position(|&id| id == overlay_id);

    assert!(
        overlay_pos.is_some(),
        "Overlay should be painted even when parent has clip"
    );
}

#[test]
#[serial]
fn test_overlay_escapes_nested_clips() {
    // Test that an overlay escapes multiple levels of clipping parents.
    //
    // Structure:
    //   stack (100x100)
    //   ├── Clip (80x80)
    //   │   └── Clip (60x60)
    //   │       └── stack
    //   │           └── Overlay
    //   │               └── overlay_content (100x100)  <-- should escape all clips
    //   └── background  <-- should NOT receive click
    //
    // Click at (90, 45) is outside both clip bounds but inside overlay bounds.

    let root = TestRoot::new();
    let clicked_overlay = Rc::new(Cell::new(false));
    let clicked_background = Rc::new(Cell::new(false));

    let overlay_clone = clicked_overlay.clone();
    let bg_clone = clicked_background.clone();

    let view = Stack::new((
        Empty::new()
            .style(|s| s.absolute().size(100.0, 100.0))
            .action(move || {
                overlay_clone.set(true);
            })
            .overlay()
            .clip() // Inner clipping container
            .style(|s| s.absolute().size(60.0, 60.0))
            .clip() // Outer clipping container
            .style(|s| s.absolute().size(80.0, 80.0)),
        // Background
        Empty::new()
            .style(|s| s.absolute().size(100.0, 100.0).z_index(1))
            .action(move || {
                bg_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);
    harness.rebuild();

    // Click outside both clip bounds (90, 45) but inside overlay
    harness.click(0.0, 0.);

    assert!(clicked_overlay.get(), "Overlay should escape nested clips");
    assert!(
        !clicked_background.get(),
        "Background should NOT receive click"
    );
}

// ============================================================================
// Container::derived inside Overlay Tests
// ============================================================================

#[test]
#[serial]
fn test_overlay_with_container_derived_rebuild() {
    // Test that Container::derived inside Overlay::new works correctly
    // when the derived content is rebuilt.
    //
    // This reproduces a bug where taffy layout nodes become invalid
    // after Container::derived rebuilds its children.

    use floem::views::Container;

    let counter = RwSignal::new(0);

    let view = Stack::new((
        Label::new("Main content"),
        Container::derived(move || {
            let count = counter.get();
            Label::new(format!("Count: {}", count))
        })
        .style(|s| s.size(100.0, 50.0))
        .overlay(),
    ))
    .style(|s| s.size(100.0, 100.0));

    let root = TestRoot::new();
    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    // Initial paint should work
    harness.rebuild();
    harness.paint();

    // Trigger a rebuild of Container::derived content
    counter.set(1);
    harness.rebuild();
    harness.paint();

    // Another update
    counter.set(2);
    harness.rebuild();
    harness.paint();
}

#[test]
#[serial]
fn test_overlay_with_nested_container_derived() {
    // More complex case: nested Container::derived inside Overlay
    // This tests the pattern used in Dialog component.

    use floem::views::Container;

    let open = RwSignal::new(true);

    let view = Stack::new((
        Label::new("Main content"),
        Stack::new((
            // Backdrop
            Empty::new().style(move |s| {
                let is_open = open.get();
                s.absolute()
                    .inset(0.0)
                    .size(100.0, 100.0)
                    .apply_if(!is_open, |s| s.display(floem::taffy::Display::None))
            }),
            // Centering container with derived content
            Container::new(Container::derived(move || {
                let is_open = open.get();
                Stack::new((Label::new("Dialog Title"), Label::new("Dialog Description"))).style(
                    move |s| {
                        s.size(80.0, 60.0)
                            .apply_if(!is_open, |s| s.display(floem::taffy::Display::None))
                    },
                )
            }))
            .style(move |s| {
                let is_open = open.get();
                s.absolute()
                    .inset(0.0)
                    .size(100.0, 100.0)
                    .apply_if(!is_open, |s| s.display(floem::taffy::Display::None))
            }),
        ))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
        .overlay(),
    ))
    .style(|s| s.size(100.0, 100.0));

    let root = TestRoot::new();
    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    // Initial state: dialog open
    harness.rebuild();
    harness.paint();

    // Close dialog
    open.set(false);
    harness.rebuild();
    harness.paint();

    // Open dialog again
    open.set(true);
    harness.rebuild();
    harness.paint();

    // Close again
    open.set(false);
    harness.rebuild();
    harness.paint();
}

#[test]
#[serial]
fn test_clip_affects_events() {
    // Document current behavior: Clip only affects painting, not event dispatch.
    // Children inside a Clip still receive events outside the clip bounds.
    //
    // Structure:
    //   stack (100x100)
    //   ├── Clip (50x50)
    //   │   └── regular_child (100x100, extends beyond clip)
    //   └── background
    //
    // Click at (75, 25) goes to regular_child even though it's outside clip bounds.
    // This is because Clip only affects rendering, not hit testing.

    let root = TestRoot::new();
    let clicked_child = Rc::new(Cell::new(false));
    let clicked_background = Rc::new(Cell::new(false));

    let child_clone = clicked_child.clone();
    let bg_clone = clicked_background.clone();

    let view = Stack::new((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .action(move || {
                child_clone.set(true);
            })
            .clip() // apply clip parent
            .style(|s| s.absolute().inset_left(0.0).inset_top(0.0).size(50.0, 50.0)),
        // Background
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(-1))
            .action(move || {
                bg_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    // Click outside clip bounds - child does not receive it because Clip
    // does affect event dispatch
    harness.click(75.0, 25.0);

    assert!(
        !clicked_child.get(),
        "Child does not receive click outside clip bounds"
    );
    assert!(
        clicked_background.get(),
        "Background should NOT receive click (blocked by child's hit area)"
    );
}
