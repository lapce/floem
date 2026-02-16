//! Tests for keyboard-triggered interactions.
//!
//! These tests verify that:
//! - Focus styles are applied correctly
//! - Active styles work with pointer events
//! - Container/child interaction works correctly

use floem::event::Event;
use floem::peniko::Brush;
use floem::prelude::*;
use floem::style::{Background, StyleSelector};
use floem_test::prelude::*;
use ui_events::keyboard::{Code, Key, KeyState, KeyboardEvent, Location, Modifiers};

/// Test that focused view shows focus style.
#[test]
fn test_focus_style_applied_when_focused() {
    let root = TestRoot::new();
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .focus(true)
            .background(palette::css::BLUE)
            .focus(|s| s.background(palette::css::YELLOW))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    // Initially not focused
    assert!(!harness.is_focused(id), "Should not be focused initially");

    // Click to focus
    harness.click(50.0, 50.0);

    // Should now be focused
    assert!(harness.is_focused(id), "Should be focused after click");

    // Check focus style is applied
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::YELLOW),
        "Focus style should be applied, got {:?}",
        bg
    );
}

/// Test that view has focus_visible selector.
#[test]
fn test_focus_visible_selector_detected() {
    let root = TestRoot::new();
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .focus(true)
            .background(palette::css::BLUE)
            .focus_visible(|s| s.background(palette::css::ORANGE))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    // Check that focus_visible selector is detected
    assert!(
        harness.has_style_for_selector(id, StyleSelector::FocusVisible),
        "View should have FocusVisible selector"
    );
}

/// Test container with child - clicking child focuses container if focusable.
#[test]
fn test_container_child_click_interaction() {
    let root = TestRoot::new();
    let tracker = ClickTracker::new();

    let child = tracker
        .track_named("child", Empty::new())
        .style(|s| s.size(50.0, 50.0).background(palette::css::RED));

    let container = Container::new(child).style(|s| {
        s.size(100.0, 100.0)
            .background(palette::css::BLUE)
            .active(|s| s.background(palette::css::DARK_BLUE))
    });
    let container_id = container.view_id();

    let mut harness = HeadlessHarness::new_with_size(root, container, 100.0, 100.0);

    // Click on the child area
    harness.pointer_down(25.0, 25.0);

    // Container should be in clicking state
    assert!(
        harness.is_active(container_id),
        "Container should be clicking when child is clicked"
    );

    // Container should have active style
    let style = harness.get_computed_style(container_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::DARK_BLUE),
        "Container should have active style when clicking, got {:?}",
        bg
    );

    // Release
    harness.pointer_up(25.0, 25.0);

    // Child should have received click
    assert!(
        tracker.was_clicked(),
        "Child should have received click event"
    );
}

/// Test that active style is removed after pointer up.
#[test]
fn test_active_style_removed_after_pointer_up() {
    let root = TestRoot::new();
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .background(palette::css::BLUE)
            .active(|s| s.background(palette::css::RED))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    // Initial: BLUE
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE));

    // Pointer down: should be RED (active)
    harness.pointer_down(50.0, 50.0);
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::RED),
        "Should be RED during click"
    );

    // Pointer up: should be BLUE again
    harness.pointer_up(50.0, 50.0);
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Should be BLUE after pointer up, got {:?}",
        bg
    );
}

/// Test multiple focusable views - only one should be focused at a time.
#[test]
fn test_only_one_view_focused_at_time() {
    let root = TestRoot::new();
    let view1 = Empty::new().style(|s| s.size(50.0, 50.0).focus(true));
    let id1 = view1.view_id();

    let view2 = Empty::new().style(|s| s.size(50.0, 50.0).focus(true));
    let id2 = view2.view_id();

    let view = Stack::new((view1, view2)).style(|s| s.size(100.0, 50.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 50.0);

    // Click first view
    harness.click(25.0, 25.0);
    assert!(harness.is_focused(id1), "View 1 should be focused");
    assert!(!harness.is_focused(id2), "View 2 should not be focused");

    // Click second view
    harness.click(75.0, 25.0);
    assert!(
        !harness.is_focused(id1),
        "View 1 should no longer be focused"
    );
    assert!(harness.is_focused(id2), "View 2 should now be focused");
}

/// Test that repeated Enter key presses emit Click events (key repeat).
#[test]
fn test_repeated_enter_emits_click() {
    let root = TestRoot::new();
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named("button", Empty::new())
        .style(|s| s.size(100.0, 100.0).focus(true));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    // Focus the view first by clicking it
    harness.click(50.0, 50.0);

    // Reset tracker to clear the click from focusing
    tracker.reset();

    // Create a key down event for Enter (initial press)
    let key_down = Event::Key(KeyboardEvent {
        key: Key::Named(ui_events::keyboard::NamedKey::Enter),
        code: Code::Enter,
        modifiers: Modifiers::default(),
        location: Location::Standard,
        is_composing: false,
        repeat: false,
        state: KeyState::Down,
    });

    // Initial key down shouldn't emit click
    harness.dispatch_event(key_down);
    assert_eq!(
        tracker.click_count(),
        0,
        "Initial key down should not emit click"
    );

    // Create a key up event for Enter (release)
    let key_up = Event::Key(KeyboardEvent {
        key: Key::Named(ui_events::keyboard::NamedKey::Enter),
        code: Code::Enter,
        modifiers: Modifiers::default(),
        location: Location::Standard,
        is_composing: false,
        repeat: false,
        state: KeyState::Up,
    });

    // Key up should emit click
    harness.dispatch_event(key_up);
    assert_eq!(tracker.click_count(), 1, "Key up should emit click event");

    // Now test key repeat - simulate holding the key down
    // Key repeat events have state: KeyState::Down and repeat: true
    let key_repeat = Event::Key(KeyboardEvent {
        key: Key::Named(ui_events::keyboard::NamedKey::Enter),
        code: Code::Enter,
        modifiers: Modifiers::default(),
        location: Location::Standard,
        is_composing: false,
        repeat: true, // This is the key difference - repeat flag is true
        state: KeyState::Down,
    });

    // Send first repeat event
    harness.dispatch_event(key_repeat.clone());
    assert_eq!(
        tracker.click_count(),
        2,
        "First key repeat should emit click event"
    );

    // Send second repeat event
    harness.dispatch_event(key_repeat.clone());
    assert_eq!(
        tracker.click_count(),
        3,
        "Second key repeat should emit click event"
    );

    // Send third repeat event
    harness.dispatch_event(key_repeat);
    assert_eq!(
        tracker.click_count(),
        4,
        "Third key repeat should emit click event"
    );
}

/// Test that Space key repeat also emits Click events.
#[test]
fn test_repeated_space_emits_click() {
    let root = TestRoot::new();
    let tracker = ClickTracker::new();

    let view = tracker
        .track_named("button", Empty::new())
        .style(|s| s.size(100.0, 100.0).focus(true));

    let mut harness = HeadlessHarness::new_with_size(root, view, 100.0, 100.0);

    // Focus the view first
    harness.click(50.0, 50.0);
    tracker.reset();

    // Space key repeat
    let space_repeat = Event::Key(KeyboardEvent {
        key: Key::Character(" ".to_string()),
        code: Code::Space,
        modifiers: Modifiers::default(),
        location: Location::Standard,
        is_composing: false,
        repeat: true,
        state: KeyState::Up,
    });

    // Send repeat event
    harness.dispatch_event(space_repeat);
    assert_eq!(
        tracker.click_count(),
        1,
        "Space key repeat should emit click event"
    );
}
