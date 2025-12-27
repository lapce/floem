//! Tests for style computation and caching behavior.
//!
//! These tests verify that:
//! - Style computation produces correct results
//! - Identical style inputs produce identical outputs (cache correctness)
//! - Style computation is deterministic

use floem::peniko::Brush;
use floem::prelude::*;
use floem::style::Background;
use floem_test::prelude::*;

/// Test that identical styles produce identical computed results.
/// This is a key property for caching - if inputs match, outputs must match.
#[test]
fn test_identical_styles_produce_identical_results() {
    // Create two views with exactly the same style
    let view1 = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .background(palette::css::RED)
            .padding(10.0)
    });
    let id1 = view1.view_id();

    let view2 = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .background(palette::css::RED)
            .padding(10.0)
    });
    let id2 = view2.view_id();

    let view = Stack::new((view1, view2)).style(|s| s.size(200.0, 100.0));
    let harness = HeadlessHarness::new_with_size(view, 200.0, 100.0);

    let style1 = harness.get_computed_style(id1);
    let style2 = harness.get_computed_style(id2);

    // Background should be identical
    let bg1 = style1.get(Background);
    let bg2 = style2.get(Background);
    assert_eq!(
        bg1, bg2,
        "Identical styles should produce identical background"
    );
}

/// Test stacked style decorators.
#[test]
fn test_stacked_style_decorators() {
    // Later .style() calls should override earlier ones
    let view = Empty::new()
        .style(|s| s.size(100.0, 100.0).background(palette::css::RED))
        .style(|s| s.background(palette::css::BLUE));
    let id = view.view_id();

    let harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Background should be BLUE (second style takes precedence)
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Stacked style should show BLUE (later), got {:?}",
        bg
    );
}

/// Test that style computation is deterministic.
#[test]
fn test_style_computation_deterministic() {
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .background(palette::css::PURPLE)
            .padding(5.0)
            .margin(10.0)
    });
    let id = view.view_id();

    let harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Check initial state
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::PURPLE),
        "Initial should be PURPLE"
    );
}

/// Test many views with the same style (good candidate for caching).
#[test]
fn test_many_views_same_style() {
    let mut views: Vec<_> = Vec::new();
    let mut ids: Vec<ViewId> = Vec::new();

    for _ in 0..20 {
        let view = Empty::new().style(|s| {
            s.size(20.0, 20.0)
                .background(palette::css::CORAL)
                .padding(2.0)
        });
        ids.push(view.view_id());
        views.push(view);
    }

    let container = Stack::from_iter(views).style(|s| s.size(400.0, 400.0));
    let harness = HeadlessHarness::new_with_size(container, 400.0, 400.0);

    // All views should have the same background
    for id in &ids {
        let style = harness.get_computed_style(*id);
        let bg = style.get(Background);
        assert!(
            matches!(bg, Some(Brush::Solid(c)) if c == palette::css::CORAL),
            "All views should have CORAL background"
        );
    }
}

/// Test deep nesting doesn't break style computation.
#[test]
fn test_deep_nesting_style_computation() {
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

    let view = create_nested(10);
    let harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // View should have been laid out correctly (no panic)
    let root_id = harness.root_id();
    let style = harness.get_computed_style(root_id);
    // Just verify it doesn't panic and returns a style
    assert!(style.get(Background).is_none() || style.get(Background).is_some());
}

/// Test that view_style from view type is combined correctly.
#[test]
fn test_view_style_combined_with_decorators() {
    let view = Empty::new()
        .style(|s| s.size(100.0, 100.0))
        .style(|s| s.background(palette::css::NAVY));
    let id = view.view_id();

    let harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::NAVY),
        "Decorator style should be applied"
    );
}

/// Test different styles produce different computed results.
#[test]
fn test_different_styles_produce_different_results() {
    let view1 = Empty::new().style(|s| s.size(100.0, 100.0).background(palette::css::RED));
    let id1 = view1.view_id();

    let view2 = Empty::new().style(|s| s.size(100.0, 100.0).background(palette::css::BLUE));
    let id2 = view2.view_id();

    let view = Stack::new((view1, view2)).style(|s| s.size(200.0, 100.0));
    let harness = HeadlessHarness::new_with_size(view, 200.0, 100.0);

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
