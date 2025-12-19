//! Tests for reactive style updates and repaint triggering.
//!
//! These tests verify that:
//! - When a signal used in a style closure changes, the style is recalculated
//! - Disabled state changes via reactive signals are properly reflected
//!
//! KNOWN BUG: The disabled selector isn't applied correctly when set_disabled()
//! is used in a reactive style closure because resolve_nested_maps checks
//! interact_state.is_disabled which is computed from the OLD computed_style,
//! not the NEW style being computed.

use floem::prelude::*;
use floem::style::{Background, Disabled};
use floem::peniko::Brush;
use floem_test::prelude::*;

/// Test that a reactive style closure re-runs when its signal changes.
/// This test verifies the basic reactive style update mechanism.
#[test]
fn test_reactive_style_updates_on_signal_change() {
    let counter = RwSignal::new(0);

    let view = Empty::new()
        .style(move |s| {
            s.size(100.0, 100.0)
                .background(if counter.get() == 0 {
                    palette::css::GRAY
                } else {
                    palette::css::BLUE
                })
        });
    let id = view.view_id();

    let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

    // Initial state: counter is 0, background should be GRAY
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "Initial background should be GRAY, got {:?}",
        bg
    );

    // Update the signal
    counter.set(1);

    // Rebuild to process the reactive update
    harness.rebuild();

    // Check that background changed to BLUE
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Background should be BLUE after signal change, got {:?}",
        bg
    );
}

/// Test that the Disabled property is set correctly via set_disabled().
/// This verifies that set_disabled(true) sets the Disabled property in the style.
#[test]
fn test_set_disabled_sets_property() {
    let counter = RwSignal::new(0);

    let view = Empty::new()
        .style(move |s| {
            s.size(100.0, 100.0)
                .set_disabled(counter.get() == 0)
        });
    let id = view.view_id();

    let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

    // Initial state: counter is 0, Disabled should be true
    let style = harness.get_computed_style(id);
    let disabled = style.get(Disabled);
    assert!(
        disabled,
        "Disabled property should be true when counter == 0"
    );

    // Update the signal
    counter.set(1);
    harness.rebuild();

    // Now Disabled should be false
    let style = harness.get_computed_style(id);
    let disabled = style.get(Disabled);
    assert!(
        !disabled,
        "Disabled property should be false when counter != 0"
    );
}

/// Test that the disabled selector styles are applied correctly on first pass.
///
/// KNOWN BUG: This test currently FAILS because resolve_nested_maps checks
/// interact_state.is_disabled which is computed from the OLD computed_style,
/// not the NEW style. This means .disabled() selector isn't applied on the
/// first style pass after set_disabled(true) is set.
#[test]
fn test_disabled_selector_applied_on_first_pass() {
    let counter = RwSignal::new(0);

    let view = Empty::new()
        .style(move |s| {
            s.size(100.0, 100.0)
                .background(palette::css::LIGHT_BLUE)
                .set_disabled(counter.get() == 0)
                .disabled(|s| s.background(palette::css::LIGHT_GRAY))
        });
    let id = view.view_id();

    let harness = TestHarness::new_with_size(view, 100.0, 100.0);

    // Initial state: counter is 0, should be disabled with LIGHT_GRAY background
    // BUG: This currently shows LIGHT_BLUE because the disabled selector isn't applied
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);

    // This assertion documents the BUG - the disabled selector should apply LIGHT_GRAY
    // but it currently shows LIGHT_BLUE because interact_state.is_disabled is computed
    // from the OLD computed_style
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::LIGHT_GRAY),
        "BUG: Disabled view should have LIGHT_GRAY background, got {:?}. \
         This fails because resolve_nested_maps checks interact_state.is_disabled \
         which is computed from the OLD computed_style.",
        bg
    );
}

