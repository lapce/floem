//! Tests for overlay visibility behavior.
//!
//! These tests verify that overlays correctly handle visibility changes:
//! - When an overlay wrapper has display:none, its content should not receive events
//! - When overlay content has display:none, background views should receive events

use floem::headless::HeadlessHarness;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
use floem::views::{Decorators, Empty, Overlay, stack};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn test_overlay_wrapper_display_none() {
    // Test: When the Overlay wrapper has display:none, its content should not receive clicks
    // and the background should receive them instead.
    let visible = RwSignal::new(true);
    let clicked_overlay = Rc::new(Cell::new(false));
    let clicked_bg = Rc::new(Cell::new(false));

    let overlay_clone = clicked_overlay.clone();
    let bg_clone = clicked_bg.clone();

    let view = stack((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .on_click_stop(move |_| {
                bg_clone.set(true);
            }),
        Overlay::new(
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
                .on_click_stop(move |_| {
                    overlay_clone.set(true);
                }),
        )
        .style(move |s| {
            s.apply_if(!visible.get(), |s| {
                s.display(floem::taffy::Display::None)
            })
        }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // When visible, overlay should receive click
    harness.click(50.0, 50.0);
    assert!(
        clicked_overlay.get(),
        "Overlay should receive click when visible"
    );
    assert!(
        !clicked_bg.get(),
        "Background should NOT receive click when overlay is visible"
    );

    // Reset and hide the overlay
    clicked_overlay.set(false);
    clicked_bg.set(false);
    visible.set(false);
    harness.rebuild();

    // Background should now receive the click
    harness.click(50.0, 50.0);
    assert!(
        !clicked_overlay.get(),
        "Hidden overlay content should NOT receive click"
    );
    assert!(
        clicked_bg.get(),
        "Background should receive click when overlay is hidden"
    );
}

#[test]
fn test_overlay_content_display_none() {
    // Test: When the content inside an Overlay has display:none,
    // the background should receive clicks instead.
    let visible = RwSignal::new(true);
    let clicked_overlay = Rc::new(Cell::new(false));
    let clicked_bg = Rc::new(Cell::new(false));

    let overlay_clone = clicked_overlay.clone();
    let bg_clone = clicked_bg.clone();

    let view = stack((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .on_click_stop(move |_| {
                bg_clone.set(true);
            }),
        Overlay::new(
            Empty::new()
                .style(move |s| {
                    s.absolute()
                        .inset(0.0)
                        .size(100.0, 100.0)
                        .apply_if(!visible.get(), |s| s.display(floem::taffy::Display::None))
                })
                .on_click_stop(move |_| {
                    overlay_clone.set(true);
                }),
        ),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // When visible, overlay content should receive click
    harness.click(50.0, 50.0);
    assert!(
        clicked_overlay.get(),
        "Overlay content should receive click when visible"
    );
    assert!(
        !clicked_bg.get(),
        "Background should NOT receive click when overlay visible"
    );

    // Reset and hide the content
    clicked_overlay.set(false);
    clicked_bg.set(false);
    visible.set(false);
    harness.rebuild();

    // Background should now receive the click
    harness.click(50.0, 50.0);
    assert!(
        !clicked_overlay.get(),
        "Hidden overlay content should NOT receive click"
    );
    assert!(
        clicked_bg.get(),
        "Background should receive click when overlay content is hidden"
    );
}
