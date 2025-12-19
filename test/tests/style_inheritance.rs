//! Tests for style inheritance through the view tree.
//!
//! These tests verify that:
//! - Inherited style props flow correctly from parent to child views
//! - Non-inherited props don't flow to children
//! - Nested views can override inherited props

use floem::peniko::{Brush, Color};
use floem::prelude::*;
use floem::prop;
use floem::style::{Background, Style};
use floem_test::prelude::*;

// Define an inherited prop
prop!(
    pub InheritedColorProp: Color { inherited } = palette::css::BLACK
);

/// Helper to access inherited color in styles
trait InheritedColorExt {
    fn with_inherited_color(self, f: impl Fn(Self, &Color) -> Self + 'static) -> Self
    where
        Self: Sized;
}

impl InheritedColorExt for Style {
    fn with_inherited_color(self, f: impl Fn(Self, &Color) -> Self + 'static) -> Self {
        self.with_context::<InheritedColorProp>(f)
    }
}

/// Test that inherited props flow from parent to child.
#[test]
fn test_inherited_prop_flows_to_child() {
    let child = Empty::new().style(|s| {
        s.size(50.0, 50.0)
            .with_inherited_color(|s, color| s.background(*color))
    });
    let child_id = child.view_id();

    let parent = Container::new(child).style(|s| {
        s.size(100.0, 100.0)
            .set(InheritedColorProp, palette::css::RED)
    });

    let harness = TestHarness::new_with_size(parent, 100.0, 100.0);

    // Child should have RED background from inherited prop
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::RED),
        "Child should inherit RED from parent, got {:?}",
        bg
    );
}

/// Test that child can override inherited prop.
#[test]
fn test_child_can_override_inherited_prop() {
    let child = Empty::new().style(|s| {
        s.size(50.0, 50.0)
            // Override the inherited prop
            .set(InheritedColorProp, palette::css::BLUE)
            .with_inherited_color(|s, color| s.background(*color))
    });
    let child_id = child.view_id();

    let parent = Container::new(child).style(|s| {
        s.size(100.0, 100.0)
            .set(InheritedColorProp, palette::css::RED)
    });

    let harness = TestHarness::new_with_size(parent, 100.0, 100.0);

    // Child should have BLUE background (overridden)
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Child should override to BLUE, got {:?}",
        bg
    );
}

/// Test deeply nested inheritance.
#[test]
fn test_deeply_nested_inheritance() {
    let grandchild = Empty::new().style(|s| {
        s.size(25.0, 25.0)
            .with_inherited_color(|s, color| s.background(*color))
    });
    let grandchild_id = grandchild.view_id();

    let child = Container::new(grandchild).style(|s| s.size(50.0, 50.0));

    let parent = Container::new(child).style(|s| {
        s.size(100.0, 100.0)
            .set(InheritedColorProp, palette::css::GREEN)
    });

    let harness = TestHarness::new_with_size(parent, 100.0, 100.0);

    // Grandchild should inherit GREEN from grandparent
    let style = harness.get_computed_style(grandchild_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Grandchild should inherit GREEN from grandparent, got {:?}",
        bg
    );
}

/// Test that inherited prop updates propagate to children.
#[test]
fn test_inherited_prop_updates_propagate() {
    let color_signal = RwSignal::new(palette::css::RED);

    let child = Empty::new().style(|s| {
        s.size(50.0, 50.0)
            .with_inherited_color(|s, color| s.background(*color))
    });
    let child_id = child.view_id();

    let parent = Container::new(child).style(move |s| {
        s.size(100.0, 100.0)
            .set(InheritedColorProp, color_signal.get())
    });

    let mut harness = TestHarness::new_with_size(parent, 100.0, 100.0);

    // Initial: child should be RED
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::RED),
        "Initial child should be RED, got {:?}",
        bg
    );

    // Update the signal
    color_signal.set(palette::css::BLUE);
    harness.rebuild();

    // Child should now be BLUE
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "After update, child should be BLUE, got {:?}",
        bg
    );
}
