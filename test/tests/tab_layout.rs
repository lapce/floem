//! Tests for Tab view layout behavior.
//!
//! These tests verify that:
//! - Inactive tabs are hidden and don't participate in layout
//! - Only the active tab contributes to the Tab container's size
//! - Switching tabs updates layout correctly

use serial_test::serial;
use std::cell::RefCell;
use std::rc::Rc;

use floem::prelude::*;
use floem::views::tab;
use floem_test::prelude::*;

// =============================================================================
// Basic Tab Layout Tests
// =============================================================================

/// Test that inactive tabs have zero size in layout.
/// This was the original bug: hidden tabs still had full size.
#[test]
#[serial]
fn test_inactive_tabs_have_zero_size() {
    let active_tab = RwSignal::new(Some(0usize));
    let tabs = vec![0, 1, 2];

    let child_ids: Rc<RefCell<Vec<ViewId>>> = Rc::new(RefCell::new(Vec::new()));
    let child_ids_clone = child_ids.clone();

    // Each tab has a different height
    let tab_view = tab(
        move || active_tab.get(),
        move || tabs.clone(),
        |t| *t,
        move |t| {
            let height = match t {
                0 => 100.0,
                1 => 200.0,
                2 => 300.0,
                _ => 50.0,
            };
            let view = Empty::new().style(move |s| s.width(100.0).height(height));
            child_ids_clone.borrow_mut().push(view.view_id());
            view
        },
    );

    let mut harness = HeadlessHarness::new_with_size(tab_view, 200.0, 400.0);
    harness.rebuild();

    let ids = child_ids.borrow();
    assert!(ids.len() >= 3, "Should have 3 tab children");

    // Active tab (index 0) should have proper size
    let tab0_layout = ids[0].get_layout().expect("Tab 0 should have layout");
    assert!(
        (tab0_layout.size.height - 100.0).abs() < 0.1,
        "Active tab (0) height should be 100.0, got {}",
        tab0_layout.size.height
    );

    // Inactive tabs should have 0 size (display: none)
    let tab1_layout = ids[1].get_layout().expect("Tab 1 should have layout");
    assert!(
        tab1_layout.size.height < 0.1,
        "Inactive tab (1) height should be 0, got {}",
        tab1_layout.size.height
    );

    let tab2_layout = ids[2].get_layout().expect("Tab 2 should have layout");
    assert!(
        tab2_layout.size.height < 0.1,
        "Inactive tab (2) height should be 0, got {}",
        tab2_layout.size.height
    );
}

/// Test that switching tabs changes which tab has layout.
#[test]
#[serial]
fn test_switching_tabs_updates_child_sizes() {
    let active_tab = RwSignal::new(Some(0usize));
    let tabs = vec![0, 1];

    let child_ids: Rc<RefCell<Vec<ViewId>>> = Rc::new(RefCell::new(Vec::new()));
    let child_ids_clone = child_ids.clone();

    let tab_view = tab(
        move || active_tab.get(),
        move || tabs.clone(),
        |t| *t,
        move |t| {
            let view = Empty::new()
                .style(move |s| s.width(100.0).height(if t == 0 { 80.0 } else { 120.0 }));
            child_ids_clone.borrow_mut().push(view.view_id());
            view
        },
    );

    let mut harness = HeadlessHarness::new_with_size(tab_view, 200.0, 200.0);
    harness.rebuild();

    let ids = child_ids.borrow();

    // Initial: tab 0 is active (has size), tab 1 is hidden (no size)
    let tab0_layout = ids[0].get_layout().expect("Tab 0 should have layout");
    let tab1_layout = ids[1].get_layout().expect("Tab 1 should have layout");

    assert!(
        (tab0_layout.size.height - 80.0).abs() < 0.1,
        "Initially, tab 0 height should be 80.0, got {}",
        tab0_layout.size.height
    );
    assert!(
        tab1_layout.size.height < 0.1,
        "Initially, tab 1 height should be 0 (hidden), got {}",
        tab1_layout.size.height
    );

    // Switch to tab 1
    drop(ids); // Release the borrow before modifying signal
    active_tab.set(Some(1));
    harness.rebuild();

    // Now tab 1 should have size, tab 0 should be hidden
    let ids = child_ids.borrow();
    let tab0_layout = ids[0].get_layout().expect("Tab 0 should have layout");
    let tab1_layout = ids[1].get_layout().expect("Tab 1 should have layout");

    assert!(
        tab0_layout.size.height < 0.1,
        "After switch, tab 0 height should be 0 (hidden), got {}",
        tab0_layout.size.height
    );
    assert!(
        (tab1_layout.size.height - 120.0).abs() < 0.1,
        "After switch, tab 1 height should be 120.0, got {}",
        tab1_layout.size.height
    );
}

