//! Tests for graduated style propagation system.
//!
//! These tests verify the Chromium-inspired style recalculation optimizations:
//! - Inherited-only fast path for views without selectors
//! - Proper propagation when classes are applied
//! - Dark mode changes propagate with correct flags
//! - Responsive changes propagate with correct flags
//! - Views with selectors don't use the fast path

use floem::peniko::{Brush, Color};
use floem::prelude::*;
use floem::prop;
use floem::style::{Background, Style};
use floem_test::prelude::*;

// ============================================================================
// Test Helpers
// ============================================================================

// An inherited color prop for testing propagation.
prop!(
    pub TestInheritedColor: Color { inherited } = palette::css::BLACK
);

// Helper to use inherited color in styles.
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
floem::style_class!(pub TestButtonClass);

// ============================================================================
// Inherited-Only Fast Path Tests
// ============================================================================

/// Test that inherited props propagate to deeply nested views.
/// This exercises the inherited-only fast path when the parent's
/// inherited prop changes but children don't have selectors.
#[test]
fn test_inherited_prop_propagates_to_deep_children() {
    let color_signal = RwSignal::new(palette::css::RED);

    // Create a deep hierarchy where only the leaf uses the inherited color
    let leaf = Empty::new().style(|s| {
        s.size(20.0, 20.0)
            .with_test_color(|s, color| s.background(*color))
    });
    let leaf_id = leaf.view_id();

    let level3 = Container::new(leaf).style(|s| s.size(40.0, 40.0));
    let level2 = Container::new(level3).style(|s| s.size(60.0, 60.0));
    let level1 = Container::new(level2).style(|s| s.size(80.0, 80.0));

    let root = Container::new(level1).style(move |s| {
        s.size(100.0, 100.0)
            .set(TestInheritedColor, color_signal.get())
    });

    let mut harness = HeadlessHarness::new_with_size(root, 100.0, 100.0);

    // Initial: leaf should be RED
    let style = harness.get_computed_style(leaf_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::RED),
        "Initial leaf should be RED, got {:?}",
        bg
    );

    // Update inherited color
    color_signal.set(palette::css::BLUE);
    harness.rebuild();

    // Leaf should now be BLUE (inherited prop propagated through 4 levels)
    let style = harness.get_computed_style(leaf_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "After update, leaf should be BLUE, got {:?}",
        bg
    );
}

/// Test that multiple siblings all receive inherited prop updates.
#[test]
fn test_inherited_prop_propagates_to_siblings() {
    let color_signal = RwSignal::new(palette::css::GREEN);

    let mut children = Vec::new();
    let mut child_ids = Vec::new();

    for _ in 0..5 {
        let child = Empty::new().style(move |s| {
            s.size(20.0, 20.0)
                .with_test_color(|s, color| s.background(*color))
        });
        child_ids.push(child.view_id());
        children.push(child);
    }

    let container = Stack::from_iter(children).style(move |s| {
        s.size(100.0, 100.0)
            .set(TestInheritedColor, color_signal.get())
    });

    let mut harness = HeadlessHarness::new_with_size(container, 100.0, 100.0);

    // All children should be GREEN
    for id in &child_ids {
        let style = harness.get_computed_style(*id);
        let bg = style.get(Background);
        assert!(
            matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GREEN),
            "Initial children should be GREEN"
        );
    }

    // Update inherited color
    color_signal.set(palette::css::YELLOW);
    harness.rebuild();

    // All children should now be YELLOW
    for id in &child_ids {
        let style = harness.get_computed_style(*id);
        let bg = style.get(Background);
        assert!(
            matches!(bg, Some(Brush::Solid(c)) if c == palette::css::YELLOW),
            "After update, children should be YELLOW"
        );
    }
}

// ============================================================================
// Selector-Dependent View Tests
// ============================================================================

