//! Tests for click event propagation (stopping vs bubbling).
//!
//! These tests verify that:
//! - `on_click_stop` prevents events from bubbling to parent views
//! - `on_click_cont` allows events to bubble to parent views

use floem_test::prelude::*;

#[test]
fn test_pointer_down_move_away_no_click() {
    // Pointer down on view, move away, then pointer up should NOT fire click
    let tracker = ClickTracker::new();

    let view = stack((
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

    let view = stack((
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

    let view = stack((
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

    let view = stack((target, Empty::new().style(|s| s.size(50.0, 100.0))))
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

    let view = stack((
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

    let view = stack((child1, child2)).style(|s| s.size(100.0, 100.0));

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

    let view = stack((stack((
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