/// Test that Tab with no active tab hides all children.
#[test]
#[serial]
fn test_no_active_tab_all_hidden() {
    let active_tab = RwSignal::new(None::<usize>);
    let tabs = vec![0, 1];

    let child_ids: Rc<RefCell<Vec<ViewId>>> = Rc::new(RefCell::new(Vec::new()));
    let child_ids_clone = child_ids.clone();

    let tab_view = tab(
        move || active_tab.get(),
        move || tabs.clone(),
        |t| *t,
        move |_| {
            let view = Empty::new().style(|s| s.size(100.0, 100.0));
            child_ids_clone.borrow_mut().push(view.view_id());
            view
        },
    );

    let mut harness = HeadlessHarness::new_with_size(tab_view, 200.0, 200.0);
    harness.rebuild();

    let ids = child_ids.borrow();

    // All tabs should be hidden (0 size)
    for (i, id) in ids.iter().enumerate() {
        let layout = id.get_layout().expect("Tab should have layout");
        assert!(
            layout.size.height < 0.1,
            "Tab {} should be hidden (0 height), got {}",
            i,
            layout.size.height
        );
    }
}

// =============================================================================
// ViewId set_hidden/set_visible Tests
// =============================================================================

/// Test that set_hidden() makes a view not participate in layout.
#[test]
#[serial]
fn test_set_hidden_removes_from_layout() {
    let child1 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child1_id = child1.view_id();
    let child2 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child2_id = child2.view_id();

    let container = Stack::new((child1, child2)).style(|s| s.flex_row().size(200.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 100.0);
    harness.rebuild();

    // Initially both visible, child2 at x=50
    let layout2 = child2_id.get_layout().expect("Child2 should have layout");
    assert!(
        (layout2.location.x - 50.0).abs() < 0.1,
        "Child2 x should be 50.0, got {}",
        layout2.location.x
    );

    // Hide child1
    child1_id.set_hidden();
    harness.rebuild();

    // Child1 should now have 0 size
    let layout1 = child1_id.get_layout().expect("Child1 should have layout");
    assert!(
        layout1.size.width < 0.1 && layout1.size.height < 0.1,
        "Hidden child1 should have 0 size, got {:?}",
        layout1.size
    );

    // Child2 should now be at x=0 (child1 takes no space)
    let layout2 = child2_id.get_layout().expect("Child2 should have layout");
    assert!(
        (layout2.location.x - 0.0).abs() < 0.1,
        "Child2 x should be 0.0 after child1 hidden, got {}",
        layout2.location.x
    );
}

/// Test that is_hidden() returns correct state.
#[test]
#[serial]
fn test_is_hidden_state() {
    let view = Empty::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // Initially visible
    assert!(!id.is_hidden(), "View should not be hidden initially");

    // Hide it
    id.set_hidden();
    harness.rebuild();
    assert!(id.is_hidden(), "View should be hidden after set_hidden()");

    // Show it
    id.set_visible();
    harness.rebuild();
    assert!(
        !id.is_hidden(),
        "View should not be hidden after set_visible()"
    );
}

/// Test that set_visible() can restore a hidden view.
#[test]
#[serial]
fn test_set_visible_clears_hidden_flag() {
    let view = Empty::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // Hide it
    id.set_hidden();
    harness.rebuild();

    // Verify hidden
    let layout = id.get_layout().expect("View should have layout");
    assert!(
        layout.size.width < 0.1,
        "Hidden view should have 0 width, got {}",
        layout.size.width
    );

    // Show it - note: set_visible resets to is_hidden_state::None,
    // which needs the style pass to transition to Visible
    id.set_visible();
    // Multiple rebuilds may be needed for the transition
    harness.rebuild();
    harness.rebuild();

    // Verify visible again
    let layout = id.get_layout().expect("View should have layout");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "Restored view should have width 100.0, got {}",
        layout.size.width
    );
}