/// Test that transitioning from disabled to enabled works correctly.
/// This is the scenario from the counter example where clicking "Increment"
/// should change the "Reset to 0" button from disabled (gray) to enabled (blue).
///
/// KNOWN BUG: This test currently FAILS because when the signal changes:
/// 1. set_disabled(false) is called
/// 2. The Disabled property correctly becomes false
/// 3. BUT interact_state.is_disabled is still true (from OLD computed_style)
/// 4. So the disabled selector is incorrectly still applied
/// 5. Result: background stays LIGHT_GRAY instead of becoming LIGHT_BLUE
///
/// This is exactly the bug the user reported: clicking "Increment" doesn't
/// update the "Reset to 0" button's appearance until hover/leave.
#[test]
fn test_disabled_to_enabled_transition() {
    let counter = RwSignal::new(0);

    let view = Empty::new()
        .style(move |s| {
            s.size(100.0, 100.0)
                .background(palette::css::LIGHT_BLUE)
                .set_disabled(counter.get() == 0)
                .disabled(|s| s.background(palette::css::LIGHT_GRAY))
        });
    let id = view.view_id();

    let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

    // Initial state: disabled=true, Disabled property is set
    let style = harness.get_computed_style(id);
    let disabled = style.get(Disabled);
    assert!(disabled, "Initially Disabled should be true");

    // Request a style recalculation to apply the disabled selector
    id.request_style();
    harness.rebuild();

    // Now background should be LIGHT_GRAY (disabled)
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::LIGHT_GRAY),
        "After style request, disabled view should have LIGHT_GRAY, got {:?}",
        bg
    );

    // Now change signal to make it enabled
    counter.set(1);
    harness.rebuild();

    // After signal change, Disabled PROPERTY should be false
    let style = harness.get_computed_style(id);
    let disabled = style.get(Disabled);
    assert!(!disabled, "After counter=1, Disabled property should be false");

    // BUG: Background should change to LIGHT_BLUE (enabled)
    // But it stays LIGHT_GRAY because interact_state.is_disabled was true
    // (computed from OLD computed_style before the reactive update)
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::LIGHT_BLUE),
        "BUG: After enabling, view should have LIGHT_BLUE, got {:?}. \
         This fails because interact_state.is_disabled is computed from \
         the OLD computed_style, not the NEW style being applied.",
        bg
    );
}

/// Test that disabled selector works after explicitly requesting a style recalculation.
/// This verifies that the disabled selector IS applied correctly once the
/// computed_style has been updated to include Disabled: true AND we force
/// a style recalculation.
#[test]
fn test_disabled_selector_after_style_request() {
    let counter = RwSignal::new(0);

    let view = Empty::new()
        .style(move |s| {
            s.size(100.0, 100.0)
                .background(palette::css::LIGHT_BLUE)
                .set_disabled(counter.get() == 0)
                .disabled(|s| s.background(palette::css::LIGHT_GRAY))
        });
    let id = view.view_id();

    let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

    // After first style pass, Disabled property should be true
    let style = harness.get_computed_style(id);
    let disabled = style.get(Disabled);
    assert!(disabled, "Disabled property should be true after first pass");

    // But the disabled selector wasn't applied (BUG)
    // Now explicitly request a style recalculation
    id.request_style();

    // Rebuild - this should now run the style pass because we requested it
    harness.rebuild();

    let style = harness.get_computed_style(id);
    let bg = style.get(Background);

    // After explicit style request + rebuild, the disabled selector SHOULD be applied
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::LIGHT_GRAY),
        "After explicit style request, disabled view should have LIGHT_GRAY background, got {:?}",
        bg
    );
}

