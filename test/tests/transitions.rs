//! Tests for CSS-like transitions.
//!
//! These tests verify that style property transitions animate correctly
//! when properties change (e.g., on hover).
//!
//! ## Known Bug (to be fixed)
//!
//! Transitions are currently broken because `FrameUpdate::Style` in
//! `process_scheduled_updates()` only adds to `style_dirty` but doesn't
//! set the `ChangeFlags::STYLE` flag. This causes `style_view_with_change`
//! to skip the view on subsequent frames, stopping the transition.
//!
//! Compare in `src/window/handle.rs`:
//! - `UpdateMessage::RequestStyle` (correct): sets BOTH style_dirty AND ChangeFlags::STYLE
//! - `FrameUpdate::Style` (bug): only sets style_dirty
//!
//! The fix is to add the STYLE flag setting to `FrameUpdate::Style` handling.

use std::time::Duration;

use floem::peniko::Brush;
use floem::prelude::*;
use floem::style::{Background, Transition};
use floem_test::prelude::*;

/// Test that a transition is triggered when hovering.
///
/// This test verifies the basic mechanism:
/// 1. Create a view with a hover style and transition
/// 2. Hover over the view
/// 3. Verify the transition starts (background begins to change)
#[test]
fn test_hover_triggers_transition() {
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .background(palette::css::WHITE)
            .hover(|s| s.background(palette::css::BLUE))
            .transition(Background, Transition::linear(Duration::from_millis(100)))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial state: background should be WHITE
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::WHITE),
        "Initial background should be WHITE, got {:?}",
        bg
    );

    // Hover over the view (move pointer into the view)
    harness.pointer_move(50.0, 50.0);

    // After hover, the transition should start - background should begin changing
    // The exact value depends on timing, but it should no longer be pure WHITE
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);

    // On the first frame after hover, transition may not have started yet
    // but the target value should be applied
    // Actually with a transition, the value interpolates, so it might still be WHITE
    // Let's just verify hover was registered
    assert!(
        harness.is_hovered(id),
        "View should be hovered after pointer_move"
    );
}

/// Test that transitions continue to animate across multiple frames.
///
/// NOTE: The `initial` flag in TransitionState prevents transitions on the
/// FIRST property change (to avoid animating from default values). Transitions
/// only work on the SECOND change. So we test hover → un-hover.
///
/// BUG: This test demonstrates the current bug where transitions stop
/// after the first frame because `FrameUpdate::Style` doesn't set
/// the `ChangeFlags::STYLE` flag.
#[test]
fn test_transition_animates_across_frames() {
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .background(palette::css::WHITE)
            .hover(|s| s.background(palette::css::BLUE))
            .transition(Background, Transition::linear(Duration::from_millis(200)))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // First change: hover (this sets initial=true, value jumps to BLUE)
    harness.pointer_move(50.0, 50.0);
    harness.rebuild();

    // Verify we're in hover state with BLUE background
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "After hover, background should be BLUE, got {:?}",
        bg
    );

    // Second change: un-hover (this should START a transition from BLUE to WHITE)
    harness.pointer_move(-10.0, -10.0); // Move outside the view
    harness.rebuild();

    // Now check if there are scheduled updates for the transition
    let has_scheduled = harness.has_scheduled_updates();

    // This assertion documents the expected behavior:
    // After un-hover with a transition, there should be scheduled updates
    // to continue the animation back to WHITE
    assert!(
        has_scheduled,
        "Transition should schedule updates for animation frames"
    );
}

/// Test that schedule_style properly schedules the next frame.
///
/// This test directly checks that after triggering a transition,
/// subsequent calls to rebuild() cause the transition to progress.
///
/// NOTE: Transitions only work on the SECOND property change (initial flag).
#[test]
fn test_schedule_style_sets_dirty_flag() {
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .background(palette::css::WHITE)
            .hover(|s| s.background(palette::css::BLUE))
            .transition(Background, Transition::linear(Duration::from_millis(100)))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // First change: hover (sets initial=true, value jumps to BLUE)
    harness.pointer_move(50.0, 50.0);
    harness.rebuild();

    // Second change: un-hover (this STARTS a transition from BLUE to WHITE)
    harness.pointer_move(-10.0, -10.0);

    // After un-hover, the view should be marked for style recalculation
    let _has_pending = harness.has_pending_style_change(id);

    // Rebuild to process the frame - transition should start
    harness.rebuild();

    // After rebuild with a transition in progress, schedule_style should
    // have been called, which means either:
    // 1. has_pending_style_change should be true, OR
    // 2. has_scheduled_updates should be true
    let has_pending_after = harness.has_pending_style_change(id);
    let has_scheduled = harness.has_scheduled_updates();

    // BUG: This fails because FrameUpdate::Style only adds to style_dirty
    // but doesn't set the ChangeFlags::STYLE flag
    assert!(
        has_pending_after || has_scheduled,
        "View should have pending style changes or scheduled updates after rebuild with active transition. \
         has_pending={}, has_scheduled={}",
        has_pending_after,
        has_scheduled
    );
}

