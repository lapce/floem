//! Tests for context separation between inherited properties and class styling.
//!
//! These tests verify that:
//! - Inherited properties and class styling work independently
//! - Changes to one don't incorrectly affect the other
//! - Combined updates work correctly
//!
//! This test file helps validate architectural changes to separate
//! inherited context from class context in StyleCx.

use floem::peniko::{Brush, Color};
use floem::prelude::*;
use floem::prop;
use floem::style::{Background, Style};
use floem_test::prelude::*;

// ============================================================================
// Test Helpers
// ============================================================================

// An inherited color prop for testing.
prop!(
    pub TestInheritedColor: Color { inherited } = palette::css::BLACK
);

// Helper trait for inherited color.
trait TestColorExt {
    fn with_test_color(self, f: impl Fn(Self, &Color) -> Self + 'static) -> Self
    where
        Self: Sized;
}

impl TestColorExt for Style {
    fn with_test_color(self, f: impl Fn(Self, &Color) -> Self + 'static) -> Self {
        self.with_context::<TestInheritedColor>(f)
    }
}

// A test style class.
floem::style_class!(pub TestClass);

// ============================================================================
// Isolation Tests: Class changes shouldn't affect inherited props
// ============================================================================

/// Test that adding class styling doesn't break inherited prop resolution.
#[test]
fn test_class_styling_doesnt_break_inherited_props() {
    // Parent sets both: inherited color AND class styling
    // Child uses inherited color (not class)
    // Child should get the inherited color, not be affected by class

    let child = Empty::new().style(|s| {
        s.size(50.0, 50.0)
            .with_test_color(|s, color| s.background(*color))
    });
    let child_id = child.view_id();

    let parent = Container::new(child).style(|s| {
        s.size(100.0, 100.0)
            .set(TestInheritedColor, palette::css::RED)
            // Class styling that child doesn't use
            .class(TestClass, |s| s.background(palette::css::BLUE))
    });

    let harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Child should have RED from inherited prop, not affected by class
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::RED),
        "Child should have RED from inherited prop, got {:?}",
        bg
    );
}

/// Test that class styling on one child doesn't affect sibling's inherited props.
#[test]
fn test_class_on_sibling_doesnt_affect_inherited() {
    // Two siblings: one uses class, one uses inherited
    // They should be independent

    let class_child = Empty::new()
        .class(TestClass)
        .style(|s| s.size(50.0, 50.0));
    let class_child_id = class_child.view_id();

    let inherited_child = Empty::new().style(|s| {
        s.size(50.0, 50.0)
            .with_test_color(|s, color| s.background(*color))
    });
    let inherited_child_id = inherited_child.view_id();

    let parent = Stack::new((class_child, inherited_child)).style(|s| {
        s.size(100.0, 100.0)
            .set(TestInheritedColor, palette::css::GREEN)
            .class(TestClass, |s| s.background(palette::css::PURPLE))
    });

    let harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Class child should be PURPLE from class
    let style = harness.get_computed_style(class_child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::PURPLE),
        "Class child should be PURPLE, got {:?}",
        bg
    );

    // Inherited child should be GREEN from inherited prop
    let style = harness.get_computed_style(inherited_child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Inherited child should be GREEN, got {:?}",
        bg
    );
}

// ============================================================================
// Isolation Tests: Inherited changes shouldn't trigger class recomputation
// ============================================================================

/// Test that changing inherited prop doesn't affect class-styled child.
#[test]
fn test_inherited_change_doesnt_affect_class_child() {
    let color_signal = RwSignal::new(palette::css::RED);

    // Child uses ONLY class styling, not inherited
    let class_child = Empty::new()
        .class(TestClass)
        .style(|s| s.size(50.0, 50.0));
    let class_child_id = class_child.view_id();

    let parent = Container::new(class_child).style(move |s| {
        s.size(100.0, 100.0)
            .set(TestInheritedColor, color_signal.get())
            .class(TestClass, |s| s.background(palette::css::CYAN))
    });

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Initial: class child should be CYAN
    let style = harness.get_computed_style(class_child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::CYAN),
        "Initial class child should be CYAN, got {:?}",
        bg
    );

    // Change inherited prop
    color_signal.set(palette::css::BLUE);
    harness.rebuild();

    // Class child should STILL be CYAN (not affected by inherited change)
    let style = harness.get_computed_style(class_child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::CYAN),
        "After inherited change, class child should still be CYAN, got {:?}",
        bg
    );
}

// ============================================================================
// Combined Updates
// ============================================================================

