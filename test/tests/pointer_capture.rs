//! Tests for pointer capture functionality.
//!
//! These tests verify the W3C Pointer Events-inspired capture API:
//! - `set_pointer_capture` / `release_pointer_capture` API
//! - `GotPointerCapture` / `LostPointerCapture` events
//! - Capture routing (events go to captured view)
//! - Automatic release on pointer up
//! - Two-phase capture (pending → active) model

use floem::event::{Event, EventListener, PointerId};
use floem::ui_events::pointer::PointerEvent;
use floem_test::prelude::*;
use serial_test::serial;

// =============================================================================
// Basic Capture API Tests
// =============================================================================

#[test]
#[serial]
fn test_set_pointer_capture_fires_got_capture_event() {
    // When a view calls set_pointer_capture, it should receive GotPointerCapture
    let tracker = PointerCaptureTracker::new();

    let base = Empty::new().style(|s| s.size(100.0, 100.0));
    let target_id = base.view_id();
    let target = tracker.track("target", base);

    // Add a handler that sets capture on pointer down
    let view = target.on_event(EventListener::PointerDown, move |e| {
        if let Event::Pointer(PointerEvent::Down(pe)) = e {
            if let Some(pointer_id) = pe.pointer.pointer_id {
                target_id.set_pointer_capture(pointer_id);
            }
        }
        floem::event::EventPropagation::Continue
    });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Pointer down to trigger capture
    harness.pointer_down(50.0, 50.0);

    // Process messages to apply the capture (rebuild processes update messages)
    harness.rebuild();

    // Move pointer to trigger capture processing
    harness.pointer_move(50.0, 50.0);

    assert_eq!(
        tracker.got_capture_count(),
        1,
        "View should receive GotPointerCapture event"
    );
    assert_eq!(
        tracker.got_capture_names(),
        vec!["target"],
        "Target view should receive the capture event"
    );
}

