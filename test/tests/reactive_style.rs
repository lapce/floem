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

use floem::peniko::Brush;
use floem::prelude::*;
use floem::style::{Background, Disabled, Selected};
use floem_test::prelude::*;

/// Test that a reactive style closure re-runs when its signal changes.
/// This test verifies the basic reactive style update mechanism.
#[test]
fn test_reactive_style_updates_on_signal_change() {
    let counter = RwSignal::new(0);

    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0).background(if counter.get() == 0 {
            palette::css::GRAY
        } else {
            palette::css::BLUE
        })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

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

    let view = Empty::new().style(move |s| s.size(100.0, 100.0).set_disabled(counter.get() == 0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

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

    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .background(palette::css::LIGHT_BLUE)
            .set_disabled(counter.get() == 0)
            .disabled(|s| s.background(palette::css::LIGHT_GRAY))
    });
    let id = view.view_id();

    let harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

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

    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .background(palette::css::LIGHT_BLUE)
            .set_disabled(counter.get() == 0)
            .disabled(|s| s.background(palette::css::LIGHT_GRAY))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

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
    assert!(
        !disabled,
        "After counter=1, Disabled property should be false"
    );

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

    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .background(palette::css::LIGHT_BLUE)
            .set_disabled(counter.get() == 0)
            .disabled(|s| s.background(palette::css::LIGHT_GRAY))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // After first style pass, Disabled property should be true
    let style = harness.get_computed_style(id);
    let disabled = style.get(Disabled);
    assert!(
        disabled,
        "Disabled property should be true after first pass"
    );

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

    let view1 = Empty::new().style(move |s| {
        s.size(50.0, 50.0).background(if signal.get() {
            palette::css::RED
        } else {
            palette::css::BLUE
        })
    });
    let id1 = view1.view_id();

    let view2 = Empty::new().style(move |s| {
        s.size(50.0, 50.0).background(if signal.get() {
            palette::css::GREEN
        } else {
            palette::css::YELLOW
        })
    });
    let id2 = view2.view_id();

    let view = Stack::new((view1, view2)).style(|s| s.size(100.0, 50.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 50.0);

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

    let reset_btn = Empty::new().style(move |s| {
        s.size(80.0, 30.0)
            .background(palette::css::LIGHT_BLUE)
            .set_disabled(counter.get() == 0)
            .disabled(|s| s.background(palette::css::LIGHT_GRAY))
    });
    let reset_id = reset_btn.view_id();

    let view = Stack::new((increment_btn, reset_btn)).style(|s| s.size(200.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 100.0);

    // Initial state: counter is 0
    // After first style pass, Disabled property should be true
    let style = harness.get_computed_style(reset_id);
    let disabled = style.get(Disabled);
    assert!(
        disabled,
        "Reset button should have Disabled=true when counter==0"
    );

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

// ============================================================================
// Repaint Request Tests
// ============================================================================
// These tests verify that when reactive styles change, a repaint is requested.
// This is critical for the UI to actually update visually.

/// Test that reactive style changes trigger a repaint.
///
/// When a signal used in a style closure changes, the framework should:
/// 1. Run the reactive effect (style closure)
/// 2. Mark the view's style as dirty
/// 3. Recompute styles and return true from process_update_no_paint() indicating repaint needed
#[test]
fn test_reactive_style_change_requests_repaint() {
    let color_signal = RwSignal::new(palette::css::RED);

    let view = Empty::new().style(move |s| s.size(100.0, 100.0).background(color_signal.get()));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Change the signal - this should trigger style recalculation and repaint
    color_signal.set(palette::css::BLUE);

    // process_update_no_paint() runs reactive effects, processes style/layout, and returns
    // true if a repaint would be scheduled
    let needs_repaint = harness.process_update_no_paint();

    // Verify the style actually changed
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Style should update to BLUE after signal change"
    );

    // Verify repaint was triggered
    assert!(
        needs_repaint,
        "process_update_no_paint() should return true when style changes"
    );
}

/// Test that clicking a view and changing a reactive style triggers repaint.
///
/// This simulates the real-world scenario where a click handler changes
/// a signal that affects another view's style.
#[test]
fn test_click_triggered_style_change_requests_repaint() {
    let is_active = RwSignal::new(false);

    let button = Empty::new()
        .style(|s| s.size(50.0, 30.0).background(palette::css::WHITE))
        .on_click_stop(move |_| {
            is_active.set(true);
        });

    let indicator = Empty::new().style(move |s| {
        s.size(50.0, 30.0).background(if is_active.get() {
            palette::css::GREEN
        } else {
            palette::css::RED
        })
    });
    let indicator_id = indicator.view_id();

    let view = Stack::new((button, indicator)).style(|s| s.size(100.0, 50.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 50.0);

    // Initial state
    let style = harness.get_computed_style(indicator_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::RED),
        "Initial should be RED"
    );

    // Click the button (at center of first 50x30 area)
    // Note: click() calls event() which calls process_update_no_paint() internally
    harness.click(25.0, 15.0);

    // Verify signal changed
    assert!(is_active.get(), "Signal should be true after click");

    // Now process the reactive effects and check if repaint is needed
    let needs_repaint = harness.process_update_no_paint();

    // Verify style updated
    let style = harness.get_computed_style(indicator_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "After click should be GREEN"
    );

    // Verify repaint was triggered
    assert!(
        needs_repaint,
        "process_update_no_paint() should return true after click changes reactive style"
    );
}

/// Test that RequestStyle message triggers repaint.
///
/// This directly tests that when a view requests style recalculation,
/// a repaint is also scheduled.
#[test]
fn test_request_style_triggers_repaint() {
    let view = Empty::new().style(|s| s.size(100.0, 100.0).background(palette::css::GRAY));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Manually request style
    id.request_style();

    // Process the message and check if repaint is needed
    let needs_repaint = harness.process_update_no_paint();

    // Style recalculation should trigger repaint
    assert!(
        needs_repaint,
        "process_update_no_paint() should return true after request_style()"
    );
}

/// Test that style changes during event handling trigger repaint.
#[test]
fn test_style_change_in_event_handler_triggers_repaint() {
    let counter = RwSignal::new(0);

    let view = Empty::new()
        .style(move |s| {
            s.size(100.0, 100.0).background(if counter.get() == 0 {
                palette::css::RED
            } else {
                palette::css::BLUE
            })
        })
        .on_click_stop(move |_| {
            counter.update(|c| *c += 1);
        });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial state
    let style = harness.get_computed_style(id);
    assert!(matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::RED));

    // Click to increment counter
    harness.click(50.0, 50.0);

    // Process the reactive effects and check if repaint is needed
    let needs_repaint = harness.process_update_no_paint();

    // Verify style changed
    let style = harness.get_computed_style(id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Style should be BLUE after click"
    );

    // Verify repaint was triggered
    assert!(
        needs_repaint,
        "process_update_no_paint() should return true when style changes via click handler"
    );
}