/// Test that hidden children don't affect flex container siblings.
#[test]
#[serial]
fn test_hidden_child_doesnt_affect_flex_siblings() {
    let child1 = Empty::new().style(|s| s.size(50.0, 30.0));
    let _child1_id = child1.view_id();
    let child2 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child2_id = child2.view_id();
    let child3 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child3_id = child3.view_id();

    let container = Stack::new((child1, child2, child3)).style(|s| s.flex_row().size(200.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 100.0);
    harness.rebuild();

    // Initially: child3 at x=100 (50+50)
    let layout3 = child3_id.get_layout().expect("Child3 should have layout");
    assert!(
        (layout3.location.x - 100.0).abs() < 0.1,
        "Initially, child3 x should be 100.0, got {}",
        layout3.location.x
    );

    // Hide child2 (middle child)
    child2_id.set_hidden();
    harness.rebuild();

    // Child3 should now be at x=50 (only child1 before it)
    let layout3 = child3_id.get_layout().expect("Child3 should have layout");
    assert!(
        (layout3.location.x - 50.0).abs() < 0.1,
        "After hiding child2, child3 x should be 50.0, got {}",
        layout3.location.x
    );

    // Child2 should have 0 size
    let layout2 = child2_id.get_layout().expect("Child2 should have layout");
    assert!(
        layout2.size.width < 0.1,
        "Hidden child2 should have 0 width, got {}",
        layout2.size.width
    );
}

// =============================================================================
// Hidden State Tests
// =============================================================================

/// Test that set_hidden() and set_visible() can be called multiple times.
#[test]
#[serial]
fn test_hidden_toggle_multiple_times() {
    let view = Empty::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // Toggle hidden state multiple times
    for i in 0..3 {
        id.set_hidden();
        harness.rebuild();
        assert!(
            id.is_hidden(),
            "Iteration {}: should be hidden after set_hidden()",
            i
        );

        let layout = id.get_layout().expect("View should have layout");
        assert!(
            layout.size.width < 0.1,
            "Iteration {}: hidden view should have 0 width",
            i
        );

        id.set_visible();
        harness.rebuild();
        assert!(
            !id.is_hidden(),
            "Iteration {}: should be visible after set_visible()",
            i
        );

        let layout = id.get_layout().expect("View should have layout");
        assert!(
            (layout.size.width - 100.0).abs() < 0.1,
            "Iteration {}: visible view should have width 100.0, got {}",
            i,
            layout.size.width
        );
    }
}

/// Test that set_hidden() on already hidden view is idempotent.
#[test]
#[serial]
fn test_set_hidden_idempotent() {
    let view = Empty::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // Hide once
    id.set_hidden();
    harness.rebuild();
    assert!(id.is_hidden());

    // Hide again - should be no-op
    id.set_hidden();
    harness.rebuild();
    assert!(id.is_hidden());

    let layout = id.get_layout().expect("View should have layout");
    assert!(layout.size.width < 0.1, "Should still be hidden");
}

/// Test that set_visible() on already visible view is idempotent.
#[test]
#[serial]
fn test_set_visible_idempotent() {
    let view = Empty::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // Already visible
    assert!(!id.is_hidden());

    // Call set_visible - should be no-op
    id.set_visible();
    harness.rebuild();
    assert!(!id.is_hidden());

    let layout = id.get_layout().expect("View should have layout");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "Should still be visible with proper size"
    );
}

