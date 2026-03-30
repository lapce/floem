//! Tests for window close request interception via `handle_default_behaviors`.
//!
//! These tests verify that:
//! - An unhandled `CloseRequested` event enqueues a `CloseWindow` app update event
//! - A handler calling `cx.prevent_default()` prevents the close
//! - `on_event_stop` (stop propagation only) does NOT prevent the close
//! - Close prevention on one window does not affect another

use floem::event::{Event, WindowEvent};
use floem::take_close_window_event_count;
use floem_test::prelude::*;
use serial_test::serial;

#[test]
#[serial]
fn close_requested_defaults_to_closing() {
    let root = TestRoot::new();
    let view = Empty::new().style(|s| s.size(100.0, 100.0));
    let mut harness = HeadlessHarness::new_with_size(root, view, 400.0, 300.0);

    let wid = harness.window_id();
    // Clear any stale events
    take_close_window_event_count(wid);

    harness.dispatch_event(Event::Window(WindowEvent::CloseRequested));

    assert_eq!(
        take_close_window_event_count(wid),
        1,
        "Unhandled CloseRequested should enqueue a CloseWindow event"
    );
}

#[test]
#[serial]
fn close_requested_can_be_prevented() {
    let root = TestRoot::new();
    let view = Empty::new().style(|s| s.size(100.0, 100.0)).on_event_cont(
        floem::event::listener::WindowCloseRequested,
        |cx, _| {
            cx.prevent_default();
        },
    );
    let mut harness = HeadlessHarness::new_with_size(root, view, 400.0, 300.0);

    let wid = harness.window_id();
    take_close_window_event_count(wid);

    harness.dispatch_event(Event::Window(WindowEvent::CloseRequested));

    assert_eq!(
        take_close_window_event_count(wid),
        0,
        "prevent_default() should block CloseWindow from being enqueued"
    );
}

#[test]
#[serial]
fn stop_propagation_without_prevent_default_still_closes() {
    let root = TestRoot::new();
    let view = Empty::new()
        .style(|s| s.size(100.0, 100.0))
        .on_event_stop(floem::event::listener::WindowCloseRequested, |_cx, _| {});
    let mut harness = HeadlessHarness::new_with_size(root, view, 400.0, 300.0);

    let wid = harness.window_id();
    take_close_window_event_count(wid);

    harness.dispatch_event(Event::Window(WindowEvent::CloseRequested));

    assert_eq!(
        take_close_window_event_count(wid),
        1,
        "on_event_stop without prevent_default should still enqueue CloseWindow"
    );
}

#[test]
#[serial]
fn preventing_one_window_close_does_not_affect_another() {
    // Window 1: has prevent_default handler
    let root1 = TestRoot::new();
    let view1 = Empty::new().style(|s| s.size(100.0, 100.0)).on_event_cont(
        floem::event::listener::WindowCloseRequested,
        |cx, _| {
            cx.prevent_default();
        },
    );
    let mut harness1 = HeadlessHarness::new_with_size(root1, view1, 400.0, 300.0);
    let wid1 = harness1.window_id();

    // Window 2: no handler
    let root2 = TestRoot::new();
    let view2 = Empty::new().style(|s| s.size(100.0, 100.0));
    let mut harness2 = HeadlessHarness::new_with_size(root2, view2, 400.0, 300.0);
    let wid2 = harness2.window_id();

    // Clear stale events
    take_close_window_event_count(wid1);
    take_close_window_event_count(wid2);

    // Close request on prevented window
    harness1.dispatch_event(Event::Window(WindowEvent::CloseRequested));
    assert_eq!(
        take_close_window_event_count(wid1),
        0,
        "Prevented window should not enqueue CloseWindow"
    );

    // Close request on plain window
    harness2.dispatch_event(Event::Window(WindowEvent::CloseRequested));
    assert_eq!(
        take_close_window_event_count(wid2),
        1,
        "Plain window should enqueue CloseWindow"
    );
}
