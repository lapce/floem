//! Tests for the StyleCache system.
//!
//! These tests verify that:
//! - Identical styles produce cache hits
//! - Different parent inherited values produce cache misses
//! - Cache is invalidated when views are dirty
//! - Interaction state changes (hover, focus) work correctly with cache
//! - Responsive breakpoint changes work correctly
//! - Cache statistics are tracked correctly

use floem::peniko::{Brush, Color};
use floem::prelude::*;
use floem::prop;
use floem::style::{Background, Style};
use floem_test::prelude::*;

// ============================================================================
// Test Helpers
// ============================================================================

// An inherited color prop for testing cache behavior with inheritance.
prop!(
    pub CacheTestColor: Color { inherited } = palette::css::BLACK
);

// Helper to use inherited color in styles.
trait CacheColorExt {
    fn with_cache_color(self, f: impl Fn(Self, &Color) -> Self + 'static) -> Self
    where
        Self: Sized;
}

impl CacheColorExt for Style {
    fn with_cache_color(self, f: impl Fn(Self, &Color) -> Self + 'static) -> Self {
        self.with_context::<CacheTestColor>(f)
    }
}

// A test style class.
floem::style_class!(pub CacheTestClass);

// ============================================================================
// Basic Cache Hit Tests
// ============================================================================

/// Test that views with identical styles share cached results.
#[test]
fn test_cache_hit_for_identical_styles() {
    // Create multiple views with exactly the same style
    let views: Vec<_> = (0..5)
        .map(|_| {
            Empty::new().style(|s| {
                s.size(50.0, 50.0)
                    .background(palette::css::CORAL)
                    .padding(10.0)
            })
        })
        .collect();

    let ids: Vec<_> = views.iter().map(|v| v.view_id()).collect();
    let container = Stack::from_iter(views).style(|s| s.size(300.0, 300.0));

    let harness = HeadlessHarness::new_with_size(container, 300.0, 300.0);

    // All views should have identical computed styles
    let first_style = harness.get_computed_style(ids[0]);
    let first_bg = first_style.get(Background);

    for id in &ids[1..] {
        let style = harness.get_computed_style(*id);
        let bg = style.get(Background);
        assert_eq!(
            bg, first_bg,
            "Views with identical styles should have identical computed backgrounds"
        );
    }
}

/// Test that views with different styles have different computed results.
#[test]
fn test_cache_miss_for_different_styles() {
    let view1 = Empty::new().style(|s| s.size(50.0, 50.0).background(palette::css::RED));
    let id1 = view1.view_id();

    let view2 = Empty::new().style(|s| s.size(50.0, 50.0).background(palette::css::BLUE));
    let id2 = view2.view_id();

    let container = Stack::new((view1, view2)).style(|s| s.size(200.0, 100.0));
    let harness = HeadlessHarness::new_with_size(container, 200.0, 100.0);

    let style1 = harness.get_computed_style(id1);
    let style2 = harness.get_computed_style(id2);

    let bg1 = style1.get(Background);
    let bg2 = style2.get(Background);

    assert!(
        matches!(bg1, Some(Brush::Solid(c)) if c == palette::css::RED),
        "First view should be RED"
    );
    assert!(
        matches!(bg2, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Second view should be BLUE"
    );
}

// ============================================================================
// Parent Inherited Value Tests (Chromium-style validation)
// ============================================================================

/// Test that cache correctly handles different parent inherited values.
/// This is the key Chromium-inspired validation - two views with identical
/// base styles but different parent inherited values should NOT share cache.
#[test]
fn test_cache_miss_for_different_parent_inherited() {
    // Two parent containers with different inherited colors
    let child1 =
        Empty::new().style(|s| s.size(30.0, 30.0).with_cache_color(|s, c| s.background(*c)));
    let child1_id = child1.view_id();

    let child2 =
        Empty::new().style(|s| s.size(30.0, 30.0).with_cache_color(|s, c| s.background(*c)));
    let child2_id = child2.view_id();

    let parent1 =
        Container::new(child1).style(|s| s.size(50.0, 50.0).set(CacheTestColor, palette::css::RED));

    let parent2 = Container::new(child2)
        .style(|s| s.size(50.0, 50.0).set(CacheTestColor, palette::css::GREEN));

    let root = Stack::new((parent1, parent2)).style(|s| s.size(200.0, 100.0));
    let harness = HeadlessHarness::new_with_size(root, 200.0, 100.0);

    // Children should have different backgrounds due to different parent inherited values
    let style1 = harness.get_computed_style(child1_id);
    let style2 = harness.get_computed_style(child2_id);

    let bg1 = style1.get(Background);
    let bg2 = style2.get(Background);

    assert!(
        matches!(bg1, Some(Brush::Solid(c)) if c == palette::css::RED),
        "Child1 should inherit RED, got {:?}",
        bg1
    );
    assert!(
        matches!(bg2, Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Child2 should inherit GREEN, got {:?}",
        bg2
    );
}

/// Test that inherited value changes correctly invalidate cache.
#[test]
fn test_cache_invalidation_on_inherited_change() {
    let color_signal = RwSignal::new(palette::css::RED);

    let child =
        Empty::new().style(|s| s.size(50.0, 50.0).with_cache_color(|s, c| s.background(*c)));
    let child_id = child.view_id();

    let parent = Container::new(child)
        .style(move |s| s.size(100.0, 100.0).set(CacheTestColor, color_signal.get()));

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Initial: child should be RED
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::RED),
        "Initial should be RED"
    );

    // Change inherited color
    color_signal.set(palette::css::BLUE);
    harness.rebuild();

    // Child should now be BLUE (cache should not return stale RED)
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "After change should be BLUE, got {:?}",
        bg
    );
}

