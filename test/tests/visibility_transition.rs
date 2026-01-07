//! Integration tests for visibility phase transitions.
//!
//! These tests verify the CSS-driven visibility transition system through public APIs:
//! - display:none in styles triggers is_hidden()
//! - set_hidden()/set_visible() API works correctly
//! - Animation integration with visibility changes (run_on_remove)

use std::time::Duration;

use floem::prelude::*;
use floem::taffy::Display;
use floem::view::{Visibility, VisibilityPhase};
use floem_test::prelude::*;

// =============================================================================
// Visibility struct tests (testing public fields and methods)
// =============================================================================

/// Test visibility is_hidden() considers both force_hidden and phase.
#[test]
fn test_is_hidden_checks_both_conditions() {
    // force_hidden = false, phase = Visible -> not hidden
    let vis = Visibility {
        phase: VisibilityPhase::Visible(Display::Flex),
        force_hidden: false,
    };
    assert!(!vis.is_hidden());

    // force_hidden = true, phase = Visible -> hidden
    let vis = Visibility {
        phase: VisibilityPhase::Visible(Display::Flex),
        force_hidden: true,
    };
    assert!(vis.is_hidden());

    // force_hidden = false, phase = Hidden -> hidden
    let vis = Visibility {
        phase: VisibilityPhase::Hidden,
        force_hidden: false,
    };
    assert!(vis.is_hidden());

    // force_hidden = false, phase = Animating -> not hidden (still animating out)
    let vis = Visibility {
        phase: VisibilityPhase::Animating(Display::Flex),
        force_hidden: false,
    };
    assert!(!vis.is_hidden());

    // force_hidden = false, phase = Initial -> not hidden
    let vis = Visibility {
        phase: VisibilityPhase::Initial,
        force_hidden: false,
    };
    assert!(!vis.is_hidden());
}

// =============================================================================
// Integration Tests with Views
// =============================================================================

/// Test that display:none in style triggers is_hidden().
#[test]
fn test_display_none_style_triggers_hidden() {
    let is_display_none = RwSignal::new(false);

    let view = Empty::new().style(move |s| {
        let mut s = s.size(100.0, 100.0);
        if is_display_none.get() {
            s = s.display(Display::None);
        }
        s
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // Initially visible
    assert!(!id.is_hidden(), "View should not be hidden initially");

    // Set display:none
    is_display_none.set(true);
    harness.rebuild();
    // Extra rebuild for phase transition
    harness.rebuild();

    // Should now be hidden
    assert!(id.is_hidden(), "View should be hidden after display:none");

    // Layout should be zero
    let layout = id.get_layout().expect("Should have layout");
    assert!(
        layout.size.width < 0.1,
        "Hidden view should have zero width"
    );
}

/// Test that force_hidden (set_hidden) works independently of style.
#[test]
fn test_force_hidden_independent_of_style() {
    let view = Empty::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // Initially visible
    assert!(!id.is_hidden());
    let layout = id.get_layout().expect("Should have layout");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "Initial width should be 100"
    );

    // Use set_hidden() - should hide immediately
    id.set_hidden();
    harness.rebuild();

    assert!(id.is_hidden(), "View should be hidden after set_hidden()");
    let layout = id.get_layout().expect("Should have layout");
    assert!(
        layout.size.width < 0.1,
        "Hidden view should have zero width"
    );

    // Use set_visible() - should show again
    id.set_visible();
    harness.rebuild();

    assert!(
        !id.is_hidden(),
        "View should not be hidden after set_visible()"
    );
    let layout = id.get_layout().expect("Should have layout");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "Visible view should have width 100"
    );
}

/// Test that a view with run_on_remove animation stays visible during animation.
#[test]
fn test_run_on_remove_animation_delays_hidden() {
    let is_hidden = RwSignal::new(false);

    let view = Empty::new()
        .animation(|a| a.run_on_remove(true).duration(Duration::from_millis(100)))
        .style(move |s| {
            let mut s = s.size(100.0, 100.0);
            if is_hidden.get() {
                s = s.display(Display::None);
            }
            s
        });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // Initially visible
    assert!(!id.is_hidden());

    // Trigger hide with animation
    is_hidden.set(true);
    harness.rebuild();

    // Should NOT be hidden yet (animation in progress)
    // The Animating phase means is_hidden() returns false
    assert!(
        !id.is_hidden(),
        "View should not be hidden during exit animation"
    );

    // Layout should still have size during animation
    let layout = id.get_layout().expect("Should have layout");
    assert!(
        layout.size.width > 0.1,
        "View should have size during exit animation, got {}",
        layout.size.width
    );
}

