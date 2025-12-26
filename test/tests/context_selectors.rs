//! Tests for selector detection inside `with_context` closures.
//!
//! These tests verify that selectors (like `.hover()` and `.active()`) defined
//! inside `with_context` closures are properly detected by floem's selector
//! detection mechanism.
//!
//! This was a bug where selectors inside `with_context` were invisible because
//! they were only created when the context mapping function was executed, but
//! selector detection happened before that.
//!
//! The fix probes the context mapping at construction time to discover selectors.

use floem::peniko::{Brush, Color};
use floem::prelude::*;
use floem::prop;
use floem::style::{Background, Style, StyleSelector};
use floem_test::prelude::*;

// Define a simple theme prop for testing
prop!(
    pub TestThemeProp: TestTheme { inherited } = TestTheme::default()
);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TestTheme {
    pub primary: Color,
    pub hover: Color,
    pub active: Color,
}

impl Default for TestTheme {
    fn default() -> Self {
        Self {
            primary: palette::css::BLUE,
            hover: palette::css::LIGHT_BLUE,
            active: palette::css::DARK_BLUE,
        }
    }
}

impl floem::style::StylePropValue for TestTheme {
    fn interpolate(&self, _other: &Self, _value: f64) -> Option<Self> {
        None
    }
}

/// Helper extension trait for using the test theme
trait TestThemeExt {
    fn with_test_theme(self, f: impl Fn(Self, &TestTheme) -> Self + 'static) -> Self
    where
        Self: Sized;
}

impl TestThemeExt for Style {
    fn with_test_theme(self, f: impl Fn(Self, &TestTheme) -> Self + 'static) -> Self {
        self.with_context::<TestThemeProp>(f)
    }
}

/// Test that selectors defined inside `with_context` are detected.
#[test]
fn test_active_selector_detected_inside_with_context() {
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0).with_test_theme(|s, t| {
            s.background(t.primary)
                .active(|s| s.background(palette::css::RED))
        })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // The Active selector should be detected even though it's inside with_context
    assert!(
        harness.has_style_for_selector(id, StyleSelector::Active),
        "Active selector should be detected inside with_context"
    );
}

/// Test that hover selectors inside `with_context` are detected.
#[test]
fn test_hover_selector_detected_inside_with_context() {
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0).with_test_theme(|s, t| {
            s.background(t.primary)
                .hover(|s| s.background(palette::css::GREEN))
        })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // The Hover selector should be detected even though it's inside with_context
    assert!(
        harness.has_style_for_selector(id, StyleSelector::Hover),
        "Hover selector should be detected inside with_context"
    );
}

/// Test that multiple selectors inside `with_context` are all detected.
#[test]
fn test_multiple_selectors_detected_inside_with_context() {
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0).with_test_theme(|s, _t| {
            s.background(palette::css::BLUE)
                .hover(|s| s.background(palette::css::GREEN))
                .active(|s| s.background(palette::css::RED))
                .focus(|s| s.background(palette::css::YELLOW))
        })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    assert!(
        harness.has_style_for_selector(id, StyleSelector::Hover),
        "Hover selector should be detected"
    );
    assert!(
        harness.has_style_for_selector(id, StyleSelector::Active),
        "Active selector should be detected"
    );
    assert!(
        harness.has_style_for_selector(id, StyleSelector::Focus),
        "Focus selector should be detected"
    );
}

/// Test that active style is applied when clicking a view with active selector inside with_context.
#[test]
fn test_active_style_applied_from_with_context() {
    let theme = TestTheme::default();
    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .set(TestThemeProp, theme)
            .with_test_theme(|s, _t| {
                s.background(palette::css::BLUE)
                    .active(|s| s.background(palette::css::RED))
            })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial state: background should be BLUE
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Initial background should be BLUE, got {:?}",
        bg
    );

    // Pointer down - should trigger active style
    harness.pointer_down(50.0, 50.0);

    // Now background should be RED (active)
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::RED),
        "Background should be RED when clicking, got {:?}",
        bg
    );

    // Pointer up - should go back to BLUE
    harness.pointer_up(50.0, 50.0);

    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Background should be BLUE after pointer up, got {:?}",
        bg
    );
}

