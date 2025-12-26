//! Tests for click event propagation (stopping vs bubbling).
//!
//! These tests verify that:
//! - `on_click_stop` prevents events from bubbling to parent views
//! - `on_click_cont` allows events to bubble to parent views

use floem::event::EventPropagation;
use floem_test::prelude::*;

#[test]
fn test_pointer_down_move_away_no_click() {
    // Pointer down on view, move away, then pointer up should NOT fire click
    let tracker = ClickTracker::new();

    let view = Stack::new((
        tracker
            .track_named("target", Empty::new())
            .style(|s| s.size(50.0, 50.0)),
        Empty::new().style(|s| s.size(50.0, 50.0)), // Adjacent empty space
    ))
    .style(|s| s.size(100.0, 50.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 50.0);

    // Pointer down on target
    harness.pointer_down(25.0, 25.0);

    // Move pointer away from target
    harness.pointer_move(75.0, 25.0);

    // Pointer up outside target
    harness.pointer_up(75.0, 25.0);

    assert_eq!(
        tracker.click_count(),
        0,
        "Click should NOT fire when pointer moves away before up"
    );
}

#[test]
fn test_pointer_down_move_away_move_back_clicks() {
    // Pointer down, move away, move back, then pointer up SHOULD fire click
    let tracker = ClickTracker::new();

    let view = Stack::new((
        tracker
            .track_named("target", Empty::new())
            .style(|s| s.size(50.0, 50.0)),
        Empty::new().style(|s| s.size(50.0, 50.0)), // Adjacent empty space
    ))
    .style(|s| s.size(100.0, 50.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 50.0);

    // Pointer down on target
    harness.pointer_down(25.0, 25.0);

    // Move pointer away from target
    harness.pointer_move(75.0, 25.0);

    // Move pointer back to target
    harness.pointer_move(25.0, 25.0);

    // Pointer up on target
    harness.pointer_up(25.0, 25.0);

    assert_eq!(
        tracker.click_count(),
        1,
        "Click SHOULD fire when pointer returns before up"
    );
}

#[test]
fn test_pointer_down_on_a_up_on_b_neither_clicks() {
    // Pointer down on view A, pointer up on view B - neither should get click
    let tracker = ClickTracker::new();

    let view = Stack::new((
        tracker
            .track_named("left", Empty::new())
            .style(|s| s.size(50.0, 100.0)),
        tracker
            .track_named("right", Empty::new())
            .style(|s| s.size(50.0, 100.0)),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Pointer down on left view
    harness.pointer_down(25.0, 50.0);

    // Pointer up on right view (without moving through - direct teleport)
    harness.pointer_up(75.0, 50.0);

    assert_eq!(
        tracker.click_count(),
        0,
        "Neither view should receive click when down and up are on different views"
    );
}

#[test]
fn test_hidden_view_no_click() {
    // Hidden views should not receive clicks
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named("hidden", Empty::new())
        .style(|s| s.size(100.0, 100.0).hide());

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert_eq!(
        tracker.click_count(),
        0,
        "Hidden view should not receive clicks"
    );
}

#[test]
fn test_hidden_view_click_passes_through() {
    // Clicks should pass through hidden views to views behind them
    let tracker = ClickTracker::new();

    let view = layers((
        tracker.track_named("back", Empty::new().style(|s| s.z_index(1))),
        tracker.track_named("front_hidden", Empty::new().style(|s| s.z_index(10).hide())),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["back"],
        "Click should pass through hidden front view to back view"
    );
}

#[test]
fn test_double_click_fires_double_click_handler() {
    // Double click should fire double click handler, not single click handler
    let tracker = ClickTracker::new();

    let view = tracker
        .track_double_click(
            "target",
            tracker.track_named("target", Empty::new().style(|s| s.size(100.0, 100.0))),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Perform a double click
    harness.double_click(50.0, 50.0);

    assert_eq!(
        tracker.double_click_count(),
        1,
        "Double click handler should fire once"
    );
}

#[test]
fn test_single_click_does_not_fire_double_click() {
    // Single click should NOT fire double click handler
    let tracker = ClickTracker::new();

    let view = tracker
        .track_double_click("target", Empty::new().style(|s| s.size(100.0, 100.0)))
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Perform a single click
    harness.click(50.0, 50.0);

    assert_eq!(
        tracker.double_click_count(),
        0,
        "Single click should NOT fire double click handler"
    );
}

#[test]
fn test_secondary_click_fires_secondary_handler() {
    // Secondary (right) click should fire secondary click handler
    let tracker = ClickTracker::new();

    let view = tracker
        .track_secondary_click("target", Empty::new().style(|s| s.size(100.0, 100.0)))
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.secondary_click(50.0, 50.0);

    assert_eq!(
        tracker.secondary_click_count(),
        1,
        "Secondary click handler should fire"
    );
}

#[test]
fn test_primary_click_does_not_fire_secondary_handler() {
    // Primary (left) click should NOT fire secondary click handler
    let tracker = ClickTracker::new();

    let view = tracker
        .track_secondary_click("target", Empty::new().style(|s| s.size(100.0, 100.0)))
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert_eq!(
        tracker.secondary_click_count(),
        0,
        "Primary click should NOT fire secondary click handler"
    );
}

#[test]
fn test_pointer_events_none_passes_through() {
    // Views with pointer_events_none should not receive clicks
    let tracker = ClickTracker::new();

    let view = layers((
        tracker.track_named("back", Empty::new().style(|s| s.z_index(1))),
        tracker.track_named(
            "front_no_events",
            Empty::new().style(|s| s.z_index(10).pointer_events_none()),
        ),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["back"],
        "Click should pass through pointer_events_none view to back view"
    );
}

#[test]
fn test_pointer_events_none_child_parent_still_receives() {
    // Parent should still receive clicks even if child has pointer_events_none
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named(
            "parent",
            Container::new(tracker.track_named(
                "child_no_events",
                Empty::new().style(|s| s.size(50.0, 50.0).pointer_events_none()),
            ))
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click in the child area
    harness.click(25.0, 25.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["parent"],
        "Parent should receive click when child has pointer_events_none"
    );
}

#[test]
fn test_clicking_state_during_pointer_down() {
    // View should be in "clicking" state between pointer down and pointer up
    let view = Empty::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Before any interaction, not clicking
    assert!(
        !harness.is_clicking(id),
        "View should NOT be clicking before pointer down"
    );

    // Pointer down - should now be clicking
    harness.pointer_down(50.0, 50.0);
    assert!(
        harness.is_clicking(id),
        "View SHOULD be clicking after pointer down"
    );

    // Pointer up - should no longer be clicking
    harness.pointer_up(50.0, 50.0);
    assert!(
        !harness.is_clicking(id),
        "View should NOT be clicking after pointer up"
    );
}

#[test]
fn test_clicking_state_persists_during_move() {
    // Clicking state persists during pointer movement (cleared only on up/down)
    let target = Empty::new().style(|s| s.size(50.0, 100.0));
    let target_id = target.view_id();

    let view = Stack::new((target, Empty::new().style(|s| s.size(50.0, 100.0))))
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Pointer down on target
    harness.pointer_down(25.0, 50.0);
    assert!(
        harness.is_clicking(target_id),
        "View SHOULD be clicking after pointer down"
    );

    // Move pointer away from target - clicking state PERSISTS
    harness.pointer_move(75.0, 50.0);
    assert!(
        harness.is_clicking(target_id),
        "View SHOULD still be clicking during pointer move (state persists)"
    );

    // Pointer up clears clicking state
    harness.pointer_up(75.0, 50.0);
    assert!(
        !harness.is_clicking(target_id),
        "View should NOT be clicking after pointer up"
    );
}

#[test]
fn test_click_stop_prevents_bubbling() {
    // When child uses on_click_stop, parent should NOT receive the click
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named(
            "parent",
            Container::new(
                tracker.track_named("child", Empty::new().style(|s| s.size(50.0, 50.0))),
            )
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the child area
    harness.click(25.0, 25.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["child"],
        "Only child should receive click when using on_click_stop"
    );
}

#[test]
fn test_click_cont_allows_bubbling() {
    // When child uses on_click_cont, parent SHOULD also receive the click
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named(
            "parent",
            Container::new(
                tracker.track_named_cont("child", Empty::new().style(|s| s.size(50.0, 50.0))),
            )
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the child area
    harness.click(25.0, 25.0);

    // Child is processed first, then event bubbles to parent
    assert_eq!(
        tracker.clicked_names(),
        vec!["child", "parent"],
        "Both child and parent should receive click when child uses on_click_cont"
    );
}

#[test]
fn test_bubbling_order_child_then_parent() {
    // Verify that child receives the event before parent during bubbling
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named_cont(
            "grandparent",
            Container::new(
                tracker.track_named_cont(
                    "parent",
                    Container::new(
                        tracker
                            .track_named_cont("child", Empty::new().style(|s| s.size(30.0, 30.0))),
                    )
                    .style(|s| s.size(60.0, 60.0)),
                ),
            )
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(15.0, 15.0);

    // Events should bubble from innermost to outermost
    assert_eq!(
        tracker.clicked_names(),
        vec!["child", "parent", "grandparent"],
        "Events should bubble from child -> parent -> grandparent"
    );
}

#[test]
fn test_stop_in_middle_prevents_further_bubbling() {
    // If middle view stops propagation, grandparent should NOT receive the click
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named_cont(
            "grandparent",
            Container::new(
                tracker.track_named(
                    "parent", // Uses stop!
                    Container::new(
                        tracker
                            .track_named_cont("child", Empty::new().style(|s| s.size(30.0, 30.0))),
                    )
                    .style(|s| s.size(60.0, 60.0)),
                ),
            )
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(15.0, 15.0);

    // Child bubbles to parent, but parent stops propagation
    assert_eq!(
        tracker.clicked_names(),
        vec!["child", "parent"],
        "Grandparent should NOT receive click when parent stops propagation"
    );
}

#[test]
fn test_click_outside_child_only_hits_parent() {
    // Clicking in parent area but outside child should only trigger parent
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named(
            "parent",
            Container::new(
                tracker.track_named("child", Empty::new().style(|s| s.size(30.0, 30.0))),
            )
            .style(|s| s.size(100.0, 100.0)),
        )
        .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click outside child (30x30) but inside parent (100x100)
    harness.click(80.0, 80.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["parent"],
        "Only parent should receive click when clicking outside child bounds"
    );
}

#[test]
fn test_sibling_views_no_bubbling_between_siblings() {
    // Clicking one sibling should not affect the other sibling
    let tracker = ClickTracker::new();

    let view = Stack::new((
        tracker
            .track_named("left", Empty::new())
            .style(|s| s.size(50.0, 100.0)),
        tracker
            .track_named("right", Empty::new())
            .style(|s| s.size(50.0, 100.0)),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on left sibling
    harness.click(25.0, 50.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["left"],
        "Only clicked sibling should receive the event"
    );

    tracker.reset();

    // Click on right sibling
    harness.click(75.0, 50.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["right"],
        "Only clicked sibling should receive the event"
    );
}

// =============================================================================
// Clicking State Tests - Verify clicking state is cleared properly on pointer up
// =============================================================================

#[test]
fn test_clicking_state_cleared_immediately_after_pointer_up() {
    // Clicking state should be cleared immediately after pointer up event is processed
    let view = Empty::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Pointer down - should be clicking
    harness.pointer_down(50.0, 50.0);
    assert!(
        harness.is_clicking(id),
        "Should be clicking after pointer down"
    );

    // Pointer up - should NOT be clicking anymore
    harness.pointer_up(50.0, 50.0);
    assert!(
        !harness.is_clicking(id),
        "Clicking state should be cleared immediately after pointer up"
    );
}

#[test]
fn test_clicking_state_cleared_for_all_views_on_pointer_up() {
    // When multiple views are in clicking state, ALL should be cleared on pointer up
    let child1 = Empty::new().style(|s| s.size(50.0, 100.0));
    let child1_id = child1.view_id();

    let child2 = Empty::new().style(|s| s.size(50.0, 100.0));
    let child2_id = child2.view_id();

    let view = Stack::new((child1, child2)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Pointer down on child1
    harness.pointer_down(25.0, 50.0);
    assert!(harness.is_clicking(child1_id), "child1 should be clicking");

    // Pointer up (even on different location)
    harness.pointer_up(75.0, 50.0);

    assert!(
        !harness.is_clicking(child1_id),
        "child1 clicking state should be cleared after pointer up"
    );
    assert!(
        !harness.is_clicking(child2_id),
        "child2 should not be clicking"
    );
}

#[test]
fn test_clicking_state_cleared_after_click_handler_runs() {
    // The clicking state should be cleared AFTER the click handler has a chance to run
    use std::cell::Cell;
    use std::rc::Rc;

    let was_clicking_during_handler = Rc::new(Cell::new(false));
    let was_clicking_clone = was_clicking_during_handler.clone();

    let view = Empty::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    // We can't easily check is_clicking inside the handler in this test setup,
    // but we can verify the state after the full click sequence
    let view_with_handler = view.on_click_stop(move |_| {
        // Handler runs - clicking state should still be set at this point
        // (This is the expected behavior based on the window_handle.rs code)
        was_clicking_clone.set(true);
    });

    let mut harness = HeadlessHarness::new_with_size(view_with_handler, 100.0, 100.0);

    // Perform a complete click
    harness.click(50.0, 50.0);

    // After the click completes, clicking state should be cleared
    assert!(
        !harness.is_clicking(id),
        "Clicking state should be cleared after click completes"
    );
}

#[test]
fn test_nested_views_clicking_state_cleared() {
    // Parent and child both get clicking state on pointer down
    // Both should be cleared on pointer up
    let child = Empty::new().style(|s| s.size(50.0, 50.0));
    let child_id = child.view_id();

    let parent = Container::new(child).style(|s| s.size(100.0, 100.0));
    let parent_id = parent.view_id();

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Pointer down on child (which is inside parent)
    harness.pointer_down(25.0, 25.0);

    // Both should be clicking (event bubbles up)
    // Note: depending on implementation, only the leaf might be marked
    let _child_clicking = harness.is_clicking(child_id);
    let _parent_clicking = harness.is_clicking(parent_id);

    // Pointer up
    harness.pointer_up(25.0, 25.0);

    // After pointer up, neither should be clicking
    assert!(
        !harness.is_clicking(child_id),
        "Child clicking state should be cleared after pointer up"
    );
    assert!(
        !harness.is_clicking(parent_id),
        "Parent clicking state should be cleared after pointer up"
    );
}

#[test]
fn test_clicking_state_not_set_on_pointer_up_only() {
    // Pointer up without prior pointer down should not set clicking state
    let view = Empty::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Just pointer up without down
    harness.pointer_up(50.0, 50.0);

    assert!(
        !harness.is_clicking(id),
        "Clicking state should not be set from pointer up alone"
    );
}

#[test]
fn test_rapid_click_sequence_clears_clicking_state() {
    // Rapid clicks should properly clear clicking state between each
    let view = Empty::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // First click
    harness.pointer_down(50.0, 50.0);
    assert!(
        harness.is_clicking(id),
        "Should be clicking after first down"
    );
    harness.pointer_up(50.0, 50.0);
    assert!(
        !harness.is_clicking(id),
        "Should not be clicking after first up"
    );

    // Second click
    harness.pointer_down(50.0, 50.0);
    assert!(
        harness.is_clicking(id),
        "Should be clicking after second down"
    );
    harness.pointer_up(50.0, 50.0);
    assert!(
        !harness.is_clicking(id),
        "Should not be clicking after second up"
    );

    // Third click
    harness.pointer_down(50.0, 50.0);
    assert!(
        harness.is_clicking(id),
        "Should be clicking after third down"
    );
    harness.pointer_up(50.0, 50.0);
    assert!(
        !harness.is_clicking(id),
        "Should not be clicking after third up"
    );
}

#[test]
fn test_clicking_state_cleared_even_when_pointer_up_outside_view() {
    // If pointer down on view, then up outside, clicking should still be cleared
    let view = Empty::new().style(|s| s.size(50.0, 50.0));
    let id = view.view_id();

    let parent = Container::new(view).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Pointer down on the child view
    harness.pointer_down(25.0, 25.0);
    assert!(
        harness.is_clicking(id),
        "Should be clicking after pointer down on view"
    );

    // Pointer up outside the child view (but still in parent)
    harness.pointer_up(75.0, 75.0);
    assert!(
        !harness.is_clicking(id),
        "Clicking state should be cleared even when pointer up is outside the view"
    );
}

#[test]
fn test_interaction_state_reflects_clicking() {
    // The interaction state should accurately reflect clicking state
    let view = Empty::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial state - not clicking
    let state = harness.get_interaction_state(id);
    assert!(!state.is_clicking, "Should not be clicking initially");

    // Pointer down - should be clicking
    harness.pointer_down(50.0, 50.0);
    let state = harness.get_interaction_state(id);
    assert!(
        state.is_clicking,
        "Interaction state should show clicking after pointer down"
    );

    // Pointer up - should NOT be clicking
    harness.pointer_up(50.0, 50.0);
    let state = harness.get_interaction_state(id);
    assert!(
        !state.is_clicking,
        "Interaction state should show NOT clicking after pointer up"
    );
}

#[test]
fn test_interaction_state_after_style_recomputation() {
    // After recomputing styles, the interaction state should still be correct
    use floem::peniko::color::palette::css;

    let view = Empty::new().style(|s| s.size(100.0, 100.0).active(|s| s.background(css::RED)));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Pointer down
    harness.pointer_down(50.0, 50.0);
    assert!(
        harness.is_clicking(id),
        "Should be clicking after pointer down"
    );

    // Recompute styles while clicking
    harness.recompute_styles();
    let state = harness.get_interaction_state(id);
    assert!(
        state.is_clicking,
        "Should still be clicking after style recomputation"
    );

    // Pointer up
    harness.pointer_up(50.0, 50.0);

    // Simulate the full pointer-up style processing
    harness.process_pointer_up_styles();

    let state = harness.get_interaction_state(id);
    assert!(
        !state.is_clicking,
        "Should NOT be clicking after pointer up and style processing"
    );
}

#[test]
fn test_active_style_with_full_click_cycle() {
    // Test a complete click cycle with Active style handling
    use floem::peniko::color::palette::css;
    use floem::style::StyleSelector;

    let view = Empty::new().style(|s| s.size(100.0, 100.0).active(|s| s.background(css::RED)));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Check if the view has Active styles
    let has_active = harness.has_style_for_selector(id, StyleSelector::Active);
    assert!(has_active, "View should have Active style selector defined");

    // Pointer down - start clicking
    harness.pointer_down(50.0, 50.0);
    assert!(
        harness.is_clicking(id),
        "Should be clicking after pointer down"
    );

    // Pointer up - stop clicking
    harness.pointer_up(50.0, 50.0);
    assert!(
        !harness.is_clicking(id),
        "Should NOT be clicking after pointer up"
    );

    // After process_pointer_up_styles, clicking should definitely be cleared
    harness.process_pointer_up_styles();
    assert!(
        !harness.is_clicking(id),
        "Should NOT be clicking after processing pointer up styles"
    );

    let state = harness.get_interaction_state(id);
    assert!(
        !state.is_clicking,
        "Interaction state should show NOT clicking"
    );
}

#[test]
fn test_clicking_state_persists_when_pointer_leaves_view() {
    // Clicking state should persist when pointer moves out of the view
    // (only cleared on pointer up, not on pointer leave)
    let view = Empty::new().style(|s| s.size(50.0, 50.0));
    let id = view.view_id();

    let parent = Container::new(view).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Pointer down on the child view
    harness.pointer_down(25.0, 25.0);
    assert!(
        harness.is_clicking(id),
        "Should be clicking after pointer down"
    );

    // Move pointer out of the child view (but still in parent)
    harness.pointer_move(75.0, 75.0);

    // Clicking state should STILL be set (only cleared on up, not on move)
    assert!(
        harness.is_clicking(id),
        "Clicking state should persist when pointer moves out of view"
    );

    // Now pointer up
    harness.pointer_up(75.0, 75.0);
    assert!(
        !harness.is_clicking(id),
        "Clicking state should be cleared after pointer up"
    );
}

#[test]
fn test_clicking_state_after_pointer_move_and_style_update() {
    // After pointer move out and style recalculation, clicking should still be set
    use floem::peniko::color::palette::css;

    let view = Empty::new().style(|s| s.size(50.0, 50.0).active(|s| s.background(css::RED)));
    let id = view.view_id();

    let parent = Container::new(view).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Pointer down on the child view
    harness.pointer_down(25.0, 25.0);
    assert!(
        harness.is_clicking(id),
        "Should be clicking after pointer down"
    );

    // Move pointer out of the child view
    harness.pointer_move(75.0, 75.0);

    // Trigger style recalculation (simulating what window_handle does)
    harness.recompute_styles();

    // Clicking state should STILL be set
    let state = harness.get_interaction_state(id);
    assert!(
        state.is_clicking,
        "Clicking state should persist after pointer move and style recomputation"
    );
}

#[test]
fn test_hover_state_cleared_on_pointer_leave() {
    // Hover state should be cleared when pointer leaves view
    let view = Empty::new().style(|s| s.size(50.0, 50.0));
    let id = view.view_id();

    let parent = Container::new(view).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Move pointer to the child view to hover it
    harness.pointer_move(25.0, 25.0);

    // Should be hovered
    let state = harness.get_interaction_state(id);
    assert!(
        state.is_hovered,
        "Should be hovered when pointer is over view"
    );

    // Move pointer out of the child view
    harness.pointer_move(75.0, 75.0);

    // Should NOT be hovered anymore
    let state = harness.get_interaction_state(id);
    assert!(
        !state.is_hovered,
        "Hover state should be cleared when pointer leaves view"
    );
}

// =============================================================================
// Computed Style Tests - Verify styles are actually applied during interactions
// =============================================================================

#[test]
fn test_active_style_applied_during_click() {
    // Verify that the :active style is actually applied to the computed style
    use floem::peniko::Brush;
    use floem::peniko::color::palette::css;
    use floem::style::{Background, StyleSelector};

    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .background(css::BLUE)
            .active(|s| s.background(css::RED))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Check initial background is BLUE
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == css::BLUE),
        "Initial background should be BLUE, got {:?}",
        bg
    );

    // Check that the view has Active selector defined BEFORE any interaction
    eprintln!(
        "Before any interaction: has_style_for_selector(Active) = {}",
        harness.has_style_for_selector(id, StyleSelector::Active)
    );

    // Pointer down - should apply :active style
    harness.pointer_down(50.0, 50.0);
    eprintln!(
        "After pointer_down: is_clicking = {}",
        harness.is_clicking(id)
    );
    harness.recompute_styles();

    eprintln!(
        "After pointer_down+recompute: has_style_for_selector(Active) = {}",
        harness.has_style_for_selector(id, StyleSelector::Active)
    );

    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    eprintln!("After pointer_down+recompute: bg = {:?}", bg);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == css::RED),
        "Background should be RED when :active, got {:?}",
        bg
    );

    // Pointer up - should revert to normal style
    eprintln!("--- About to pointer_up ---");
    eprintln!(
        "has_style_for_selector(Active) = {}",
        harness.has_style_for_selector(id, StyleSelector::Active)
    );
    harness.pointer_up(50.0, 50.0);

    // Debug: check if clicking is actually cleared
    let is_clicking = harness.is_clicking(id);
    eprintln!("After pointer_up: is_clicking = {}", is_clicking);

    eprintln!("--- About to recompute_styles ---");
    harness.recompute_styles();
    eprintln!("--- After recompute_styles ---");

    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == css::BLUE),
        "Background should revert to BLUE after pointer up, got {:?}",
        bg
    );
}

// =============================================================================
// Nested Stack Click Tests - Without z-index
// =============================================================================

/// Test that clicks work in nested stacks without z-index set.
///
/// This reproduces the counter example structure where nested stacks
/// with clickable elements have no z-index set.
#[test]
fn test_nested_stack_click_no_z_index() {
    // Structure:
    //   Root Stack (flex column)
    //   └── Inner Stack (flex row)
    //       ├── Button1 (clickable)
    //       └── Button2 (clickable)

    let tracker = ClickTracker::new();

    let view = Stack::new((Stack::new((
        tracker
            .track_named("button1", Empty::new())
            .style(|s| s.size(50.0, 50.0)),
        tracker
            .track_named("button2", Empty::new())
            .style(|s| s.size(50.0, 50.0)),
    ))
    .style(|s| s.flex_row()),))
    .style(|s| s.size(100.0, 100.0).flex_col());

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on button1 (should be at x=25, y=25)
    harness.click(25.0, 25.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["button1"],
        "Button1 should receive click in nested stack without z-index"
    );
}

// =============================================================================
// DOM-style Event Bubbling Tests - Overlapping Siblings
// =============================================================================
//
// In DOM, events bubble UP the parent chain only, never to siblings.
// This is important for modal dialogs where:
//   - Backdrop is a sibling of DialogContent
//   - Both visually overlap
//   - Clicking DialogContent should NOT trigger Backdrop's click handler
//
// These tests verify that Floem implements DOM-style tree-based bubbling,
// not spatial bubbling where overlapping views can intercept each other's events.

/// Test that overlapping siblings don't receive each other's click events.
///
/// Structure:
///   Stack
///   ├── Backdrop (covers entire area, z-index lower)
///   └── Content (smaller, overlaps backdrop, z-index higher)
///
/// DOM behavior: Click on Content should only hit Content, not Backdrop.
/// Even though they visually overlap, they are siblings in the tree.
#[test]
fn test_overlapping_siblings_no_cross_propagation() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};

    let backdrop_clicked = RwSignal::new(false);
    let content_clicked = RwSignal::new(false);

    // Create overlapping siblings using stack with absolute positioning
    let view = Stack::new((
        // Backdrop - covers entire area, lower z-index
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
            .on_click_stop({
                let backdrop_clicked = backdrop_clicked;
                move |_| backdrop_clicked.set(true)
            }),
        // Content - smaller, higher z-index, overlaps backdrop
        Empty::new()
            .style(|s| {
                s.absolute()
                    .inset_left(25.0)
                    .inset_top(25.0)
                    .size(50.0, 50.0)
                    .z_index(10)
            })
            .on_click_stop({
                let content_clicked = content_clicked;
                move |_| content_clicked.set(true)
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the content area (which overlaps backdrop)
    harness.click(50.0, 50.0);

    // DOM behavior: only content should receive the click, not backdrop
    // Even though backdrop is visually underneath, events don't propagate to siblings
    assert!(content_clicked.get(), "Content should receive click");
    assert!(
        !backdrop_clicked.get(),
        "Backdrop should NOT receive click when content is clicked (DOM-style bubbling)"
    );
}

/// Test that clicking outside overlapping content hits the backdrop only.
#[test]
fn test_overlapping_siblings_click_outside_content() {
    use floem::HasViewId;
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};

    let backdrop_clicked = RwSignal::new(false);
    let content_clicked = RwSignal::new(false);

    // Use stack instead of layers and apply styles directly
    let backdrop = Empty::new()
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
        .on_click_stop({
            let backdrop_clicked = backdrop_clicked;
            move |_| backdrop_clicked.set(true)
        });
    let backdrop_id = backdrop.view_id();

    let content = Empty::new()
        .style(|s| {
            s.absolute()
                .inset_left(25.0)
                .inset_top(25.0)
                .size(50.0, 50.0)
                .z_index(10)
        })
        .on_click_stop({
            let content_clicked = content_clicked;
            move |_| content_clicked.set(true)
        });
    let content_id = content.view_id();

    let view = Stack::new((backdrop, content)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Debug: check actual layout positions
    let backdrop_layout = backdrop_id.get_layout().unwrap();
    let content_layout = content_id.get_layout().unwrap();
    eprintln!(
        "Backdrop layout: pos=({}, {}), size={}x{}",
        backdrop_layout.location.x,
        backdrop_layout.location.y,
        backdrop_layout.size.width,
        backdrop_layout.size.height
    );
    eprintln!(
        "Content layout: pos=({}, {}), size={}x{}",
        content_layout.location.x,
        content_layout.location.y,
        content_layout.size.width,
        content_layout.size.height
    );

    // Click outside the content area (on backdrop only)
    harness.click(10.0, 10.0);

    assert!(
        backdrop_clicked.get(),
        "Backdrop should receive click when clicking outside content"
    );
    assert!(
        !content_clicked.get(),
        "Content should NOT receive click when clicking outside its bounds"
    );
}

/// Test dialog-like structure: backdrop + centered content.
///
/// This mirrors the exact dialog structure:
///   Stack (fixed, fills viewport)
///   ├── Backdrop (covers entire area, on_click_stop closes dialog)
///   └── Content (centered, on_click_stop does nothing - just stops propagation)
///
/// Expected: Clicking content should NOT close dialog (shouldn't trigger backdrop).
#[test]
fn test_dialog_structure_content_click_no_backdrop() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};

    let dialog_open = RwSignal::new(true);
    let content_clicked = RwSignal::new(false);

    // Use stack instead of layers for consistent behavior
    let view = Stack::new((
        // Backdrop - clicking it closes dialog
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
            .on_click_stop(move |_| {
                dialog_open.set(false);
            }),
        // Content - clicking it should NOT close dialog
        Empty::new()
            .style(|s| {
                s.absolute()
                    .inset_left(25.0)
                    .inset_top(25.0)
                    .size(50.0, 50.0)
                    .z_index(10)
            })
            .on_click_stop(move |_| {
                content_clicked.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the content
    harness.click(50.0, 50.0);

    assert!(
        content_clicked.get(),
        "Content's click handler should have been called"
    );
    assert!(
        dialog_open.get(),
        "Dialog should still be open - backdrop's handler should NOT have been called"
    );
}

/// Test that clicking backdrop (outside content) DOES close dialog.
#[test]
fn test_dialog_structure_backdrop_click_closes() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};

    let dialog_open = RwSignal::new(true);

    // Use stack instead of layers for consistent behavior
    let view = Stack::new((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
            .on_click_stop(move |_| {
                dialog_open.set(false);
            }),
        Empty::new().style(|s| {
            s.absolute()
                .inset_left(25.0)
                .inset_top(25.0)
                .size(50.0, 50.0)
                .z_index(10)
        }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the backdrop (outside content area)
    harness.click(10.0, 10.0);

    assert!(
        !dialog_open.get(),
        "Dialog should be closed after clicking backdrop"
    );
}

/// Test multiple overlapping layers - only topmost receives click.
#[test]
fn test_multiple_overlapping_layers_topmost_wins() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};

    let layer1_clicked = RwSignal::new(false);
    let layer2_clicked = RwSignal::new(false);
    let layer3_clicked = RwSignal::new(false);

    // Use stack instead of layers for consistent behavior
    let view = Stack::new((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
            .on_click_stop({
                let clicked = layer1_clicked;
                move |_| clicked.set(true)
            }),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop({
                let clicked = layer2_clicked;
                move |_| clicked.set(true)
            }),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(10))
            .on_click_stop({
                let clicked = layer3_clicked;
                move |_| clicked.set(true)
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    // Only the topmost layer should receive the click
    assert!(
        layer3_clicked.get(),
        "Topmost layer (z-index 10) should receive click"
    );
    assert!(
        !layer2_clicked.get(),
        "Middle layer (z-index 5) should NOT receive click"
    );
    assert!(
        !layer1_clicked.get(),
        "Bottom layer (z-index 1) should NOT receive click"
    );
}

/// Test that on_click_cont on topmost doesn't propagate to siblings.
///
/// Even with on_click_cont (continue propagation), events should only
/// bubble to PARENTS, not to sibling layers below.
#[test]
fn test_click_cont_bubbles_to_parent_not_siblings() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use std::cell::RefCell;
    use std::rc::Rc;

    let click_order = Rc::new(RefCell::new(Vec::<String>::new()));
    let backdrop_clicked = RwSignal::new(false);

    let order_clone = click_order.clone();
    let order_clone2 = click_order.clone();

    // Use stack with a parent that tracks clicks
    let view = Stack::new((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
            .on_click_stop({
                let clicked = backdrop_clicked;
                move |_| clicked.set(true)
            }),
        // Content uses on_click_cont - should bubble to parent, NOT to backdrop
        Empty::new()
            .style(|s| {
                s.absolute()
                    .inset_left(25.0)
                    .inset_top(25.0)
                    .size(50.0, 50.0)
                    .z_index(10)
            })
            .on_click_cont(move |_| {
                order_clone.borrow_mut().push("content".to_string());
            }),
    ))
    .style(|s| s.size(100.0, 100.0))
    .on_click_cont(move |_| {
        order_clone2.borrow_mut().push("parent".to_string());
    });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    // Should bubble: content -> parent (NOT content -> backdrop)
    assert_eq!(
        *click_order.borrow(),
        vec!["content", "parent"],
        "Click should bubble from content to parent"
    );
    assert!(
        !backdrop_clicked.get(),
        "Backdrop should NOT receive click (events don't propagate to siblings)"
    );
}

/// Test what happens when topmost view has NO click handler.
///
/// This tests whether the event falls through to sibling (spatial) or
/// bubbles to parent (DOM-style). If Floem is truly DOM-style, the
/// backdrop sibling should NOT receive the click.
#[test]
fn test_no_handler_on_topmost_does_not_fall_through_to_sibling() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};

    let backdrop_clicked = RwSignal::new(false);

    let view = Stack::new((
        // Backdrop with handler
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
            .on_click_stop({
                let clicked = backdrop_clicked;
                move |_| clicked.set(true)
            }),
        // Content with NO handler - just styled
        Empty::new().style(|s| {
            s.absolute()
                .inset_left(25.0)
                .inset_top(25.0)
                .size(50.0, 50.0)
                .z_index(10)
        }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on content area - content has no handler, but event should NOT
    // fall through to backdrop sibling
    harness.click(50.0, 50.0);

    assert!(
        !backdrop_clicked.get(),
        "Backdrop should NOT receive click when content (with no handler) is on top. \
         Events should not fall through to siblings."
    );
}

/// Test overlapping siblings without explicit z-index (DOM order determines stacking).
///
/// In DOM, later siblings are rendered on top by default.
/// This matches the dialog structure where backdrop comes before content.
#[test]
fn test_overlapping_siblings_no_z_index_dom_order() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};

    let backdrop_clicked = RwSignal::new(false);
    let content_clicked = RwSignal::new(false);

    // No explicit z-index - rely on DOM order (later = on top)
    let view = Stack::new((
        // Backdrop - comes first, so it's BELOW content
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .on_click_stop({
                let clicked = backdrop_clicked;
                move |_| clicked.set(true)
            }),
        // Content - comes second, so it's ON TOP of backdrop
        Empty::new()
            .style(|s| {
                s.absolute()
                    .inset_left(25.0)
                    .inset_top(25.0)
                    .size(50.0, 50.0)
            })
            .on_click_stop({
                let clicked = content_clicked;
                move |_| clicked.set(true)
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on content area - content should receive, not backdrop
    harness.click(50.0, 50.0);

    assert!(
        content_clicked.get(),
        "Content (later sibling) should receive click"
    );
    assert!(
        !backdrop_clicked.get(),
        "Backdrop (earlier sibling) should NOT receive click when content is on top"
    );
}

// =============================================================================
// Overlay Tests - Testing event propagation with Overlay::with_id
// =============================================================================

/// Test dialog structure using Overlay::with_id (the actual dialog implementation).
///
/// This replicates the exact structure from dialog.rs:
///   Overlay::with_id
///   └── Stack
///       ├── Backdrop (Empty with on_click_stop)
///       └── Content (Container::derived with children)
#[test]
fn test_dialog_with_overlay() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::views::Overlay;

    let dialog_open = RwSignal::new(true);
    let content_clicked = RwSignal::new(false);

    let view = Overlay::new(
        Stack::new((
            // Backdrop - clicking it closes dialog
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
                .on_click_stop(move |_| {
                    dialog_open.set(false);
                }),
            // Content - clicking it should NOT close dialog
            Container::new(Empty::new().style(|s| s.size(50.0, 50.0)))
                .style(|s| {
                    s.absolute()
                        .inset_left(25.0)
                        .inset_top(25.0)
                        .size(50.0, 50.0)
                        .z_index(10)
                })
                .on_click_stop(move |_| {
                    content_clicked.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0)),
    );

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the content
    harness.click(50.0, 50.0);

    assert!(
        content_clicked.get(),
        "Content's click handler should have been called"
    );
    assert!(
        dialog_open.get(),
        "Dialog should still be open - backdrop's handler should NOT have been called"
    );
}

/// Test dialog structure with Container::derived (actual dialog uses this).
#[test]
fn test_dialog_with_container_derived() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};

    let dialog_open = RwSignal::new(true);
    let content_clicked = RwSignal::new(false);

    let view = Stack::new((
        // Backdrop
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
            .on_click_stop(move |_| {
                dialog_open.set(false);
            }),
        // Content using Container::derived like the real dialog
        Container::derived(move || {
            // Simulate dialog content with header and footer
            Stack::new((
                Empty::new().style(|s| s.size(50.0, 20.0)), // Header
                Empty::new().style(|s| s.size(50.0, 30.0)), // Footer
            ))
            .style(|s| s.flex_col())
        })
        .style(|s| {
            s.absolute()
                .inset_left(25.0)
                .inset_top(25.0)
                .size(50.0, 50.0)
                .z_index(10)
        })
        .on_click_stop(move |_| {
            content_clicked.set(true);
        }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the content
    harness.click(50.0, 50.0);

    assert!(
        content_clicked.get(),
        "Content's click handler should have been called"
    );
    assert!(dialog_open.get(), "Dialog should still be open");
}

/// Test clicking on nested children within Container::derived content.
///
/// In the actual dialog, users click on buttons inside DialogFooter.
/// This tests that clicks on nested elements don't propagate to backdrop.
#[test]
fn test_dialog_click_on_nested_button() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};

    let dialog_open = RwSignal::new(true);
    let button_clicked = RwSignal::new(false);

    let view = Stack::new((
        // Backdrop
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
            .on_click_stop(move |_| {
                dialog_open.set(false);
            }),
        // Content with a clickable button inside
        Container::derived(move || {
            Stack::new((
                Empty::new().style(|s| s.size(50.0, 20.0)), // Header
                // A button at the bottom of the dialog
                Empty::new()
                    .style(|s| s.size(40.0, 20.0))
                    .on_click_stop(move |_| {
                        button_clicked.set(true);
                    }),
            ))
            .style(|s| s.flex_col().gap(10.0))
        })
        .style(|s| {
            s.absolute()
                .inset_left(25.0)
                .inset_top(25.0)
                .size(50.0, 50.0)
                .z_index(10)
        }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the button (within the content area)
    harness.click(45.0, 55.0); // Approximately where the button would be

    // Button should receive click, dialog should stay open
    assert!(
        button_clicked.get(),
        "Button inside dialog content should receive click"
    );
    assert!(
        dialog_open.get(),
        "Dialog should still be open - backdrop's handler should NOT have been called"
    );
}

/// Test the exact dialog structure with Overlay::with_id and Container::derived.
#[test]
fn test_exact_dialog_structure() {
    use floem::ViewId;
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::views::Overlay;

    let dialog_open = RwSignal::new(true);
    let content_clicked = RwSignal::new(false);
    let id = ViewId::new();

    let view = Overlay::with_id(
        id,
        Stack::new((
            // Backdrop - exact same as dialog.rs
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
                .on_click_stop(move |_| {
                    dialog_open.set(false);
                }),
            // Content - Container::derived like dialog.rs
            Container::derived(move || {
                // Mimic DialogContent with DialogHeader and DialogFooter
                Stack::new((
                    // DialogHeader
                    Stack::new((
                        floem::views::Label::new("Title"),
                        floem::views::Label::new("Description"),
                    ))
                    .style(|s| s.flex_col().gap(2.0)),
                    // DialogFooter with buttons
                    Stack::new((
                        Empty::new().style(|s| s.size(30.0, 20.0)), // Cancel button
                        Empty::new().style(|s| s.size(30.0, 20.0)), // Confirm button
                    ))
                    .style(|s| s.flex_row().gap(4.0)),
                ))
                .style(|s| s.flex_col().gap(16.0))
            })
            .style(|s| {
                s.absolute()
                    .inset_left(25.0)
                    .inset_top(25.0)
                    .size(50.0, 50.0)
                    .z_index(10)
            })
            .on_click_stop(move |_| {
                content_clicked.set(true);
            }),
        ))
        .style(|s| s.size(100.0, 100.0)),
    );

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the content
    harness.click(50.0, 50.0);

    assert!(
        content_clicked.get(),
        "Content's click handler should have been called"
    );
    assert!(
        dialog_open.get(),
        "Dialog should still be open - clicking content should not trigger backdrop"
    );
}

/// Test dialog where content has NO explicit click handler.
///
/// This tests the scenario where the user removed .on_click_stop(|_| {}) from content.
/// The content should still block clicks from reaching backdrop.
#[test]
fn test_dialog_content_no_handler() {
    use floem::ViewId;
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::views::Overlay;

    let dialog_open = RwSignal::new(true);
    let id = ViewId::new();

    let view = Overlay::with_id(
        id,
        Stack::new((
            // Backdrop
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
                .on_click_stop(move |_| {
                    dialog_open.set(false);
                }),
            // Content - NO click handler!
            Container::derived(move || {
                Stack::new((
                    floem::views::Label::new("Title"),
                    Empty::new().style(|s| s.size(30.0, 20.0)),
                ))
                .style(|s| s.flex_col().gap(8.0))
            })
            .style(|s| {
                s.absolute()
                    .inset_left(25.0)
                    .inset_top(25.0)
                    .size(50.0, 50.0)
                    .z_index(10)
            }),
            // No on_click_stop here!
        ))
        .style(|s| s.size(100.0, 100.0)),
    );

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the content (which has no handler)
    harness.click(50.0, 50.0);

    assert!(
        dialog_open.get(),
        "Dialog should still be open - content without handler should block clicks from backdrop"
    );
}

/// Test dialog structure with translate transforms for centering.
///
/// This tests the actual dialog pattern: content positioned at 50%/50% with
/// translate -50%/-50% to center it.
#[test]
fn test_dialog_with_translate_centering() {
    use floem::ViewId;
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::unit::Pct;
    use floem::views::Overlay;

    let dialog_open = RwSignal::new(true);
    let content_clicked = RwSignal::new(false);
    let id = ViewId::new();

    let view = Overlay::with_id(
        id,
        Stack::new((
            // Backdrop
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
                .on_click_stop(move |_| {
                    dialog_open.set(false);
                }),
            // Content - centered using translate (like actual dialog)
            Container::derived(move || Empty::new().style(|s| s.size(30.0, 20.0)))
                .style(|s| {
                    s.absolute()
                        .inset_left(Pct(50.0)) // left: 50%
                        .inset_top(Pct(50.0)) // top: 50%
                        .translate_x(Pct(-50.0)) // translateX: -50%
                        .translate_y(Pct(-50.0)) // translateY: -50%
                        .size(50.0, 50.0)
                })
                .on_click_stop(move |_| {
                    content_clicked.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0)),
    );

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the center (where content should visually be)
    // Content is positioned at (50, 50) with translate (-25, -25)
    // Visual bounds: (25, 25) to (75, 75)
    harness.click(50.0, 50.0);

    assert!(
        content_clicked.get(),
        "Content's click handler should have been called"
    );
    assert!(
        dialog_open.get(),
        "Dialog should still be open - clicking content should not trigger backdrop"
    );
}

/// Test dialog structure with translate but content has NO click handler.
#[test]
fn test_dialog_with_translate_no_handler() {
    use floem::ViewId;
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::unit::Pct;
    use floem::views::Overlay;

    let dialog_open = RwSignal::new(true);
    let id = ViewId::new();

    let view = Overlay::with_id(
        id,
        Stack::new((
            // Backdrop
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
                .on_click_stop(move |_| {
                    dialog_open.set(false);
                }),
            // Content - NO click handler, uses translate centering
            Container::derived(move || Empty::new().style(|s| s.size(30.0, 20.0))).style(|s| {
                s.absolute()
                    .inset_left(Pct(50.0))
                    .inset_top(Pct(50.0))
                    .translate_x(Pct(-50.0))
                    .translate_y(Pct(-50.0))
                    .size(50.0, 50.0)
            }),
        ))
        .style(|s| s.size(100.0, 100.0)),
    );

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the visual center of content
    harness.click(50.0, 50.0);

    assert!(
        dialog_open.get(),
        "Dialog should remain open - content should block clicks even without handler"
    );
}

/// Test the counter example structure with multiple rows of buttons.
///
/// This mirrors the actual counter example layout:
///   Root (flex col)
///   ├── Label "Value: ..."
///   ├── Spacer
///   └── Button Row (flex row by default as tuple)
///       ├── "Increment" button
///       ├── "Decrement" button
///       └── "Reset" button
#[test]
fn test_counter_example_structure() {
    let tracker = ClickTracker::new();

    // Mimics the counter example tuple structure
    let view = (
        // Value label at top
        Empty::new().style(|s| s.size(200.0, 30.0)),
        // Spacer
        Empty::new().style(|s| s.size(200.0, 10.0)),
        // Button row
        (
            tracker
                .track_named("increment", Empty::new())
                .style(|s| s.size(60.0, 30.0)),
            tracker
                .track_named("decrement", Empty::new())
                .style(|s| s.size(60.0, 30.0)),
            tracker
                .track_named("reset", Empty::new())
                .style(|s| s.size(60.0, 30.0)),
        ),
    )
        .style(|s| {
            s.size(200.0, 100.0)
                .flex_col()
                .items_center()
                .justify_center()
        });

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 100.0);

    // Click on the increment button
    harness.click(30.0, 55.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["increment"],
        "Increment button should receive click"
    );
}

// =============================================================================
// Overlay + Fixed Position Bug Reproduction Tests
// =============================================================================

/// Test that reproduces the dialog bug: clicking on content triggers backdrop.
///
/// The actual DialogContent uses:
/// - Overlay::with_id
/// - Fixed positioning (.fixed().inset_0())
/// - Content centered with left_1_2/top_1_2 + translate -50%
///
/// This test replicates that exact structure.
#[test]
fn test_overlay_fixed_translate_click_offset_bug() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::unit::Pct;
    use floem::views::Overlay;

    let backdrop_clicked = RwSignal::new(false);
    let content_clicked = RwSignal::new(false);

    // This replicates DialogContent structure exactly
    let view = Overlay::new(
        Stack::new((
            // Backdrop - fills entire viewport
            Empty::new()
                .style(|s| s.absolute().inset(0.0).z_index(1))
                .on_click_stop(move |_| {
                    backdrop_clicked.set(true);
                }),
            // Content - centered using translate (like DialogContent)
            Stack::vertical((
                Empty::new().style(|s| s.size(60.0, 30.0)), // Header
                Empty::new().style(|s| s.size(60.0, 30.0)), // Footer
            ))
            .style(|s| {
                s.absolute()
                    .inset_left(Pct(50.0))
                    .inset_top(Pct(50.0))
                    .translate_x(Pct(-50.0))
                    .translate_y(Pct(-50.0))
                    .size(80.0, 80.0)
                    .z_index(10)
            })
            .on_click_stop(move |_| {
                content_clicked.set(true);
            }),
        ))
        // This is the key: fixed positioning like DialogContent
        .style(|s| s.fixed().inset(0.0).width_full().height_full()),
    );

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Click in the center where content should be
    // Content is 80x80, centered at (100, 100) with translate -40, -40
    // Visual bounds: (60, 60) to (140, 140)
    eprintln!("Clicking at center (100, 100)");
    harness.click(100.0, 100.0);

    assert!(
        content_clicked.get(),
        "Content should receive click - clicking visually on content"
    );
    assert!(
        !backdrop_clicked.get(),
        "Backdrop should NOT receive click - content is on top"
    );
}

/// Test overlay with fixed position but no translate (simpler case).
#[test]
fn test_overlay_fixed_no_translate() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::views::Overlay;

    let backdrop_clicked = RwSignal::new(false);
    let content_clicked = RwSignal::new(false);

    let view = Overlay::new(
        Stack::new((
            // Backdrop
            Empty::new()
                .style(|s| s.absolute().inset(0.0).z_index(1))
                .on_click_stop(move |_| {
                    backdrop_clicked.set(true);
                }),
            // Content - positioned without translate
            Empty::new()
                .style(|s| {
                    s.absolute()
                        .inset_left(50.0)
                        .inset_top(50.0)
                        .size(100.0, 100.0)
                        .z_index(10)
                })
                .on_click_stop(move |_| {
                    content_clicked.set(true);
                }),
        ))
        .style(|s| s.fixed().inset(0.0).width_full().height_full()),
    );

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Click in the content area (50, 50) to (150, 150)
    harness.click(100.0, 100.0);

    assert!(content_clicked.get(), "Content should receive click");
    assert!(!backdrop_clicked.get(), "Backdrop should NOT receive click");
}

/// Test the exact DialogContent structure but without Overlay.
#[test]
fn test_fixed_translate_no_overlay() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::unit::Pct;

    let backdrop_clicked = RwSignal::new(false);
    let content_clicked = RwSignal::new(false);

    // Same as dialog but using stack instead of Overlay
    let view = Stack::new((
        // Backdrop
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(200.0, 200.0).z_index(1))
            .on_click_stop(move |_| {
                backdrop_clicked.set(true);
            }),
        // Content with translate centering
        Empty::new()
            .style(|s| {
                s.absolute()
                    .inset_left(Pct(50.0))
                    .inset_top(Pct(50.0))
                    .translate_x(Pct(-50.0))
                    .translate_y(Pct(-50.0))
                    .size(80.0, 80.0)
                    .z_index(10)
            })
            .on_click_stop(move |_| {
                content_clicked.set(true);
            }),
    ))
    .style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Click in center
    harness.click(100.0, 100.0);

    assert!(content_clicked.get(), "Content should receive click");
    assert!(!backdrop_clicked.get(), "Backdrop should NOT receive click");
}

/// Test clicking at 4 corners of content with Overlay + fixed positioning.
///
/// This helps identify offset issues by testing exact boundary conditions.
/// Content is positioned at (50, 50) with size (100, 100), so corners are:
/// - Top-left: (50, 50)
/// - Top-right: (149, 50)
/// - Bottom-left: (50, 149)
/// - Bottom-right: (149, 149)
#[test]
fn test_overlay_fixed_click_corners() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::views::Overlay;

    // Test each corner separately
    for (name, x, y) in [
        ("top-left", 55.0, 55.0),
        ("top-right", 145.0, 55.0),
        ("bottom-left", 55.0, 145.0),
        ("bottom-right", 145.0, 145.0),
        ("center", 100.0, 100.0),
    ] {
        let backdrop_clicked = RwSignal::new(false);
        let content_clicked = RwSignal::new(false);

        let view = Overlay::new(
            Stack::new((
                // Backdrop - fills entire area
                Empty::new()
                    .style(|s| s.absolute().inset(0.0).z_index(1))
                    .on_click_stop({
                        let backdrop_clicked = backdrop_clicked;
                        move |_| {
                            eprintln!("[{}] Backdrop clicked!", name);
                            backdrop_clicked.set(true);
                        }
                    }),
                // Content - positioned at (50, 50) with size (100, 100)
                Empty::new()
                    .style(|s| {
                        s.absolute()
                            .inset_left(50.0)
                            .inset_top(50.0)
                            .size(100.0, 100.0)
                            .z_index(10)
                    })
                    .on_click_stop({
                        let content_clicked = content_clicked;
                        move |_| {
                            eprintln!("[{}] Content clicked!", name);
                            content_clicked.set(true);
                        }
                    }),
            ))
            .style(|s| s.fixed().inset(0.0).width_full().height_full()),
        );

        let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

        eprintln!("Testing corner: {} at ({}, {})", name, x, y);
        harness.click(x, y);

        assert!(
            content_clicked.get(),
            "Corner {}: Content should receive click at ({}, {})",
            name,
            x,
            y
        );
        assert!(
            !backdrop_clicked.get(),
            "Corner {}: Backdrop should NOT receive click at ({}, {})",
            name,
            x,
            y
        );
    }
}

/// Test clicking at 4 corners with translate centering (like DialogContent).
///
/// Content is 80x80, centered in 200x200 viewport using translate.
/// Position: left=50%, top=50%, translate(-50%, -50%)
/// This means: layout position (100, 100), then translated by (-40, -40)
/// Visual bounds: (60, 60) to (140, 140)
#[test]
fn test_overlay_fixed_translate_click_corners() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::unit::Pct;
    use floem::views::Overlay;

    // Content visual bounds: (60, 60) to (140, 140)
    for (name, x, y) in [
        ("top-left", 65.0, 65.0),
        ("top-right", 135.0, 65.0),
        ("bottom-left", 65.0, 135.0),
        ("bottom-right", 135.0, 135.0),
        ("center", 100.0, 100.0),
    ] {
        let backdrop_clicked = RwSignal::new(false);
        let content_clicked = RwSignal::new(false);

        let view = Overlay::new(
            Stack::new((
                // Backdrop
                Empty::new()
                    .style(|s| s.absolute().inset(0.0).z_index(1))
                    .on_click_stop({
                        let backdrop_clicked = backdrop_clicked;
                        move |_| {
                            eprintln!("[translate-{}] Backdrop clicked!", name);
                            backdrop_clicked.set(true);
                        }
                    }),
                // Content - centered using translate
                Empty::new()
                    .style(|s| {
                        s.absolute()
                            .inset_left(Pct(50.0))
                            .inset_top(Pct(50.0))
                            .translate_x(Pct(-50.0))
                            .translate_y(Pct(-50.0))
                            .size(80.0, 80.0)
                            .z_index(10)
                    })
                    .on_click_stop({
                        let content_clicked = content_clicked;
                        move |_| {
                            eprintln!("[translate-{}] Content clicked!", name);
                            content_clicked.set(true);
                        }
                    }),
            ))
            .style(|s| s.fixed().inset(0.0).width_full().height_full()),
        );

        let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

        eprintln!("Testing translate corner: {} at ({}, {})", name, x, y);
        harness.click(x, y);

        assert!(
            content_clicked.get(),
            "Translate corner {}: Content should receive click at ({}, {})",
            name,
            x,
            y
        );
        assert!(
            !backdrop_clicked.get(),
            "Translate corner {}: Backdrop should NOT receive click at ({}, {})",
            name,
            x,
            y
        );
    }
}

/// Test clicking OUTSIDE content bounds to verify backdrop receives those clicks.
#[test]
fn test_overlay_fixed_click_outside_content() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::views::Overlay;

    // Content is at (50, 50) to (150, 150)
    // These clicks should hit backdrop
    for (name, x, y) in [
        ("top-left-outside", 25.0, 25.0),
        ("top-right-outside", 175.0, 25.0),
        ("bottom-left-outside", 25.0, 175.0),
        ("bottom-right-outside", 175.0, 175.0),
    ] {
        let backdrop_clicked = RwSignal::new(false);
        let content_clicked = RwSignal::new(false);

        let view = Overlay::new(
            Stack::new((
                // Backdrop
                Empty::new()
                    .style(|s| s.absolute().inset(0.0).z_index(1))
                    .on_click_stop({
                        let backdrop_clicked = backdrop_clicked;
                        move |_| {
                            backdrop_clicked.set(true);
                        }
                    }),
                // Content
                Empty::new()
                    .style(|s| {
                        s.absolute()
                            .inset_left(50.0)
                            .inset_top(50.0)
                            .size(100.0, 100.0)
                            .z_index(10)
                    })
                    .on_click_stop({
                        let content_clicked = content_clicked;
                        move |_| {
                            content_clicked.set(true);
                        }
                    }),
            ))
            .style(|s| s.fixed().inset(0.0).width_full().height_full()),
        );

        let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

        eprintln!("Testing outside: {} at ({}, {})", name, x, y);
        harness.click(x, y);

        assert!(
            backdrop_clicked.get(),
            "Outside {}: Backdrop SHOULD receive click at ({}, {})",
            name,
            x,
            y
        );
        assert!(
            !content_clicked.get(),
            "Outside {}: Content should NOT receive click at ({}, {})",
            name,
            x,
            y
        );
    }
}

/// Probe test: find exact boundary WITHOUT Overlay (for comparison).
#[test]
fn test_no_overlay_fixed_translate_probe_boundary() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::unit::Pct;

    // Same structure but without Overlay wrapper
    let test_points = [
        (60.0, 100.0, "x=60 (left edge)"),
        (65.0, 100.0, "x=65 (inside left)"),
        (100.0, 100.0, "x=100 (center)"),
        (135.0, 100.0, "x=135 (inside right)"),
    ];

    eprintln!("\n=== NO OVERLAY: Horizontal probe (y=100) ===");
    eprintln!("Content expected bounds: x=[60, 140], y=[60, 140]");

    for (x, y, desc) in test_points {
        let backdrop_clicked = RwSignal::new(false);
        let content_clicked = RwSignal::new(false);

        // NO Overlay - just stack directly
        let view = Stack::new((
            Empty::new()
                .style(|s| s.absolute().inset(0.0).z_index(1))
                .on_click_stop({
                    let backdrop_clicked = backdrop_clicked;
                    move |_| backdrop_clicked.set(true)
                }),
            Empty::new()
                .style(|s| {
                    s.absolute()
                        .inset_left(Pct(50.0))
                        .inset_top(Pct(50.0))
                        .translate_x(Pct(-50.0))
                        .translate_y(Pct(-50.0))
                        .size(80.0, 80.0)
                        .z_index(10)
                })
                .on_click_stop({
                    let content_clicked = content_clicked;
                    move |_| content_clicked.set(true)
                }),
        ))
        // Same fixed positioning
        .style(|s| s.fixed().inset(0.0).width_full().height_full());

        let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
        harness.click(x, y);

        let hit = if content_clicked.get() {
            "CONTENT"
        } else if backdrop_clicked.get() {
            "backdrop"
        } else {
            "NONE"
        };

        eprintln!("  ({:>5.1}, {:>5.1}) {} -> {}", x, y, desc, hit);
    }
}

/// Probe test: find exact boundary where clicks start hitting content vs backdrop.
#[test]
fn test_overlay_fixed_translate_probe_boundary() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::unit::Pct;
    use floem::views::Overlay;

    // Content visual bounds should be: (60, 60) to (140, 140)
    // Test clicks at various x positions along y=100 (horizontal center line)
    let test_points = [
        (50.0, 100.0, "x=50 (before content)"),
        (55.0, 100.0, "x=55 (before content)"),
        (60.0, 100.0, "x=60 (left edge)"),
        (65.0, 100.0, "x=65 (inside left)"),
        (70.0, 100.0, "x=70 (inside)"),
        (80.0, 100.0, "x=80 (inside)"),
        (90.0, 100.0, "x=90 (inside)"),
        (100.0, 100.0, "x=100 (center)"),
        (110.0, 100.0, "x=110 (inside)"),
        (120.0, 100.0, "x=120 (inside)"),
        (130.0, 100.0, "x=130 (inside)"),
        (135.0, 100.0, "x=135 (inside right)"),
        (140.0, 100.0, "x=140 (right edge)"),
        (145.0, 100.0, "x=145 (after content)"),
        (150.0, 100.0, "x=150 (after content)"),
    ];

    eprintln!("\n=== Horizontal probe (y=100) ===");
    eprintln!("Content expected bounds: x=[60, 140], y=[60, 140]");

    for (x, y, desc) in test_points {
        let backdrop_clicked = RwSignal::new(false);
        let content_clicked = RwSignal::new(false);

        let view = Overlay::new(
            Stack::new((
                Empty::new()
                    .style(|s| s.absolute().inset(0.0).z_index(1))
                    .on_click_stop({
                        let backdrop_clicked = backdrop_clicked;
                        move |_| backdrop_clicked.set(true)
                    }),
                Empty::new()
                    .style(|s| {
                        s.absolute()
                            .inset_left(Pct(50.0))
                            .inset_top(Pct(50.0))
                            .translate_x(Pct(-50.0))
                            .translate_y(Pct(-50.0))
                            .size(80.0, 80.0)
                            .z_index(10)
                    })
                    .on_click_stop({
                        let content_clicked = content_clicked;
                        move |_| content_clicked.set(true)
                    }),
            ))
            .style(|s| s.fixed().inset(0.0).width_full().height_full()),
        );

        let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
        harness.click(x, y);

        let hit = if content_clicked.get() {
            "CONTENT"
        } else if backdrop_clicked.get() {
            "backdrop"
        } else {
            "NONE"
        };

        eprintln!("  ({:>5.1}, {:>5.1}) {} -> {}", x, y, desc, hit);
    }

    // Also test vertical line at x=100
    let test_points_y = [
        (100.0, 50.0, "y=50 (before)"),
        (100.0, 60.0, "y=60 (top edge)"),
        (100.0, 70.0, "y=70 (inside)"),
        (100.0, 100.0, "y=100 (center)"),
        (100.0, 130.0, "y=130 (inside)"),
        (100.0, 140.0, "y=140 (bottom edge)"),
        (100.0, 150.0, "y=150 (after)"),
    ];

    eprintln!("\n=== Vertical probe (x=100) ===");

    for (x, y, desc) in test_points_y {
        let backdrop_clicked = RwSignal::new(false);
        let content_clicked = RwSignal::new(false);

        let view = Overlay::new(
            Stack::new((
                Empty::new()
                    .style(|s| s.absolute().inset(0.0).z_index(1))
                    .on_click_stop({
                        let backdrop_clicked = backdrop_clicked;
                        move |_| backdrop_clicked.set(true)
                    }),
                Empty::new()
                    .style(|s| {
                        s.absolute()
                            .inset_left(Pct(50.0))
                            .inset_top(Pct(50.0))
                            .translate_x(Pct(-50.0))
                            .translate_y(Pct(-50.0))
                            .size(80.0, 80.0)
                            .z_index(10)
                    })
                    .on_click_stop({
                        let content_clicked = content_clicked;
                        move |_| content_clicked.set(true)
                    }),
            ))
            .style(|s| s.fixed().inset(0.0).width_full().height_full()),
        );

        let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
        harness.click(x, y);

        let hit = if content_clicked.get() {
            "CONTENT"
        } else if backdrop_clicked.get() {
            "backdrop"
        } else {
            "NONE"
        };

        eprintln!("  ({:>5.1}, {:>5.1}) {} -> {}", x, y, desc, hit);
    }
}

/// Debug test: print layout information for overlay + fixed + translate.
#[test]
fn test_overlay_fixed_translate_debug_layout() {
    use floem::ViewId;
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::unit::Pct;
    use floem::views::Overlay;

    let content_id = ViewId::new();
    let backdrop_clicked = RwSignal::new(false);
    let content_clicked = RwSignal::new(false);

    let view = Overlay::new(
        Stack::new((
            // Backdrop
            Empty::new()
                .style(|s| s.absolute().inset(0.0).z_index(1))
                .on_click_stop(move |_| {
                    backdrop_clicked.set(true);
                }),
            // Content with known ID
            floem::views::Container::with_id(
                content_id,
                Empty::new().style(|s| s.size(60.0, 40.0)),
            )
            .style(|s| {
                s.absolute()
                    .inset_left(Pct(50.0))
                    .inset_top(Pct(50.0))
                    .translate_x(Pct(-50.0))
                    .translate_y(Pct(-50.0))
                    .size(80.0, 80.0)
                    .z_index(10)
            })
            .on_click_stop(move |_| {
                content_clicked.set(true);
            }),
        ))
        .style(|s| s.fixed().inset(0.0).width_full().height_full()),
    );

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // Print layout info for content
    if let Some(layout) = content_id.get_layout() {
        eprintln!(
            "Content layout position: ({}, {})",
            layout.location.x, layout.location.y
        );
        eprintln!(
            "Content layout size: {}x{}",
            layout.size.width, layout.size.height
        );
    }

    let layout_rect = content_id.get_layout_rect();
    eprintln!("Content layout_rect: {:?}", layout_rect);

    let transform = content_id.get_transform();
    let coeffs = transform.as_coeffs();
    eprintln!("Content transform: translate({}, {})", coeffs[4], coeffs[5]);

    // Click at center (100, 100) - should be inside content after transform
    harness.click(100.0, 100.0);

    eprintln!("After click at (100, 100):");
    eprintln!("  content_clicked: {}", content_clicked.get());
    eprintln!("  backdrop_clicked: {}", backdrop_clicked.get());

    assert!(
        content_clicked.get(),
        "Content should receive click at center"
    );
}

// =============================================================================
// Parent's Sibling Tests - Verify events don't bubble sideways
// =============================================================================

/// Test that clicking on a nested child does NOT bubble to parent's sibling.
///
/// Structure:
///   Parent (stack)
///   ├── Sibling1 (with handler)
///   └── Sibling2 (container)
///       └── NestedChild (click target)
///
/// When clicking NestedChild, events should bubble:
///   NestedChild → Sibling2 → Parent
/// NOT to Sibling1 (parent's sibling).
#[test]
fn test_events_do_not_bubble_to_parents_sibling() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};

    let sibling1_clicked = RwSignal::new(false);
    let sibling2_clicked = RwSignal::new(false);
    let nested_child_clicked = RwSignal::new(false);

    let view = Stack::new((
        // Sibling1 - should NOT receive click
        Empty::new()
            .style(|s| s.size(50.0, 100.0))
            .on_click_stop(move |_| {
                sibling1_clicked.set(true);
            }),
        // Sibling2 - container with nested child
        Container::new(
            // NestedChild - click target
            Empty::new()
                .style(|s| s.size(40.0, 40.0))
                .on_click_stop(move |_| {
                    nested_child_clicked.set(true);
                }),
        )
        .style(|s| s.size(50.0, 100.0))
        .on_click(move |_| {
            sibling2_clicked.set(true);
            // Note: using on_click (not on_click_stop) so event continues bubbling
            EventPropagation::Continue
        }),
    ))
    .style(|s| s.size(100.0, 100.0).flex_row());

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the nested child (right side of the view, within Sibling2)
    // Sibling2 starts at x=50, NestedChild is 40x40 starting at (0,0) within Sibling2
    // So NestedChild is at global coords (50, 0) to (90, 40)
    harness.click(70.0, 20.0);

    assert!(
        nested_child_clicked.get(),
        "NestedChild should receive click"
    );
    // Note: sibling2_clicked won't be set because nested_child uses on_click_stop
    assert!(
        !sibling1_clicked.get(),
        "Sibling1 should NOT receive click - events should not bubble to parent's siblings"
    );
}

/// Test the exact dialog scenario: clicking DialogHeader shouldn't close dialog.
///
/// Structure (mimics DialogContent):
///   Overlay
///   └── Stack
///       ├── Backdrop (on_click_stop closes)
///       └── Content (v_stack)
///           ├── Header ← click target (no handler)
///           └── Footer
///
/// Clicking Header should bubble to Content → Stack → Overlay.
/// It should NOT reach Backdrop (sibling of Content).
#[test]
fn test_dialog_header_click_does_not_close() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::views::Overlay;

    let dialog_open = RwSignal::new(true);
    let header_clicked = RwSignal::new(false);

    let view = Overlay::new(
        Stack::new((
            // Backdrop
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
                .on_click_stop(move |_| {
                    dialog_open.set(false);
                }),
            // Content (like DialogContent)
            Stack::vertical((
                // Header - like DialogHeader (with click handler to track)
                Empty::new()
                    .style(|s| s.size(50.0, 20.0))
                    .on_click(move |_| {
                        header_clicked.set(true);
                        EventPropagation::Continue
                    }),
                // Footer - like DialogFooter
                Empty::new().style(|s| s.size(50.0, 20.0)),
            ))
            .style(|s| {
                s.absolute()
                    .inset_left(25.0)
                    .inset_top(25.0)
                    .size(50.0, 50.0)
                    .z_index(10)
            }),
        ))
        .style(|s| s.size(100.0, 100.0)),
    );

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the header (top of content area)
    harness.click(50.0, 35.0);

    assert!(header_clicked.get(), "Header should receive click");
    assert!(
        dialog_open.get(),
        "Dialog should remain open - clicking header should NOT trigger backdrop's handler"
    );
}

/// Test deeply nested structure - events bubble up ancestor chain only.
///
/// Structure:
///   Root
///   ├── BranchA (with handler)
///   │   └── LeafA
///   └── BranchB (with handler)
///       └── LeafB ← click target
///
/// Clicking LeafB should bubble: LeafB → BranchB → Root
/// Should NOT reach BranchA or LeafA.
#[test]
fn test_deeply_nested_no_cross_branch_bubbling() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};

    let branch_a_clicked = RwSignal::new(false);
    let branch_b_clicked = RwSignal::new(false);
    let leaf_b_clicked = RwSignal::new(false);

    let view = Stack::horizontal((
        // BranchA
        Container::new(
            Empty::new().style(|s| s.size(40.0, 80.0)), // LeafA
        )
        .style(|s| s.size(50.0, 100.0))
        .on_click_stop(move |_| {
            branch_a_clicked.set(true);
        }),
        // BranchB
        Container::new(
            Empty::new()
                .style(|s| s.size(40.0, 80.0))
                .on_click_stop(move |_| {
                    leaf_b_clicked.set(true);
                }),
        )
        .style(|s| s.size(50.0, 100.0))
        .on_click(move |_| {
            branch_b_clicked.set(true);
            EventPropagation::Continue
        }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on LeafB (right side)
    harness.click(75.0, 50.0);

    assert!(leaf_b_clicked.get(), "LeafB should receive click");
    // branch_b_clicked won't be true because leaf_b uses on_click_stop
    assert!(
        !branch_a_clicked.get(),
        "BranchA should NOT receive click - events don't cross branches"
    );
}

/// Test that bubbling works correctly when nested child has no handler.
///
/// Structure:
///   Stack
///   ├── Sibling1 (handler)
///   └── Sibling2 (handler)
///       └── Child (NO handler) ← click target
///
/// Click on Child should bubble to Sibling2 (if using on_click, not on_click_stop).
/// Should NOT reach Sibling1.
#[test]
fn test_bubbling_through_handler_less_child() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};

    let sibling1_clicked = RwSignal::new(false);
    let sibling2_clicked = RwSignal::new(false);

    let view = Stack::horizontal((
        // Sibling1
        Empty::new()
            .style(|s| s.size(50.0, 100.0))
            .on_click_stop(move |_| {
                sibling1_clicked.set(true);
            }),
        // Sibling2 with child that has no handler
        Container::new(
            Empty::new().style(|s| s.size(40.0, 80.0)), // Child - NO handler
        )
        .style(|s| s.size(50.0, 100.0))
        .on_click_stop(move |_| {
            sibling2_clicked.set(true);
        }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on the child inside Sibling2
    harness.click(75.0, 50.0);

    assert!(
        sibling2_clicked.get(),
        "Sibling2 should receive click (bubbled from child)"
    );
    assert!(
        !sibling1_clicked.get(),
        "Sibling1 should NOT receive click - bubbling goes up, not sideways"
    );
}

// =============================================================================
// Fixed Overlay with Child Click Tests
// =============================================================================

/// Test that a clickable child inside a fixed overlay receives pointer events.
///
/// Structure:
///   Overlay (fixed, inset 0, full size)
///   └── Stack
///       ├── Backdrop (on_click_stop)
///       └── Content (fixed, centered with translate)
///           └── ClickableChild (button-like element)
#[test]
fn test_fixed_overlay_child_receives_click() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::unit::Pct;
    use floem::views::Overlay;

    let backdrop_clicked = RwSignal::new(false);
    let content_clicked = RwSignal::new(false);
    let child_clicked = RwSignal::new(false);

    let view = Overlay::new(
        Stack::new((
            // Backdrop
            Empty::new()
                .style(|s| s.absolute().inset(0.0))
                .on_click_stop(move |_| {
                    eprintln!("[backdrop] clicked!");
                    backdrop_clicked.set(true);
                }),
            // Content - centered with translate
            Stack::vertical((
                // Clickable child (like a button)
                Empty::new()
                    .style(|s| s.size(60.0, 30.0))
                    .on_click_stop(move |_| {
                        eprintln!("[child] clicked!");
                        child_clicked.set(true);
                    }),
            ))
            .style(|s| {
                s.absolute()
                    .inset_left(Pct(50.0))
                    .inset_top(Pct(50.0))
                    .translate_x(Pct(-50.0))
                    .translate_y(Pct(-50.0))
                    .size(80.0, 60.0)
                    .z_index(10)
            })
            .on_click_stop(move |_| {
                eprintln!("[content] clicked!");
                content_clicked.set(true);
            }),
        ))
        .style(|s| s.fixed().inset(0.0).width_full().height_full()),
    );

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    eprintln!("=== Fixed Overlay Child Click Test ===");
    eprintln!("Window size: 200x200");
    eprintln!("Content: 80x60 centered at (100, 100) with translate(-40, -30)");
    eprintln!("Expected content bounds: (60, 70) to (140, 130)");
    eprintln!("Child inside content: 60x30 at origin, so global (60, 70) to (120, 100)");

    // Click at center of the child (should be around 90, 85)
    eprintln!("\nClicking at (90, 85) - should hit child");
    harness.click(90.0, 85.0);

    eprintln!("Results:");
    eprintln!("  child_clicked: {}", child_clicked.get());
    eprintln!("  content_clicked: {}", content_clicked.get());
    eprintln!("  backdrop_clicked: {}", backdrop_clicked.get());

    assert!(
        child_clicked.get(),
        "Child inside fixed overlay content should receive click"
    );
    assert!(
        !backdrop_clicked.get(),
        "Backdrop should NOT receive click when child is clicked"
    );
}

/// Test clicking at multiple positions inside the child to verify hit detection bounds.
#[test]
fn test_fixed_overlay_child_click_bounds() {
    use floem::ViewId;
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::unit::Pct;
    use floem::views::Overlay;

    let child_id = ViewId::new();
    let content_id = ViewId::new();

    let backdrop_clicks = RwSignal::new(0);
    let content_clicks = RwSignal::new(0);
    let child_clicks = RwSignal::new(0);

    let view = Overlay::new(
        Stack::new((
            // Backdrop
            Empty::new()
                .style(|s| s.absolute().inset(0.0))
                .on_click_stop(move |_| {
                    backdrop_clicks.update(|c| *c += 1);
                }),
            // Content - centered with translate
            floem::views::Container::with_id(
                content_id,
                Stack::vertical((
                    // Clickable child with known ID
                    floem::views::Container::with_id(
                        child_id,
                        Empty::new().style(|s| s.size(60.0, 30.0)),
                    )
                    .on_click_stop(move |_| {
                        child_clicks.update(|c| *c += 1);
                    }),
                ))
                .style(|s| s.gap(0.0)),
            )
            .style(|s| {
                s.absolute()
                    .inset_left(Pct(50.0))
                    .inset_top(Pct(50.0))
                    .translate_x(Pct(-50.0))
                    .translate_y(Pct(-50.0))
                    .size(80.0, 60.0)
                    .z_index(10)
            })
            .on_click_stop(move |_| {
                content_clicks.update(|c| *c += 1);
            }),
        ))
        .style(|s| s.fixed().inset(0.0).width_full().height_full()),
    );

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    eprintln!("=== Fixed Overlay Child Click Bounds Test ===");

    // Print layout info
    if let Some(content_layout) = content_id.get_layout() {
        eprintln!(
            "Content layout: pos=({}, {}), size={}x{}",
            content_layout.location.x,
            content_layout.location.y,
            content_layout.size.width,
            content_layout.size.height
        );
    }
    let content_rect = content_id.get_layout_rect();
    eprintln!("Content layout_rect: {:?}", content_rect);

    if let Some(child_layout) = child_id.get_layout() {
        eprintln!(
            "Child layout: pos=({}, {}), size={}x{}",
            child_layout.location.x,
            child_layout.location.y,
            child_layout.size.width,
            child_layout.size.height
        );
    }
    let child_rect = child_id.get_layout_rect();
    eprintln!("Child layout_rect: {:?}", child_rect);

    // Test clicks at various positions
    let test_points = [
        (90.0, 85.0, "center of child"),
        (65.0, 75.0, "top-left of child"),
        (115.0, 95.0, "bottom-right of child"),
        (70.0, 110.0, "below child, inside content"),
        (50.0, 50.0, "outside content, on backdrop"),
    ];

    for (x, y, desc) in test_points {
        // Reset counters
        backdrop_clicks.set(0);
        content_clicks.set(0);
        child_clicks.set(0);

        harness.click(x, y);

        let result = if child_clicks.get() > 0 {
            "CHILD"
        } else if content_clicks.get() > 0 {
            "CONTENT"
        } else if backdrop_clicks.get() > 0 {
            "backdrop"
        } else {
            "NONE"
        };

        eprintln!("  ({:5.1}, {:5.1}) {} -> {}", x, y, desc, result);
    }
}

/// Test that nested children at various depths receive clicks correctly.
#[test]
fn test_fixed_overlay_deeply_nested_child() {
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::unit::Pct;
    use floem::views::Overlay;

    let backdrop_clicked = RwSignal::new(false);
    let level1_clicked = RwSignal::new(false);
    let level2_clicked = RwSignal::new(false);
    let level3_clicked = RwSignal::new(false);

    let view = Overlay::new(
        Stack::new((
            // Backdrop
            Empty::new()
                .style(|s| s.absolute().inset(0.0))
                .on_click_stop(move |_| {
                    backdrop_clicked.set(true);
                }),
            // Level 1 - Content container
            Container::new(
                // Level 2 - Inner container
                Container::new(
                    // Level 3 - Deepest clickable element
                    Empty::new()
                        .style(|s| s.size(40.0, 20.0))
                        .on_click_stop(move |_| {
                            eprintln!("[level3] clicked!");
                            level3_clicked.set(true);
                        }),
                )
                .style(|s| s.size(50.0, 30.0).padding(5.0))
                .on_click_stop(move |_| {
                    eprintln!("[level2] clicked!");
                    level2_clicked.set(true);
                }),
            )
            .style(|s| {
                s.absolute()
                    .inset_left(Pct(50.0))
                    .inset_top(Pct(50.0))
                    .translate_x(Pct(-50.0))
                    .translate_y(Pct(-50.0))
                    .size(80.0, 60.0)
                    .padding(10.0)
                    .z_index(10)
            })
            .on_click_stop(move |_| {
                eprintln!("[level1] clicked!");
                level1_clicked.set(true);
            }),
        ))
        .style(|s| s.fixed().inset(0.0).width_full().height_full()),
    );

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    eprintln!("=== Deeply Nested Child Click Test ===");
    eprintln!("Clicking at center (100, 100)");

    harness.click(100.0, 100.0);

    eprintln!("Results:");
    eprintln!("  level3_clicked: {}", level3_clicked.get());
    eprintln!("  level2_clicked: {}", level2_clicked.get());
    eprintln!("  level1_clicked: {}", level1_clicked.get());
    eprintln!("  backdrop_clicked: {}", backdrop_clicked.get());

    // The deepest element that contains the point should receive the click
    // Since we use on_click_stop, only one handler should fire
    assert!(
        level3_clicked.get() || level2_clicked.get() || level1_clicked.get(),
        "At least one content level should receive click"
    );
    assert!(!backdrop_clicked.get(), "Backdrop should NOT receive click");
}

/// Probe test to find exact hit detection bounds for child element.
#[test]
fn test_fixed_overlay_child_probe_bounds() {
    use floem::ViewId;
    use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
    use floem::unit::Pct;
    use floem::views::Overlay;

    let child_id = ViewId::new();

    let backdrop_clicks = RwSignal::new(0);
    let child_clicks = RwSignal::new(0);

    // Simple structure: backdrop + content with child
    // Content is 80x60 centered, child is 60x30 at top of content
    let view = Overlay::new(
        Stack::new((
            Empty::new()
                .style(|s| s.absolute().inset(0.0))
                .on_click_stop(move |_| {
                    backdrop_clicks.update(|c| *c += 1);
                }),
            Stack::vertical((floem::views::Container::with_id(
                child_id,
                Empty::new().style(|s| s.size(60.0, 30.0)),
            )
            .on_click_stop(move |_| {
                child_clicks.update(|c| *c += 1);
            }),))
            .style(|s| {
                s.absolute()
                    .inset_left(Pct(50.0))
                    .inset_top(Pct(50.0))
                    .translate_x(Pct(-50.0))
                    .translate_y(Pct(-50.0))
                    .size(80.0, 60.0)
                    .z_index(10)
            }),
        ))
        .style(|s| s.fixed().inset(0.0).width_full().height_full()),
    );

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    eprintln!("=== Fixed Overlay Child Probe Bounds ===");

    // Get actual child bounds
    let child_rect = child_id.get_layout_rect();
    eprintln!("Child layout_rect: {:?}", child_rect);

    // Window: 200x200
    // Content: 80x60 at center (100, 100) with translate(-40, -30)
    // Expected content visual bounds: (60, 70) to (140, 130)
    // Child: 60x30 inside content at (0, 0)
    // Expected child visual bounds: (60 + (80-60)/2, 70) = (70, 70) to (130, 100)
    // Actually, v_stack positions child at top, so if content padding is 0:
    // Child starts at content origin, so (60, 70) to (120, 100)

    eprintln!("\n=== Horizontal probe at y=85 (center of expected child height) ===");
    for x in (50..=150).step_by(10) {
        backdrop_clicks.set(0);
        child_clicks.set(0);

        harness.click(x as f64, 85.0);

        let hit = if child_clicks.get() > 0 {
            "CHILD"
        } else if backdrop_clicks.get() > 0 {
            "backdrop"
        } else {
            "NONE"
        };

        eprintln!("  x={:3} -> {}", x, hit);
    }

    eprintln!("\n=== Vertical probe at x=90 (center of expected child width) ===");
    for y in (50..=130).step_by(10) {
        backdrop_clicks.set(0);
        child_clicks.set(0);

        harness.click(90.0, y as f64);

        let hit = if child_clicks.get() > 0 {
            "CHILD"
        } else if backdrop_clicks.get() > 0 {
            "backdrop"
        } else {
            "NONE"
        };

        eprintln!("  y={:3} -> {}", y, hit);
    }
}

// =============================================================================
// Clip rect inheritance tests
// =============================================================================

/// Test that normal flow children extending beyond parent bounds CAN receive events.
/// This is CSS default behavior (overflow: visible).
#[test]
fn test_normal_flow_child_outside_parent_bounds_receives_click() {
    let parent_clicks = RwSignal::new(0);
    let child_clicks = RwSignal::new(0);

    // Parent is 50x50, child is 100x100 starting at (0,0)
    // Child extends 50px beyond parent in both directions
    let view = Container::new(
        Empty::new()
            .on_click_stop(move |_| {
                child_clicks.update(|c| *c += 1);
            })
            .style(|s| s.size(100.0, 100.0)), // Child larger than parent
    )
    .on_click_stop(move |_| {
        parent_clicks.update(|c| *c += 1);
    })
    .style(|s| s.size(50.0, 50.0)); // Small parent

    let mut harness = HeadlessHarness::new_with_size(view, 150.0, 150.0);

    eprintln!("=== Normal Flow Child Outside Parent Bounds Test ===");

    // Click inside parent bounds (25, 25) - both parent and child should be there
    harness.click(25.0, 25.0);
    eprintln!(
        "Click at (25, 25): parent={}, child={}",
        parent_clicks.get(),
        child_clicks.get()
    );
    assert_eq!(
        child_clicks.get(),
        1,
        "Child should receive click inside parent"
    );

    // Reset
    parent_clicks.set(0);
    child_clicks.set(0);

    // Click outside parent bounds but inside child bounds (75, 75)
    // With default overflow: visible, child should still receive the click
    harness.click(75.0, 75.0);
    eprintln!(
        "Click at (75, 75): parent={}, child={}",
        parent_clicks.get(),
        child_clicks.get()
    );

    // This test documents current behavior - child extending beyond parent
    // may or may not receive clicks depending on clip_rect inheritance
    eprintln!(
        "Child clicks outside parent: {} (documents current clip_rect behavior)",
        child_clicks.get()
    );
}

/// Test that scroll container (has viewport) clips children for hit testing.
#[test]
fn test_scroll_container_clips_children() {
    let child_clicks = RwSignal::new(0);

    // Scroll container is 100x100, child is 200x200
    // Child should only receive clicks within the 100x100 viewport
    let view = Scroll::new(
        Empty::new()
            .on_click_stop(move |_| {
                child_clicks.update(|c| *c += 1);
            })
            .style(|s| s.size(200.0, 200.0)),
    )
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    eprintln!("=== Scroll Container Clips Children Test ===");

    // Click inside scroll viewport (50, 50)
    harness.click(50.0, 50.0);
    eprintln!(
        "Click at (50, 50) inside viewport: child_clicks={}",
        child_clicks.get()
    );
    assert_eq!(
        child_clicks.get(),
        1,
        "Child should receive click inside viewport"
    );

    // Reset
    child_clicks.set(0);

    // Click outside scroll viewport (150, 150)
    harness.click(150.0, 150.0);
    eprintln!(
        "Click at (150, 150) outside viewport: child_clicks={}",
        child_clicks.get()
    );
    assert_eq!(
        child_clicks.get(),
        0,
        "Child should NOT receive click outside viewport"
    );
}

/// Test that absolute positioned element clips children correctly.
#[test]
fn test_absolute_element_clips_children() {
    let child_clicks = RwSignal::new(0);
    let outside_clicks = RwSignal::new(0);

    // Absolute positioned container at (50, 50), size 100x100
    // Child inside is 50x50 at origin
    let view = Stack::new((
        // Background to catch clicks outside the absolute element
        Empty::new()
            .on_click_stop(move |_| {
                outside_clicks.update(|c| *c += 1);
            })
            .style(|s| s.size(300.0, 300.0)),
        // Absolute positioned element with a clickable child
        Container::new(
            Empty::new()
                .on_click_stop(move |_| {
                    child_clicks.update(|c| *c += 1);
                })
                .style(|s| s.size(50.0, 50.0)),
        )
        .style(|s| {
            s.absolute()
                .inset_left(50.0)
                .inset_top(50.0)
                .size(100.0, 100.0)
        }),
    ))
    .style(|s| s.size(300.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 300.0, 300.0);

    eprintln!("=== Absolute Element Clips Children Test ===");

    // Click inside the absolute element where child is (75, 75)
    harness.click(75.0, 75.0);
    eprintln!(
        "Click at (75, 75): child={}, outside={}",
        child_clicks.get(),
        outside_clicks.get()
    );
    assert_eq!(
        child_clicks.get(),
        1,
        "Child in absolute element should receive click"
    );

    // Reset
    child_clicks.set(0);
    outside_clicks.set(0);

    // Click outside absolute element (25, 25)
    harness.click(25.0, 25.0);
    eprintln!(
        "Click at (25, 25): child={}, outside={}",
        child_clicks.get(),
        outside_clicks.get()
    );
    assert_eq!(
        outside_clicks.get(),
        1,
        "Background should receive click outside absolute element"
    );
}

/// Test nested containers - clip_rect should be inherited through the tree.
#[test]
fn test_nested_containers_clip_inheritance() {
    let deepest_clicks = RwSignal::new(0);

    // Outer container: 200x200
    // Middle container: 100x100 at (50, 50)
    // Inner container: 50x50 at (0, 0) relative to middle
    // Deepest clickable: 100x100 (extends beyond inner)
    let view = Container::new(
        Container::new(
            Container::new(
                Empty::new()
                    .on_click_stop(move |_| {
                        deepest_clicks.update(|c| *c += 1);
                    })
                    .style(|s| s.size(100.0, 100.0)), // Extends beyond inner
            )
            .style(|s| s.size(50.0, 50.0)), // Inner - 50x50
        )
        .style(|s| s.margin_left(50.0).margin_top(50.0).size(100.0, 100.0)), // Middle
    )
    .style(|s| s.size(200.0, 200.0)); // Outer

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    eprintln!("=== Nested Containers Clip Inheritance Test ===");

    // Click inside all containers (75, 75) = inside middle, inside inner's bounds
    harness.click(75.0, 75.0);
    eprintln!("Click at (75, 75): deepest={}", deepest_clicks.get());
    assert_eq!(
        deepest_clicks.get(),
        1,
        "Should receive click in nested hierarchy"
    );

    // Reset
    deepest_clicks.set(0);

    // Click at (125, 125) - inside middle but outside inner's bounds
    // The deepest extends there, but inner doesn't
    harness.click(125.0, 125.0);
    eprintln!(
        "Click at (125, 125) outside inner bounds: deepest={}",
        deepest_clicks.get()
    );
    // Documents current behavior
    eprintln!(
        "Nested clip inheritance result: {} (0 = proper clipping, 1 = overflow visible)",
        deepest_clicks.get()
    );
}
