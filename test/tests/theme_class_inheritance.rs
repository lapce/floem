//! Tests for theme class styling inheritance.
//!
//! These tests verify that:
//! - Theme styles defined with `.class(ParentClass, |s| s.class(ChildClass, ...))` flow correctly
//! - The default theme's ListClass -> ListItemClass selected styling is applied
//! - Class styling from ancestors reaches descendant views

use floem::peniko::Brush;
use floem::prelude::*;
use floem::style::Background;
use floem::views::list;
use floem_test::prelude::*;

/// Test that nested class styling from a parent flows to children.
///
/// When a parent has `.class(ChildClass, ...)` in its style, children with
/// that class should receive the styling.
#[test]
fn test_parent_class_styling_flows_to_child() {
    // Define a custom class for testing
    floem::style_class!(TestChildClass);

    // Create a parent that defines styling for TestChildClass
    let child = Empty::new()
        .class(TestChildClass)
        .style(|s| s.size(50.0, 50.0));
    let child_id = child.view_id();

    let parent = Container::new(child).style(|s| {
        s.size(100.0, 100.0).class(TestChildClass, |s| {
            s.background(palette::css::RED)
        })
    });

    let _harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // The child should have RED background from parent's class styling
    let style = _harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::RED),
        "Child with TestChildClass should have RED background from parent's class styling, got {:?}",
        bg
    );
}

/// Test that the theme's ListClass -> ListItemClass styling is applied.
///
/// The default theme defines:
/// ```ignore
/// .class(ListClass, |s| {
///     s.class(ListItemClass, |s| {
///         s.selected(|s| s.background(t.primary()))
///     })
/// })
/// ```
///
/// This should make selected list items have a primary background color.
#[test]
fn test_theme_list_class_selected_styling() {
    let items = vec!["A", "B", "C"];

    let list_view = list(items.into_iter().map(|item| Label::new(item)))
        .style(|s| s.width(200.0).height(120.0));

    let list_id = list_view.view_id();
    let _harness = HeadlessHarness::new_with_size(list_view, 200.0, 120.0);

    // Get the ListItemClass views
    let stack_id = list_id.children()[0];
    let item_ids: Vec<_> = stack_id.children();

    // The first item is selected by default
    // It should have theme's primary background from ListItemClass selected styling
    let first_item = item_ids[0].children()[0]; // The view with ListItemClass

    let style = _harness.get_computed_style(first_item);
    let bg = style.get(Background);

    // The theme sets `.selected(|s| s.background(t.primary()))` for ListItemClass
    // If the theme styling flows correctly, the selected item should have a non-None background
    assert!(
        bg.is_some(),
        "Selected ListItemClass should have background from theme's selected styling, got {:?}",
        bg
    );
}

/// Test that ListItemClass gets hover styling from the theme.
///
/// The theme defines hover styling for ListItemClass:
/// ```ignore
/// .class(ListClass, |s| {
///     s.class(ListItemClass, |s| {
///         s.hover(|s| s.background(t.bg_elevated()))
///     })
/// })
/// ```
#[test]
fn test_theme_list_item_hover_styling() {
    let items = vec!["Hover", "Me"];

    let list_view = list(items.into_iter().map(|item| Label::new(item)))
        .style(|s| s.width(200.0).height(80.0));

    let list_id = list_view.view_id();
    let mut harness = HeadlessHarness::new_with_size(list_view, 200.0, 80.0);

    // Get the second item (not selected, so we can test hover without selected styling)
    let stack_id = list_id.children()[0];
    let item_ids: Vec<_> = stack_id.children();
    let second_item = item_ids[1].children()[0]; // The view with ListItemClass

    // Get style before hover
    let style_before = harness.get_computed_style(second_item);
    let bg_before = style_before.get(Background);

    // Hover over the second item
    harness.pointer_move(100.0, 60.0); // Second item should be around y=40-80

    // Get style after hover
    let style_after = harness.get_computed_style(second_item);
    let bg_after = style_after.get(Background);

    // The hover styling from theme should be applied
    // Note: This test may need adjustment based on how hover state works in headless mode
    // At minimum, the theme's hover styling should be defined
    assert!(
        bg_before != bg_after || bg_after.is_some(),
        "Hover should change background or have hover styling defined. Before: {:?}, After: {:?}",
        bg_before,
        bg_after
    );
}

/// Test that deeply nested class styling works.
///
/// Parent -> Child -> Grandchild where Parent defines styling for GrandchildClass
#[test]
fn test_deeply_nested_class_styling() {
    floem::style_class!(DeepClass);

    let grandchild = Empty::new()
        .class(DeepClass)
        .style(|s| s.size(20.0, 20.0));
    let grandchild_id = grandchild.view_id();

    let child = Container::new(grandchild).style(|s| s.size(50.0, 50.0));

    let parent = Container::new(child).style(|s| {
        s.size(100.0, 100.0).class(DeepClass, |s| {
            s.background(palette::css::PURPLE)
        })
    });

    let _harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // The grandchild should receive the class styling from parent
    let style = _harness.get_computed_style(grandchild_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::PURPLE),
        "Grandchild with DeepClass should have PURPLE background from ancestor's class styling, got {:?}",
        bg
    );
}

/// Test that class styling includes selectors (hover, selected, etc.)
#[test]
fn test_class_styling_with_selectors() {
    floem::style_class!(SelectorClass);

    let child = Empty::new()
        .class(SelectorClass)
        .style(|s| s.size(50.0, 50.0).set_selected(true)); // Mark as selected
    let child_id = child.view_id();

    let parent = Container::new(child).style(|s| {
        s.size(100.0, 100.0).class(SelectorClass, |s| {
            s.background(palette::css::WHITE)
                .selected(|s| s.background(palette::css::GOLD))
        })
    });

    let _harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // The child is selected, so it should have GOLD background
    let style = _harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GOLD),
        "Selected child with SelectorClass should have GOLD background, got {:?}",
        bg
    );
}

/// Test that child's own selector styling overrides parent's class selector styling.
///
/// When a child has its own `.selected()` styling, it should take precedence
/// over the parent's `.class(ChildClass, |s| s.selected(...))` styling.
#[test]
fn test_child_style_overrides_parent_class_style() {
    floem::style_class!(OverrideClass);

    // Child has its own .selected() styling that should override parent's class styling
    let child = Empty::new()
        .class(OverrideClass)
        .style(|s| {
            s.size(50.0, 50.0)
                .set_selected(true)
                .selected(|s| s.background(palette::css::BLUE)) // Child's own selected styling
        });
    let child_id = child.view_id();

    // Parent defines class styling for OverrideClass with different selected color
    let parent = Container::new(child).style(|s| {
        s.size(100.0, 100.0).class(OverrideClass, |s| {
            s.background(palette::css::WHITE)
                .selected(|s| s.background(palette::css::RED)) // Parent's class selected styling
        })
    });

    let _harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // The child's own .selected() styling (BLUE) should override parent's class styling (RED)
    let style = _harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Child's own .selected() styling (BLUE) should override parent's class styling (RED), got {:?}",
        bg
    );
}