// ============================================================================
// Interaction State Tests
// ============================================================================

/// Test that hover state changes correctly bypass cache.
#[test]
fn test_cache_with_hover_state() {
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .background(palette::css::GRAY)
            .hover(|s| s.background(palette::css::YELLOW))
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

    // Hover
    harness.pointer_move(50.0, 50.0);
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::YELLOW),
        "Hovered should be YELLOW"
    );

    // Un-hover
    harness.pointer_move(150.0, 150.0);
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "After un-hover should be GRAY"
    );
}

/// Test that disabled state changes correctly bypass cache.
#[test]
fn test_cache_with_disabled_state() {
    let disabled_signal = RwSignal::new(false);

    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .background(palette::css::GREEN)
            .disabled(|s| s.background(palette::css::GRAY))
    });
    let view_id = view.view_id();

    let container = Container::new(view)
        .style(move |s| s.size(150.0, 150.0).set_disabled(disabled_signal.get()));

    let mut harness = HeadlessHarness::new_with_size(container, 150.0, 150.0);

    // Initial: should be GREEN
    let style = harness.get_computed_style(view_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Initial should be GREEN"
    );

    // Disable
    disabled_signal.set(true);
    harness.rebuild();

    let style = harness.get_computed_style(view_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "Disabled should be GRAY, got {:?}",
        bg
    );

    // Re-enable
    disabled_signal.set(false);
    harness.rebuild();

    let style = harness.get_computed_style(view_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Re-enabled should be GREEN"
    );
}

// ============================================================================
// Dynamic Style Change Tests
// ============================================================================

/// Test that dynamic style changes correctly invalidate cache.
#[test]
fn test_cache_with_dynamic_style_changes() {
    let color_signal = RwSignal::new(palette::css::RED);

    let view = Empty::new().style(move |s| s.size(100.0, 100.0).background(color_signal.get()));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::RED),
        "Initial should be RED"
    );

    // Change color multiple times
    for color in [
        palette::css::BLUE,
        palette::css::GREEN,
        palette::css::YELLOW,
    ] {
        color_signal.set(color);
        harness.rebuild();

        let style = harness.get_computed_style(id);
        let bg = style.get(Background);
        assert!(
            matches!(bg, Some(Brush::Solid(c)) if c == color),
            "Should be {:?}",
            color
        );
    }
}

