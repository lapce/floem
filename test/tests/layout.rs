//! Core layout tests for Floem.
//!
//! These tests verify that taffy-based layout computation works correctly
//! for various layout properties: sizing, flex, padding, margin, positioning.

use floem::prelude::*;
use floem::unit::Pct;
use floem_test::prelude::*;
use serial_test::serial;

// =============================================================================
// Basic Sizing Tests
// =============================================================================

#[test]
#[serial]
fn test_explicit_size() {
    let view = Empty::new().style(|s| s.size(100.0, 50.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let layout = id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "Width should be 100.0, got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 50.0).abs() < 0.1,
        "Height should be 50.0, got {}",
        layout.size.height
    );
}

#[test]
#[serial]
fn test_width_and_height_separate() {
    let view = Empty::new().style(|s| s.width(80.0).height(40.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let layout = id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 80.0).abs() < 0.1,
        "Width should be 80.0, got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 40.0).abs() < 0.1,
        "Height should be 40.0, got {}",
        layout.size.height
    );
}

#[test]
#[serial]
fn test_min_width_and_min_height() {
    // Content would be 0, but min-size forces larger
    let view = Empty::new().style(|s| s.min_width(60.0).min_height(30.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let layout = id.get_layout().expect("Layout should exist");
    assert!(
        layout.size.width >= 60.0 - 0.1,
        "Width should be at least 60.0, got {}",
        layout.size.width
    );
    assert!(
        layout.size.height >= 30.0 - 0.1,
        "Height should be at least 30.0, got {}",
        layout.size.height
    );
}

#[test]
#[serial]
fn test_max_width_and_max_height() {
    // Explicit size larger than max should be clamped
    let view = Empty::new().style(|s| s.size(200.0, 200.0).max_width(100.0).max_height(50.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 300.0, 300.0);
    harness.rebuild();

    let layout = id.get_layout().expect("Layout should exist");
    assert!(
        layout.size.width <= 100.0 + 0.1,
        "Width should be at most 100.0, got {}",
        layout.size.width
    );
    assert!(
        layout.size.height <= 50.0 + 0.1,
        "Height should be at most 50.0, got {}",
        layout.size.height
    );
}

#[test]
#[serial]
fn test_percentage_size() {
    let child = Empty::new().style(|s| s.width(Pct(50.0)).height(Pct(25.0)));
    let child_id = child.view_id();

    let container = Stack::new((child,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "Width should be 100.0 (50% of 200), got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 50.0).abs() < 0.1,
        "Height should be 50.0 (25% of 200), got {}",
        layout.size.height
    );
}

#[test]
#[serial]
fn test_size_full() {
    let child = Empty::new().style(|s| s.size_full());
    let child_id = child.view_id();

    let container = Stack::new((child,)).style(|s| s.size(150.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 150.0).abs() < 0.1,
        "Width should be 150.0 (full), got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 100.0).abs() < 0.1,
        "Height should be 100.0 (full), got {}",
        layout.size.height
    );
}

// =============================================================================
// Flex Layout Tests
// =============================================================================

#[test]
#[serial]
fn test_flex_row_basic() {
    let child1 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child2 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child2_id = child2.view_id();

    let container = Stack::new((child1, child2)).style(|s| s.flex_row().size(200.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child2_id.get_layout().expect("Layout should exist");
    // In flex-row, second child should be after first (x = 50)
    assert!(
        (layout.location.x - 50.0).abs() < 0.1,
        "Second child x should be 50.0, got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 0.0).abs() < 0.1,
        "Second child y should be 0.0, got {}",
        layout.location.y
    );
}

#[test]
#[serial]
fn test_flex_column_basic() {
    let child1 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child2 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child2_id = child2.view_id();

    let container = Stack::new((child1, child2)).style(|s| s.flex_col().size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child2_id.get_layout().expect("Layout should exist");
    // In flex-column, second child should be below first (y = 30)
    assert!(
        (layout.location.x - 0.0).abs() < 0.1,
        "Second child x should be 0.0, got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 30.0).abs() < 0.1,
        "Second child y should be 30.0, got {}",
        layout.location.y
    );
}

#[test]
#[serial]
fn test_flex_gap() {
    let child1 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child2 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child2_id = child2.view_id();

    let container =
        Stack::new((child1, child2)).style(|s| s.flex_row().gap(20.0).size(200.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child2_id.get_layout().expect("Layout should exist");
    // Second child at x = 50 (first child) + 20 (gap) = 70
    assert!(
        (layout.location.x - 70.0).abs() < 0.1,
        "Second child x should be 70.0 (50 + 20 gap), got {}",
        layout.location.x
    );
}

#[test]
#[serial]
fn test_flex_grow() {
    let child1 = Empty::new().style(|s| s.flex_grow(1.0).height(30.0));
    let child1_id = child1.view_id();
    let child2 = Empty::new().style(|s| s.flex_grow(1.0).height(30.0));
    let child2_id = child2.view_id();

    let container = Stack::new((child1, child2)).style(|s| s.flex_row().size(200.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout1 = child1_id.get_layout().expect("Layout should exist");
    let layout2 = child2_id.get_layout().expect("Layout should exist");

    // Both children should share the 200px width equally
    assert!(
        (layout1.size.width - 100.0).abs() < 0.1,
        "First child width should be 100.0, got {}",
        layout1.size.width
    );
    assert!(
        (layout2.size.width - 100.0).abs() < 0.1,
        "Second child width should be 100.0, got {}",
        layout2.size.width
    );
}

#[test]
#[serial]
fn test_flex_grow_unequal() {
    let child1 = Empty::new().style(|s| s.flex_grow(1.0).height(30.0));
    let child1_id = child1.view_id();
    let child2 = Empty::new().style(|s| s.flex_grow(2.0).height(30.0));
    let child2_id = child2.view_id();

    let container = Stack::new((child1, child2)).style(|s| s.flex_row().size(300.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 300.0, 200.0);
    harness.rebuild();

    let layout1 = child1_id.get_layout().expect("Layout should exist");
    let layout2 = child2_id.get_layout().expect("Layout should exist");

    // Child1 gets 1/3, child2 gets 2/3 of 300px
    assert!(
        (layout1.size.width - 100.0).abs() < 0.1,
        "First child width should be 100.0 (1/3), got {}",
        layout1.size.width
    );
    assert!(
        (layout2.size.width - 200.0).abs() < 0.1,
        "Second child width should be 200.0 (2/3), got {}",
        layout2.size.width
    );
}

#[test]
#[serial]
fn test_flex_shrink() {
    // Children have base width 150 each = 300, but container is only 200
    let child1 = Empty::new().style(|s| s.width(150.0).flex_shrink(1.0).height(30.0));
    let child1_id = child1.view_id();
    let child2 = Empty::new().style(|s| s.width(150.0).flex_shrink(1.0).height(30.0));
    let child2_id = child2.view_id();

    let container = Stack::new((child1, child2)).style(|s| s.flex_row().size(200.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout1 = child1_id.get_layout().expect("Layout should exist");
    let layout2 = child2_id.get_layout().expect("Layout should exist");

    // Both should shrink equally to fit in 200px
    assert!(
        (layout1.size.width - 100.0).abs() < 0.1,
        "First child width should shrink to 100.0, got {}",
        layout1.size.width
    );
    assert!(
        (layout2.size.width - 100.0).abs() < 0.1,
        "Second child width should shrink to 100.0, got {}",
        layout2.size.width
    );
}

#[test]
#[serial]
fn test_flex_basis() {
    let child1 = Empty::new().style(|s| s.flex_basis(80.0).height(30.0));
    let child1_id = child1.view_id();

    let container = Stack::new((child1,)).style(|s| s.flex_row().size(200.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child1_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 80.0).abs() < 0.1,
        "Child width should be 80.0 (flex-basis), got {}",
        layout.size.width
    );
}

// =============================================================================
// Alignment Tests
// =============================================================================

#[test]
#[serial]
fn test_align_items_center() {
    let child = Empty::new().style(|s| s.size(50.0, 30.0));
    let child_id = child.view_id();

    let container = Stack::new((child,)).style(|s| s.flex_row().items_center().size(200.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    // Centered in 100px height container: (100 - 30) / 2 = 35
    assert!(
        (layout.location.y - 35.0).abs() < 0.1,
        "Child y should be 35.0 (centered), got {}",
        layout.location.y
    );
}

#[test]
#[serial]
fn test_align_items_end() {
    let child = Empty::new().style(|s| s.size(50.0, 30.0));
    let child_id = child.view_id();

    let container = Stack::new((child,)).style(|s| s.flex_row().items_end().size(200.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    // At end of 100px height container: 100 - 30 = 70
    assert!(
        (layout.location.y - 70.0).abs() < 0.1,
        "Child y should be 70.0 (end), got {}",
        layout.location.y
    );
}

#[test]
#[serial]
fn test_justify_content_center() {
    let child = Empty::new().style(|s| s.size(50.0, 30.0));
    let child_id = child.view_id();

    let container =
        Stack::new((child,)).style(|s| s.flex_row().justify_center().size(200.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    // Centered in 200px width container: (200 - 50) / 2 = 75
    assert!(
        (layout.location.x - 75.0).abs() < 0.1,
        "Child x should be 75.0 (centered), got {}",
        layout.location.x
    );
}

#[test]
#[serial]
fn test_justify_content_space_between() {
    let child1 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child1_id = child1.view_id();
    let child2 = Empty::new().style(|s| s.size(50.0, 30.0));
    let child2_id = child2.view_id();

    let container =
        Stack::new((child1, child2)).style(|s| s.flex_row().justify_between().size(200.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout1 = child1_id.get_layout().expect("Layout should exist");
    let layout2 = child2_id.get_layout().expect("Layout should exist");

    // First at start, second at end
    assert!(
        (layout1.location.x - 0.0).abs() < 0.1,
        "First child x should be 0.0, got {}",
        layout1.location.x
    );
    assert!(
        (layout2.location.x - 150.0).abs() < 0.1,
        "Second child x should be 150.0 (200 - 50), got {}",
        layout2.location.x
    );
}

// =============================================================================
// Padding Tests
// =============================================================================

#[test]
#[serial]
fn test_padding_uniform() {
    let child = Empty::new().style(|s| s.size(50.0, 50.0));
    let child_id = child.view_id();

    let container =
        floem::views::Container::new(child).style(|s| s.padding(10.0).size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.location.x - 10.0).abs() < 0.1,
        "Child x should be 10.0 (padding), got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 10.0).abs() < 0.1,
        "Child y should be 10.0 (padding), got {}",
        layout.location.y
    );
}

#[test]
#[serial]
fn test_padding_directional() {
    let child = Empty::new().style(|s| s.size(50.0, 50.0));
    let child_id = child.view_id();

    let container = floem::views::Container::new(child).style(|s| {
        s.padding_left(5.0)
            .padding_top(10.0)
            .padding_right(15.0)
            .padding_bottom(20.0)
            .size(100.0, 100.0)
    });

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.location.x - 5.0).abs() < 0.1,
        "Child x should be 5.0 (padding-left), got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 10.0).abs() < 0.1,
        "Child y should be 10.0 (padding-top), got {}",
        layout.location.y
    );
}

#[test]
#[serial]
fn test_padding_reduces_content_area() {
    // Child with size_full should respect parent's padding
    let child = Empty::new().style(|s| s.size_full());
    let child_id = child.view_id();

    let container =
        floem::views::Container::new(child).style(|s| s.padding(20.0).size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    // Child should be 100 - 20 - 20 = 60 in each dimension
    assert!(
        (layout.size.width - 60.0).abs() < 0.1,
        "Child width should be 60.0 (100 - 40 padding), got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 60.0).abs() < 0.1,
        "Child height should be 60.0 (100 - 40 padding), got {}",
        layout.size.height
    );
}

// =============================================================================
// Margin Tests
// =============================================================================

#[test]
#[serial]
fn test_margin_uniform() {
    let child = Empty::new().style(|s| s.margin(15.0).size(50.0, 50.0));
    let child_id = child.view_id();

    let container = Stack::new((child,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.location.x - 15.0).abs() < 0.1,
        "Child x should be 15.0 (margin), got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 15.0).abs() < 0.1,
        "Child y should be 15.0 (margin), got {}",
        layout.location.y
    );
}

#[test]
#[serial]
fn test_margin_directional() {
    let child = Empty::new().style(|s| {
        s.margin_left(5.0)
            .margin_top(10.0)
            .margin_right(15.0)
            .margin_bottom(20.0)
            .size(50.0, 50.0)
    });
    let child_id = child.view_id();

    let container = Stack::new((child,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.location.x - 5.0).abs() < 0.1,
        "Child x should be 5.0 (margin-left), got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 10.0).abs() < 0.1,
        "Child y should be 10.0 (margin-top), got {}",
        layout.location.y
    );
}

#[test]
#[serial]
fn test_margin_auto_centering() {
    // margin_horiz_auto should center horizontally in flex container
    let child = Empty::new().style(|s| s.margin_horiz(floem::style::Auto).size(50.0, 50.0));
    let child_id = child.view_id();

    let container = Stack::new((child,)).style(|s| s.flex_row().size(200.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    // Centered: (200 - 50) / 2 = 75
    assert!(
        (layout.location.x - 75.0).abs() < 0.1,
        "Child x should be 75.0 (auto margin centered), got {}",
        layout.location.x
    );
}

// =============================================================================
// Absolute Positioning Tests
// =============================================================================

#[test]
#[serial]
fn test_absolute_positioning() {
    let child = Empty::new().style(|s| {
        s.absolute()
            .inset_left(20.0)
            .inset_top(30.0)
            .size(50.0, 50.0)
    });
    let child_id = child.view_id();

    let container = Stack::new((child,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.location.x - 20.0).abs() < 0.1,
        "Child x should be 20.0 (inset-left), got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 30.0).abs() < 0.1,
        "Child y should be 30.0 (inset-top), got {}",
        layout.location.y
    );
}

#[test]
#[serial]
fn test_absolute_inset_all() {
    // inset(0) should fill the parent
    let child = Empty::new().style(|s| s.absolute().inset(0.0));
    let child_id = child.view_id();

    let container = Stack::new((child,)).style(|s| s.size(200.0, 150.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 200.0).abs() < 0.1,
        "Child width should be 200.0 (fill parent), got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 150.0).abs() < 0.1,
        "Child height should be 150.0 (fill parent), got {}",
        layout.size.height
    );
}

#[test]
#[serial]
fn test_absolute_inset_with_size() {
    // Explicit size takes precedence over filling via inset
    let child = Empty::new().style(|s| s.absolute().inset(0.0).size(50.0, 40.0));
    let child_id = child.view_id();

    let container = Stack::new((child,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 50.0).abs() < 0.1,
        "Child width should be 50.0 (explicit), got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 40.0).abs() < 0.1,
        "Child height should be 40.0 (explicit), got {}",
        layout.size.height
    );
}

#[test]
#[serial]
fn test_absolute_percentage_positioning() {
    let child = Empty::new().style(|s| {
        s.absolute()
            .inset_left(Pct(50.0))
            .inset_top(Pct(25.0))
            .size(50.0, 50.0)
    });
    let child_id = child.view_id();

    let container = Stack::new((child,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.location.x - 100.0).abs() < 0.1,
        "Child x should be 100.0 (50% of 200), got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 50.0).abs() < 0.1,
        "Child y should be 50.0 (25% of 200), got {}",
        layout.location.y
    );
}

#[test]
#[serial]
fn test_absolute_does_not_affect_siblings() {
    // Absolute positioned elements should not affect layout of siblings
    let absolute_child = Empty::new().style(|s| {
        s.absolute()
            .inset_left(10.0)
            .inset_top(10.0)
            .size(100.0, 100.0)
    });
    let normal_child = Empty::new().style(|s| s.size(50.0, 50.0));
    let normal_id = normal_child.view_id();

    let container =
        Stack::new((absolute_child, normal_child)).style(|s| s.flex_row().size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = normal_id.get_layout().expect("Layout should exist");
    // Normal child should start at x=0 (not pushed by absolute sibling)
    assert!(
        (layout.location.x - 0.0).abs() < 0.1,
        "Normal child x should be 0.0 (not affected by absolute sibling), got {}",
        layout.location.x
    );
}

// =============================================================================
// Nested Layout Tests
// =============================================================================

#[test]
#[serial]
fn test_nested_flex_containers() {
    let inner_child1 = Empty::new().style(|s| s.size(30.0, 30.0));
    let inner_child2 = Empty::new().style(|s| s.size(30.0, 30.0));
    let inner_child2_id = inner_child2.view_id();

    let inner_container =
        Stack::new((inner_child1, inner_child2)).style(|s| s.flex_col().gap(10.0));
    let inner_id = inner_container.view_id();

    let outer_container =
        Stack::new((inner_container,)).style(|s| s.padding(20.0).size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(outer_container, 200.0, 200.0);
    harness.rebuild();

    let inner_layout = inner_id.get_layout().expect("Inner layout should exist");
    let child2_layout = inner_child2_id
        .get_layout()
        .expect("Child2 layout should exist");

    // Inner container should be offset by padding
    assert!(
        (inner_layout.location.x - 20.0).abs() < 0.1,
        "Inner container x should be 20.0 (padding), got {}",
        inner_layout.location.x
    );

    // Second child should be offset within inner container: y = 30 (first child) + 10 (gap)
    assert!(
        (child2_layout.location.y - 40.0).abs() < 0.1,
        "Inner child2 y should be 40.0 (30 + 10 gap), got {}",
        child2_layout.location.y
    );
}

#[test]
#[serial]
fn test_deeply_nested_percentage() {
    // Test that percentage sizing works through multiple levels
    let deep_child = Empty::new().style(|s| s.width(Pct(50.0)).height(Pct(50.0)));
    let deep_id = deep_child.view_id();

    let level2 = Stack::new((deep_child,)).style(|s| s.width(Pct(50.0)).height(Pct(50.0)));
    let level1 = Stack::new((level2,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(level1, 200.0, 200.0);
    harness.rebuild();

    let layout = deep_id.get_layout().expect("Layout should exist");
    // level2 is 50% of 200 = 100
    // deep_child is 50% of 100 = 50
    assert!(
        (layout.size.width - 50.0).abs() < 0.1,
        "Deep child width should be 50.0 (50% of 50% of 200), got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 50.0).abs() < 0.1,
        "Deep child height should be 50.0 (50% of 50% of 200), got {}",
        layout.size.height
    );
}

// =============================================================================
// Aspect Ratio Tests
// =============================================================================

#[test]
#[serial]
fn test_aspect_ratio_with_width() {
    let child = Empty::new().style(|s| s.width(100.0).aspect_ratio(2.0)); // 2:1 ratio
    let child_id = child.view_id();

    // Use items_start to prevent stretching the child to full height
    let container = Stack::new((child,)).style(|s| s.items_start().size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "Width should be 100.0, got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 50.0).abs() < 0.1,
        "Height should be 50.0 (100 / 2), got {}",
        layout.size.height
    );
}

#[test]
#[serial]
fn test_aspect_ratio_with_height() {
    let child = Empty::new().style(|s| s.height(100.0).aspect_ratio(0.5)); // 1:2 ratio
    let child_id = child.view_id();

    // Use items_start to prevent stretching the child
    let container = Stack::new((child,)).style(|s| s.items_start().size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.height - 100.0).abs() < 0.1,
        "Height should be 100.0, got {}",
        layout.size.height
    );
    assert!(
        (layout.size.width - 50.0).abs() < 0.1,
        "Width should be 50.0 (100 * 0.5), got {}",
        layout.size.width
    );
}

// =============================================================================
// Display None Tests
// =============================================================================

#[test]
#[serial]
fn test_display_none_hides_element() {
    let child = Empty::new().style(|s| s.display(floem::taffy::Display::None).size(50.0, 50.0));
    let child_id = child.view_id();

    let container = Stack::new((child,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    // Display none elements have 0 size
    assert!(
        layout.size.width < 0.1,
        "Hidden element width should be 0, got {}",
        layout.size.width
    );
    assert!(
        layout.size.height < 0.1,
        "Hidden element height should be 0, got {}",
        layout.size.height
    );
}

#[test]
#[serial]
fn test_display_none_does_not_affect_siblings() {
    let hidden = Empty::new().style(|s| s.display(floem::taffy::Display::None).size(100.0, 100.0));
    let visible = Empty::new().style(|s| s.size(50.0, 50.0));
    let visible_id = visible.view_id();

    let container = Stack::new((hidden, visible)).style(|s| s.flex_row().size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = visible_id.get_layout().expect("Layout should exist");
    // Visible child should be at x=0 (hidden sibling takes no space)
    assert!(
        (layout.location.x - 0.0).abs() < 0.1,
        "Visible child x should be 0.0 (hidden sibling takes no space), got {}",
        layout.location.x
    );
}

// =============================================================================
// Border Tests (if border affects layout)
// =============================================================================

#[test]
#[serial]
fn test_border_affects_layout() {
    let child = Empty::new().style(|s| s.size_full());
    let child_id = child.view_id();

    let container = floem::views::Container::new(child).style(|s| s.border(5.0).size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    // Child should be inset by border: 100 - 5 - 5 = 90
    assert!(
        (layout.size.width - 90.0).abs() < 0.1,
        "Child width should be 90.0 (100 - 10 border), got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 90.0).abs() < 0.1,
        "Child height should be 90.0 (100 - 10 border), got {}",
        layout.size.height
    );
}

// =============================================================================
// Transform Tests (verify transforms don't affect layout size)
// =============================================================================

#[test]
#[serial]
fn test_transform_does_not_affect_layout_size() {
    // Transforms should affect painting but not layout
    let child = Empty::new().style(|s| s.size(50.0, 50.0).scale(Pct(200.0)));
    let child_id = child.view_id();

    let container = Stack::new((child,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    // Layout size should be the original 50x50, not scaled
    assert!(
        (layout.size.width - 50.0).abs() < 0.1,
        "Layout width should be 50.0 (unaffected by scale transform), got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 50.0).abs() < 0.1,
        "Layout height should be 50.0 (unaffected by scale transform), got {}",
        layout.size.height
    );
}

// =============================================================================
// Window Origin Tests
// =============================================================================

#[test]
#[serial]
fn test_window_origin_accumulates() {
    let deep_child = Empty::new().style(|s| s.size(20.0, 20.0));
    let deep_id = deep_child.view_id();

    let level2 = Stack::new((deep_child,)).style(|s| s.padding(10.0).size(100.0, 100.0));
    let level1 = Stack::new((level2,)).style(|s| s.padding(20.0).size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(level1, 200.0, 200.0);
    harness.rebuild();

    let window_origin = deep_id.get_window_origin();
    // level1 padding: 20, level2 padding: 10, total: 30
    assert!(
        (window_origin.x - 30.0).abs() < 0.1,
        "Window origin x should be 30.0 (20 + 10 padding), got {}",
        window_origin.x
    );
    assert!(
        (window_origin.y - 30.0).abs() < 0.1,
        "Window origin y should be 30.0 (20 + 10 padding), got {}",
        window_origin.y
    );
}