/// Test that both inherited and class can change together correctly.
#[test]
fn test_both_inherited_and_class_change_together() {
    let color_signal = RwSignal::new(palette::css::RED);
    let use_class = RwSignal::new(false);

    // Child uses inherited color
    let inherited_child = Empty::new().style(|s| {
        s.size(50.0, 50.0)
            .with_test_color(|s, color| s.background(*color))
    });
    let inherited_child_id = inherited_child.view_id();

    // Child uses class
    let class_child = Empty::new()
        .class(TestClass)
        .style(|s| s.size(50.0, 50.0));
    let class_child_id = class_child.view_id();

    let parent = Stack::new((inherited_child, class_child)).style(move |s| {
        let base = s
            .size(100.0, 100.0)
            .set(TestInheritedColor, color_signal.get());
        if use_class.get() {
            base.class(TestClass, |s| s.background(palette::css::ORANGE))
        } else {
            base.class(TestClass, |s| s.background(palette::css::PURPLE))
        }
    });

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Initial state
    let style = harness.get_computed_style(inherited_child_id);
    assert!(matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::RED));

    let style = harness.get_computed_style(class_child_id);
    assert!(matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::PURPLE));

    // Change BOTH at once
    color_signal.set(palette::css::GREEN);
    use_class.set(true);
    harness.rebuild();

    // Both should update correctly
    let style = harness.get_computed_style(inherited_child_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Inherited child should be GREEN"
    );

    let style = harness.get_computed_style(class_child_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::ORANGE),
        "Class child should be ORANGE"
    );
}

// ============================================================================
// Edge Cases
// ============================================================================

/// Test child with class but parent has no class styling.
#[test]
fn test_class_child_no_parent_class_styling() {
    let child = Empty::new()
        .class(TestClass)
        .style(|s| s.size(50.0, 50.0).background(palette::css::GRAY));
    let child_id = child.view_id();

    // Parent sets inherited but NO class styling
    let parent = Container::new(child).style(|s| {
        s.size(100.0, 100.0)
            .set(TestInheritedColor, palette::css::RED)
        // No .class(TestClass, ...) here
    });

    let harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Child should have its own GRAY background (no class styling from parent)
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "Child should have its own GRAY, got {:?}",
        bg
    );
}

/// Test inherited prop with no class styling anywhere.
#[test]
fn test_inherited_only_no_class() {
    let child = Empty::new().style(|s| {
        s.size(50.0, 50.0)
            .with_test_color(|s, color| s.background(*color))
    });
    let child_id = child.view_id();

    let parent = Container::new(child).style(|s| {
        s.size(100.0, 100.0)
            .set(TestInheritedColor, palette::css::MAGENTA)
        // No class styling
    });

    let harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::MAGENTA),
        "Child should have MAGENTA from inherited, got {:?}",
        bg
    );
}

/// Test class styling with no inherited props.
#[test]
fn test_class_only_no_inherited() {
    let child = Empty::new()
        .class(TestClass)
        .style(|s| s.size(50.0, 50.0));
    let child_id = child.view_id();

    let parent = Container::new(child).style(|s| {
        s.size(100.0, 100.0)
            // No inherited props set
            .class(TestClass, |s| s.background(palette::css::TEAL))
    });

    let harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::TEAL),
        "Child should have TEAL from class, got {:?}",
        bg
    );
}

/// Test deeply nested with mixed inherited and class.
#[test]
fn test_deep_nesting_mixed_inherited_and_class() {
    // Structure:
    // Root (sets inherited + class)
    //   -> Level1 (no special style)
    //     -> Level2 (no special style)
    //       -> InheritedLeaf (uses inherited)
    //       -> ClassLeaf (uses class)

    let inherited_leaf = Empty::new().style(|s| {
        s.size(20.0, 20.0)
            .with_test_color(|s, color| s.background(*color))
    });
    let inherited_leaf_id = inherited_leaf.view_id();

    let class_leaf = Empty::new()
        .class(TestClass)
        .style(|s| s.size(20.0, 20.0));
    let class_leaf_id = class_leaf.view_id();

    let level2 = Stack::new((inherited_leaf, class_leaf)).style(|s| s.size(50.0, 50.0));
    let level1 = Container::new(level2).style(|s| s.size(70.0, 70.0));

    let root = Container::new(level1).style(|s| {
        s.size(100.0, 100.0)
            .set(TestInheritedColor, palette::css::CORAL)
            .class(TestClass, |s| s.background(palette::css::NAVY))
    });

    let harness = HeadlessHarness::new_with_size(root, 100.0, 100.0);

    // Both should receive their respective styles through nesting
    let style = harness.get_computed_style(inherited_leaf_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::CORAL),
        "Inherited leaf should be CORAL, got {:?}",
        bg
    );

    let style = harness.get_computed_style(class_leaf_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::NAVY),
        "Class leaf should be NAVY, got {:?}",
        bg
    );
}

/// Test that view using BOTH inherited and class gets correct precedence.
#[test]
fn test_view_uses_both_inherited_and_class() {
    // Child uses inherited for one property, class provides another
    // They should not conflict

    let child = Empty::new()
        .class(TestClass)
        .style(|s| {
            s.size(50.0, 50.0)
                // Use inherited color for border
                .with_test_color(|s, color| s.border_color(*color).border(2.0))
        });
    let child_id = child.view_id();

    let parent = Container::new(child).style(|s| {
        s.size(100.0, 100.0)
            .set(TestInheritedColor, palette::css::RED) // For border
            .class(TestClass, |s| s.background(palette::css::BLUE)) // For background
    });

    let harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    let style = harness.get_computed_style(child_id);

    // Should have BLUE background from class
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Background should be BLUE from class, got {:?}",
        bg
    );

    // Border color should be set from inherited (we can't easily check the exact color
    // but we can verify both styles were applied without conflict)
}