/// Test that hidden state interacts correctly with display:none style.
#[test]
#[serial]
fn test_hidden_with_display_none_style() {
    let is_display_none = RwSignal::new(false);

    let view = Empty::new().style(move |s| {
        let mut s = s.size(100.0, 100.0);
        if is_display_none.get() {
            s = s.display(floem::taffy::Display::None);
        }
        s
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // Initially visible
    let layout = id.get_layout().expect("View should have layout");
    assert!((layout.size.width - 100.0).abs() < 0.1, "Initially visible");

    // Set display:none via style
    is_display_none.set(true);
    harness.rebuild();
    // Need extra rebuild for the VisibilityPhase from Visible -> Hidden
    harness.rebuild();

    let layout = id.get_layout().expect("View should have layout");
    assert!(
        layout.size.width < 0.1,
        "display:none should hide view, got width {}",
        layout.size.width
    );

    // Now also set_hidden - should remain hidden
    id.set_hidden();
    harness.rebuild();
    assert!(id.is_hidden());

    // Remove display:none from style, but set_hidden still active
    is_display_none.set(false);
    harness.rebuild();

    // Should still be hidden because of set_hidden()
    assert!(
        id.is_hidden(),
        "set_hidden() should keep view hidden even when display:none removed"
    );
    let layout = id.get_layout().expect("View should have layout");
    assert!(
        layout.size.width < 0.1,
        "set_hidden() should keep size at 0"
    );

    // Now set_visible - should become visible
    id.set_visible();
    harness.rebuild();

    assert!(!id.is_hidden());
    let layout = id.get_layout().expect("View should have layout");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "After set_visible, should be visible"
    );
}

/// Test that nested hidden views work correctly.
#[test]
#[serial]
fn test_nested_hidden_views() {
    let inner = Empty::new().style(|s| s.size(50.0, 50.0));
    let inner_id = inner.view_id();

    let outer = Container::new(inner).style(|s| s.size(100.0, 100.0));
    let outer_id = outer.view_id();

    let mut harness = HeadlessHarness::new_with_size(outer, 200.0, 200.0);
    harness.rebuild();

    // Hide outer - inner should also have 0 layout
    outer_id.set_hidden();
    harness.rebuild();

    let outer_layout = outer_id.get_layout().expect("Outer should have layout");
    assert!(outer_layout.size.width < 0.1, "Outer should be hidden");

    // Inner may still have size in local coordinates, but outer is hidden
    // so it won't be visible. The key is outer has 0 size.

    // Now hide inner explicitly while outer is hidden
    inner_id.set_hidden();
    harness.rebuild();

    // Show outer - inner should still be hidden
    outer_id.set_visible();
    harness.rebuild();

    let outer_layout = outer_id.get_layout().expect("Outer should have layout");
    assert!(
        (outer_layout.size.width - 100.0).abs() < 0.1,
        "Outer should be visible"
    );

    // Inner was explicitly hidden, should remain hidden
    assert!(inner_id.is_hidden(), "Inner should still be hidden");
}

/// Test that hiding a view removes it from hit testing.
#[test]
#[serial]
fn test_hidden_view_not_clickable() {
    let tracker = ClickTracker::new();

    let inner = Empty::new().style(|s| s.size(100.0, 100.0));
    let id = inner.view_id();
    let view = tracker.track_named("target", inner);

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // Click when visible
    harness.click(50.0, 50.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["target"],
        "Should click visible view"
    );

    tracker.reset();

    // Hide and click
    id.set_hidden();
    harness.rebuild();
    harness.click(50.0, 50.0);
    assert!(
        tracker.clicked_names().is_empty(),
        "Should not click hidden view"
    );
}

// =============================================================================
// Display Recovery Tests
// =============================================================================

/// Test that flex-row display is properly restored after set_visible().
#[test]
#[serial]
fn test_flex_row_display_recovered_after_visible() {
    let child1 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child2 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child2_id = child2.view_id();

    // Container with flex-row display
    let container = Stack::new((child1, child2)).style(|s| s.flex_row().size(200.0, 100.0));
    let container_id = container.view_id();

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 100.0);
    harness.rebuild();

    // Initially: child2 at x=50 (flex-row layout)
    let layout2 = child2_id.get_layout().expect("Child2 should have layout");
    assert!(
        (layout2.location.x - 50.0).abs() < 0.1,
        "Initially, child2 x should be 50.0 (flex-row), got {}",
        layout2.location.x
    );

    // Hide container
    container_id.set_hidden();
    harness.rebuild();

    // Show container
    container_id.set_visible();
    harness.rebuild();

    // After set_visible: flex-row should be restored, child2 still at x=50
    let layout2 = child2_id.get_layout().expect("Child2 should have layout");
    assert!(
        (layout2.location.x - 50.0).abs() < 0.1,
        "After set_visible, child2 x should be 50.0 (flex-row restored), got {}. \
         Bug: display:flex was not restored!",
        layout2.location.x
    );
}

