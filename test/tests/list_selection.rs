//! Tests for list selection styling behavior.
//!
//! These tests verify that:
//! - List items correctly receive selected styling when selected
//! - `parent_set_selected` properly propagates to style computation
//! - The `.selected()` style selector works with list items

use floem::peniko::Brush;
use floem::prelude::*;
use floem::style::Background;
use floem::views::list;
use floem_test::prelude::*;

/// Test that list items have correct selected styling.
///
/// Note: The list view starts with index 0 selected by default.
/// This tests that selection styling is properly applied.
#[test]
fn test_list_item_selected_style_on_click() {
    let items = vec!["Item 1", "Item 2", "Item 3"];

    // Track the view IDs of the labels inside list items
    let label_ids: std::cell::RefCell<Vec<ViewId>> = std::cell::RefCell::new(Vec::new());
    let label_ids_ref = &label_ids;

    let list_view = list(items.into_iter().map(|item| {
        let label = Label::new(item).style(|s| {
            s.width_full()
                .padding(10.0)
                .background(palette::css::WHITE)
                .selected(|s| s.background(palette::css::BLUE))
        });
        label_ids_ref.borrow_mut().push(label.view_id());
        label
    }))
    .style(|s| s.width(200.0).height(150.0));

    let mut harness = HeadlessHarness::new_with_size(list_view, 200.0, 150.0);

    let ids = label_ids.borrow().clone();
    assert_eq!(ids.len(), 3, "Should have 3 label IDs");

    // Initially, first item is selected by default (list starts with Some(0))
    // So first item should have BLUE background
    let style0 = harness.get_computed_style(ids[0]);
    let bg0 = style0.get(Background);
    assert!(
        matches!(bg0, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "First item should initially have BLUE background (selected by default), got {:?}",
        bg0
    );

    // Other items should have WHITE background (not selected)
    for (i, &id) in ids.iter().enumerate().skip(1) {
        let style = harness.get_computed_style(id);
        let bg = style.get(Background);
        assert!(
            matches!(bg, Some(Brush::Solid(c)) if c == palette::css::WHITE),
            "Item {} should have WHITE background (not selected), got {:?}",
            i,
            bg
        );
    }

    // Click on the third item (at y=100, which should be in the third item)
    harness.click(100.0, 100.0);

    // After clicking, the third item should have BLUE background
    let style2 = harness.get_computed_style(ids[2]);
    let bg2 = style2.get(Background);
    assert!(
        matches!(bg2, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Third item should have BLUE background after selection, got {:?}",
        bg2
    );

    // First item should now have WHITE background (deselected)
    let style0_after = harness.get_computed_style(ids[0]);
    let bg0_after = style0_after.get(Background);
    assert!(
        matches!(bg0_after, Some(Brush::Solid(c)) if c == palette::css::WHITE),
        "First item should have WHITE background after deselection, got {:?}",
        bg0_after
    );
}

/// Test that selecting a different item removes selected style from the previous one.
#[test]
fn test_list_item_selection_change() {
    let items = vec!["A", "B", "C"];

    let label_ids: std::cell::RefCell<Vec<ViewId>> = std::cell::RefCell::new(Vec::new());
    let label_ids_ref = &label_ids;

    let list_view = list(items.into_iter().map(|item| {
        let label = Label::new(item).style(|s| {
            s.width_full()
                .height(40.0)
                .background(palette::css::GRAY)
                .selected(|s| s.background(palette::css::GREEN))
        });
        label_ids_ref.borrow_mut().push(label.view_id());
        label
    }))
    .style(|s| s.width(200.0).height(120.0));

    let mut harness = HeadlessHarness::new_with_size(list_view, 200.0, 120.0);

    let ids = label_ids.borrow().clone();

    // Click on the first item (y=20, middle of first 40px item)
    harness.click(100.0, 20.0);

    // First item should be selected (GREEN)
    let style0 = harness.get_computed_style(ids[0]);
    let bg0 = style0.get(Background);
    assert!(
        matches!(bg0, Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "First item should be GREEN after click, got {:?}",
        bg0
    );

    // Click on the second item (y=60, middle of second 40px item)
    harness.click(100.0, 60.0);

    // Now first item should no longer be selected (back to GRAY)
    let style0_after = harness.get_computed_style(ids[0]);
    let bg0_after = style0_after.get(Background);
    assert!(
        matches!(bg0_after, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "First item should be GRAY after deselection, got {:?}",
        bg0_after
    );

    // Second item should now be selected (GREEN)
    let style1 = harness.get_computed_style(ids[1]);
    let bg1 = style1.get(Background);
    assert!(
        matches!(bg1, Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Second item should be GREEN after selection, got {:?}",
        bg1
    );
}

/// Test that the default theme's ListItemClass selected styling works.
///
/// The default theme defines selected styling for ListItemClass with
/// a primary background color. This test verifies that styling is applied.
#[test]
fn test_list_default_theme_selected_style() {
    let items = vec!["One", "Two", "Three"];

    // We'll track the list item wrapper IDs (not the labels inside)
    // by checking the list's children after creation
    let list_view = list(items.into_iter().map(|item| Label::new(item)))
        .style(|s| s.width(200.0).height(120.0));

    let list_id = list_view.view_id();
    let _harness = HeadlessHarness::new_with_size(list_view, 200.0, 120.0);

    // Get the list item IDs (the Item wrappers created by the list)
    // The structure is: List -> Stack -> [Item, Item, Item]
    // Each Item has a child which is the user's view wrapped with ListItemClass
    let stack_id = list_id.children()[0];
    let item_ids: Vec<_> = stack_id.children();
    assert_eq!(item_ids.len(), 3, "Should have 3 list items");

    // List starts with index 0 selected by default.
    // The Item's child (the view with ListItemClass) should have selected state.
    // parent_set_selected is called on item_id.children()[0]
    for (i, &item_id) in item_ids.iter().enumerate() {
        // Get the child of the Item wrapper (which has ListItemClass)
        let children = item_id.children();
        assert!(!children.is_empty(), "Item {} should have a child", i);

        let list_item_view = children[0];
        let is_selected = list_item_view.is_selected();

        if i == 0 {
            assert!(
                is_selected,
                "First item's ListItemClass view should be selected (index 0 selected by default)"
            );
        } else {
            assert!(
                !is_selected,
                "Item {}'s ListItemClass view should NOT be selected",
                i
            );
        }
    }
}