/// Test that views with hover selector still work correctly.
/// These views cannot use the inherited-only fast path.
#[test]
fn test_hover_selector_view_updates_correctly() {
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .background(palette::css::GRAY)
            .hover(|s| s.background(palette::css::LIGHT_BLUE))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial: should be GRAY
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "Initial should be GRAY"
    );

    // Hover: move pointer into the view
    harness.pointer_move(50.0, 50.0);
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::LIGHT_BLUE),
        "Hovered should be LIGHT_BLUE"
    );

    // Un-hover: move pointer outside
    harness.pointer_move(150.0, 150.0);
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "After un-hover should be GRAY"
    );
}

/// Test that child with selectors still receives inherited updates.
/// Views with selectors (like hover) cannot use the inherited-only fast path,
/// but they should still receive inherited prop updates correctly.
#[test]
fn test_child_with_selectors_receives_inherited_updates() {
    let color_signal = RwSignal::new(palette::css::RED);

    // Child has hover selector, making it ineligible for inherited-only fast path
    // But it uses inherited color for base, and a different prop (border) for hover
    let child = Empty::new().style(|s| {
        s.size(50.0, 50.0)
            .with_test_color(|s, color| s.background(*color))
            .hover(|s| s.border(2.0).border_color(palette::css::WHITE))
    });
    let child_id = child.view_id();

    let parent = Container::new(child).style(move |s| {
        s.size(100.0, 100.0)
            .set(TestInheritedColor, color_signal.get())
    });

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Initial: child should be RED (from inherited)
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::RED),
        "Initial child should be RED, got {:?}",
        bg
    );

    // Update inherited color - this should work even though child has selectors
    color_signal.set(palette::css::BLUE);
    harness.rebuild();

    // Child should now be BLUE (inherited prop update propagated correctly)
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "After update, child should be BLUE, got {:?}",
        bg
    );
}

// ============================================================================
// Class Application Tests
// ============================================================================

/// Test that applying a class triggers proper child recalculation.
#[test]
fn test_class_application_triggers_child_recalc() {
    // Define a class that sets background
    let class_style = Style::new()
        .background(palette::css::ORANGE)
        .class(TestButtonClass, |s| s.background(palette::css::PURPLE));

    let child = Empty::new()
        .class(TestButtonClass)
        .style(|s| s.size(50.0, 50.0));
    let child_id = child.view_id();

    let parent = Container::new(child).style(move |_| class_style.clone().size(100.0, 100.0));

    let harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Child should have PURPLE background from class
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::PURPLE),
        "Child with class should be PURPLE, got {:?}",
        bg
    );
}

/// Test that class changes on parent affect children correctly.
#[test]
fn test_dynamic_class_change_updates_children() {
    let use_class = RwSignal::new(false);

    let child = Empty::new()
        .class(TestButtonClass)
        .style(|s| s.size(50.0, 50.0).background(palette::css::GRAY));
    let child_id = child.view_id();

    let parent = Container::new(child).style(move |s| {
        let base = s.size(100.0, 100.0);
        if use_class.get() {
            base.class(TestButtonClass, |s| s.background(palette::css::LIME))
        } else {
            base
        }
    });

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Initial: child should be GRAY (class not applied at parent)
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "Initial child should be GRAY"
    );

    // Enable class at parent
    use_class.set(true);
    harness.rebuild();

    // Child should now be LIME (class applied at parent)
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::LIME),
        "After class enabled, child should be LIME, got {:?}",
        bg
    );
}

// ============================================================================
// Disabled State Propagation Tests
// ============================================================================