/// Test that multiple reactive styles on different views update correctly.
#[test]
fn test_multiple_reactive_styles() {
    let signal = RwSignal::new(false);

    let view1 = Empty::new()
        .style(move |s| {
            s.size(50.0, 50.0)
                .background(if signal.get() {
                    palette::css::RED
                } else {
                    palette::css::BLUE
                })
        });
    let id1 = view1.view_id();

    let view2 = Empty::new()
        .style(move |s| {
            s.size(50.0, 50.0)
                .background(if signal.get() {
                    palette::css::GREEN
                } else {
                    palette::css::YELLOW
                })
        });
    let id2 = view2.view_id();

    let view = stack((view1, view2))
        .style(|s| s.size(100.0, 50.0));

    let mut harness = TestHarness::new_with_size(view, 100.0, 50.0);

    // Initial state: signal is false
    let style1 = harness.get_computed_style(id1);
    let bg1 = style1.get(Background);
    assert!(
        matches!(bg1, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "View1 should be BLUE initially, got {:?}",
        bg1
    );

    let style2 = harness.get_computed_style(id2);
    let bg2 = style2.get(Background);
    assert!(
        matches!(bg2, Some(Brush::Solid(c)) if c == palette::css::YELLOW),
        "View2 should be YELLOW initially, got {:?}",
        bg2
    );

    // Toggle signal
    signal.set(true);
    harness.rebuild();

    // After toggle: both should have updated
    let style1 = harness.get_computed_style(id1);
    let bg1 = style1.get(Background);
    assert!(
        matches!(bg1, Some(Brush::Solid(c)) if c == palette::css::RED),
        "View1 should be RED after toggle, got {:?}",
        bg1
    );

    let style2 = harness.get_computed_style(id2);
    let bg2 = style2.get(Background);
    assert!(
        matches!(bg2, Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "View2 should be GREEN after toggle, got {:?}",
        bg2
    );
}

/// Test the counter example scenario:
/// - Click on button changes counter from 0 to 1
/// - Another view's disabled state should update accordingly
///
/// This test simulates the bug reported: clicking "Increment" doesn't cause
/// the "Reset to 0" button to update its appearance.
#[test]
fn test_counter_example_repaint_scenario() {
    let counter = RwSignal::new(0);

    // Create views similar to the counter example
    let increment_btn = Empty::new()
        .style(|s| s.size(80.0, 30.0).background(palette::css::WHITE))
        .on_click_stop({
            move |_| {
                counter.update(|value| *value += 1);
            }
        });

    let reset_btn = Empty::new()
        .style(move |s| {
            s.size(80.0, 30.0)
                .background(palette::css::LIGHT_BLUE)
                .set_disabled(counter.get() == 0)
                .disabled(|s| s.background(palette::css::LIGHT_GRAY))
        });
    let reset_id = reset_btn.view_id();

    let view = stack((increment_btn, reset_btn))
        .style(|s| s.size(200.0, 100.0));

    let mut harness = TestHarness::new_with_size(view, 200.0, 100.0);

    // Initial state: counter is 0
    // After first style pass, Disabled property should be true
    let style = harness.get_computed_style(reset_id);
    let disabled = style.get(Disabled);
    assert!(disabled, "Reset button should have Disabled=true when counter==0");

    // Click on the increment button (at position within the first button)
    harness.click(40.0, 15.0);

    // After click, counter should be 1
    assert_eq!(counter.get(), 1, "Counter should be 1 after click");

    // Verify that the Disabled property changed
    let style = harness.get_computed_style(reset_id);
    let disabled = style.get(Disabled);
    assert!(
        !disabled,
        "Reset button should have Disabled=false after counter incremented"
    );
}

/// Test that style changes trigger style recalculation flag.
#[test]
fn test_style_change_triggers_recalculation() {
    let counter = RwSignal::new(0);

    let view = Empty::new()
        .style(move |s| {
            s.size(100.0, 100.0)
                .background(if counter.get() == 0 {
                    palette::css::GRAY
                } else {
                    palette::css::BLUE
                })
        });
    let id = view.view_id();

    let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

    // Initial background is GRAY
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY));

    // Change the signal - this should trigger style recalculation
    counter.set(1);

    // Check if style change was requested
    let has_pending = harness.has_pending_style_change(id);
    assert!(
        has_pending,
        "View should have pending style changes after signal update"
    );

    // Rebuild to apply changes
    harness.rebuild();

    // After rebuild, should no longer have pending changes
    let has_pending = harness.has_pending_style_change(id);
    assert!(
        !has_pending,
        "View should NOT have pending style changes after rebuild"
    );

    // And the style should have updated
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Background should be BLUE after rebuild"
    );
}