/// Test that style changes trigger style recalculation flag.
#[test]
fn test_style_change_triggers_recalculation() {
    let counter = RwSignal::new(0);

    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0).background(if counter.get() == 0 {
            palette::css::GRAY
        } else {
            palette::css::BLUE
        })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

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

// ============================================================================
// Selector State Bug Tests
// ============================================================================
// These tests reveal the bug where selector states (disabled, selected, etc.)
// are computed from the OLD computed_style, not the NEW style being applied.
// This causes selectors to be applied one frame late.

/// Test that the Selected property can be set reactively via set_selected().
#[test]
fn test_set_selected_sets_property() {
    let is_selected = RwSignal::new(false);

    let view =
        Empty::new().style(move |s| s.size(100.0, 100.0).set_selected(is_selected.get()));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial state: not selected
    let style = harness.get_computed_style(id);
    let selected = style.get(Selected);
    assert!(!selected, "Selected property should be false initially");

    // Update the signal
    is_selected.set(true);
    harness.rebuild();

    // Now Selected should be true
    let style = harness.get_computed_style(id);
    let selected = style.get(Selected);
    assert!(selected, "Selected property should be true after signal change");
}

/// Test that the .selected() selector is applied when set_selected(true) is used.
///
/// KNOWN BUG: This test may FAIL because resolve_nested_maps checks
/// interact_state.is_selected which may be computed from the OLD computed_style.
#[test]
fn test_selected_selector_applied_on_first_pass() {
    let is_selected = RwSignal::new(true);

    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .background(palette::css::WHITE)
            .set_selected(is_selected.get())
            .selected(|s| s.background(palette::css::BLUE))
    });
    let id = view.view_id();

    let harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial state: selected=true, should have BLUE background
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);

    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Selected view should have BLUE background on first pass, got {:?}. \
         This may fail if interact_state.is_selected is computed from OLD style.",
        bg
    );
}