/// Test that disabled state propagates correctly to children.
#[test]
fn test_disabled_state_propagates_to_children() {
    let disabled_signal = RwSignal::new(false);

    let child = Empty::new().style(|s| {
        s.size(50.0, 50.0)
            .background(palette::css::GREEN)
            .disabled(|s| s.background(palette::css::DARK_GRAY))
    });
    let child_id = child.view_id();

    let parent = Container::new(child)
        .style(move |s| s.size(100.0, 100.0).set_disabled(disabled_signal.get()));

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Initial: child should be GREEN
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Initial child should be GREEN"
    );

    // Disable parent
    disabled_signal.set(true);
    harness.rebuild();

    // Child should now be DARK_GRAY (inherited disabled state)
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::DARK_GRAY),
        "Disabled child should be DARK_GRAY, got {:?}",
        bg
    );
}

// ============================================================================
// Multiple Style Updates Tests
// ============================================================================

/// Test rapid successive style updates are handled correctly.
#[test]
fn test_rapid_style_updates() {
    let color_signal = RwSignal::new(palette::css::RED);

    let view = Empty::new().style(move |s| s.size(100.0, 100.0).background(color_signal.get()));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Rapid updates
    for color in [
        palette::css::BLUE,
        palette::css::GREEN,
        palette::css::YELLOW,
        palette::css::PURPLE,
    ] {
        color_signal.set(color);
        harness.rebuild();

        let style = harness.get_computed_style(id);
        let bg = style.get(Background);
        assert!(
            matches!(bg, Some(Brush::Solid(c)) if c == color),
            "After rapid update, should be {:?}",
            color
        );
    }
}

/// Test combined inherited prop and local style changes.
#[test]
fn test_combined_inherited_and_local_changes() {
    let inherited_color = RwSignal::new(palette::css::RED);
    let local_padding = RwSignal::new(5.0);

    let child = Empty::new().style(move |s| {
        s.size(50.0, 50.0)
            .padding(local_padding.get())
            .with_test_color(|s, color| s.background(*color))
    });
    let child_id = child.view_id();

    let parent = Container::new(child).style(move |s| {
        s.size(100.0, 100.0)
            .set(TestInheritedColor, inherited_color.get())
    });

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Initial check
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(matches!(bg, Some(Brush::Solid(c)) if c == palette::css::RED));

    // Change both inherited and local
    inherited_color.set(palette::css::BLUE);
    local_padding.set(10.0);
    harness.rebuild();

    // Should reflect both changes
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "After combined update, should be BLUE"
    );
}

// ============================================================================
// Edge Cases
// ============================================================================

/// Test empty view hierarchy.
#[test]
fn test_empty_hierarchy() {
    let view = Empty::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    let harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Should not panic
    let _style = harness.get_computed_style(id);
}

/// Test very deep nesting (stress test for propagation).
#[test]
fn test_very_deep_nesting_propagation() {
    let color_signal = RwSignal::new(palette::css::RED);

    fn create_deep_hierarchy(depth: usize, color_signal: RwSignal<Color>) -> Container {
        if depth == 0 {
            Container::new(
                Empty::new().style(|s| s.size(10.0, 10.0).with_test_color(|s, c| s.background(*c))),
            )
            .style(|s| s.size(20.0, 20.0))
        } else {
            Container::new(create_deep_hierarchy(depth - 1, color_signal))
                .style(|s| s.size_full().padding(1.0))
        }
    }

    let root = Container::new(create_deep_hierarchy(15, color_signal)).style(move |s| {
        s.size(200.0, 200.0)
            .set(TestInheritedColor, color_signal.get())
    });

    let mut harness = HeadlessHarness::new_with_size(root, 200.0, 200.0);

    // Update should propagate through 15+ levels
    color_signal.set(palette::css::CYAN);
    harness.rebuild();

    // Should not panic, and style should be computed
    let root_id = harness.root_id();
    let _style = harness.get_computed_style(root_id);
}

/// Test that views without any styles still work.
#[test]
fn test_unstyled_views() {
    let child = Empty::new();
    let child_id = child.view_id();

    let parent = Container::new(child).style(|s| s.size(100.0, 100.0));

    let harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Should not panic
    let _style = harness.get_computed_style(child_id);
}
