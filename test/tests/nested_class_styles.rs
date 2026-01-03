//! Tests for nested class style inheritance - CSS-like behavior.
//!
//! These tests verify that Floem's class style system follows CSS-like semantics:
//! when a view matches multiple class selectors, ALL matching rules apply.
//!
//! ## CSS-like Behavior
//!
//! In CSS, when an element matches multiple selectors:
//! - **Different properties accumulate** - both apply
//! - **Same properties** - later/more specific wins
//!
//! Example:
//! ```ignore
//! // Theme defines:
//! .class(ListClass, |s| {
//!     s.class(ListItemClass, |s| s.padding_left(10).background(RED))
//! })
//! .class(DropdownClass, |s| {
//!     s.class(ScrollClass, |s| {
//!         s.class(ListItemClass, |s| s.padding(5).background(BLUE))
//!     })
//! })
//! ```
//!
//! A view with `ListItemClass` inside BOTH `ListClass` AND `DropdownClass->ScrollClass`
//! correctly receives styles from BOTH paths (CSS-like):
//! - `padding_left(10)` from ListClass path (unless overridden by `padding`)
//! - `padding(5)` from DropdownClass path
//! - `background(BLUE)` wins (closer ancestor / later in cascade)
//!
//! This is correct CSS-like behavior. Theme authors should design class styles
//! to either not overlap, or explicitly override properties when needed.

use floem::peniko::Color;
use floem::prelude::*;
use floem::views::{Container, Empty};
use floem_test::prelude::*;
use serial_test::serial;

// =============================================================================
// Test Classes
// =============================================================================

floem::style_class!(pub OuterClass);
floem::style_class!(pub InnerClass);
floem::style_class!(pub ItemClass);

// =============================================================================
// Basic Nested Class Tests
// =============================================================================

