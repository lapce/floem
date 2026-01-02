//! Tests for accumulated clip rect hit testing.
//!
//! These tests verify the Chromium-style accumulated clip rect model where:
//! 1. Each view's effective clip is the intersection of its bounds with all ancestor clips
//! 2. Scroll/overflow containers create clip boundaries
//! 3. Absolute positioned elements with z-index can escape parent clips
//! 4. Nested clips accumulate (intersect) properly

use floem::prelude::*;
use floem::style::{Display, PointerEvents};
use floem::views::{Container, Empty, Scroll, Stack};
use floem_test::{ClickTracker, HeadlessHarness, layers};

// =============================================================================
// Basic Scroll Clip Tests
// =============================================================================

/// Test that content extending beyond scroll viewport doesn't receive clicks
/// outside the viewport.
///
/// This is the fundamental clip behavior: a tall button inside a short scroll
/// should only be clickable within the scroll's visible area.
#[test]
fn test_scroll_clips_content_beyond_viewport() {
    let tracker = ClickTracker::new();

    // Button that's taller than the scroll container
    let button = tracker
        .track_named("button", Empty::new())
        .style(|s| s.size(100.0, 200.0));

    // Scroll container: only 50px tall
    let scroll = Scroll::new(button).style(|s| s.size(100.0, 50.0));

    // Important: make the window the same size as the scroll so clicks
    // outside scroll are also outside window
    let view = Stack::new((scroll,)).style(|s| s.size(100.0, 50.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 50.0);

    // Click inside scroll viewport - should hit button
    harness.click(50.0, 25.0);
    assert!(
        tracker.was_clicked(),
        "Click inside scroll viewport should hit button"
    );
    tracker.reset();

    // Click at y=48 - just inside bottom edge
    harness.click(50.0, 48.0);
    assert!(
        tracker.was_clicked(),
        "Click near bottom of scroll should hit button"
    );
}

/// Test that content below scroll viewport (using margin) doesn't receive clicks.
#[test]
fn test_scroll_clips_content_with_margin() {
    let tracker = ClickTracker::new();

    // Button at y=100 (below scroll viewport of 50)
    let button = tracker
        .track_named("button", Empty::new())
        .style(|s| s.size(100.0, 50.0).margin_top(100.0));

    let content = Container::new(button).style(|s| s.size(100.0, 200.0));
    let scroll = Scroll::new(content).style(|s| s.size(100.0, 50.0));

    let view = Stack::new((scroll,)).style(|s| s.size(100.0, 50.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 50.0);

    // The button is at y=100-150 in content coords, scroll shows y=0-50
    // So button is completely invisible

    // Click at y=25 - inside scroll but button not there
    harness.click(50.0, 25.0);
    assert!(
        !tracker.was_clicked(),
        "Click where button isn't should not hit"
    );
}

// =============================================================================
// Nested Scroll Tests
// =============================================================================

/// Test nested scrolls with simple non-overlapping structure.
#[test]
fn test_nested_scroll_simple() {
    let tracker = ClickTracker::new();

    // Button fills inner scroll content
    let button = tracker
        .track_named("button", Empty::new())
        .style(|s| s.size(100.0, 100.0));

    // Inner scroll: 40x40
    let inner_scroll = Scroll::new(button).style(|s| s.size(40.0, 40.0));

    // Outer scroll: 60x60
    let outer_scroll = Scroll::new(Container::new(inner_scroll).style(|s| s.size(100.0, 100.0)))
        .style(|s| s.size(60.0, 60.0));

    let view = Stack::new((outer_scroll,)).style(|s| s.size(60.0, 60.0));

    let mut harness = HeadlessHarness::new_with_size(view, 60.0, 60.0);

    // Click at (20, 20) - inside both scrolls
    harness.click(20.0, 20.0);
    assert!(
        tracker.was_clicked(),
        "Click inside nested scrolls should hit button"
    );
    tracker.reset();

    // Click at (50, 50) - inside outer scroll but outside inner scroll (inner is 0-40)
    harness.click(50.0, 50.0);
    assert!(
        !tracker.was_clicked(),
        "Click outside inner scroll should not hit button"
    );
}

/// Test nested scroll where inner scroll is offset within outer.
#[test]
fn test_nested_scroll_with_offset() {
    let tracker = ClickTracker::new();

    let button = tracker
        .track_named("button", Empty::new())
        .style(|s| s.size(50.0, 50.0));

    // Inner scroll at position (20, 20) via margin
    let inner_scroll = Scroll::new(button).style(|s| s.size(40.0, 40.0).margin(20.0));

    // Outer scroll: 80x80
    let outer_scroll = Scroll::new(Container::new(inner_scroll).style(|s| s.size(100.0, 100.0)))
        .style(|s| s.size(80.0, 80.0));

    let view = Stack::new((outer_scroll,)).style(|s| s.size(80.0, 80.0));

    let mut harness = HeadlessHarness::new_with_size(view, 80.0, 80.0);

    // Inner scroll is at (20-60, 20-60)
    // Click at (10, 10) - before inner scroll
    harness.click(10.0, 10.0);
    assert!(
        !tracker.was_clicked(),
        "Click before inner scroll should not hit button"
    );

    // Click at (30, 30) - inside inner scroll
    harness.click(30.0, 30.0);
    assert!(
        tracker.was_clicked(),
        "Click inside inner scroll should hit button"
    );
    tracker.reset();

    // Click at (70, 70) - after inner scroll ends (inner is 20-60)
    harness.click(70.0, 70.0);
    assert!(
        !tracker.was_clicked(),
        "Click after inner scroll should not hit button"
    );
}

// =============================================================================
// Absolute Positioning with Clip Escape Tests
// =============================================================================

/// Test that absolute positioned elements with z-index can receive clicks
/// outside their parent's clip bounds.
#[test]
fn test_absolute_with_z_index_escapes_clip() {
    let tracker = ClickTracker::new();

    // Dropdown that will be positioned outside the scroll viewport
    let dropdown = tracker.track_named("dropdown", Empty::new()).style(|s| {
        s.absolute()
            .inset_left(0.0)
            .inset_top(60.0) // Below scroll viewport
            .width(100.0)
            .height(40.0)
            .z_index(100)
    });

    // Content container with relative positioning
    let content = layers((
        Empty::new().style(|s| s.size(100.0, 100.0)), // Background
        dropdown,
    ))
    .style(|s| s.size(100.0, 100.0));

    // Scroll that clips at 50px
    let scroll = Scroll::new(content).style(|s| s.size(100.0, 50.0));

    // Window is taller to see the escaped dropdown
    let view = Stack::new((scroll,)).style(|s| s.size(100.0, 120.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 120.0);

    // Click at y=80 - outside scroll viewport but on the dropdown
    // The dropdown should receive this click because it has z-index and absolute positioning
    harness.click(50.0, 80.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["dropdown"],
        "Dropdown with z-index should receive click outside scroll clip"
    );
}

/// Test the trigger vs dropdown scenario: trigger inside scroll, dropdown extending below.
#[test]
fn test_trigger_and_dropdown_click_dispatch() {
    let tracker = ClickTracker::new();

    // Trigger button at the top (no z-index, just fills top 30px)
    let trigger = tracker
        .track_named("trigger", Empty::new())
        .style(|s| s.size(100.0, 30.0));

    // Dropdown positioned below trigger, with higher z-index
    // Use a stack with relative positioning to hold both
    let dropdown = tracker.track_named("dropdown", Empty::new()).style(|s| {
        s.absolute()
            .inset_left(0.0)
            .inset_top(30.0) // Right below trigger
            .width(100.0)
            .height(80.0) // Extends to y=110
            .z_index(100)
    });

    // Stack holds trigger (normal flow) and dropdown (absolute)
    // The trigger takes up the top 30px, dropdown is positioned at y=30
    let container = Stack::new((trigger, dropdown)).style(|s| s.size(100.0, 30.0));

    // Scroll clips at 50px
    let scroll = Scroll::new(container).style(|s| s.size(100.0, 50.0));

    let view = Stack::new((scroll,)).style(|s| s.size(100.0, 120.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 120.0);

    // Click at y=15 - on trigger (trigger is at y=0-30)
    // Dropdown is at y=30+ with z-index 100, but doesn't cover y=15
    harness.click(50.0, 15.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["trigger"],
        "Trigger should receive click at y=15"
    );
    tracker.reset();

    // Click at y=40 - on dropdown (inside scroll viewport, y=30-50 part of dropdown)
    harness.click(50.0, 40.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["dropdown"],
        "Dropdown should receive click at y=40"
    );
    tracker.reset();

    // Click at y=80 - on dropdown but outside scroll viewport
    harness.click(50.0, 80.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["dropdown"],
        "Dropdown should receive click at y=80 (outside scroll clip)"
    );
}

// =============================================================================
// Scroll Position and Visibility Tests
// =============================================================================

/// Test that scrolling changes which parts of content are clickable.
#[test]
fn test_scroll_position_changes_clickable_area() {
    let tracker = ClickTracker::new();

    // Two buttons vertically stacked
    let button1 = tracker
        .track_named("button1", Empty::new())
        .style(|s| s.size(100.0, 40.0));
    let button2 = tracker
        .track_named("button2", Empty::new())
        .style(|s| s.size(100.0, 40.0));

    let content = Stack::vertical((button1, button2)).style(|s| s.size(100.0, 80.0));

    // Scroll viewport only shows 50px
    let scroll = Scroll::new(content).style(|s| s.size(100.0, 50.0));

    let view = Stack::new((scroll,)).style(|s| s.size(100.0, 50.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 50.0);

    // Initially: button1 at y=0-40 (visible), button2 at y=40-80 (partially visible)

    // Click at y=20 - on button1
    harness.click(50.0, 20.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["button1"],
        "Button1 should be clickable initially"
    );
    tracker.reset();

    // Click at y=45 - on button2 (it starts at y=40, ends at y=80)
    harness.click(50.0, 45.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["button2"],
        "Button2 should be clickable at y=45"
    );
    tracker.reset();

    // Scroll down by 40px so button2 is fully visible
    harness.scroll_down(50.0, 25.0, 40.0);

    // After scroll: content offset is -40
    // button1 is now at visual y=-40 to 0 (clipped)
    // button2 is now at visual y=0 to 40 (fully visible)

    // Click at y=20 - should now hit button2
    harness.click(50.0, 20.0);
    assert!(
        tracker.clicked_names().contains(&"button2".to_string()),
        "Button2 should be clickable after scrolling"
    );
}

// =============================================================================
// Z-Index with Clips Tests
// =============================================================================

/// Test z-index ordering within a clipped region.
#[test]
fn test_z_index_ordering_within_scroll() {
    let tracker = ClickTracker::new();

    let low_z = tracker
        .track_named("low", Empty::new())
        .style(|s| s.size(100.0, 100.0).z_index(1));
    let high_z = tracker
        .track_named("high", Empty::new())
        .style(|s| s.size(100.0, 100.0).z_index(10));

    let content = layers((low_z, high_z)).style(|s| s.size(100.0, 100.0));

    let scroll = Scroll::new(content).style(|s| s.size(60.0, 60.0));

    let view = Stack::new((scroll,)).style(|s| s.size(60.0, 60.0));

    let mut harness = HeadlessHarness::new_with_size(view, 60.0, 60.0);

    // Inside scroll clip, high_z should win
    harness.click(30.0, 30.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["high"],
        "Higher z-index should receive click within clip"
    );
}

// =============================================================================
// Edge Cases
// =============================================================================

/// Test that hidden views don't affect hit testing.
#[test]
fn test_hidden_view_no_clip_effect() {
    let tracker = ClickTracker::new();

    let visible_button = tracker
        .track_named("visible", Empty::new())
        .style(|s| s.size(100.0, 50.0));

    let hidden_scroll = Scroll::new(Empty::new().style(|s| s.size(100.0, 100.0)))
        .style(|s| s.size(100.0, 50.0).display(Display::None));

    let view = Stack::vertical((hidden_scroll, visible_button)).style(|s| s.size(100.0, 50.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 50.0);

    // The hidden scroll should not affect the visible button
    harness.click(50.0, 25.0);
    assert!(tracker.was_clicked(), "Visible button should be clickable");
}

/// Test pointer_events: none combined with clips.
#[test]
fn test_pointer_events_none_with_scroll_clip() {
    let tracker = ClickTracker::new();

    let button = tracker
        .track_named("button", Empty::new())
        .style(|s| s.size(100.0, 100.0));

    // Overlay with pointer_events: none
    let overlay = Empty::new().style(|s| {
        s.absolute()
            .inset(0.0)
            .z_index(100)
            .pointer_events(PointerEvents::None)
    });

    let content = layers((button, overlay)).style(|s| s.size(100.0, 100.0));
    let scroll = Scroll::new(content).style(|s| s.size(50.0, 50.0));

    let view = Stack::new((scroll,)).style(|s| s.size(50.0, 50.0));

    let mut harness = HeadlessHarness::new_with_size(view, 50.0, 50.0);

    // Click should pass through overlay to button
    harness.click(25.0, 25.0);
    assert!(
        tracker.was_clicked(),
        "Click should pass through pointer_events:none to button"
    );
}

/// Test overlapping scroll containers (side by side with overlap).
#[test]
fn test_overlapping_scroll_containers() {
    let tracker = ClickTracker::new();

    let left_content = tracker
        .track_named("left", Empty::new())
        .style(|s| s.size(100.0, 100.0));
    let left_scroll = Scroll::new(left_content).style(|s| s.size(50.0, 50.0));

    let right_content = tracker
        .track_named("right", Empty::new())
        .style(|s| s.size(100.0, 100.0));
    // Right scroll overlaps left by using negative margin
    let right_scroll = Scroll::new(right_content).style(|s| s.size(50.0, 50.0).margin_left(-20.0));

    let view = Stack::horizontal((left_scroll, right_scroll)).style(|s| s.size(80.0, 50.0));

    let mut harness = HeadlessHarness::new_with_size(view, 80.0, 50.0);

    // Click at x=15 - in left scroll only area
    harness.click(15.0, 25.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["left"],
        "Left should receive click in non-overlapping area"
    );
    tracker.reset();

    // Click at x=40 - in overlap area
    // Right is later in DOM order, so should be on top
    harness.click(40.0, 25.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["right"],
        "Right should receive click in overlap (later in DOM)"
    );
}

// =============================================================================
// Scroll Content Outside Viewport Tests
// =============================================================================

/// Test that when scroll content extends outside the scroll viewport,
/// clicking at window coordinates outside the viewport does NOT hit the content.
///
/// This tests the scenario where:
/// - Scroll container is 100x100 at y=50 (window coords y=50-150)
/// - Content is 100x300 (extends to content y=300)
/// - Window is 300x300 to allow clicking below the scroll
/// - Click at y=200 (outside scroll container) should NOT hit content
#[test]
fn test_scroll_content_outside_viewport_not_clickable() {
    let tracker = ClickTracker::new();

    // Large button that extends well beyond the scroll viewport
    let button = tracker
        .track_named("button", Empty::new())
        .style(|s| s.size(100.0, 300.0));

    // Scroll container: 100x100, positioned at y=50 via margin
    let scroll = Scroll::new(button).style(|s| s.size(100.0, 100.0).margin_top(50.0));

    // Window is 300x300 to have space below the scroll
    let view = Stack::new((scroll,)).style(|s| s.size(100.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 300.0);

    // Click at y=100 - inside scroll viewport (y=50-150)
    harness.click(50.0, 100.0);
    assert!(
        tracker.was_clicked(),
        "Click inside scroll viewport should hit button"
    );
    tracker.reset();

    // Click at y=200 - OUTSIDE scroll viewport (below y=150)
    // The button content extends to y=300 in content coords, but the scroll
    // clips at y=150 window coords, so this click should NOT hit the button.
    harness.click(50.0, 200.0);
    assert!(
        !tracker.was_clicked(),
        "Click outside scroll viewport should NOT hit button (content should be clipped)"
    );
}

/// Test that scrolled-out content (above viewport) cannot receive events.
/// When we scroll down, content that was visible is now clipped at the top.
#[test]
fn test_scrolled_out_content_top_not_clickable() {
    let tracker = ClickTracker::new();

    // Two buttons stacked vertically
    let button1 = tracker
        .track_named("button1", Empty::new())
        .style(|s| s.size(100.0, 100.0));
    let button2 = tracker
        .track_named("button2", Empty::new())
        .style(|s| s.size(100.0, 100.0));

    let content = Stack::vertical((button1, button2)).style(|s| s.size(100.0, 200.0));

    // Scroll container: 100x100
    let scroll = Scroll::new(content).style(|s| s.size(100.0, 100.0));

    let view = Stack::new((scroll,)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initially, button1 is at y=0-100, button2 is at y=100-200 (below viewport)

    // Click at y=50 - should hit button1
    harness.click(50.0, 50.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["button1"],
        "Initially button1 should be clickable"
    );
    tracker.reset();

    // Scroll down by 100px so button1 is now above viewport
    harness.scroll_down(50.0, 50.0, 100.0);

    // After scroll: button1 is at visual y=-100 to 0 (clipped)
    // button2 is at visual y=0 to 100 (fully visible)

    // Click at y=50 - should now hit button2 (not button1)
    harness.click(50.0, 50.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["button2"],
        "After scrolling, button2 should receive click at y=50"
    );
    tracker.reset();

    // The key test: button1 should NOT receive any clicks anymore
    // because it's scrolled out of the viewport
}