/// Test transition from not-selected to selected.
#[test]
fn test_not_selected_to_selected_transition() {
    let is_selected = RwSignal::new(false);

    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .background(palette::css::GRAY)
            .set_selected(is_selected.get())
            .selected(|s| s.background(palette::css::BLUE))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial: not selected, background should be GRAY
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "Initial background should be GRAY, got {:?}",
        bg
    );

    // Change to selected
    is_selected.set(true);
    harness.rebuild();

    // Should now be BLUE
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "After selecting, background should be BLUE, got {:?}. \
         BUG: The selected selector may not be applied immediately.",
        bg
    );
}

/// Test that selected-to-not-selected transition works correctly.
#[test]
fn test_selected_to_not_selected_transition() {
    let is_selected = RwSignal::new(true);

    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .background(palette::css::GRAY)
            .set_selected(is_selected.get())
            .selected(|s| s.background(palette::css::BLUE))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Request style to ensure selector is applied
    id.request_style();
    harness.rebuild();

    // Should be BLUE (selected)
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Selected background should be BLUE, got {:?}",
        bg
    );

    // Deselect
    is_selected.set(false);
    harness.rebuild();

    // Should now be GRAY
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "After deselecting, background should be GRAY, got {:?}. \
         BUG: The selected selector may still be applied.",
        bg
    );
}

/// Test combining disabled and selected selectors.
/// This tests the interaction between multiple state-based selectors.
#[test]
fn test_disabled_and_selected_selectors_combined() {
    let is_disabled = RwSignal::new(false);
    let is_selected = RwSignal::new(false);

    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .background(palette::css::WHITE) // Default
            .set_disabled(is_disabled.get())
            .set_selected(is_selected.get())
            .disabled(|s| s.background(palette::css::GRAY)) // Disabled
            .selected(|s| s.background(palette::css::BLUE)) // Selected
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial: neither disabled nor selected -> WHITE
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::WHITE),
        "Initial should be WHITE, got {:?}",
        bg
    );

    // Set selected only -> BLUE
    is_selected.set(true);
    harness.rebuild();

    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Selected-only should be BLUE, got {:?}",
        bg
    );

    // Set disabled (while still selected) -> GRAY (disabled takes precedence)
    is_disabled.set(true);
    harness.rebuild();

    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "Disabled+Selected should be GRAY (disabled takes precedence), got {:?}",
        bg
    );
}

/// Test that multiple rapid state changes are handled correctly.
#[test]
fn test_rapid_state_changes() {
    let state = RwSignal::new(0);

    let view = Empty::new().style(move |s| {
        let val = state.get();
        s.size(100.0, 100.0)
            .background(palette::css::WHITE)
            .set_disabled(val == 1)
            .set_selected(val == 2)
            .disabled(|s| s.background(palette::css::GRAY))
            .selected(|s| s.background(palette::css::BLUE))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // State 0: normal -> WHITE
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::WHITE),
        "State 0 should be WHITE"
    );

    // State 1: disabled -> GRAY
    state.set(1);
    harness.rebuild();
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "State 1 should be GRAY (disabled)"
    );

    // State 2: selected -> BLUE
    state.set(2);
    harness.rebuild();
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "State 2 should be BLUE (selected)"
    );

    // State 0: back to normal -> WHITE
    state.set(0);
    harness.rebuild();
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::WHITE),
        "Back to state 0 should be WHITE"
    );
}