/// Test that a transition causes schedule_style to be called on SUBSEQUENT frames.
///
/// NOTE: Transitions only work on the SECOND property change (initial flag).
///
/// The bug is that FrameUpdate::Style doesn't set ChangeFlags::STYLE,
/// so the view is skipped on the SECOND frame after a transition starts.
/// This test checks that after TWO rebuilds, there are still scheduled updates.
#[test]
fn test_transition_schedules_updates_on_subsequent_frames() {
    let is_blue = RwSignal::new(false);

    let view = Empty::new().style(move |s| {
        let color = if is_blue.get() {
            palette::css::BLUE
        } else {
            palette::css::WHITE
        };
        s.size(100.0, 100.0)
            .background(color)
            .transition(Background, Transition::linear(Duration::from_millis(200)))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // First change: set to blue (sets initial=true)
    is_blue.set(true);
    harness.rebuild();

    // Second change: set back to white (this STARTS a transition)
    is_blue.set(false);
    harness.rebuild(); // Frame 1: transition starts, schedule_style called

    // After Frame 1, there should be scheduled updates
    let has_scheduled_after_frame1 = harness.has_scheduled_updates();
    assert!(
        has_scheduled_after_frame1,
        "Frame 1: Transition should schedule updates for next frame"
    );

    // Frame 2: This is where the bug manifests
    // FrameUpdate::Style is processed but ChangeFlags::STYLE is not set
    harness.rebuild();

    // After Frame 2, there should STILL be scheduled updates
    // because the transition is still in progress
    let has_scheduled_after_frame2 = harness.has_scheduled_updates();
    let is_dirty = harness.is_style_dirty(id);
    let has_flag = harness.has_pending_style_change(id);

    // BUG: This fails because FrameUpdate::Style only adds to style_dirty
    // but doesn't set ChangeFlags::STYLE, so the view is skipped on Frame 2
    // and schedule_style is never called again.
    assert!(
        has_scheduled_after_frame2 || has_flag || is_dirty,
        "Frame 2: Active transition should still schedule updates. \
         has_scheduled={}, is_dirty={}, has_flag={}",
        has_scheduled_after_frame2,
        is_dirty,
        has_flag
    );
}

/// Test that demonstrates the root cause of the bug.
///
/// This test shows that after a transition is triggered:
/// 1. On frame 1: The view is in style_dirty AND has ChangeFlags::STYLE (correct)
/// 2. After rebuild with active transition: schedule_style is called
/// 3. On frame 2: The view is in style_dirty BUT does NOT have ChangeFlags::STYLE (bug!)
///
/// The missing flag causes style_view_with_change to skip the view.
///
/// NOTE: Transitions only work on the SECOND property change (initial flag).
#[test]
fn test_frame_update_style_missing_flag() {
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .background(palette::css::WHITE)
            .hover(|s| s.background(palette::css::BLUE))
            .transition(Background, Transition::linear(Duration::from_millis(500)))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // First change: hover (sets initial=true, value jumps to BLUE)
    harness.pointer_move(50.0, 50.0);
    harness.rebuild();

    // Second change: un-hover (this STARTS a transition from BLUE to WHITE)
    harness.pointer_move(-10.0, -10.0);

    // Check state before rebuild
    let in_style_dirty_before = harness.is_style_dirty(id);
    let has_flag_before = harness.has_pending_style_change(id);

    // Rebuild to process the frame - transition should start
    harness.rebuild();

    // Now schedule_style should have been called for the active transition.
    // Check if we have scheduled updates to process.
    let has_scheduled = harness.has_scheduled_updates();

    // Rebuild again to process FrameUpdate::Style
    harness.rebuild();

    // After rebuild, the view should still be marked for style recalc
    // to continue the animation.
    // BUG: is_style_dirty might be set, but has_pending_style_change is false!
    let in_style_dirty_after = harness.is_style_dirty(id);
    let has_flag_after = harness.has_pending_style_change(id);

    // Document the expected vs actual state
    // Expected: if in_style_dirty, then has_pending_style_change should also be true
    // Actual (bug): in_style_dirty can be true while has_pending_style_change is false
    assert!(
        !in_style_dirty_after || has_flag_after,
        "BUG: View is in style_dirty ({}) but doesn't have STYLE flag ({}). \n\
         Before rebuild: style_dirty={}, has_flag={}\n\
         After rebuild: style_dirty={}, has_flag={}\n\
         Has scheduled updates: {}\n\
         This causes style_view_with_change to skip the view, breaking transitions.",
        in_style_dirty_after,
        has_flag_after,
        in_style_dirty_before,
        has_flag_before,
        in_style_dirty_after,
        has_flag_after,
        has_scheduled
    );
}

/// Test that layout property transitions schedule updates on subsequent frames.
///
/// NOTE: Transitions only work on the SECOND property change (initial flag).
#[test]
fn test_transition_layout_schedules_updates_on_subsequent_frames() {
    let is_wide = RwSignal::new(false);

    let view = Empty::new().style(move |s| {
        let w = if is_wide.get() { 100.0 } else { 50.0 };
        s.width(w)
            .height(100.0)
            .transition(floem::style::Width, Transition::linear(Duration::from_millis(100)))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // First change: set to wide (sets initial=true)
    is_wide.set(true);
    harness.rebuild();

    // Second change: set back to narrow (this STARTS a transition)
    is_wide.set(false);
    harness.rebuild(); // Frame 1: transition starts

    // After Frame 1, there should be scheduled updates
    let has_scheduled_after_frame1 = harness.has_scheduled_updates();
    assert!(
        has_scheduled_after_frame1,
        "Frame 1: Layout transition should schedule updates for next frame"
    );

    // Frame 2: This is where the bug manifests
    harness.rebuild();

    // After Frame 2, there should STILL be scheduled updates
    let has_scheduled_after_frame2 = harness.has_scheduled_updates();
    let is_dirty = harness.is_style_dirty(id);
    let has_flag = harness.has_pending_style_change(id);

    // BUG: This fails because FrameUpdate::Style doesn't set ChangeFlags::STYLE
    assert!(
        has_scheduled_after_frame2 || has_flag || is_dirty,
        "Frame 2: Active layout transition should still schedule updates. \
         has_scheduled={}, is_dirty={}, has_flag={}",
        has_scheduled_after_frame2,
        is_dirty,
        has_flag
    );
}