/// Test that hover style is applied when hovering a view with hover selector inside with_context.
#[test]
fn test_hover_style_applied_from_with_context() {
    let theme = TestTheme::default();
    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .set(TestThemeProp, theme)
            .with_test_theme(|s, _t| {
                s.background(palette::css::BLUE)
                    .hover(|s| s.background(palette::css::GREEN))
            })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial state: background should be BLUE
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Initial background should be BLUE, got {:?}",
        bg
    );

    // Move pointer over the view - should trigger hover style
    harness.pointer_move(50.0, 50.0);

    // Now background should be GREEN (hover)
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Background should be GREEN when hovering, got {:?}",
        bg
    );

    // Move pointer outside - should go back to BLUE
    harness.pointer_move(150.0, 150.0);

    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Background should be BLUE after pointer leaves, got {:?}",
        bg
    );
}

/// Test nested with_context calls - selectors should still be detected.
#[test]
fn test_nested_with_context_selectors_detected() {
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0).with_test_theme(|s, _t| {
            s.with_test_theme(|s, _t| {
                s.background(palette::css::BLUE)
                    .active(|s| s.background(palette::css::RED))
            })
        })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Active selector should still be detected in nested with_context
    assert!(
        harness.has_style_for_selector(id, StyleSelector::Active),
        "Active selector should be detected in nested with_context"
    );
}

/// Test that selectors work when using theme values inside the selector closure.
#[test]
fn test_active_style_uses_theme_values() {
    // Set up theme with specific colors
    let theme = TestTheme {
        primary: palette::css::BLUE,
        hover: palette::css::LIGHT_BLUE,
        active: palette::css::DARK_BLUE,
    };

    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .set(TestThemeProp, theme)
            .with_test_theme(|s, t| {
                let active_color = t.active;
                s.background(t.primary)
                    .active(move |s| s.background(active_color))
            })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial state: background should be BLUE (primary)
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Initial background should be BLUE (primary), got {:?}",
        bg
    );

    // Pointer down - should use theme's active color
    harness.pointer_down(50.0, 50.0);

    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::DARK_BLUE),
        "Background should be DARK_BLUE (theme active) when clicking, got {:?}",
        bg
    );
}

/// Test that clicking state is properly set for views with active selector inside with_context.
#[test]
fn test_clicking_state_set_for_with_context_active() {
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0).with_test_theme(|s, _t| {
            s.background(palette::css::BLUE)
                .active(|s| s.background(palette::css::RED))
        })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initially not clicking
    assert!(!harness.is_clicking(id), "Should not be clicking initially");

    // Pointer down
    harness.pointer_down(50.0, 50.0);

    // Should be clicking
    assert!(
        harness.is_clicking(id),
        "Should be clicking after pointer down"
    );

    // Pointer up
    harness.pointer_up(50.0, 50.0);

    // No longer clicking
    assert!(
        !harness.is_clicking(id),
        "Should not be clicking after pointer up"
    );
}

/// Test that disabled selector inside with_context is detected.
#[test]
fn test_disabled_selector_detected_inside_with_context() {
    let theme = TestTheme::default();
    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .set(TestThemeProp, theme)
            .with_test_theme(|s, _t| {
                s.background(palette::css::BLUE)
                    .disabled(|s| s.background(palette::css::GRAY))
            })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    assert!(
        harness.has_style_for_selector(id, StyleSelector::Disabled),
        "Disabled selector should be detected inside with_context"
    );
}

/// Test that focus selector inside with_context is detected.
#[test]
fn test_focus_selector_detected_inside_with_context() {
    let theme = TestTheme::default();
    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .set(TestThemeProp, theme)
            .with_test_theme(|s, _t| {
                s.background(palette::css::BLUE)
                    .focus(|s| s.background(palette::css::YELLOW))
            })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    assert!(
        harness.has_style_for_selector(id, StyleSelector::Focus),
        "Focus selector should be detected inside with_context"
    );
}

/// Test that focus_visible selector inside with_context is detected.
#[test]
fn test_focus_visible_selector_detected_inside_with_context() {
    let theme = TestTheme::default();
    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .set(TestThemeProp, theme)
            .with_test_theme(|s, _t| {
                s.background(palette::css::BLUE)
                    .focus_visible(|s| s.background(palette::css::ORANGE))
            })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    assert!(
        harness.has_style_for_selector(id, StyleSelector::FocusVisible),
        "FocusVisible selector should be detected inside with_context"
    );
}

