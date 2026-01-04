//! Tests for slider active view dispatch coordinate transformation.
//!
//! This tests the bug where the slider goes back to 0% on pointer up when nested
//! in a container. The root cause is that `dispatch_to_active_view` uses a different
//! coordinate transformation than the normal path-based dispatch.
//!
//! When a view is set as "active" (which the slider does on PointerDown via
//! `cx.update_active(self.id())`), subsequent events like PointerUp are dispatched
//! via `dispatch_to_active_view` which uses:
//!
//!   transform = translate(window_origin - layout.location + viewport)
//!
//! But this produces coordinates relative to the parent, not the view itself.
//! The correct transformation should use just `window_origin` (matching
//! how `local_to_root_transform` is computed during layout).

use floem::prelude::*;
use floem::views::slider;
use floem_test::prelude::*;
use serial_test::serial;
use std::cell::Cell;
use std::rc::Rc;

/// Test that a slider nested in a container maintains its value after pointer up.
///
/// Note: This test with single-level nesting happens to pass because for a single
/// Container with padding, layout.location equals window_origin. The bug only
/// manifests with deeper nesting where these values differ.
#[test]
#[serial]
fn test_slider_single_nested_maintains_value() {
    let slider_percent = Rc::new(Cell::new(50.0));
    let slider_percent_read = slider_percent.clone();
    let slider_percent_write = slider_percent.clone();

    // Create a slider with an on_change callback to track value changes
    let slider_view = slider::Slider::new(move || slider_percent_read.get().pct())
        .on_change_pct(move |pct| {
            slider_percent_write.set(pct.0);
        })
        .style(|s| s.width(200.0).height(20.0));

    // Single-level nesting: slider inside one container with padding
    // This happens to work because layout.location == (padding, padding) == window_origin offset
    let view = Container::new(slider_view).style(|s| s.padding(50.0).size(300.0, 120.0));

    let mut harness = HeadlessHarness::new_with_size(view, 300.0, 120.0);

    // Click at 75% position within the slider
    // Slider is at x=50 (padding), width=200, so 75% = 50 + 150 = 200
    let click_x = 50.0 + 150.0; // 200
    let click_y = 50.0 + 10.0; // Center of slider vertically

    harness.pointer_down(click_x, click_y);
    harness.rebuild();

    let value_after_down = slider_percent.get();

    // The slider should have jumped to approximately 75%
    assert!(
        value_after_down > 50.0,
        "Slider should have moved from initial 50% after pointer down. Got: {}%",
        value_after_down
    );

    // Now release the pointer at the same position
    harness.pointer_up(click_x, click_y);
    harness.rebuild();

    let value_after_up = slider_percent.get();

    // For single nesting, values should be consistent (this passes by coincidence)
    assert!(
        (value_after_up - value_after_down).abs() < 1.0,
        "Slider value should remain stable after pointer up. \
         Was: {}% after down, became: {}% after up",
        value_after_down,
        value_after_up
    );
}