/// Test switching from display:none back to visible cancels exit animation.
#[test]
fn test_cancel_exit_animation() {
    let is_hidden = RwSignal::new(false);

    let view = Empty::new()
        .animation(|a| a.run_on_remove(true).duration(Duration::from_millis(500)))
        .style(move |s| {
            let mut s = s.size(100.0, 100.0);
            if is_hidden.get() {
                s = s.display(Display::None);
            }
            s
        });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // Start hide animation
    is_hidden.set(true);
    harness.rebuild();

    // Should be in exit animation (not hidden)
    assert!(!id.is_hidden(), "Should be animating, not hidden");

    // Cancel by showing again
    is_hidden.set(false);
    harness.rebuild();

    // Should be visible again
    assert!(!id.is_hidden(), "Should be visible after cancelling");

    // Layout should have proper size
    let layout = id.get_layout().expect("Should have layout");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "After cancel, width should be 100"
    );
}

/// Test that set_hidden() overrides even when display:none is not in style.
#[test]
fn test_set_hidden_overrides_style() {
    let is_display_none = RwSignal::new(false);

    let view = Empty::new().style(move |s| {
        let mut s = s.size(100.0, 100.0);
        if is_display_none.get() {
            s = s.display(Display::None);
        }
        s
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // Style has display, but use set_hidden()
    id.set_hidden();
    harness.rebuild();

    assert!(id.is_hidden(), "set_hidden should hide view");

    // Now remove set_hidden but style still has display
    id.set_visible();
    harness.rebuild();

    assert!(!id.is_hidden(), "set_visible should show view");

    // Now set display:none in style
    is_display_none.set(true);
    harness.rebuild();
    harness.rebuild();

    assert!(
        id.is_hidden(),
        "display:none in style should hide view after set_visible"
    );
}

/// Test combining set_hidden() with display:none style.
#[test]
fn test_set_hidden_with_display_none_style() {
    let is_display_none = RwSignal::new(true); // Start with display:none

    let view = Empty::new().style(move |s| {
        let mut s = s.size(100.0, 100.0);
        if is_display_none.get() {
            s = s.display(Display::None);
        }
        s
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();
    harness.rebuild();

    // Initially hidden via style
    assert!(id.is_hidden(), "Should be hidden via style");

    // Also set_hidden()
    id.set_hidden();
    harness.rebuild();

    assert!(id.is_hidden(), "Should still be hidden");

    // Remove display:none from style, but set_hidden still active
    is_display_none.set(false);
    harness.rebuild();

    assert!(
        id.is_hidden(),
        "set_hidden() should keep view hidden even when style changes"
    );

    // Now set_visible()
    id.set_visible();
    harness.rebuild();

    assert!(!id.is_hidden(), "set_visible() should show view");
}

/// Test transitioning from hidden back to visible shows the view.
#[test]
fn test_hidden_to_visible_transition() {
    let is_display_none = RwSignal::new(true); // Start hidden

    let view = Empty::new().style(move |s| {
        let mut s = s.size(100.0, 100.0);
        if is_display_none.get() {
            s = s.display(Display::None);
        }
        s
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();
    harness.rebuild();

    // Initially hidden
    assert!(id.is_hidden(), "Should start hidden");

    // Show the view
    is_display_none.set(false);
    harness.rebuild();
    // Extra rebuild for layout to update after phase transition
    harness.rebuild();

    // Should now be visible
    assert!(
        !id.is_hidden(),
        "Should be visible after removing display:none"
    );

    // Layout should have proper size
    let layout = id.get_layout().expect("Should have layout");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "Visible view should have width 100, got {}",
        layout.size.width
    );
}