/// Test that dragging selector inside with_context is detected.
#[test]
fn test_dragging_selector_detected_inside_with_context() {
    let theme = TestTheme::default();
    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .set(TestThemeProp, theme)
            .with_test_theme(|s, _t| {
                s.background(palette::css::BLUE)
                    .drag(|s| s.background(palette::css::PURPLE))
            })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    assert!(
        harness.has_style_for_selector(id, StyleSelector::Dragging),
        "Dragging selector should be detected inside with_context"
    );
}

/// Test that selected selector inside with_context is detected.
#[test]
fn test_selected_selector_detected_inside_with_context() {
    let theme = TestTheme::default();
    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .set(TestThemeProp, theme)
            .with_test_theme(|s, _t| {
                s.background(palette::css::BLUE)
                    .selected(|s| s.background(palette::css::CYAN))
            })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    assert!(
        harness.has_style_for_selector(id, StyleSelector::Selected),
        "Selected selector should be detected inside with_context"
    );
}

/// Test that disabled style inside with_context is applied correctly.
#[test]
fn test_disabled_style_applied_from_with_context() {
    let theme = TestTheme::default();
    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0)
            .set(TestThemeProp, theme)
            .with_test_theme(|s, _t| {
                s.background(palette::css::BLUE)
                    .set_disabled(true)
                    .disabled(|s| s.background(palette::css::GRAY))
            })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Request style recalc to apply disabled selector
    id.request_style();
    harness.rebuild();

    // Background should be GRAY (disabled)
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "Disabled background should be GRAY, got {:?}",
        bg
    );
}

/// Test that selectors inside with_context work with style merging.
#[test]
fn test_with_context_selectors_merge_correctly() {
    let theme = TestTheme::default();
    let view = Empty::new()
        .style(move |s| {
            // First style call with active selector outside with_context
            s.size(100.0, 100.0)
                .set(TestThemeProp, theme)
                .active(|s| s.border(1.0).border_color(palette::css::BLACK))
        })
        .style(|s| {
            // Second style call with active selector inside with_context
            s.with_test_theme(|s, _t| s.active(|s| s.background(palette::css::RED)))
        });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Active selector should be detected
    assert!(
        harness.has_style_for_selector(id, StyleSelector::Active),
        "Active selector should be detected from merged styles"
    );

    // Pointer down
    harness.pointer_down(50.0, 50.0);

    // Both active styles should be applied (border and background)
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::RED),
        "Active background should be applied, got {:?}",
        bg
    );
}

// =============================================================================
// Active Style Trigger Tests
// =============================================================================

/// Test that active style triggers paint when clicking.
#[test]
fn test_active_style_triggers_paint() {
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .active(|s| s.background(palette::css::RED))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Check that view has Active selector
    assert!(
        harness.has_style_for_selector(id, StyleSelector::Active),
        "View should have Active selector"
    );

    // Check computed background color before clicking - should be None
    let bg_before = harness.get_computed_style(id).get(Background);
    assert!(
        bg_before.is_none(),
        "Background should be None before clicking"
    );

    // Pointer down
    harness.pointer_down(50.0, 50.0);

    // Check clicking state
    assert!(
        harness.is_clicking(id),
        "Should be clicking after pointer down"
    );

    // Check computed background color after clicking - should be RED
    let bg_after = harness.get_computed_style(id).get(Background);
    assert!(
        bg_after.is_some(),
        "Background should be set after pointer down on view with :active style"
    );
}

/// Test that style is requested when clicking state changes.
#[test]
fn test_style_request_on_clicking() {
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .active(|s| s.background(palette::css::RED))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Verify selector is detected
    assert!(
        harness.has_style_for_selector(id, StyleSelector::Active),
        "View should have Active selector"
    );

    // Check clicking state before
    assert!(!harness.is_clicking(id), "Should not be clicking initially");

    // Pointer down
    harness.pointer_down(50.0, 50.0);

    // Check clicking state after
    assert!(
        harness.is_clicking(id),
        "Should be clicking after pointer down"
    );

    // Check if has_active is still true after clicking
    assert!(
        harness.has_style_for_selector(id, StyleSelector::Active),
        "View should still have Active selector after clicking"
    );
}