/// Test that a slider deeply nested in containers works correctly.
///
/// THIS TEST DEMONSTRATES THE BUG: When a slider is nested multiple levels deep,
/// the coordinate transformation in `dispatch_to_active_view` is incorrect.
///
/// The bug occurs because:
/// - `layout.location` is the slider's position relative to its direct parent
/// - `window_origin` is the slider's absolute position in window coordinates
/// - For deep nesting, these values differ by the accumulated parent offsets
/// - But `dispatch_to_active_view` incorrectly uses `window_origin - layout.location`,
///   which produces parent-relative coordinates instead of view-local coordinates
#[test]
#[serial]
fn test_slider_deeply_nested_maintains_value() {
    let slider_percent = Rc::new(Cell::new(50.0));
    let slider_percent_read = slider_percent.clone();
    let slider_percent_write = slider_percent.clone();

    let slider_view = slider::Slider::new(move || slider_percent_read.get().pct())
        .on_change_pct(move |pct| {
            slider_percent_write.set(pct.0);
        })
        .style(|s| s.width(200.0).height(20.0));

    // Nest the slider multiple levels deep
    let view = Container::new(Container::new(Container::new(slider_view).style(|s| {
        s.padding(20.0)
    }))
    .style(|s| s.padding(30.0)))
    .style(|s| s.padding(50.0).size(400.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 200.0);

    // Slider is at x = 50 + 30 + 20 = 100, width = 200
    // Click at 50% position: 100 + 100 = 200
    let click_x = 100.0 + 100.0;
    let click_y = 100.0 + 10.0;

    harness.pointer_down(click_x, click_y);
    harness.rebuild();

    let value_after_down = slider_percent.get();

    harness.pointer_up(click_x, click_y);
    harness.rebuild();

    let value_after_up = slider_percent.get();

    assert!(
        (value_after_up - value_after_down).abs() < 1.0,
        "Deeply nested slider should maintain value. \
         Was: {}% after down, became: {}% after up",
        value_after_down,
        value_after_up
    );
}

/// Test slider behavior when clicking and releasing at different positions.
///
/// The slider should update to the release position, not jump to 0 or some other value.
#[test]
#[serial]
fn test_slider_drag_behavior() {
    let slider_percent = Rc::new(Cell::new(0.0));
    let slider_percent_read = slider_percent.clone();
    let slider_percent_write = slider_percent.clone();

    let slider_view = slider::Slider::new(move || slider_percent_read.get().pct())
        .on_change_pct(move |pct| {
            slider_percent_write.set(pct.0);
        })
        .style(|s| s.width(200.0).height(20.0));

    // Offset the slider from origin
    let view = Container::new(slider_view).style(|s| s.padding(100.0).size(400.0, 220.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 220.0);

    // Slider is at x=100, width=200
    // Start at 25%: 100 + 50 = 150
    let start_x = 150.0;
    let y = 110.0;

    harness.pointer_down(start_x, y);
    harness.rebuild();

    let value_at_start = slider_percent.get();

    // Drag to 75%: 100 + 150 = 250
    let end_x = 250.0;
    harness.pointer_move(end_x, y);
    harness.rebuild();

    let value_during_drag = slider_percent.get();

    // Release at 75%
    harness.pointer_up(end_x, y);
    harness.rebuild();

    let value_after_release = slider_percent.get();

    // Value should be higher after drag
    assert!(
        value_during_drag > value_at_start,
        "Slider should increase during drag. Start: {}%, During drag: {}%",
        value_at_start,
        value_during_drag
    );

    // Value should remain stable after release
    assert!(
        (value_after_release - value_during_drag).abs() < 5.0,
        "Slider should maintain position after release. \
         During drag: {}%, After release: {}%",
        value_during_drag,
        value_after_release
    );

    // Most importantly: it should NOT go to 0
    assert!(
        value_after_release > 50.0,
        "Slider should NOT reset to 0. Got: {}%",
        value_after_release
    );
}

/// Test that slider at the window origin works correctly (baseline test).
///
/// This should work because there's no offset to cause transformation errors.
#[test]
#[serial]
fn test_slider_at_origin_works() {
    let slider_percent = Rc::new(Cell::new(50.0));
    let slider_percent_read = slider_percent.clone();
    let slider_percent_write = slider_percent.clone();

    let slider_view = slider::Slider::new(move || slider_percent_read.get().pct())
        .on_change_pct(move |pct| {
            slider_percent_write.set(pct.0);
        })
        .style(|s| s.width(200.0).height(20.0));

    // No padding - slider is at origin
    let view = slider_view;

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 20.0);

    // Click at 75%: 150
    let click_x = 150.0;
    let click_y = 10.0;

    harness.pointer_down(click_x, click_y);
    harness.rebuild();

    let value_after_down = slider_percent.get();

    harness.pointer_up(click_x, click_y);
    harness.rebuild();

    let value_after_up = slider_percent.get();

    // Even at origin, values should be consistent
    assert!(
        (value_after_up - value_after_down).abs() < 1.0,
        "Slider at origin should maintain value. \
         Was: {}% after down, became: {}% after up",
        value_after_down,
        value_after_up
    );
}

/// Test with reactive RwSignal-based slider (widget-gallery pattern).
///
/// This replicates the exact pattern used in the widget-gallery example.
#[test]
#[serial]
fn test_slider_with_rw_signal() {
    // Use RwSignal like the widget-gallery example
    let slider_state = RwSignal::new(50.0.pct());

    let slider_view = slider::Slider::new_rw(slider_state).style(|s| s.width(200.0).height(20.0));

    // Nest in container with padding
    let view = Container::new(slider_view).style(|s| s.padding(75.0).size(350.0, 170.0));

    let mut harness = HeadlessHarness::new_with_size(view, 350.0, 170.0);

    // Slider is at x=75, width=200
    // Click at approximately 80%: 75 + 160 = 235
    let click_x = 235.0;
    let click_y = 85.0;

    harness.pointer_down(click_x, click_y);
    harness.rebuild();

    let value_after_down = slider_state.get_untracked().0;

    assert!(
        value_after_down > 60.0,
        "Slider should have moved to ~80%. Got: {}%",
        value_after_down
    );

    harness.pointer_up(click_x, click_y);
    harness.rebuild();

    let value_after_up = slider_state.get_untracked().0;

    assert!(
        (value_after_up - value_after_down).abs() < 1.0,
        "RwSignal slider should maintain value. \
         Was: {}% after down, became: {}% after up",
        value_after_down,
        value_after_up
    );

    // Should definitely NOT be 0
    assert!(
        value_after_up > 50.0,
        "Slider should NOT reset to initial value or 0. Got: {}%",
        value_after_up
    );
}