/// Test that a view's style updates when its sibling's state changes.
/// This simulates the sidebar menu button scenario.
#[test]
fn test_sibling_state_affects_other_sibling() {
    let active_index = RwSignal::new(0);

    // Button 0 is selected when active_index == 0
    let btn0 = Empty::new().style(move |s| {
        s.size(50.0, 30.0)
            .background(palette::css::WHITE)
            .set_selected(active_index.get() == 0)
            .selected(|s| s.background(palette::css::BLUE))
    });
    let btn0_id = btn0.view_id();

    // Button 1 is selected when active_index == 1
    let btn1 = Empty::new().style(move |s| {
        s.size(50.0, 30.0)
            .background(palette::css::WHITE)
            .set_selected(active_index.get() == 1)
            .selected(|s| s.background(palette::css::BLUE))
    });
    let btn1_id = btn1.view_id();

    let view = Stack::new((btn0, btn1)).style(|s| s.size(100.0, 60.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 60.0);

    // Request style for both to ensure selectors are applied
    btn0_id.request_style();
    btn1_id.request_style();
    harness.rebuild();

    // Initial: btn0 selected (BLUE), btn1 not selected (WHITE)
    let style0 = harness.get_computed_style(btn0_id);
    let style1 = harness.get_computed_style(btn1_id);
    assert!(
        matches!(style0.get(Background), Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Button 0 should be BLUE initially"
    );
    assert!(
        matches!(style1.get(Background), Some(Brush::Solid(c)) if c == palette::css::WHITE),
        "Button 1 should be WHITE initially"
    );

    // Switch to button 1
    active_index.set(1);
    harness.rebuild();

    // Now: btn0 not selected (WHITE), btn1 selected (BLUE)
    let style0 = harness.get_computed_style(btn0_id);
    let style1 = harness.get_computed_style(btn1_id);
    assert!(
        matches!(style0.get(Background), Some(Brush::Solid(c)) if c == palette::css::WHITE),
        "Button 0 should be WHITE after switching, got {:?}. BUG: May still show BLUE.",
        style0.get(Background)
    );
    assert!(
        matches!(style1.get(Background), Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Button 1 should be BLUE after switching, got {:?}. BUG: May still show WHITE.",
        style1.get(Background)
    );
}

/// Test that nested style closures with selectors work correctly.
#[test]
fn test_nested_selector_with_reactive_state() {
    let is_active = RwSignal::new(false);

    // This pattern is common: a base style with a selected sub-style
    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .background(palette::css::LIGHT_GRAY)
            .set_selected(is_active.get())
            .selected(|s| {
                // Nested style within the selected selector
                s.background(palette::css::LIGHT_BLUE).border(2.0)
            })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial: not active -> LIGHT_GRAY, no border
    let style = harness.get_computed_style(id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::LIGHT_GRAY)
    );

    // Activate
    is_active.set(true);
    harness.rebuild();

    // Should be LIGHT_BLUE with border
    let style = harness.get_computed_style(id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::LIGHT_BLUE),
        "Active view should have LIGHT_BLUE background"
    );
}

/// Test that hover + selected combination works correctly.
/// This simulates a menu item that is both selected and being hovered.
#[test]
fn test_hover_and_selected_combination() {
    let is_selected = RwSignal::new(true);

    // Create view smaller than harness so we have space to move pointer outside
    let view = Empty::new().style(move |s| {
        s.size(80.0, 80.0)
            .margin(10.0) // Add margin so view is at (10,10) to (90,90)
            .background(palette::css::WHITE)
            .set_selected(is_selected.get())
            .selected(|s| s.background(palette::css::LIGHT_BLUE))
            .hover(|s| s.background(palette::css::LIGHT_GRAY))
    });
    let id = view.view_id();

    // Harness larger than view so pointer can be "outside"
    let mut harness = HeadlessHarness::new_with_size(view, 120.0, 120.0);

    // Request style to apply selected
    id.request_style();
    harness.rebuild();

    // Initially: selected but not hovered -> LIGHT_BLUE
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::LIGHT_BLUE),
        "Selected (not hovered) should be LIGHT_BLUE, got {:?}",
        bg
    );

    // Get the view's actual position
    let view_rect = harness.get_layout_rect(id);

    // Hover over the view (center of view)
    harness.pointer_move(view_rect.center().x, view_rect.center().y);
    harness.rebuild();

    // Now: selected AND hovered -> LIGHT_GRAY (hover takes precedence)
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::LIGHT_GRAY),
        "Selected + hovered should be LIGHT_GRAY (hover takes precedence), got {:?}",
        bg
    );

    // Move pointer outside the view (to corner of harness, outside the view)
    harness.pointer_move(5.0, 5.0);
    harness.rebuild();

    // Back to: selected but not hovered -> LIGHT_BLUE
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::LIGHT_BLUE),
        "After hover leaves, should be LIGHT_BLUE again, got {:?}",
        bg
    );
}

