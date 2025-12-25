//! Tests for the declarative Overlay view.
//!
//! These tests verify that the Overlay view correctly manages overlays,
//! including event dispatch order and paint order.

use floem::headless::HeadlessHarness;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
use floem::views::{Decorators, Empty, Label, Overlay, stack};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn test_overlay_new() {
    // Test that an Overlay can be created with static content
    let view = stack((
        Label::new("Main content"),
        Overlay::new(Label::new("Overlay content")),
    ))
    .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
}

#[test]
fn test_overlay_derived() {
    // Test that an Overlay can be created with derived content
    let view = stack((
        Label::new("Main content"),
        Overlay::derived(|| Label::derived(|| "Overlay content".to_string())),
    ))
    .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
}

#[test]
fn test_overlay_with_visibility_control() {
    // Test that overlay visibility can be controlled via styles
    let visible = RwSignal::new(true);

    let view = stack((
        Label::new("Main content"),
        Overlay::derived(move || {
            Label::derived(|| "Overlay content".to_string())
                .style(move |s| s.apply_if(!visible.get(), |s| s.hide()))
        }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Toggle visibility
    visible.set(false);
    harness.rebuild();

    visible.set(true);
    harness.rebuild();
}

#[test]
fn test_overlay_content_factory_called() {
    // Test that the content factory is called when creating overlay
    let factory_called = Rc::new(Cell::new(false));
    let factory_called_clone = factory_called.clone();

    let view = stack((
        Label::new("Main content"),
        Overlay::derived(move || {
            factory_called_clone.set(true);
            Label::new("Overlay content")
        }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    assert!(
        factory_called.get(),
        "Content factory should be called when overlay is created"
    );
}

#[test]
fn test_overlay_in_nested_structure() {
    // Test that Overlay works in nested view structures
    let view = stack((stack((
        Label::new("Nested label"),
        Overlay::derived(|| {
            stack((Label::new("Nested overlay"), Empty::new())).style(|s| s.size(50.0, 50.0))
        }),
    ))
    .style(|s| s.size(80.0, 80.0)),))
    .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
}

#[test]
fn test_multiple_overlays() {
    // Test that multiple overlays can coexist
    let view = stack((
        Label::new("Main content"),
        Overlay::new(Label::new("Overlay 1")),
        Overlay::new(Label::new("Overlay 2")),
    ))
    .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
}

#[test]
fn test_overlay_with_styled_content() {
    // Test that overlay content can be styled
    let view = stack((
        Label::new("Main content"),
        Overlay::new(Label::new("Styled overlay").style(|s| {
            s.background(floem::peniko::Color::WHITE)
                .padding(20.0)
                .border_radius(8.0)
        })),
    ))
    .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
}

// ============================================================================
// Event Dispatch Order Tests
// ============================================================================

#[test]
fn test_overlay_receives_events_before_regular_views() {
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

    let view = stack((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
            .on_click_stop(move |_| {
                clicked_regular_clone.set(true);
            }),
        Overlay::new(
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
                .on_click_stop(move |_| {
                    clicked_overlay_clone.set(true);
                }),
        ),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_overlay.get(),
        "Overlay should receive click (overlays are always on top)"
    );
    assert!(
        !clicked_regular.get(),
        "Regular view should NOT receive click (blocked by overlay)"
    );
}

#[test]
fn test_multiple_overlays_respect_z_index() {
    // Test that multiple overlays respect z-index ordering among themselves.
    //
    // Structure:
    //   stack
    //   ├── Overlay (z-index: 1)
    //   │   └── overlay1_content
    //   └── Overlay (z-index: 10)  <-- should receive click
    //       └── overlay2_content
    //
    // The second overlay (z-index: 10) should receive the click.

    let clicked_overlay1 = Rc::new(Cell::new(false));
    let clicked_overlay2 = Rc::new(Cell::new(false));

    let clicked1_clone = clicked_overlay1.clone();
    let clicked2_clone = clicked_overlay2.clone();

    let view = stack((
        Overlay::new(
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
                .on_click_stop(move |_| {
                    clicked1_clone.set(true);
                }),
        ),
        Overlay::new(
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(10))
                .on_click_stop(move |_| {
                    clicked2_clone.set(true);
                }),
        ),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_overlay2.get(),
        "Overlay2 (z-index: 10) should receive click"
    );
    assert!(
        !clicked_overlay1.get(),
        "Overlay1 (z-index: 1) should NOT receive click"
    );
}

#[test]
fn test_overlay_dom_order_tiebreaker() {
    // Test that when overlays have equal z-index, DOM order is used as tiebreaker.
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

    let view = stack((
        Overlay::new(
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                .on_click_stop(move |_| {
                    clicked1_clone.set(true);
                }),
        ),
        Overlay::new(
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                .on_click_stop(move |_| {
                    clicked2_clone.set(true);
                }),
        ),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_overlay2.get(),
        "Overlay2 (later in DOM) should receive click when z-index is equal"
    );
    assert!(
        !clicked_overlay1.get(),
        "Overlay1 should NOT receive click"
    );
}

#[test]
fn test_nested_overlay_escapes_parent_z_index() {
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

    let view = stack((
        // Parent with low z-index containing an overlay
        stack((Overlay::new(
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
                .on_click_stop(move |_| {
                    overlay_clone.set(true);
                }),
        ),))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1)),
        // Sibling with high z-index
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
            .on_click_stop(move |_| {
                sibling_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

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
fn test_hidden_overlay_does_not_block_events() {
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

    let view = stack((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .on_click_stop(move |_| {
                regular_clone.set(true);
            }),
        Overlay::new(
            Empty::new()
                .style(|s| {
                    s.absolute()
                        .inset(0.0)
                        .size(100.0, 100.0)
                        .display(floem::taffy::Display::None)
                })
                .on_click_stop(move |_| {
                    overlay_clone.set(true);
                }),
        ),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

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

#[test]
fn test_overlay_toggle_visibility() {
    // Test that toggling overlay visibility works correctly for event dispatch.

    let is_visible = RwSignal::new(true);
    let clicked_regular = Rc::new(Cell::new(false));
    let clicked_overlay = Rc::new(Cell::new(false));

    let regular_clone = clicked_regular.clone();
    let overlay_clone = clicked_overlay.clone();

    let view = stack((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .on_click_stop(move |_| {
                regular_clone.set(true);
            }),
        Overlay::new(
            Empty::new()
                .style(move |s| {
                    let base = s.absolute().inset(0.0).size(100.0, 100.0);
                    if is_visible.get() {
                        base
                    } else {
                        base.display(floem::taffy::Display::None)
                    }
                })
                .on_click_stop(move |_| {
                    overlay_clone.set(true);
                }),
        ),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click while overlay is visible - overlay should receive click
    harness.click(50.0, 50.0);
    assert!(clicked_overlay.get(), "Visible overlay should receive click");
    assert!(!clicked_regular.get(), "Regular view should NOT receive click when overlay is visible");

    // Reset and hide overlay
    clicked_overlay.set(false);
    clicked_regular.set(false);
    is_visible.set(false);
    harness.rebuild();

    // Click while overlay is hidden - regular view should receive click
    harness.click(50.0, 50.0);
    assert!(clicked_regular.get(), "Regular view should receive click when overlay is hidden");
    assert!(!clicked_overlay.get(), "Hidden overlay should NOT receive click");
}
