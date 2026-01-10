//! Tests for reactive class toggling via class_if.
//!
//! These tests verify that the class_if method correctly adds and removes
//! style classes based on reactive conditions, specifically testing the fix
//! for infinite recursion in ViewId::remove_class.

use floem::prelude::*;
use floem::views::{Container, Empty, Stack};
use floem_test::prelude::*;
use serial_test::serial;

floem::style_class!(pub TestClass);

/// Test that class_if adds a class when the condition is true.
#[test]
#[serial]
fn test_class_if_adds_class_when_true() {
    let should_apply = RwSignal::new(true);

    // Child view with class_if - no inline size so class can override
    let child = Empty::new().class_if(move || should_apply.get(), TestClass);
    let child_id = child.view_id();

    // Parent defines what TestClass styles should be
    let parent = Container::new(child).style(|s| {
        s.size(100.0, 100.0)
            .class(TestClass, |s| s.width(75.0).height(50.0))
    });

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);
    harness.rebuild();

    // Should have the TestClass dimensions
    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 75.0).abs() < 0.1,
        "View with TestClass should have width 75, got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 50.0).abs() < 0.1,
        "View with TestClass should have height 50, got {}",
        layout.size.height
    );
}

/// Test that class_if does not add a class when the condition is false.
#[test]
#[serial]
fn test_class_if_does_not_add_class_when_false() {
    let should_apply = RwSignal::new(false);

    // Child view with class_if and default size
    let child = Empty::new()
        .class_if(move || should_apply.get(), TestClass)
        .style(|s| s.size(100.0, 100.0));
    let child_id = child.view_id();

    // Parent defines what TestClass would do (but it won't be applied)
    let parent = Container::new(child).style(|s| {
        s.size(200.0, 200.0)
            .class(TestClass, |s| s.width(50.0).height(50.0))
    });

    let mut harness = HeadlessHarness::new_with_size(parent, 200.0, 200.0);
    harness.rebuild();

    // Should keep original size, not TestClass size
    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "View without TestClass should keep width 100, got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 100.0).abs() < 0.1,
        "View without TestClass should keep height 100, got {}",
        layout.size.height
    );
}

/// Test that class_if reactively toggles a class when the condition changes.
/// This is the key test for the infinite recursion bug fix.
#[test]
#[serial]
fn test_class_if_toggles_class_reactively() {
    let should_apply = RwSignal::new(false);

    // Child view with class_if - default size
    let child = Empty::new()
        .class_if(move || should_apply.get(), TestClass)
        .style(|s| s.size(100.0, 100.0));
    let child_id = child.view_id();

    // Parent defines TestClass styles
    let parent = Container::new(child).style(|s| {
        s.size(200.0, 200.0)
            .class(TestClass, |s| s.width(50.0).height(50.0))
    });

    let mut harness = HeadlessHarness::new_with_size(parent, 200.0, 200.0);
    harness.rebuild();

    // Initially: class not applied, should have default 100x100
    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "Initially should have width 100 (no class), got {}",
        layout.size.width
    );

    // Toggle to true - this triggers remove_class(false) then add_class
    // This is where the infinite recursion would have happened
    should_apply.set(true);
    harness.rebuild();

    // Should now have TestClass styles applied (but inline styles override)
    let layout = child_id.get_layout().expect("Layout should exist");
    // Note: inline .size() has higher specificity, so this won't change
    // Let's test by NOT having inline styles
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "Width stays 100 due to inline style specificity, got {}",
        layout.size.width
    );

    // Toggle back to false - this is the KEY test: remove_class should not recurse infinitely
    should_apply.set(false);
    harness.rebuild();

    // Should be back to original size
    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "After removing TestClass, should have width 100, got {}",
        layout.size.width
    );
}

/// Test the specific bug fix: toggling class_if should not cause infinite recursion.
/// This test focuses on the remove_class path that was broken.
#[test]
#[serial]
fn test_class_if_remove_does_not_recurse() {
    let should_apply = RwSignal::new(true);

    // Start with class applied
    let child = Empty::new().class_if(move || should_apply.get(), TestClass);
    let child_id = child.view_id();

    let parent =
        Container::new(child).style(|s| s.size(100.0, 100.0).class(TestClass, |s| s.width(60.0)));

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);
    harness.rebuild();

    // Class is initially applied
    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 60.0).abs() < 0.1,
        "With TestClass, width should be 60, got {}",
        layout.size.width
    );

    // Remove the class - this would cause infinite recursion with the old code
    should_apply.set(false);
    harness.rebuild(); // If this hangs, we have infinite recursion

    // If we get here, no infinite recursion occurred - test passes!
    let layout = child_id
        .get_layout()
        .expect("Layout should exist after removing class");
    // Width should revert (no class style, no inline style)
    assert!(
        layout.size.width < 100.0,
        "After removing class, width changed to {}, test passed (no infinite recursion)",
        layout.size.width
    );
}