/// Test that flex-col display is properly restored after set_visible().
#[test]
#[serial]
fn test_flex_col_display_recovered_after_visible() {
    let child1 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child2 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child2_id = child2.view_id();

    // Container with flex-col display
    let container = Stack::new((child1, child2)).style(|s| s.flex_col().size(100.0, 200.0));
    let container_id = container.view_id();

    let mut harness = HeadlessHarness::new_with_size(container, 100.0, 200.0);
    harness.rebuild();

    // Initially: child2 at y=30 (flex-col layout)
    let layout2 = child2_id.get_layout().expect("Child2 should have layout");
    assert!(
        (layout2.location.y - 30.0).abs() < 0.1,
        "Initially, child2 y should be 30.0 (flex-col), got {}",
        layout2.location.y
    );

    // Hide container
    container_id.set_hidden();
    harness.rebuild();

    // Show container
    container_id.set_visible();
    harness.rebuild();

    // After set_visible: flex-col should be restored, child2 still at y=30
    let layout2 = child2_id.get_layout().expect("Child2 should have layout");
    assert!(
        (layout2.location.y - 30.0).abs() < 0.1,
        "After set_visible, child2 y should be 30.0 (flex-col restored), got {}. \
         Bug: display:flex was not restored!",
        layout2.location.y
    );
}

/// Test that Tab properly restores display when switching tabs.
#[test]
#[serial]
fn test_tab_restores_display_on_switch() {
    let active_tab = RwSignal::new(Some(0usize));
    let tabs = vec![0, 1];

    let child_ids: Rc<RefCell<Vec<ViewId>>> = Rc::new(RefCell::new(Vec::new()));
    let inner_child_ids: Rc<RefCell<Vec<ViewId>>> = Rc::new(RefCell::new(Vec::new()));
    let child_ids_clone = child_ids.clone();
    let inner_child_ids_clone = inner_child_ids.clone();

    // Each tab contains a flex-row container
    let tab_view = tab(
        move || active_tab.get(),
        move || tabs.clone(),
        |t| *t,
        move |_t| {
            let inner1 = Empty::new().style(|s| s.size(40.0, 30.0));
            let inner2 = Empty::new().style(|s| s.size(40.0, 30.0));
            inner_child_ids_clone.borrow_mut().push(inner2.view_id());

            let container =
                Stack::new((inner1, inner2)).style(move |s| s.flex_row().size(100.0, 50.0));
            child_ids_clone.borrow_mut().push(container.view_id());
            container
        },
    );

    let mut harness = HeadlessHarness::new_with_size(tab_view, 200.0, 200.0);
    harness.rebuild();

    let inner_ids = inner_child_ids.borrow();

    // Tab 0 is active: inner2 of tab 0 should be at x=40 (flex-row)
    let inner2_tab0 = inner_ids[0].get_layout().expect("Inner2 of tab0");
    assert!(
        (inner2_tab0.location.x - 40.0).abs() < 0.1,
        "Tab0 inner2 x should be 40.0 (flex-row), got {}",
        inner2_tab0.location.x
    );

    // Switch to tab 1
    drop(inner_ids);
    active_tab.set(Some(1));
    harness.rebuild();

    // Tab 1 is now active: inner2 of tab 1 should be at x=40 (flex-row)
    let inner_ids = inner_child_ids.borrow();
    let inner2_tab1 = inner_ids[1].get_layout().expect("Inner2 of tab1");
    assert!(
        (inner2_tab1.location.x - 40.0).abs() < 0.1,
        "Tab1 inner2 x should be 40.0 (flex-row), got {}",
        inner2_tab1.location.x
    );

    // Switch back to tab 0
    drop(inner_ids);
    active_tab.set(Some(0));
    harness.rebuild();

    // Tab 0 should be restored with flex-row layout
    let inner_ids = inner_child_ids.borrow();
    let inner2_tab0 = inner_ids[0].get_layout().expect("Inner2 of tab0");
    assert!(
        (inner2_tab0.location.x - 40.0).abs() < 0.1,
        "After switching back, tab0 inner2 x should be 40.0 (flex-row restored), got {}",
        inner2_tab0.location.x
    );
}
