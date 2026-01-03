//! Tests for theme inherited properties propagation.
//!
//! These tests verify that inherited properties from the default theme
//! (like font_size, color) are accessible via `with_context` in descendant views.
//!
//! KNOWN ISSUES:
//!
//! 1. **Root-level inherited props not accessible** - The default theme sets
//!    `font_size(14.0)` at its root level, but this value is not accessible via
//!    `with_context::<FontSize>` because root-level inherited properties from
//!    the theme are stored in `class_context` but never applied to views'
//!    `inherited` context.
//!
//! 2. **Context mappings stripped from class styles** - When applying class
//!    styles from `class_context`, the context mappings are stripped (line 588
//!    in mod.rs). This means `with_context` closures defined inside theme class
//!    styles (like toggle_button_style's height calculation) are never evaluated,
//!    even if the required context values would be available.

use floem::prelude::*;
use floem::style::{FontSize, Style};
use floem::views::toggle_button;
use floem_test::prelude::*;
use serial_test::serial;

/// Helper extension trait for testing with FontSize context.
trait FontSizeContextExt {
    fn with_font_size_context(self, f: impl Fn(Self, &Option<f32>) -> Self + 'static) -> Self
    where
        Self: Sized;
}

impl FontSizeContextExt for Style {
    fn with_font_size_context(self, f: impl Fn(Self, &Option<f32>) -> Self + 'static) -> Self {
        self.with_context::<FontSize>(f)
    }
}

// =============================================================================
// Tests for FontSize from default theme
// =============================================================================

/// Test that FontSize is accessible via with_context when set by the default theme.
///
/// The default theme sets `font_size(14.0)` at its root level. This should be
/// accessible to all views via `with_context::<FontSize>`.
///
/// KNOWN ISSUE: This test demonstrates the bug where font_size from the default
/// theme is not accessible via with_context.
#[test]
#[serial]
fn test_font_size_from_default_theme() {
    // View that uses font_size from context to set its height
    let view = Empty::new().style(|s| {
        s.width(100.0).with_font_size_context(|s, fs| {
            // Should get Some(14.0) from default theme
            s.apply_opt(*fs, |s, fs| s.height(fs * 2.0))
        })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let layout = id.get_layout().expect("Layout should exist");

    // If font_size from default theme (14.0) is accessible,
    // height should be 14.0 * 2.0 = 28.0
    eprintln!(
        "View layout with font_size from theme: width={}, height={}",
        layout.size.width, layout.size.height
    );

    // This assertion will FAIL if font_size is not accessible from default theme
    assert!(
        layout.size.height > 0.1,
        "Height should be set from font_size context (expected ~28.0), got {}. \
         This indicates font_size from default theme is not accessible via with_context.",
        layout.size.height
    );

    // More specific check: if it works correctly, height should be ~28.0
    assert!(
        (layout.size.height - 28.0).abs() < 0.1,
        "Height should be 28.0 (14.0 * 2.0 from default theme font_size), got {}",
        layout.size.height
    );
}

/// Test that FontSize is accessible when explicitly set on a parent.
///
/// This should work because the parent sets font_size directly, which then
/// propagates to the inherited context for children.
#[test]
#[serial]
fn test_font_size_from_explicit_parent() {
    // Child uses font_size from context
    let child = Empty::new().style(|s| {
        s.width(50.0)
            .with_font_size_context(|s, fs| s.apply_opt(*fs, |s, fs| s.height(fs * 2.0)))
    });
    let child_id = child.view_id();

    // Parent explicitly sets font_size
    let parent = Container::new(child).style(|s| s.size(200.0, 200.0).font_size(20.0));

    let mut harness = HeadlessHarness::new_with_size(parent, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");

    eprintln!(
        "Child layout with explicit parent font_size: width={}, height={}",
        layout.size.width, layout.size.height
    );

    // With explicit font_size(20.0) on parent, child height should be 20.0 * 2.0 = 40.0
    assert!(
        (layout.size.height - 40.0).abs() < 0.1,
        "Height should be 40.0 (20.0 * 2.0 from parent font_size), got {}",
        layout.size.height
    );
}

/// Test deeply nested view receiving font_size from ancestor.
#[test]
#[serial]
fn test_font_size_deeply_nested() {
    // Grandchild uses font_size from context
    let grandchild = Empty::new().style(|s| {
        s.width(25.0)
            .with_font_size_context(|s, fs| s.apply_opt(*fs, |s, fs| s.height(fs * 1.5)))
    });
    let grandchild_id = grandchild.view_id();

    let child = Container::new(grandchild).style(|s| s.size(50.0, 50.0));

    // Grandparent sets font_size
    let parent = Container::new(child).style(|s| s.size(200.0, 200.0).font_size(16.0));

    let mut harness = HeadlessHarness::new_with_size(parent, 200.0, 200.0);
    harness.rebuild();

    let layout = grandchild_id.get_layout().expect("Layout should exist");

    // Height should be 16.0 * 1.5 = 24.0
    assert!(
        (layout.size.height - 24.0).abs() < 0.1,
        "Grandchild height should be 24.0 (16.0 * 1.5), got {}",
        layout.size.height
    );
}

// =============================================================================
// Tests for toggle button height (the original issue)
// =============================================================================

/// Test that toggle button has proper height when font_size comes from default theme.
///
/// The toggle button's default style uses:
/// ```ignore
/// .with_context::<FontSize>(|s, fs| s.apply_opt(*fs, |s, fs| s.height(fs * 1.75)))
/// ```
///
/// With the default theme's font_size of 14.0, the height should be ~24.5.
/// If font_size is not accessible, the height will be derived from aspect_ratio
/// and the available width, causing the toggle to expand incorrectly.
#[test]
#[serial]
fn test_toggle_button_height_from_theme_font_size() {
    let toggle = toggle_button(|| true).style(|s| s.width(50.0));
    let toggle_id = toggle.view_id();

    let mut harness = HeadlessHarness::new_with_size(toggle, 200.0, 200.0);
    harness.rebuild();

    let layout = toggle_id.get_layout().expect("Layout should exist");

    eprintln!(
        "Toggle button layout: width={}, height={}",
        layout.size.width, layout.size.height
    );

    // Expected height with font_size 14.0: 14.0 * 1.75 = 24.5
    // The toggle has aspect_ratio(2.0), so if height is set correctly to ~24.5,
    // the width should be ~49.0 (or clamped to our explicit 50.0)

    // The key assertion: height should be reasonable (around 24-25 pixels),
    // NOT expanded to fill available space
    assert!(
        layout.size.height < 50.0,
        "Toggle button height should be reasonable (~24.5), not expanded. Got {}. \
         This indicates font_size from default theme is not being used.",
        layout.size.height
    );

    // More specific check
    assert!(
        (layout.size.height - 24.5).abs() < 5.0,
        "Toggle button height should be ~24.5 (14.0 * 1.75), got {}",
        layout.size.height
    );
}

/// Test that toggle button has proper height when font_size is set on parent.
///
/// This is the workaround scenario where font_size is explicitly set.
#[test]
#[serial]
fn test_toggle_button_height_with_explicit_font_size() {
    let toggle = toggle_button(|| true);
    let toggle_id = toggle.view_id();

    // Wrap in a container that sets font_size explicitly
    let container = Container::new(toggle).style(|s| s.size(200.0, 200.0).font_size(16.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = toggle_id.get_layout().expect("Layout should exist");

    eprintln!(
        "Toggle with explicit font_size: width={}, height={}",
        layout.size.width, layout.size.height
    );

    // Expected height with font_size 16.0: 16.0 * 1.75 = 28.0
    // With aspect_ratio(2.0), width should be 56.0
    assert!(
        (layout.size.height - 28.0).abs() < 1.0,
        "Toggle button height should be ~28.0 (16.0 * 1.75), got {}",
        layout.size.height
    );
}

/// Test toggle button in a grid layout (similar to widget-gallery).
///
/// The original issue was toggle buttons expanding to fill the grid cell.
#[test]
#[serial]
fn test_toggle_button_in_grid() {
    use floem::taffy::prelude::{auto, fr};

    let toggle = toggle_button(|| true);
    let toggle_id = toggle.view_id();

    // Simulate the widget-gallery form layout
    let grid = (Label::new("Toggle:"), toggle)
        .style(|s| {
            s.grid()
                .grid_template_columns([auto(), fr(1.)])
                .items_center()
                .row_gap(20.0)
                .col_gap(10.0)
                .padding(30.0)
        })
        .style(|s| s.size(400.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(grid, 400.0, 200.0);
    harness.rebuild();

    let layout = toggle_id.get_layout().expect("Layout should exist");

    eprintln!(
        "Toggle in grid: width={}, height={}, x={}, y={}",
        layout.size.width, layout.size.height, layout.location.x, layout.location.y
    );

    // The toggle button should NOT expand to fill the grid cell height
    // Expected: height around 24.5 (14.0 * 1.75) with aspect_ratio giving width ~49
    assert!(
        layout.size.height < 100.0,
        "Toggle in grid should not expand to fill cell. Height={}, expected ~24.5",
        layout.size.height
    );
}

/// Test toggle button in grid with explicit parent font_size (workaround).
#[test]
#[serial]
fn test_toggle_button_in_grid_with_font_size() {
    use floem::taffy::prelude::{auto, fr};

    let toggle = toggle_button(|| true);
    let toggle_id = toggle.view_id();

    // Grid with explicit font_size
    let grid = (Label::new("Toggle:"), toggle)
        .style(|s| {
            s.font_size(14.0) // Explicit font_size as workaround
                .grid()
                .grid_template_columns([auto(), fr(1.)])
                .items_center()
                .row_gap(20.0)
                .col_gap(10.0)
                .padding(30.0)
        })
        .style(|s| s.size(400.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(grid, 400.0, 200.0);
    harness.rebuild();

    let layout = toggle_id.get_layout().expect("Layout should exist");

    eprintln!(
        "Toggle in grid (with font_size): width={}, height={}",
        layout.size.width, layout.size.height
    );

    // With explicit font_size, toggle should have proper dimensions
    assert!(
        (layout.size.height - 24.5).abs() < 5.0,
        "Toggle height should be ~24.5 when font_size is explicit, got {}",
        layout.size.height
    );
}

// =============================================================================
// Tests verifying the inherited context contains theme values
// =============================================================================

/// Test that verifies what's actually in the inherited context.
///
/// This diagnostic test helps understand why font_size might not be accessible.
#[test]
#[serial]
fn test_inherited_context_contents() {
    use std::cell::Cell;
    use std::rc::Rc;

    let received_font_size: Rc<Cell<Option<f32>>> = Rc::new(Cell::new(None));
    let captured = received_font_size.clone();

    let view = Empty::new().style(move |s| {
        let captured = captured.clone();
        s.size(100.0, 100.0).with_font_size_context(move |s, fs| {
            captured.set(*fs);
            s
        })
    });

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let fs = received_font_size.get();
    eprintln!("FontSize received in with_context: {:?}", fs);

    // This will show whether font_size is None or Some(14.0)
    assert!(
        fs.is_some(),
        "FontSize should be Some(14.0) from default theme, but got None. \
         This confirms the bug: default theme's root-level inherited props \
         are not accessible via with_context."
    );

    assert!(
        (fs.unwrap() - 14.0).abs() < 0.1,
        "FontSize should be 14.0 from default theme, got {:?}",
        fs
    );
}

/// Test with explicit font_size to verify with_context mechanism works.
#[test]
#[serial]
fn test_with_context_works_when_font_size_is_explicit() {
    use std::cell::Cell;
    use std::rc::Rc;

    let received_font_size: Rc<Cell<Option<f32>>> = Rc::new(Cell::new(None));
    let captured = received_font_size.clone();

    let child = Empty::new().style(move |s| {
        let captured = captured.clone();
        s.size(50.0, 50.0).with_font_size_context(move |s, fs| {
            captured.set(*fs);
            s
        })
    });

    let parent = Container::new(child).style(|s| s.size(200.0, 200.0).font_size(18.0));

    let mut harness = HeadlessHarness::new_with_size(parent, 200.0, 200.0);
    harness.rebuild();

    let fs = received_font_size.get();
    eprintln!("FontSize with explicit parent: {:?}", fs);

    // When font_size is explicitly set, it should be accessible
    assert!(
        fs.is_some(),
        "FontSize should be Some(18.0) from explicit parent, got None"
    );
    assert!(
        (fs.unwrap() - 18.0).abs() < 0.1,
        "FontSize should be 18.0 from explicit parent, got {:?}",
        fs
    );
}

// =============================================================================
// Tests for Issue #2: Context mappings stripped from class styles
// =============================================================================

/// Test that demonstrates context mappings in class styles are stripped.
///
/// When a class style is applied from class_context, any `with_context` closures
/// inside that style are removed before the style is applied. This means views
/// that rely on theme class styles with `with_context` won't get those styles.
///
/// The toggle button's height is calculated via:
/// ```ignore
/// .with_context::<FontSize>(|s, fs| s.apply_opt(*fs, |s, fs| s.height(fs * 1.75)))
/// ```
///
/// But this is defined in toggle_button_style which is applied via
/// `.class(ToggleButtonClass, |_| toggle_button_style)`.
/// When the class style is applied, the context mapping is stripped.
#[test]
#[serial]
fn test_class_style_context_mappings_stripped() {
    // Create a custom class and define a style with with_context for it
    floem::style_class!(TestContextClass);

    // A view that uses the custom class
    let child = Empty::new()
        .class(TestContextClass)
        .style(|s| s.width(100.0));
    let child_id = child.view_id();

    // Parent defines the class style WITH a with_context closure
    // This simulates how the theme defines ToggleButtonClass
    let parent = Container::new(child).style(|s| {
        s.size(200.0, 200.0)
            .font_size(20.0) // Set font_size in context
            .class(TestContextClass, |s| {
                // This with_context should set height based on font_size
                s.with_font_size_context(|s, fs| s.apply_opt(*fs, |s, fs| s.height(fs * 2.0)))
            })
    });

    let mut harness = HeadlessHarness::new_with_size(parent, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");

    eprintln!(
        "Child with class context mapping: width={}, height={}",
        layout.size.width, layout.size.height
    );

    // If context mappings in class styles worked, height would be 20.0 * 2.0 = 40.0
    // Currently this FAILS because context mappings are stripped from class styles
    assert!(
        (layout.size.height - 40.0).abs() < 0.1,
        "Child height should be 40.0 (from class style's with_context), got {}. \
         This indicates context mappings in class styles are stripped.",
        layout.size.height
    );
}

/// Test that with_context in the VIEW's own style (not class style) works.
///
/// This should pass because the view's own context mappings are not stripped.
#[test]
#[serial]
fn test_view_own_context_mappings_work() {
    // View with its own with_context (not from a class)
    let child = Empty::new().style(|s| {
        s.width(100.0)
            .with_font_size_context(|s, fs| s.apply_opt(*fs, |s, fs| s.height(fs * 2.0)))
    });
    let child_id = child.view_id();

    // Parent provides the font_size context
    let parent = Container::new(child).style(|s| s.size(200.0, 200.0).font_size(20.0));

    let mut harness = HeadlessHarness::new_with_size(parent, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");

    eprintln!(
        "Child with own context mapping: width={}, height={}",
        layout.size.width, layout.size.height
    );

    // This SHOULD pass because the view's own with_context is evaluated
    assert!(
        (layout.size.height - 40.0).abs() < 0.1,
        "Child height should be 40.0 (from view's own with_context), got {}",
        layout.size.height
    );
}
