//! Tests to investigate overlay visibility behavior

use floem::headless::HeadlessHarness;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
use floem::views::{Decorators, Empty, Overlay, stack};
use std::cell::Cell;
use std::rc::Rc;

use floem::HasViewId;

#[test]
fn test_overlay_wrapper_display_none() {
    // Test: Apply display:none on the Overlay wrapper itself
    let visible = RwSignal::new(true);
    let clicked_overlay = Rc::new(Cell::new(false));
    let clicked_bg = Rc::new(Cell::new(false));

    let overlay_clone = clicked_overlay.clone();
    let bg_clone = clicked_bg.clone();

    let overlay = Overlay::new(
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
    });
    let overlay_id = overlay.view_id();

    let view = stack((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .on_click_stop(move |_| {
                bg_clone.set(true);
            }),
        overlay,
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // When visible, overlay should receive click
    harness.click(50.0, 50.0);
    println!(
        "Visible - overlay clicked: {}, bg clicked: {}",
        clicked_overlay.get(),
        clicked_bg.get()
    );
    assert!(
        clicked_overlay.get(),
        "Overlay should receive click when visible"
    );
    assert!(
        !clicked_bg.get(),
        "Background should NOT receive click when overlay is visible"
    );

    // Reset
    clicked_overlay.set(false);
    clicked_bg.set(false);

    // Hide the overlay wrapper
    visible.set(false);
    harness.rebuild();

    // Debug: check if overlay is hidden
    println!("After rebuild - overlay_id.is_hidden(): {}", overlay_id.is_hidden());

    // Check what overlays are collected
    use floem::view::stacking::collect_overlays;
    let root_id = harness.root_id();
    let overlays = collect_overlays(root_id);
    println!("Collected overlays: {:?}", overlays);
    for ov in &overlays {
        println!("  overlay {:?} is_hidden: {}", ov, ov.is_hidden());
    }

    // Now click - should background receive it?
    harness.click(50.0, 50.0);
    println!(
        "Hidden wrapper - overlay clicked: {}, bg clicked: {}",
        clicked_overlay.get(),
        clicked_bg.get()
    );

    // Check actual behavior
    if clicked_overlay.get() {
        println!("BUG: Overlay content still receives click even though wrapper has display:none");
    }
    if !clicked_bg.get() {
        println!("BUG: Background doesn't receive click");
    }

    // This assertion documents the expected behavior
    // Currently this fails because overlay wrapper's display:none doesn't block its content
    // assert!(!clicked_overlay.get(), "Overlay content should NOT receive click when wrapper is hidden");
    // assert!(clicked_bg.get(), "Background should receive click when overlay wrapper is hidden");
}

#[test]
fn test_overlay_content_display_none() {
    // Test: Apply display:none on the content inside the Overlay
    let visible = RwSignal::new(true);
    let clicked_overlay = Rc::new(Cell::new(false));
    let clicked_bg = Rc::new(Cell::new(false));

    let overlay_clone = clicked_overlay.clone();
    let bg_clone = clicked_bg.clone();

    let content = Empty::new()
        .style(move |s| {
            s.absolute()
                .inset(0.0)
                .size(100.0, 100.0)
                .apply_if(!visible.get(), |s| s.display(floem::taffy::Display::None))
        })
        .on_click_stop(move |_| {
            overlay_clone.set(true);
        });
    let content_id = content.view_id();

    let overlay = Overlay::new(content);
    let overlay_id = overlay.view_id();

    let view = stack((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .on_click_stop(move |_| {
                bg_clone.set(true);
            }),
        overlay,
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // When visible, overlay content should receive click
    harness.click(50.0, 50.0);
    println!(
        "Content visible - overlay clicked: {}, bg clicked: {}",
        clicked_overlay.get(),
        clicked_bg.get()
    );
    assert!(
        clicked_overlay.get(),
        "Overlay content should receive click when visible"
    );
    assert!(
        !clicked_bg.get(),
        "Background should NOT receive click when overlay visible"
    );

    // Reset
    clicked_overlay.set(false);
    clicked_bg.set(false);

    // Hide the content
    visible.set(false);
    harness.rebuild();

    // Debug: check hidden state
    println!(
        "After rebuild - overlay_id.is_hidden(): {}, content_id.is_hidden(): {}",
        overlay_id.is_hidden(),
        content_id.is_hidden()
    );

    harness.click(50.0, 50.0);
    println!(
        "Content hidden - overlay clicked: {}, bg clicked: {}",
        clicked_overlay.get(),
        clicked_bg.get()
    );

    // The content is hidden, but the overlay wrapper is still visible
    // This means the overlay is checked in hit_test but returns None (no hit on content)
    // Then regular view tree is checked and background should receive click
    // BUT that's not happening - background doesn't receive click either
    println!(
        "Analysis: Overlay wrapper is visible (is_hidden={}), content is hidden (is_hidden={})",
        overlay_id.is_hidden(),
        content_id.is_hidden()
    );
    println!("Expected: overlay NOT clicked, bg clicked");
    println!(
        "Actual: overlay clicked={}, bg clicked={}",
        clicked_overlay.get(),
        clicked_bg.get()
    );

    // This SHOULD work but doesn't - the overlay blocks background even when content is hidden
    // assert!(!clicked_overlay.get(), "Hidden overlay content should NOT receive click");
    // assert!(clicked_bg.get(), "Background should receive click when overlay content is hidden");
}