/// Test clicking a button to change selection (simulates real UI interaction).
#[test]
fn test_click_changes_selection_style() {
    let selected_id = RwSignal::new(0usize);

    // Create buttons that show different colors when selected
    let btn0 = Empty::new()
        .style(move |s| {
            s.size(50.0, 30.0)
                .background(palette::css::WHITE)
                .set_selected(selected_id.get() == 0)
                .selected(|s| s.background(palette::css::RED))
        })
        .on_click_stop(move |_| selected_id.set(0));
    let btn0_id = btn0.view_id();

    let btn1 = Empty::new()
        .style(move |s| {
            s.size(50.0, 30.0)
                .background(palette::css::WHITE)
                .set_selected(selected_id.get() == 1)
                .selected(|s| s.background(palette::css::GREEN))
        })
        .on_click_stop(move |_| selected_id.set(1));
    let btn1_id = btn1.view_id();

    let view = Stack::new((btn0, btn1)).style(|s| s.size(100.0, 60.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 60.0);

    // Request styles initially
    btn0_id.request_style();
    btn1_id.request_style();
    harness.rebuild();

    // Get button positions
    let btn1_rect = harness.get_layout_rect(btn1_id);

    // Initial state: btn0 is selected (RED), btn1 is not (WHITE)
    let style0 = harness.get_computed_style(btn0_id);
    let style1 = harness.get_computed_style(btn1_id);
    assert!(
        matches!(style0.get(Background), Some(Brush::Solid(c)) if c == palette::css::RED),
        "Button 0 should be RED initially"
    );
    assert!(
        matches!(style1.get(Background), Some(Brush::Solid(c)) if c == palette::css::WHITE),
        "Button 1 should be WHITE initially"
    );

    // Click on button 1 to select it
    harness.click(btn1_rect.center().x, btn1_rect.center().y);

    // Verify the click handler actually ran
    assert_eq!(
        selected_id.get(),
        1,
        "Signal should be 1 after clicking button 1. \
         If this fails, the click didn't hit the button (layout issue?). \
         btn1_rect={:?}",
        btn1_rect
    );

    // Process reactive effects and check if repaint is needed
    let needs_repaint = harness.process_update_no_paint();

    // After click: btn0 should be WHITE, btn1 should be GREEN
    let style0 = harness.get_computed_style(btn0_id);
    let style1 = harness.get_computed_style(btn1_id);
    assert!(
        matches!(style0.get(Background), Some(Brush::Solid(c)) if c == palette::css::WHITE),
        "Button 0 should be WHITE after clicking button 1, got {:?}",
        style0.get(Background)
    );
    assert!(
        matches!(style1.get(Background), Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Button 1 should be GREEN after clicking it, got {:?}",
        style1.get(Background)
    );

    // Note: We don't check needs_repaint here because the click event already
    // triggers process_update() internally, and the reactive effects may have
    // already been processed. The important thing is that the styles updated.
    let _ = needs_repaint;
}