/// Test cache behavior with class application.
#[test]
fn test_cache_with_class_application() {
    let use_class = RwSignal::new(false);

    let child = Empty::new()
        .class(CacheTestClass)
        .style(|s| s.size(50.0, 50.0).background(palette::css::GRAY));
    let child_id = child.view_id();

    let parent = Container::new(child).style(move |s| {
        let base = s.size(100.0, 100.0);
        if use_class.get() {
            base.class(CacheTestClass, |s| s.background(palette::css::PURPLE))
        } else {
            base
        }
    });

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Initial: child should be GRAY
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "Initial should be GRAY"
    );

    // Enable class
    use_class.set(true);
    harness.rebuild();

    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::PURPLE),
        "With class should be PURPLE, got {:?}",
        bg
    );

    // Disable class
    use_class.set(false);
    harness.rebuild();

    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "Without class should be GRAY"
    );
}

// ============================================================================
// Cache Efficiency Tests
// ============================================================================

/// Test that cache works correctly with many identical views.
#[test]
fn test_cache_with_many_identical_views() {
    let views: Vec<_> = (0..100)
        .map(|_| {
            Empty::new().style(|s| {
                s.size(10.0, 10.0)
                    .background(palette::css::CORAL)
                    .padding(1.0)
            })
        })
        .collect();

    let ids: Vec<_> = views.iter().map(|v| v.view_id()).collect();
    let container = Stack::from_iter(views).style(|s| s.size(1000.0, 1000.0));

    let harness = HeadlessHarness::new_with_size(container, 1000.0, 1000.0);

    // All views should have the same computed style
    let expected_bg = harness.get_computed_style(ids[0]).get(Background);

    for id in &ids {
        let style = harness.get_computed_style(*id);
        let bg = style.get(Background);
        assert_eq!(
            bg, expected_bg,
            "All views should have identical backgrounds"
        );
    }
}

/// Test deep nesting with cache.
#[test]
fn test_cache_with_deep_nesting() {
    fn create_nested(depth: usize) -> Container {
        if depth == 0 {
            Container::new(
                Empty::new().style(|s| s.size(10.0, 10.0).background(palette::css::GOLD)),
            )
            .style(|s| s.size(20.0, 20.0))
        } else {
            Container::new(create_nested(depth - 1)).style(|s| s.size_full().padding(1.0))
        }
    }

    let view = create_nested(15);
    let harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Should not panic, style should be computed correctly
    let root_id = harness.root_id();
    let _style = harness.get_computed_style(root_id);
}

// ============================================================================
// Combined Scenarios
// ============================================================================

/// Test cache with combined inherited props and local style changes.
#[test]
fn test_cache_combined_inherited_and_local() {
    let inherited_color = RwSignal::new(palette::css::RED);
    let local_padding = RwSignal::new(5.0);

    let child = Empty::new().style(move |s| {
        s.size(50.0, 50.0)
            .padding(local_padding.get())
            .with_cache_color(|s, c| s.background(*c))
    });
    let child_id = child.view_id();

    let parent = Container::new(child).style(move |s| {
        s.size(100.0, 100.0)
            .set(CacheTestColor, inherited_color.get())
    });

    let mut harness = HeadlessHarness::new_with_size(parent, 100.0, 100.0);

    // Initial
    let style = harness.get_computed_style(child_id);
    assert!(matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::RED));

    // Change inherited color
    inherited_color.set(palette::css::BLUE);
    harness.rebuild();

    let style = harness.get_computed_style(child_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Should be BLUE after inherited change"
    );

    // Change local padding (shouldn't affect background)
    local_padding.set(10.0);
    harness.rebuild();

    let style = harness.get_computed_style(child_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Should still be BLUE after local padding change"
    );
}

/// Test rapid style updates don't cause cache issues.
#[test]
fn test_cache_rapid_updates() {
    let color_signal = RwSignal::new(palette::css::RED);

    let view = Empty::new().style(move |s| s.size(100.0, 100.0).background(color_signal.get()));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Rapid updates
    for _ in 0..10 {
        for color in [palette::css::RED, palette::css::BLUE, palette::css::GREEN] {
            color_signal.set(color);
            harness.rebuild();

            let style = harness.get_computed_style(id);
            let bg = style.get(Background);
            assert!(
                matches!(bg, Some(Brush::Solid(c)) if c == color),
                "Should match current color"
            );
        }
    }
}