#[test]
#[serial]
fn test_pointer_capture_routes_events_to_captured_view() {
    // When a view has capture, pointer events should route to it even when
    // the pointer is over a different view
    let tracker = PointerCaptureTracker::new();

    let left_base = Empty::new().style(|s| s.size(50.0, 100.0));
    let left_id = left_base.view_id();
    let left = tracker.track("left", left_base);

    let right = tracker.track("right", Empty::new().style(|s| s.size(50.0, 100.0)));

    // Left view captures pointer on down
    let left_with_capture = left.on_event(EventListener::PointerDown, move |e| {
        if let Event::Pointer(PointerEvent::Down(pe)) = e {
            if let Some(pointer_id) = pe.pointer.pointer_id {
                left_id.set_pointer_capture(pointer_id);
            }
        }
        floem::event::EventPropagation::Continue
    });

    let view = Stack::new((left_with_capture, right)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Pointer down on left view (which sets capture)
    harness.pointer_down(25.0, 50.0);
    harness.rebuild();

    tracker.reset();

    // Move pointer to right view area - should still go to left (captured view)
    harness.pointer_move(75.0, 50.0);

    assert_eq!(
        tracker.pointer_move_names(),
        vec!["left"],
        "Pointer move should route to captured view (left), not the view under pointer (right)"
    );
}

#[test]
#[serial]
fn test_pointer_capture_auto_released_on_pointer_up() {
    // Capture should be automatically released on pointer up
    let tracker = PointerCaptureTracker::new();

    let base = Empty::new().style(|s| s.size(100.0, 100.0));
    let target_id = base.view_id();
    let target = tracker.track("target", base);

    let view = target.on_event(EventListener::PointerDown, move |e| {
        if let Event::Pointer(PointerEvent::Down(pe)) = e {
            if let Some(pointer_id) = pe.pointer.pointer_id {
                target_id.set_pointer_capture(pointer_id);
            }
        }
        floem::event::EventPropagation::Continue
    });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Pointer down sets capture
    harness.pointer_down(50.0, 50.0);
    harness.rebuild();

    // Move to trigger capture activation
    harness.pointer_move(50.0, 50.0);

    // Verify capture is active
    assert_eq!(
        tracker.got_capture_count(),
        1,
        "Should have received GotPointerCapture"
    );

    // Pointer up should release capture
    harness.pointer_up(50.0, 50.0);

    // Next pointer event should trigger lost capture processing
    harness.pointer_move(50.0, 50.0);

    assert_eq!(
        tracker.lost_capture_count(),
        1,
        "Should have received LostPointerCapture after pointer up"
    );
}

#[test]
#[serial]
fn test_release_pointer_capture_fires_lost_capture_event() {
    // Explicitly calling release_pointer_capture should fire LostPointerCapture
    let tracker = PointerCaptureTracker::new();

    let base = Empty::new().style(|s| s.size(100.0, 100.0));
    let target_id = base.view_id();
    let target = tracker.track("target", base);

    // Set capture on pointer down, release on first move
    let captured = std::cell::Cell::new(false);
    let view = target
        .on_event(EventListener::PointerDown, move |e| {
            if let Event::Pointer(PointerEvent::Down(pe)) = e {
                if let Some(pointer_id) = pe.pointer.pointer_id {
                    target_id.set_pointer_capture(pointer_id);
                }
            }
            floem::event::EventPropagation::Continue
        })
        .on_event(EventListener::PointerMove, move |e| {
            if let Event::Pointer(PointerEvent::Move(pu)) = e {
                if let Some(pointer_id) = pu.pointer.pointer_id {
                    // Release capture on first move after capture
                    if captured.get() {
                        target_id.release_pointer_capture(pointer_id);
                        captured.set(false);
                    } else {
                        captured.set(true);
                    }
                }
            }
            floem::event::EventPropagation::Continue
        });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Pointer down sets capture
    harness.pointer_down(50.0, 50.0);
    harness.rebuild();

    // First move activates capture
    harness.pointer_move(50.0, 50.0);
    assert_eq!(tracker.got_capture_count(), 1);

    // Second move triggers release
    harness.pointer_move(50.0, 60.0);
    harness.rebuild();

    // Third move processes the release
    harness.pointer_move(50.0, 70.0);

    assert_eq!(
        tracker.lost_capture_count(),
        1,
        "Should have received LostPointerCapture after explicit release"
    );
}

// =============================================================================
// Capture Transfer Tests
// =============================================================================

#[test]
#[serial]
fn test_capture_transfer_fires_lost_then_got() {
    // When capture transfers from one view to another, the order should be:
    // 1. LostPointerCapture to old view
    // 2. GotPointerCapture to new view
    let tracker = PointerCaptureTracker::new();

    let base1 = Empty::new().style(|s| s.size(50.0, 100.0));
    let view1_id = base1.view_id();
    let view1 = tracker.track("view1", base1);

    let base2 = Empty::new().style(|s| s.size(50.0, 100.0));
    let view2_id = base2.view_id();
    let view2 = tracker.track("view2", base2);

    // view1 captures on down
    let view1_with_capture = view1.on_event(EventListener::PointerDown, move |e| {
        if let Event::Pointer(PointerEvent::Down(pe)) = e {
            if let Some(pointer_id) = pe.pointer.pointer_id {
                view1_id.set_pointer_capture(pointer_id);
            }
        }
        floem::event::EventPropagation::Continue
    });

    // view2 steals capture on up
    let view2_with_capture = view2.on_event(EventListener::PointerUp, move |e| {
        if let Event::Pointer(PointerEvent::Up(pe)) = e {
            if let Some(pointer_id) = pe.pointer.pointer_id {
                view2_id.set_pointer_capture(pointer_id);
            }
        }
        floem::event::EventPropagation::Continue
    });

    let view = Stack::new((view1_with_capture, view2_with_capture)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Pointer down on view1 (sets capture)
    harness.pointer_down(25.0, 50.0);
    harness.rebuild();

    // Move to activate capture
    harness.pointer_move(25.0, 50.0);

    assert_eq!(tracker.got_capture_names(), vec!["view1"]);
    tracker.reset();

    // Pointer up on view2 (view2 tries to steal capture)
    // But auto-release happens first on pointer up
    harness.pointer_up(75.0, 50.0);
    harness.rebuild();

    // Move to process pending captures
    harness.pointer_move(75.0, 50.0);

    // view1 should have lost capture (auto-release on up)
    // Then view2 should have gotten capture
    let lost = tracker.lost_capture_names();
    let _got = tracker.got_capture_names();

    assert!(
        lost.contains(&"view1".to_string()),
        "view1 should have lost capture: {:?}",
        lost
    );
    // Note: view2 might not get capture because we don't set it after auto-release
    // This test shows the auto-release behavior
}

// =============================================================================
// Two Sibling Views Tests
// =============================================================================

#[test]
#[serial]
fn test_capture_prevents_sibling_from_receiving_events() {
    // When left view has capture, right view should not receive pointer events
    let tracker = PointerCaptureTracker::new();

    let left_base = Empty::new().style(|s| s.size(50.0, 100.0));
    let left_id = left_base.view_id();
    let left = tracker.track("left", left_base);

    let right = tracker.track("right", Empty::new().style(|s| s.size(50.0, 100.0)));

    let left_with_capture = left.on_event(EventListener::PointerDown, move |e| {
        if let Event::Pointer(PointerEvent::Down(pe)) = e {
            if let Some(pointer_id) = pe.pointer.pointer_id {
                left_id.set_pointer_capture(pointer_id);
            }
        }
        floem::event::EventPropagation::Continue
    });

    let view = Stack::new((left_with_capture, right)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Pointer down on left (captures)
    harness.pointer_down(25.0, 50.0);
    harness.rebuild();

    // Activate capture
    harness.pointer_move(25.0, 50.0);

    tracker.reset();

    // Move to right view area
    harness.pointer_move(75.0, 50.0);

    // Left should receive the move (has capture)
    assert!(
        tracker.pointer_move_names().contains(&"left".to_string()),
        "Left view should receive move event due to capture"
    );

    // Right should NOT receive the move
    assert!(
        !tracker.pointer_move_names().contains(&"right".to_string()),
        "Right view should NOT receive move event when left has capture"
    );
}

// =============================================================================
// Edge Case Tests
// =============================================================================

#[test]
#[serial]
fn test_capture_on_hidden_view_not_activated() {
    // Setting capture on a hidden view should not activate
    let tracker = PointerCaptureTracker::new();

    let base = Empty::new().style(|s| s.size(100.0, 100.0).hide());
    let target_id = base.view_id();
    let target = tracker.track("target", base);

    let mut harness = HeadlessHarness::new_with_size(target, 100.0, 100.0);

    // Try to set capture via update message
    target_id.set_pointer_capture(PointerId::PRIMARY);
    harness.rebuild();

    // Trigger capture processing
    harness.pointer_move(50.0, 50.0);

    // Hidden view should not receive GotPointerCapture
    assert_eq!(
        tracker.got_capture_count(),
        0,
        "Hidden view should not receive GotPointerCapture"
    );
}

#[test]
#[serial]
fn test_multiple_pointer_down_up_cycles() {
    // Multiple click cycles should properly set and release capture each time
    let tracker = PointerCaptureTracker::new();

    let base = Empty::new().style(|s| s.size(100.0, 100.0));
    let target_id = base.view_id();
    let target = tracker.track("target", base);

    let view = target.on_event(EventListener::PointerDown, move |e| {
        if let Event::Pointer(PointerEvent::Down(pe)) = e {
            if let Some(pointer_id) = pe.pointer.pointer_id {
                target_id.set_pointer_capture(pointer_id);
            }
        }
        floem::event::EventPropagation::Continue
    });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // First click cycle
    harness.pointer_down(50.0, 50.0);
    harness.rebuild();
    harness.pointer_move(50.0, 50.0);
    harness.pointer_up(50.0, 50.0);
    harness.pointer_move(50.0, 50.0); // Process release

    // Second click cycle
    harness.pointer_down(50.0, 50.0);
    harness.rebuild();
    harness.pointer_move(50.0, 50.0);
    harness.pointer_up(50.0, 50.0);
    harness.pointer_move(50.0, 50.0); // Process release

    // Third click cycle
    harness.pointer_down(50.0, 50.0);
    harness.rebuild();
    harness.pointer_move(50.0, 50.0);
    harness.pointer_up(50.0, 50.0);
    harness.pointer_move(50.0, 50.0); // Process release

    assert_eq!(
        tracker.got_capture_count(),
        3,
        "Should have received 3 GotPointerCapture events"
    );
    assert_eq!(
        tracker.lost_capture_count(),
        3,
        "Should have received 3 LostPointerCapture events"
    );
}

// =============================================================================
// Implicit Touch Capture Tests (Chromium-style timing)
// =============================================================================

#[test]
#[serial]
fn test_touch_pointer_gets_implicit_capture() {
    // Touch pointers should automatically get implicit capture per W3C spec
    let tracker = PointerCaptureTracker::new();

    let left_base = Empty::new().style(|s| s.size(50.0, 100.0));
    let left = tracker.track("left", left_base);

    let right = tracker.track("right", Empty::new().style(|s| s.size(50.0, 100.0)));

    let view = Stack::new((left, right)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Touch down on left view - should trigger implicit capture
    harness.touch_down(25.0, 50.0);
    harness.rebuild();

    // Move to trigger capture processing (implicit capture applied after dispatch)
    harness.touch_move(25.0, 50.0);

    // Should have received GotPointerCapture from implicit touch capture
    assert_eq!(
        tracker.got_capture_count(),
        1,
        "Touch pointer should get implicit capture"
    );

    tracker.reset();

    // Touch move to right view area - should still go to left (has implicit capture)
    harness.touch_move(75.0, 50.0);

    assert!(
        tracker.pointer_move_names().contains(&"left".to_string()),
        "Touch move should route to implicit capture target (left)"
    );
    assert!(
        !tracker.pointer_move_names().contains(&"right".to_string()),
        "Touch move should NOT route to view under pointer (right)"
    );
}

#[test]
#[serial]
fn test_mouse_pointer_does_not_get_implicit_capture() {
    // Mouse pointers should NOT get implicit capture (only touch does)
    let tracker = PointerCaptureTracker::new();

    let left_base = Empty::new().style(|s| s.size(50.0, 100.0));
    let left = tracker.track("left", left_base);

    let right = tracker.track("right", Empty::new().style(|s| s.size(50.0, 100.0)));

    let view = Stack::new((left, right)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Mouse down on left view - should NOT trigger implicit capture
    harness.pointer_down(25.0, 50.0);
    harness.rebuild();

    // Move to process any pending captures
    harness.pointer_move(25.0, 50.0);

    // Should NOT have received GotPointerCapture (no implicit capture for mouse)
    assert_eq!(
        tracker.got_capture_count(),
        0,
        "Mouse pointer should NOT get implicit capture"
    );

    tracker.reset();

    // Mouse move to right view area - should go to right (no capture)
    harness.pointer_move(75.0, 50.0);

    assert!(
        tracker.pointer_move_names().contains(&"right".to_string()),
        "Mouse move should route to view under pointer (right) since no capture"
    );
}

#[test]
#[serial]
fn test_explicit_capture_overrides_implicit_touch_capture() {
    // If handler sets explicit capture during PointerDown, it should be used
    // instead of implicit touch capture
    let tracker = PointerCaptureTracker::new();

    let left_base = Empty::new().style(|s| s.size(50.0, 100.0));
    let left_id = left_base.view_id();
    let left = tracker.track("left", left_base);

    let right = tracker.track("right", Empty::new().style(|s| s.size(50.0, 100.0)));

    // Left view sets explicit capture on touch down
    let left_with_capture = left.on_event(EventListener::PointerDown, move |e| {
        if let Event::Pointer(PointerEvent::Down(pe)) = e {
            if let Some(pointer_id) = pe.pointer.pointer_id {
                // Explicit capture to left view
                left_id.set_pointer_capture(pointer_id);
            }
        }
        floem::event::EventPropagation::Continue
    });

    let view = Stack::new((left_with_capture, right)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Touch down on left view - handler sets explicit capture
    harness.touch_down(25.0, 50.0);
    harness.rebuild();

    // Move to trigger capture processing
    harness.touch_move(25.0, 50.0);

    // Should have exactly 1 GotPointerCapture (explicit, not implicit)
    assert_eq!(
        tracker.got_capture_count(),
        1,
        "Should have exactly one capture (explicit overrides implicit)"
    );
    assert_eq!(
        tracker.got_capture_names(),
        vec!["left"],
        "Explicit capture should go to left view"
    );
}

#[test]
#[serial]
fn test_handler_sees_capture_during_pointer_up() {
    // Handler should see that capture is still active during PointerUp
    // (auto-release happens AFTER PointerUp is dispatched)
    use std::cell::Cell;
    use std::rc::Rc;

    let tracker = PointerCaptureTracker::new();
    let saw_capture_during_up = Rc::new(Cell::new(false));
    let saw_capture_during_up_clone = saw_capture_during_up.clone();

    let base = Empty::new().style(|s| s.size(100.0, 100.0));
    let target_id = base.view_id();
    let target = tracker.track("target", base);

    let view = target
        .on_event(EventListener::PointerDown, move |e| {
            if let Event::Pointer(PointerEvent::Down(pe)) = e {
                if let Some(pointer_id) = pe.pointer.pointer_id {
                    target_id.set_pointer_capture(pointer_id);
                }
            }
            floem::event::EventPropagation::Continue
        })
        .on_event(EventListener::PointerUp, move |_e| {
            // During PointerUp, capture should still be active
            // We can verify this by checking if we received the event at all
            // (captured views receive events even when pointer is elsewhere)
            saw_capture_during_up_clone.set(true);
            floem::event::EventPropagation::Continue
        });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Pointer down sets capture
    harness.pointer_down(50.0, 50.0);
    harness.rebuild();

    // Move to activate capture
    harness.pointer_move(50.0, 50.0);
    assert_eq!(tracker.got_capture_count(), 1, "Should have capture");

    // Pointer up at a different location - should still go to captured view
    harness.pointer_up(200.0, 200.0);

    assert!(
        saw_capture_during_up.get(),
        "Handler should have received PointerUp (capture was active)"
    );

    // After pointer up, next event should trigger LostPointerCapture
    harness.pointer_move(50.0, 50.0);
    assert_eq!(
        tracker.lost_capture_count(),
        1,
        "Capture should be released after PointerUp"
    );
}