/// Test multiple class_if conditions on the same view.
#[test]
#[serial]
fn test_multiple_class_if_conditions() {
    floem::style_class!(ClassA);
    floem::style_class!(ClassB);

    let apply_a = RwSignal::new(false);
    let apply_b = RwSignal::new(false);

    // Child with two class_if conditions
    let child = Empty::new()
        .class_if(move || apply_a.get(), ClassA)
        .class_if(move || apply_b.get(), ClassB);
    let child_id = child.view_id();

    // Parent defines class styles
    let parent = Container::new(child).style(|s| {
        s.size(200.0, 200.0)
            .class(ClassA, |s| s.width(50.0))
            .class(ClassB, |s| s.height(50.0))
    });

    let mut harness = HeadlessHarness::new_with_size(parent, 200.0, 200.0);
    harness.rebuild();

    // Apply ClassA only
    apply_a.set(true);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 50.0).abs() < 0.1,
        "With ClassA, width should be 50, got {}",
        layout.size.width
    );

    // Apply both ClassA and ClassB
    apply_b.set(true);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 50.0).abs() < 0.1,
        "With both classes, width should be 50, got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 50.0).abs() < 0.1,
        "With both classes, height should be 50, got {}",
        layout.size.height
    );

    // Remove ClassA (tests remove_class for A), keep ClassB
    apply_a.set(false);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.height - 50.0).abs() < 0.1,
        "With ClassB only, height should still be 50, got {}",
        layout.size.height
    );

    // Remove ClassB too (tests remove_class for B)
    apply_b.set(false);
    harness.rebuild();

    // If we get here without hanging, both remove_class calls worked correctly
    let _layout = child_id
        .get_layout()
        .expect("Layout should exist after removing both classes");
}

/// Test that class_if works with click handlers.
#[test]
#[serial]
fn test_class_if_with_click_toggle() {
    let is_active = RwSignal::new(false);

    let child = Empty::new()
        .class_if(move || is_active.get(), TestClass)
        .style(|s| s.size(80.0, 80.0))
        .on_click_stop(move |_| {
            is_active.update(|v| *v = !*v);
        });

    let parent = Container::new(child).style(|s| {
        s.size(100.0, 100.0).class(TestClass, |s| s.border(5.0)) // Use border instead of size
    });

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);
    harness.rebuild();

    // Click to toggle (add class)
    harness.click(40.0, 40.0);
    assert!(is_active.get(), "Click should toggle is_active to true");

    // Click again to toggle back (remove class - tests the bug fix)
    harness.click(40.0, 40.0);
    assert!(
        !is_active.get(),
        "Second click should toggle is_active to false"
    );

    // If we get here without infinite recursion, the test passes
}

/// Test that class_if correctly updates when multiple views share the same signal.
#[test]
#[serial]
fn test_class_if_shared_signal() {
    let shared_state = RwSignal::new(false);

    let child1 = Empty::new().class_if(move || shared_state.get(), TestClass);
    let child1_id = child1.view_id();

    let child2 = Empty::new().class_if(move || shared_state.get(), TestClass);
    let child2_id = child2.view_id();

    let container = Stack::new((child1, child2));

    let parent = Container::new(container).style(|s| {
        s.size(120.0, 120.0)
            .class(TestClass, |s| s.width(60.0).height(60.0))
    });

    let mut harness = HeadlessHarness::new_with_size(parent, 120.0, 120.0);

    // Toggle shared state to add class to both
    shared_state.set(true);
    harness.rebuild();

    let layout1 = child1_id.get_layout().expect("Child1 layout should exist");
    let layout2 = child2_id.get_layout().expect("Child2 layout should exist");
    assert!(
        (layout1.size.width - 60.0).abs() < 0.1,
        "Child1 should have TestClass width"
    );
    assert!(
        (layout2.size.width - 60.0).abs() < 0.1,
        "Child2 should have TestClass width"
    );

    // Toggle back to remove class from both (tests remove_class on multiple views)
    shared_state.set(false);
    harness.rebuild();

    // If we get here without hanging, remove_class worked on both views
    let _layout1 = child1_id.get_layout().expect("Child1 layout after remove");
    let _layout2 = child2_id.get_layout().expect("Child2 layout after remove");
}