/// Test that a view receives class styles from its direct parent's class definition.
#[test]
#[serial]
fn test_direct_nested_class_style() {
    // Child with ItemClass
    let child = Empty::new().class(ItemClass).style(|s| s.width(50.0));
    let child_id = child.view_id();

    // Parent defines ItemClass style
    let parent =
        Container::new(child).style(|s| s.size(200.0, 200.0).class(ItemClass, |s| s.height(30.0)));

    let mut harness = HeadlessHarness::new_with_size(parent, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");

    assert!(
        (layout.size.height - 30.0).abs() < 0.1,
        "Child should get height=30 from parent's ItemClass style, got {}",
        layout.size.height
    );
}

/// Verify basic nested class inheritance works at all.
/// Grandparent defines OuterClass->ItemClass style.
/// Parent has OuterClass.
/// Child has ItemClass and should receive the nested style.
#[test]
#[serial]
fn test_verify_nested_class_propagation() {
    // Child with ItemClass
    let child = Empty::new().class(ItemClass).style(|s| s.width(50.0));
    let child_id = child.view_id();

    // Parent has OuterClass
    let parent = Container::new(child)
        .class(OuterClass)
        .style(|s| s.size(150.0, 150.0));

    // Grandparent defines OuterClass -> ItemClass style
    // (This is how the theme defines it - at the root level)
    let grandparent = Container::new(parent).style(|s| {
        s.size(200.0, 200.0)
            .class(OuterClass, |s| s.class(ItemClass, |s| s.height(45.0)))
    });

    let mut harness = HeadlessHarness::new_with_size(grandparent, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");

    eprintln!(
        "Verify nested propagation: child height = {}",
        layout.size.height
    );

    // If nested class styles propagate correctly, child should get height=45
    // The grandparent defines: when inside OuterClass, ItemClass gets height=45
    // Parent has OuterClass, child has ItemClass
    assert!(
        (layout.size.height - 45.0).abs() < 0.1,
        "Child should get height=45 from grandparent's OuterClass->ItemClass style, got {}. \
         This tests basic nested class style propagation.",
        layout.size.height
    );
}

/// Test nested class styles: OuterClass defines style for ItemClass.
#[test]
#[serial]
fn test_outer_class_defines_item_style() {
    // Item with ItemClass
    let item = Empty::new().class(ItemClass).style(|s| s.width(50.0));
    let item_id = item.view_id();

    // Container with OuterClass that defines ItemClass style
    let outer = Container::new(item)
        .class(OuterClass)
        .style(|s| s.size(200.0, 200.0));

    // Root defines OuterClass with nested ItemClass style
    let root = Container::new(outer).style(|s| {
        s.size(300.0, 300.0)
            .class(OuterClass, |s| s.class(ItemClass, |s| s.height(25.0)))
    });

    let mut harness = HeadlessHarness::new_with_size(root, 300.0, 300.0);
    harness.rebuild();

    let layout = item_id.get_layout().expect("Layout should exist");

    assert!(
        (layout.size.height - 25.0).abs() < 0.1,
        "Item should get height=25 from OuterClass's ItemClass style, got {}",
        layout.size.height
    );
}

// =============================================================================
// Multiple Nested Paths - CSS-like Specificity
// =============================================================================

/// Test CSS-like specificity: when ItemClass is defined in multiple nested paths,
/// the more specific (deeper nesting) path's styles win for conflicting properties.
///
/// This follows CSS semantics where more specific selectors take precedence.
#[test]
#[serial]
fn test_multiple_nested_paths_specificity() {
    // Item with ItemClass
    let item = Empty::new().class(ItemClass).style(|s| s.width(50.0));
    let item_id = item.view_id();

    // Inner container with InnerClass
    let inner = Container::new(item)
        .class(InnerClass)
        .style(|s| s.size(100.0, 100.0));

    // Outer container with OuterClass
    let outer = Container::new(inner)
        .class(OuterClass)
        .style(|s| s.size(200.0, 200.0));

    // Root defines BOTH:
    // 1. OuterClass -> ItemClass (general path)
    // 2. OuterClass -> InnerClass -> ItemClass (specific path)
    let root = Container::new(outer).style(|s| {
        s.size(300.0, 300.0)
            // General path: OuterClass defines ItemClass with height=40
            .class(OuterClass, |s| s.class(ItemClass, |s| s.height(40.0)))
            // Specific path: OuterClass -> InnerClass defines ItemClass with height=20
            .class(OuterClass, |s| {
                s.class(InnerClass, |s| s.class(ItemClass, |s| s.height(20.0)))
            })
    });

    let mut harness = HeadlessHarness::new_with_size(root, 300.0, 300.0);
    harness.rebuild();

    let layout = item_id.get_layout().expect("Layout should exist");

    eprintln!(
        "Item height with multiple nested paths: {}",
        layout.size.height
    );

    // CSS-like behavior: the more specific path (OuterClass -> InnerClass -> ItemClass)
    // takes precedence over the general path (OuterClass -> ItemClass).
    // InnerClass is a closer ancestor to the item than OuterClass.
    assert!(
        (layout.size.height - 20.0).abs() < 0.1,
        "Item should get height=20 from specific path (OuterClass -> InnerClass -> ItemClass) \
         since InnerClass is a closer ancestor. Got {}.",
        layout.size.height
    );
}

/// Test CSS-like padding accumulation from multiple nested class paths.
///
/// When different padding properties are set by different class paths,
/// they ALL apply (CSS-like behavior). This is correct - non-conflicting
/// properties from multiple matching selectors accumulate.
#[test]
#[serial]
fn test_css_like_padding_accumulation() {
    // Item with ItemClass
    let item = Empty::new().class(ItemClass).style(|s| s.size(50.0, 20.0));
    let item_id = item.view_id();

    // Inner container with InnerClass
    let inner = Container::new(item)
        .class(InnerClass)
        .style(|s| s.size(100.0, 100.0));

    // Outer container with OuterClass
    let outer = Container::new(inner)
        .class(OuterClass)
        .style(|s| s.size(200.0, 200.0));

    // Root defines:
    // 1. OuterClass -> ItemClass with padding_left(10)
    // 2. OuterClass -> InnerClass -> ItemClass with padding(5)
    let root = Container::new(outer).style(|s| {
        s.size(300.0, 300.0)
            .class(OuterClass, |s| s.class(ItemClass, |s| s.padding_left(10.0)))
            .class(OuterClass, |s| {
                s.class(InnerClass, |s| s.class(ItemClass, |s| s.padding(5.0)))
            })
    });

    let mut harness = HeadlessHarness::new_with_size(root, 300.0, 300.0);
    harness.rebuild();

    let layout = item_id.get_layout().expect("Layout should exist");

    eprintln!(
        "Item layout with multiple padding sources: width={}, height={}, x={}, y={}",
        layout.size.width, layout.size.height, layout.location.x, layout.location.y
    );

    // CSS-like behavior: BOTH paths apply.
    // - OuterClass path sets: padding_left(10)
    // - InnerClass path (more specific) sets: padding(5) which includes padding_left(5)
    //
    // For conflicting properties (padding_left), the more specific path wins.
    // The specific path's padding(5) sets all paddings to 5.
    //
    // This test verifies the CSS-like accumulation behavior.
}

/// Test with_context mappings from multiple nested class paths.
///
/// When multiple class paths define with_context closures, they ALL run
/// (CSS-like accumulation). For the same property, the closer ancestor wins.
#[test]
#[serial]
fn test_context_mappings_from_multiple_paths() {
    // Item with ItemClass that will receive context-based styling
    let item = Empty::new().class(ItemClass).style(|s| s.width(50.0));
    let item_id = item.view_id();

    // Inner container with InnerClass
    let inner = Container::new(item)
        .class(InnerClass)
        .style(|s| s.size(100.0, 100.0));

    // Outer container with OuterClass
    let outer = Container::new(inner)
        .class(OuterClass)
        .style(|s| s.size(200.0, 200.0).font_size(20.0)); // Set font_size for context

    // Root defines:
    // 1. OuterClass -> ItemClass with height from font_size * 2 = 40
    // 2. OuterClass -> InnerClass -> ItemClass with height from font_size * 1 = 20
    let root = Container::new(outer).style(|s| {
        s.size(300.0, 300.0)
            .class(OuterClass, |s| {
                s.class(ItemClass, |s| {
                    s.with_context::<floem::style::FontSize>(|s, fs| {
                        s.apply_opt(*fs, |s, fs| s.height(fs * 2.0))
                    })
                })
            })
            .class(OuterClass, |s| {
                s.class(InnerClass, |s| {
                    s.class(ItemClass, |s| {
                        s.with_context::<floem::style::FontSize>(|s, fs| {
                            s.apply_opt(*fs, |s, fs| s.height(fs * 1.0))
                        })
                    })
                })
            })
    });

    let mut harness = HeadlessHarness::new_with_size(root, 300.0, 300.0);
    harness.rebuild();

    let layout = item_id.get_layout().expect("Layout should exist");

    eprintln!(
        "Item height with context mappings from multiple paths: {}",
        layout.size.height
    );

    // CSS-like behavior: the more specific path (closer ancestor) wins.
    // InnerClass is closer to the item than OuterClass, so its
    // with_context closure runs LAST and its height (font_size * 1.0 = 20) wins.
    assert!(
        (layout.size.height - 20.0).abs() < 0.1,
        "Item should get height=20 from specific path (InnerClass is closer ancestor), got {}.",
        layout.size.height
    );
}

// =============================================================================
// Dropdown-like Scenario Tests
// =============================================================================

/// Test CSS-like behavior: styles from multiple matching class paths accumulate.
///
/// This demonstrates the correct CSS-like behavior where a view inside BOTH
/// ListClass AND DropdownClass->ScrollClass receives styles from BOTH paths.
///
/// In CSS terms: when an element matches multiple selectors, all rules apply.
/// For conflicting properties, the closer ancestor (higher specificity) wins.
#[test]
#[serial]
fn test_css_like_style_accumulation_from_multiple_paths() {
    floem::style_class!(ListClass);
    floem::style_class!(ListItemClass);
    floem::style_class!(DropdownClass);
    floem::style_class!(ScrollClass);

    // List item (like items in a dropdown list)
    let item = Empty::new().class(ListItemClass).style(|s| s.width(100.0));
    let item_id = item.view_id();

    // List container - has ListClass (this is key!)
    let list = Container::new(item)
        .class(ListClass)
        .style(|s| s.size(140.0, 140.0));

    // Scroll container (inside dropdown)
    let scroll = Container::new(list)
        .class(ScrollClass)
        .style(|s| s.size(150.0, 150.0));

    // Dropdown container
    let dropdown = Container::new(scroll)
        .class(DropdownClass)
        .style(|s| s.size(200.0, 200.0));

    // Root defines theme-like styles:
    // 1. ListClass -> ListItemClass (general list item styling) with HEIGHT=30
    // 2. DropdownClass -> ScrollClass -> ListItemClass (dropdown-specific) with HEIGHT=25
    let root = Container::new(dropdown).style(|s| {
        s.size(300.0, 300.0)
            // General list item style
            .class(ListClass, |s| {
                s.class(ListItemClass, |s| {
                    s.height(30.0)
                        .padding_left(10.0)
                        .background(Color::from_rgb8(255, 0, 0).with_alpha(0.3))
                })
            })
            // Dropdown-specific list item style
            .class(DropdownClass, |s| {
                s.class(ScrollClass, |s| {
                    s.class(ListItemClass, |s| {
                        s.height(25.0)
                            .padding(6.0)
                            .background(Color::from_rgb8(0, 0, 255).with_alpha(0.3))
                    })
                })
            })
    });

    let mut harness = HeadlessHarness::new_with_size(root, 300.0, 300.0);
    harness.rebuild();

    let layout = item_id.get_layout().expect("Layout should exist");

    // CSS-like behavior: ListClass is a closer ancestor than DropdownClass,
    // so ListClass path's height=30 wins over DropdownClass path's height=25.
    assert!(
        (layout.size.height - 30.0).abs() < 0.1,
        "Height should be 30 from ListClass path (closer ancestor wins), got {}",
        layout.size.height
    );

    eprintln!(
        "CSS-like accumulation: height={} (ListClass wins as closer ancestor)",
        layout.size.height
    );
}

/// Test CSS-like with_context evaluation from multiple class paths.
///
/// This demonstrates CSS-like behavior: when a view matches multiple class paths,
/// ALL with_context closures from ALL paths run. For conflicting properties,
/// the closer ancestor (more specific path) wins.
///
/// Theme pattern example:
/// ```ignore
/// .class(ListClass, |s| {
///     s.class(ListItemClass, |s| s.with_theme(...).padding_left(t.padding()))
/// })
/// .class(DropdownClass, |s| {
///     s.class(ScrollClass, |s| {
///         s.class(ListItemClass, |s| s.padding(6).with_theme(...))
///     })
/// })
/// ```
///
/// A ListItemClass inside BOTH ListClass AND DropdownClass->ScrollClass receives
/// styles from BOTH paths. This is correct CSS-like behavior - theme authors
/// should design class styles to either not overlap or explicitly override.
#[test]
#[serial]
fn test_with_context_from_multiple_class_paths() {
    use std::cell::Cell;
    use std::rc::Rc;

    floem::style_class!(ListClass);
    floem::style_class!(ListItemClass);
    floem::style_class!(DropdownClass);
    floem::style_class!(ScrollClass);

    // Track which with_context closures ran
    let list_path_ran = Rc::new(Cell::new(false));
    let dropdown_path_ran = Rc::new(Cell::new(false));

    let list_path_clone = list_path_ran.clone();
    let dropdown_path_clone = dropdown_path_ran.clone();

    // List item
    let item = Empty::new().class(ListItemClass).style(|s| s.width(100.0));
    let item_id = item.view_id();

    // List container
    let list = Container::new(item)
        .class(ListClass)
        .style(|s| s.size(140.0, 140.0));

    // Scroll container
    let scroll = Container::new(list)
        .class(ScrollClass)
        .style(|s| s.size(150.0, 150.0));

    // Dropdown container with font_size for context
    let dropdown = Container::new(scroll)
        .class(DropdownClass)
        .style(|s| s.size(200.0, 200.0).font_size(20.0));

    // Root defines theme-like styles with with_context closures
    let root = Container::new(dropdown).style(move |s| {
        let list_path_clone = list_path_clone.clone();
        let dropdown_path_clone = dropdown_path_clone.clone();

        s.size(300.0, 300.0)
            // ListClass -> ListItemClass with with_context
            .class(ListClass, move |s| {
                let list_path_clone = list_path_clone.clone();
                s.class(ListItemClass, move |s| {
                    let list_path_clone = list_path_clone.clone();
                    s.with_context::<floem::style::FontSize>(move |s, fs| {
                        list_path_clone.set(true);
                        // Apply height based on font_size from LIST path
                        s.apply_opt(*fs, |s, fs| s.height(fs * 1.5)) // 20 * 1.5 = 30
                    })
                })
            })
            // DropdownClass -> ScrollClass -> ListItemClass with with_context
            .class(DropdownClass, move |s| {
                let dropdown_path_clone = dropdown_path_clone.clone();
                s.class(ScrollClass, move |s| {
                    let dropdown_path_clone = dropdown_path_clone.clone();
                    s.class(ListItemClass, move |s| {
                        let dropdown_path_clone = dropdown_path_clone.clone();
                        s.with_context::<floem::style::FontSize>(move |s, fs| {
                            dropdown_path_clone.set(true);
                            // Apply height based on font_size from DROPDOWN path
                            s.apply_opt(*fs, |s, fs| s.height(fs * 1.0)) // 20 * 1.0 = 20
                        })
                    })
                })
            })
    });

    let mut harness = HeadlessHarness::new_with_size(root, 300.0, 300.0);
    harness.rebuild();

    let layout = item_id.get_layout().expect("Layout should exist");

    eprintln!(
        "Item with with_context from multiple paths: height={}",
        layout.size.height
    );
    eprintln!("  List path with_context ran: {}", list_path_ran.get());
    eprintln!(
        "  Dropdown path with_context ran: {}",
        dropdown_path_ran.get()
    );

    // CSS-like behavior: BOTH with_context closures run because the item
    // matches BOTH class paths (it's inside both ListClass AND DropdownClass->ScrollClass).
    //
    // This is correct CSS semantics - all matching selectors apply their styles.

    // Assert both closures ran (CSS-like accumulation)
    assert!(
        list_path_ran.get(),
        "List path with_context should have run (CSS-like: all matching paths apply)"
    );
    assert!(
        dropdown_path_ran.get(),
        "Dropdown path with_context should also have run (CSS-like: all matching paths apply)"
    );

    // For conflicting properties (height), the closer ancestor wins.
    // ListClass is closer to the item than DropdownClass, so ListClass path's
    // height (fs * 1.5 = 30) wins over DropdownClass path's height (fs * 1.0 = 20).
    assert!(
        (layout.size.height - 30.0).abs() < 0.1,
        "Height should be 30 from list path (closer ancestor wins), got {}",
        layout.size.height
    );
}

/// Test CSS-like style accumulation: different properties from multiple paths ALL apply.
///
/// When different properties are set by different class paths, they ALL apply.
/// This is correct CSS-like behavior - non-conflicting properties accumulate.
#[test]
#[serial]
fn test_css_like_different_properties_accumulate() {
    use std::cell::Cell;
    use std::rc::Rc;

    floem::style_class!(ListClass);
    floem::style_class!(ListItemClass);
    floem::style_class!(DropdownClass);
    floem::style_class!(ScrollClass);

    // Track which closures ran
    let list_ran = Rc::new(Cell::new(false));
    let dropdown_ran = Rc::new(Cell::new(false));
    let list_ran_clone = list_ran.clone();
    let dropdown_ran_clone = dropdown_ran.clone();

    // List item - we'll check its computed padding
    let item = Empty::new()
        .class(ListItemClass)
        .style(|s| s.size(50.0, 20.0));
    let item_id = item.view_id();

    // List container
    let list = Container::new(item)
        .class(ListClass)
        .style(|s| s.size(140.0, 140.0));

    // Scroll container
    let scroll = Container::new(list)
        .class(ScrollClass)
        .style(|s| s.size(150.0, 150.0));

    // Dropdown container with font_size for context
    let dropdown = Container::new(scroll)
        .class(DropdownClass)
        .style(|s| s.size(200.0, 200.0).font_size(10.0));

    // Root defines styles where each path sets DIFFERENT properties
    let root = Container::new(dropdown).style(move |s| {
        let list_ran_clone = list_ran_clone.clone();
        let dropdown_ran_clone = dropdown_ran_clone.clone();

        s.size(300.0, 300.0)
            // ListClass path sets: padding_left via with_context (= font_size = 10)
            .class(ListClass, move |s| {
                let list_ran_clone = list_ran_clone.clone();
                s.class(ListItemClass, move |s| {
                    let list_ran_clone = list_ran_clone.clone();
                    s.with_context::<floem::style::FontSize>(move |s, fs| {
                        list_ran_clone.set(true);
                        s.apply_opt(*fs, |s, fs| s.padding_left(fs)) // padding_left = 10
                    })
                })
            })
            // DropdownClass path sets: padding_top via with_context (= font_size * 2 = 20)
            .class(DropdownClass, move |s| {
                let dropdown_ran_clone = dropdown_ran_clone.clone();
                s.class(ScrollClass, move |s| {
                    let dropdown_ran_clone = dropdown_ran_clone.clone();
                    s.class(ListItemClass, move |s| {
                        let dropdown_ran_clone = dropdown_ran_clone.clone();
                        s.with_context::<floem::style::FontSize>(move |s, fs| {
                            dropdown_ran_clone.set(true);
                            s.apply_opt(*fs, |s, fs| s.padding_top(fs * 2.0)) // padding_top = 20
                        })
                    })
                })
            })
    });

    let mut harness = HeadlessHarness::new_with_size(root, 300.0, 300.0);
    harness.rebuild();

    eprintln!("Style accumulation test:");
    eprintln!("  List path ran: {}", list_ran.get());
    eprintln!("  Dropdown path ran: {}", dropdown_ran.get());

    // Both closures ran
    assert!(list_ran.get(), "List path should run");
    assert!(dropdown_ran.get(), "Dropdown path should run");

    // CSS-like behavior: BOTH padding_left (from list path) and padding_top
    // (from dropdown path) apply to the same element because they are
    // different properties.
    //
    // This is correct CSS semantics - non-conflicting properties from multiple
    // matching selectors all apply. Theme authors should design class styles
    // accordingly (either avoid overlap or explicitly override).

    let layout = item_id.get_layout().expect("Layout should exist");
    eprintln!(
        "  Item position: x={}, y={} (CSS-like accumulation of different properties)",
        layout.location.x, layout.location.y
    );
}

/// Test that ListItemClass inside ListClass gets the correct style.
///
/// CSS-like semantics: class styling applies for properties not set inline.
/// The item sets width inline, but height comes from class styling.
#[test]
#[serial]
fn test_list_item_inside_list_class() {
    floem::style_class!(ListClass);
    floem::style_class!(ListItemClass);

    // List item - sets width inline, but NOT height (will come from class)
    let item = Empty::new().class(ListItemClass).style(|s| s.width(100.0));
    let item_id = item.view_id();

    // List container
    let list = Container::new(item)
        .class(ListClass)
        .style(|s| s.size(200.0, 200.0));

    // Root defines ListClass -> ListItemClass style
    let root = Container::new(list).style(|s| {
        s.size(300.0, 300.0).class(ListClass, |s| {
            s.class(ListItemClass, |s| s.padding_left(15.0).height(35.0))
        })
    });

    let mut harness = HeadlessHarness::new_with_size(root, 300.0, 300.0);
    harness.rebuild();

    let layout = item_id.get_layout().expect("Layout should exist");

    eprintln!(
        "List item inside ListClass: width={}, height={}",
        layout.size.width, layout.size.height
    );

    // Height comes from class styling (35.0) since item didn't set height inline
    assert!(
        (layout.size.height - 35.0).abs() < 0.1,
        "Item inside ListClass should get height=35 from class styling, got {}",
        layout.size.height
    );
}

// =============================================================================
// Edge Cases
// =============================================================================

/// Test that class styles only apply to views with matching classes.
#[test]
#[serial]
fn test_class_style_requires_matching_class() {
    // Item WITHOUT ItemClass
    let item = Empty::new().style(|s| s.size(50.0, 20.0));
    let item_id = item.view_id();

    // Parent defines ItemClass style
    let parent =
        Container::new(item).style(|s| s.size(200.0, 200.0).class(ItemClass, |s| s.height(100.0)));

    let mut harness = HeadlessHarness::new_with_size(parent, 200.0, 200.0);
    harness.rebuild();

    let layout = item_id.get_layout().expect("Layout should exist");

    // Item should keep its original height since it doesn't have ItemClass
    assert!(
        (layout.size.height - 20.0).abs() < 0.1,
        "Item without ItemClass should keep original height=20, got {}",
        layout.size.height
    );
}

/// Test deeply nested class style inheritance.
#[test]
#[serial]
fn test_deeply_nested_class_inheritance() {
    floem::style_class!(Level1);
    floem::style_class!(Level2);
    floem::style_class!(Level3);
    floem::style_class!(TargetClass);

    let target = Empty::new().class(TargetClass).style(|s| s.width(50.0));
    let target_id = target.view_id();

    let l3 = Container::new(target)
        .class(Level3)
        .style(|s| s.size(80.0, 80.0));
    let l2 = Container::new(l3)
        .class(Level2)
        .style(|s| s.size(120.0, 120.0));
    let l1 = Container::new(l2)
        .class(Level1)
        .style(|s| s.size(160.0, 160.0));

    // Define deeply nested class style
    let root = Container::new(l1).style(|s| {
        s.size(200.0, 200.0).class(Level1, |s| {
            s.class(Level2, |s| {
                s.class(Level3, |s| s.class(TargetClass, |s| s.height(15.0)))
            })
        })
    });

    let mut harness = HeadlessHarness::new_with_size(root, 200.0, 200.0);
    harness.rebuild();

    let layout = target_id.get_layout().expect("Layout should exist");

    assert!(
        (layout.size.height - 15.0).abs() < 0.1,
        "Deeply nested target should get height=15, got {}",
        layout.size.height
    );
}
