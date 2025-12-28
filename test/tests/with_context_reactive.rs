//! Tests for reactive signal tracking inside with_context closures.
//!
//! These tests verify that:
//! 1. Signals accessed inside `with_context` closures are properly tracked
//! 2. When inherited properties (like font-weight) change in a parent's with_context,
//!    child views that inherit those properties are also re-styled
//!
//! The fix involved:
//! - Adding a `ContextInherited` flag to track when with_context closures set inherited props
//! - Checking this flag in `any_inherited()` so `request_style_recursive()` is called
//! - This ensures children are added to style_dirty when parent's inherited props change

use floem::peniko::{Brush, Color};
use floem::prelude::*;
use floem::prop;
use floem::reactive::RwSignal;
use floem::style::{Background, FontWeight, Style, TextColor};
use floem::text::Weight;
use floem_test::prelude::*;

// Define a simple theme prop for testing
prop!(
    pub TestThemeProp: TestTheme { inherited } = TestTheme::default()
);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TestTheme {
    pub primary_bg: Color,
    pub secondary_bg: Color,
    pub primary_color: Color,
    pub secondary_color: Color,
}

impl Default for TestTheme {
    fn default() -> Self {
        Self {
            primary_bg: palette::css::BLUE,
            secondary_bg: palette::css::GRAY,
            primary_color: palette::css::WHITE,
            secondary_color: palette::css::BLACK,
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

/// Test that signals accessed OUTSIDE with_context are properly tracked.
/// This should pass - it's the baseline for comparison.
#[test]
fn test_signal_outside_with_context_is_tracked() {
    let is_active = RwSignal::new(false);

    let view = Empty::new().style(move |s| {
        // Signal accessed OUTSIDE with_context - should be tracked
        let active = is_active.get();
        s.size(100.0, 100.0).with_test_theme(move |s, theme| {
            // Use the captured `active` value
            if active {
                s.background(theme.primary_bg)
                    .color(theme.primary_color)
                    .font_weight(Weight::BOLD)
            } else {
                s.background(theme.secondary_bg)
                    .color(theme.secondary_color)
                    .font_weight(Weight::NORMAL)
            }
        })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial state: not active
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    let color = style.get(TextColor);
    let weight = style.get(FontWeight);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "Initial background should be GRAY, got {:?}",
        bg
    );
    assert!(
        matches!(color, Some(c) if c == palette::css::BLACK),
        "Initial color should be BLACK, got {:?}",
        color
    );
    assert!(
        matches!(weight, Some(w) if w == Weight::NORMAL),
        "Initial font-weight should be NORMAL, got {:?}",
        weight
    );

    // Change the signal
    is_active.set(true);
    harness.rebuild();

    // After change: active
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    let color = style.get(TextColor);
    let weight = style.get(FontWeight);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "After signal change, background should be BLUE, got {:?}",
        bg
    );
    assert!(
        matches!(color, Some(c) if c == palette::css::WHITE),
        "After signal change, color should be WHITE, got {:?}",
        color
    );
    assert!(
        matches!(weight, Some(w) if w == Weight::BOLD),
        "After signal change, font-weight should be BOLD, got {:?}",
        weight
    );
}

/// Test that signals accessed INSIDE with_context are properly tracked.
/// The closure is probed at construction time, which establishes reactive tracking.
#[test]
fn test_signal_inside_with_context_is_tracked() {
    let is_active = RwSignal::new(false);

    let view = Empty::new().style(move |s| {
        s.size(100.0, 100.0).with_test_theme(move |s, theme| {
            // Signal accessed INSIDE with_context - tracked via probing
            let active = is_active.get();
            if active {
                s.background(theme.primary_bg)
                    .color(theme.primary_color)
                    .font_weight(Weight::BOLD)
            } else {
                s.background(theme.secondary_bg)
                    .color(theme.secondary_color)
                    .font_weight(Weight::NORMAL)
            }
        })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial state: not active
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    let color = style.get(TextColor);
    let weight = style.get(FontWeight);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::GRAY),
        "Initial background should be GRAY, got {:?}",
        bg
    );
    assert!(
        matches!(color, Some(c) if c == palette::css::BLACK),
        "Initial color should be BLACK, got {:?}",
        color
    );
    assert!(
        matches!(weight, Some(w) if w == Weight::NORMAL),
        "Initial font-weight should be NORMAL, got {:?}",
        weight
    );

    // Change the signal
    is_active.set(true);
    harness.rebuild();

    // After change: active - style updates correctly
    let style = harness.get_computed_style(id);
    let bg = style.get(Background);
    let color = style.get(TextColor);
    let weight = style.get(FontWeight);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "BUG: After signal change, background should be BLUE, got {:?}. \
         The signal inside with_context is not being tracked.",
        bg
    );
    assert!(
        matches!(color, Some(c) if c == palette::css::WHITE),
        "BUG: After signal change, color should be WHITE, got {:?}. \
         The signal inside with_context is not being tracked.",
        color
    );
    assert!(
        matches!(weight, Some(w) if w == Weight::BOLD),
        "BUG: After signal change, font-weight should be BOLD, got {:?}. \
         The signal inside with_context is not being tracked.",
        weight
    );
}

/// Test with a closure-based signal accessor (like SidebarMenuButton's is_active prop).
/// This replicates the exact pattern used in the sidebar component.
#[test]
fn test_closure_signal_inside_with_context() {
    use std::rc::Rc;

    let active_item = RwSignal::new("none");

    // This pattern matches SidebarMenuButton:
    // - is_active is a closure that reads a signal
    // - The closure is called inside with_context
    let is_active: Rc<dyn Fn() -> bool> = Rc::new(move || active_item.get() == "first");

    let view = Empty::new().style(move |s| {
        let is_active = is_active.clone();
        s.size(100.0, 100.0).with_test_theme(move |s, theme| {
            // Call the is_active closure inside with_context
            let active = is_active();
            if active {
                s.background(theme.primary_bg)
                    .color(theme.primary_color)
                    .font_weight(Weight::BOLD)
            } else {
                s.background(theme.secondary_bg)
                    .color(theme.secondary_color)
                    .font_weight(Weight::NORMAL)
            }
        })
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Initial state: active_item is "none", so is_active() returns false
    let style = harness.get_computed_style(id);
    let weight = style.get(FontWeight);
    assert!(
        matches!(weight, Some(w) if w == Weight::NORMAL),
        "Initial font-weight should be NORMAL, got {:?}",
        weight
    );

    // Change the signal to activate the item
    active_item.set("first");
    harness.rebuild();

    // After change: is_active() should return true
    let style = harness.get_computed_style(id);
    let weight = style.get(FontWeight);
    assert!(
        matches!(weight, Some(w) if w == Weight::BOLD),
        "BUG: After signal change, font-weight should be BOLD, got {:?}. \
         The closure signal inside with_context is not being tracked.",
        weight
    );
}

/// Test that multiple views with signals inside with_context all update correctly.
#[test]
fn test_multiple_views_with_signals_inside_with_context() {
    let active_index = RwSignal::new(0usize);

    let view0 = Empty::new().style(move |s| {
        s.size(50.0, 50.0).with_test_theme(move |s, theme| {
            if active_index.get() == 0 {
                s.background(theme.primary_bg).font_weight(Weight::BOLD)
            } else {
                s.background(theme.secondary_bg).font_weight(Weight::NORMAL)
            }
        })
    });
    let id0 = view0.view_id();

    let view1 = Empty::new().style(move |s| {
        s.size(50.0, 50.0).with_test_theme(move |s, theme| {
            if active_index.get() == 1 {
                s.background(theme.primary_bg).font_weight(Weight::BOLD)
            } else {
                s.background(theme.secondary_bg).font_weight(Weight::NORMAL)
            }
        })
    });
    let id1 = view1.view_id();

    let container = Stack::new((view0, view1)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 100.0, 100.0);

    // Initial: view0 active (BOLD), view1 inactive (NORMAL)
    let style0 = harness.get_computed_style(id0);
    let style1 = harness.get_computed_style(id1);
    assert!(
        matches!(style0.get(FontWeight), Some(w) if w == Weight::BOLD),
        "View0 should be BOLD initially, got {:?}",
        style0.get(FontWeight)
    );
    assert!(
        matches!(style1.get(FontWeight), Some(w) if w == Weight::NORMAL),
        "View1 should be NORMAL initially, got {:?}",
        style1.get(FontWeight)
    );

    // Switch to view1
    active_index.set(1);
    harness.rebuild();

    // After switch: view0 inactive (NORMAL), view1 active (BOLD)
    let style0 = harness.get_computed_style(id0);
    let style1 = harness.get_computed_style(id1);
    assert!(
        matches!(style0.get(FontWeight), Some(w) if w == Weight::NORMAL),
        "BUG: View0 should be NORMAL after switch, got {:?}",
        style0.get(FontWeight)
    );
    assert!(
        matches!(style1.get(FontWeight), Some(w) if w == Weight::BOLD),
        "BUG: View1 should be BOLD after switch, got {:?}",
        style1.get(FontWeight)
    );
}

/// Test click handler changing signal that affects with_context style.
/// This is the exact scenario from the sidebar bug.
#[test]
fn test_click_changes_signal_inside_with_context() {
    let is_active = RwSignal::new(false);

    let button = Empty::new()
        .style(move |s| {
            s.size(100.0, 50.0).with_test_theme(move |s, theme| {
                if is_active.get() {
                    s.background(theme.primary_bg)
                        .color(theme.primary_color)
                        .font_weight(Weight::BOLD)
                } else {
                    s.background(theme.secondary_bg)
                        .color(theme.secondary_color)
                        .font_weight(Weight::NORMAL)
                }
            })
        })
        .on_click_stop(move |_| {
            is_active.set(true);
        });
    let id = button.view_id();

    let container = Stack::new((button,)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 100.0, 100.0);

    // Initial: not active
    let style = harness.get_computed_style(id);
    assert!(
        matches!(style.get(FontWeight), Some(w) if w == Weight::NORMAL),
        "Initial font-weight should be NORMAL, got {:?}",
        style.get(FontWeight)
    );

    // Click to activate
    harness.click(50.0, 25.0);

    // Verify signal changed
    assert!(is_active.get(), "Signal should be true after click");

    // Check style immediately after click
    let style = harness.get_computed_style(id);
    assert!(
        matches!(style.get(FontWeight), Some(w) if w == Weight::BOLD),
        "BUG: After click, font-weight should be BOLD, got {:?}. \
         The signal inside with_context is not being tracked.",
        style.get(FontWeight)
    );
}

/// Test that a child Label inherits font-weight from parent when parent's style changes.
/// This tests the actual sidebar issue - when a parent's style changes an inherited
/// property (font-weight), child views like Label should also update.
/// Fixed by adding ContextInherited flag to propagate inherited changes to children.
#[test]
fn test_child_label_inherits_font_weight_from_parent() {
    use floem::views::Label;

    let is_active = RwSignal::new(false);

    // Create a Label as child - it will inherit font-weight from parent
    let label = Label::new("Test Label");
    let label_id = label.view_id();

    // Parent container with the style that sets font-weight
    let container = Stack::new((label,)).style(move |s| {
        s.size(100.0, 50.0).with_test_theme(move |s, _theme| {
            if is_active.get() {
                s.font_weight(Weight::BOLD)
            } else {
                s.font_weight(Weight::NORMAL)
            }
        })
    });

    let outer = Stack::new((container,)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(outer, 100.0, 100.0);

    // Initial: parent has NORMAL, label should inherit NORMAL
    let label_style = harness.get_computed_style(label_id);
    assert!(
        matches!(label_style.get(FontWeight), Some(w) if w == Weight::NORMAL),
        "Initial: Label should inherit NORMAL font-weight from parent, got {:?}",
        label_style.get(FontWeight)
    );

    // Change signal - parent's font-weight changes to BOLD
    is_active.set(true);
    harness.rebuild();

    // After change: label should inherit BOLD from parent
    let label_style = harness.get_computed_style(label_id);
    assert!(
        matches!(label_style.get(FontWeight), Some(w) if w == Weight::BOLD),
        "BUG: Label should inherit BOLD font-weight from parent after signal change, got {:?}. \
         The Label's style_pass is not being called when parent's inherited props change.",
        label_style.get(FontWeight)
    );
}

// ============================================================================
// Tests for selectors with actual (non-default) theme colors from ancestors
// ============================================================================

/// Test that hover selector uses actual theme colors from parent, not defaults.
#[test]
fn test_hover_selector_with_parent_theme_colors() {
    // Create a custom theme with different colors than defaults
    let custom_theme = TestTheme {
        primary_bg: palette::css::RED,      // Different from default BLUE
        secondary_bg: palette::css::YELLOW, // Different from default GRAY
        primary_color: palette::css::GREEN,
        secondary_color: palette::css::PURPLE,
    };

    // Child uses hover with theme colors
    let child = Empty::new().style(|s| {
        s.size(50.0, 50.0).with_test_theme(|s, theme| {
            s.background(theme.secondary_bg)
                .hover(|s| s.background(theme.primary_bg))
        })
    });
    let child_id = child.view_id();

    // Parent sets the custom theme
    let root =
        Container::new(child).style(move |s| s.size(100.0, 100.0).set(TestThemeProp, custom_theme));

    let mut harness = HeadlessHarness::new_with_size(root, 100.0, 100.0);

    // Initial: should use parent's secondary_bg (YELLOW), not default (GRAY)
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::YELLOW),
        "Initial background should be YELLOW (from parent theme), not GRAY (default). Got {:?}",
        bg
    );

    // Simulate hover
    harness.pointer_move(25.0, 25.0);

    // After hover: should use parent's primary_bg (RED), not default (BLUE)
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::RED),
        "Hover background should be RED (from parent theme), not BLUE (default). Got {:?}",
        bg
    );
}

/// Test that active selector uses actual theme colors from parent.
#[test]
fn test_active_selector_with_parent_theme_colors() {
    let custom_theme = TestTheme {
        primary_bg: palette::css::ORANGE,
        secondary_bg: palette::css::CYAN,
        primary_color: palette::css::MAGENTA,
        secondary_color: palette::css::LIME,
    };

    let child = Empty::new().style(|s| {
        s.size(50.0, 50.0).with_test_theme(|s, theme| {
            s.background(theme.secondary_bg)
                .active(|s| s.background(theme.primary_bg))
        })
    });
    let child_id = child.view_id();

    let root =
        Container::new(child).style(move |s| s.size(100.0, 100.0).set(TestThemeProp, custom_theme));

    let mut harness = HeadlessHarness::new_with_size(root, 100.0, 100.0);

    // Initial: CYAN from parent theme
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::CYAN),
        "Initial should be CYAN from parent theme, got {:?}",
        bg
    );

    // Simulate active (pointer down)
    harness.pointer_down(25.0, 25.0);

    // After active: ORANGE from parent theme
    let style = harness.get_computed_style(child_id);
    let bg = style.get(Background);
    assert!(
        matches!(bg, Some(Brush::Solid(c)) if c == palette::css::ORANGE),
        "Active should be ORANGE from parent theme, got {:?}",
        bg
    );
}

/// Test multiple selectors (hover + active) with parent theme colors.
#[test]
fn test_multiple_selectors_with_parent_theme_colors() {
    let custom_theme = TestTheme {
        primary_bg: palette::css::RED,
        secondary_bg: palette::css::GREEN,
        primary_color: palette::css::BLUE,
        secondary_color: palette::css::YELLOW,
    };

    let child = Empty::new().style(|s| {
        s.size(50.0, 50.0).with_test_theme(|s, theme| {
            s.background(theme.secondary_bg) // GREEN
                .hover(|s| s.background(theme.primary_bg)) // RED on hover
                .active(|s| s.background(theme.primary_color)) // BLUE on active
        })
    });
    let child_id = child.view_id();

    let root =
        Container::new(child).style(move |s| s.size(100.0, 100.0).set(TestThemeProp, custom_theme));

    let mut harness = HeadlessHarness::new_with_size(root, 100.0, 100.0);

    // Initial: GREEN
    let style = harness.get_computed_style(child_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Initial should be GREEN, got {:?}",
        style.get(Background)
    );

    // Hover: RED
    harness.pointer_move(25.0, 25.0);
    let style = harness.get_computed_style(child_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::RED),
        "Hover should be RED, got {:?}",
        style.get(Background)
    );

    // Active (pointer down while hovering): BLUE
    harness.pointer_down(25.0, 25.0);
    let style = harness.get_computed_style(child_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Active should be BLUE, got {:?}",
        style.get(Background)
    );
}

/// Test that when parent's theme changes, child's selector styles also update.
#[test]
fn test_selector_updates_when_parent_theme_changes() {
    let theme_signal = RwSignal::new(TestTheme {
        primary_bg: palette::css::RED,
        secondary_bg: palette::css::GREEN,
        primary_color: palette::css::WHITE,
        secondary_color: palette::css::BLACK,
    });

    let child = Empty::new().style(|s| {
        s.size(50.0, 50.0).with_test_theme(|s, theme| {
            s.background(theme.secondary_bg)
                .hover(|s| s.background(theme.primary_bg))
        })
    });
    let child_id = child.view_id();

    let root = Container::new(child)
        .style(move |s| s.size(100.0, 100.0).set(TestThemeProp, theme_signal.get()));

    let mut harness = HeadlessHarness::new_with_size(root, 100.0, 100.0);

    // Hover to activate hover style
    harness.pointer_move(25.0, 25.0);

    // Initial hover: RED from first theme
    let style = harness.get_computed_style(child_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::RED),
        "Initial hover should be RED, got {:?}",
        style.get(Background)
    );

    // Change the theme
    theme_signal.set(TestTheme {
        primary_bg: palette::css::BLUE, // Changed!
        secondary_bg: palette::css::YELLOW,
        primary_color: palette::css::WHITE,
        secondary_color: palette::css::BLACK,
    });
    harness.rebuild();

    // Hover should now be BLUE from new theme
    let style = harness.get_computed_style(child_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "After theme change, hover should be BLUE, got {:?}",
        style.get(Background)
    );
}

/// Test nested selectors with theme colors (hover containing focus_visible).
#[test]
fn test_nested_selectors_with_parent_theme_colors() {
    let custom_theme = TestTheme {
        primary_bg: palette::css::RED,
        secondary_bg: palette::css::GREEN,
        primary_color: palette::css::BLUE,
        secondary_color: palette::css::YELLOW,
    };

    let child = Empty::new().keyboard_navigable().style(|s| {
        s.size(50.0, 50.0).with_test_theme(|s, theme| {
            s.background(theme.secondary_bg) // GREEN
                .hover(|s| {
                    s.background(theme.primary_bg) // RED on hover
                        .focus_visible(|s| s.background(theme.primary_color)) // BLUE when focused while hovering
                })
        })
    });
    let child_id = child.view_id();

    let root =
        Container::new(child).style(move |s| s.size(100.0, 100.0).set(TestThemeProp, custom_theme));

    let mut harness = HeadlessHarness::new_with_size(root, 100.0, 100.0);

    // Initial: GREEN
    let style = harness.get_computed_style(child_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Initial should be GREEN, got {:?}",
        style.get(Background)
    );

    // Hover: RED
    harness.pointer_move(25.0, 25.0);
    let style = harness.get_computed_style(child_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::RED),
        "Hover should be RED, got {:?}",
        style.get(Background)
    );
}

/// Test that selectors work with conditional theme usage based on signal.
#[test]
fn test_selector_with_conditional_theme_and_signal() {
    let is_active = RwSignal::new(false);

    let custom_theme = TestTheme {
        primary_bg: palette::css::RED,
        secondary_bg: palette::css::GREEN,
        primary_color: palette::css::BLUE,
        secondary_color: palette::css::YELLOW,
    };

    let child = Empty::new().style(move |s| {
        s.size(50.0, 50.0).with_test_theme(move |s, theme| {
            let base = if is_active.get() {
                s.background(theme.primary_bg) // RED when active
            } else {
                s.background(theme.secondary_bg) // GREEN when inactive
            };
            // Hover always uses primary_color (BLUE)
            base.hover(|s| s.background(theme.primary_color))
        })
    });
    let child_id = child.view_id();

    let root =
        Container::new(child).style(move |s| s.size(100.0, 100.0).set(TestThemeProp, custom_theme));

    let mut harness = HeadlessHarness::new_with_size(root, 100.0, 100.0);

    // Initial (inactive): GREEN
    let style = harness.get_computed_style(child_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Initial (inactive) should be GREEN, got {:?}",
        style.get(Background)
    );

    // Hover while inactive: BLUE
    harness.pointer_move(25.0, 25.0);
    let style = harness.get_computed_style(child_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Hover while inactive should be BLUE, got {:?}",
        style.get(Background)
    );

    // Move pointer away, then activate
    harness.pointer_move(75.0, 75.0); // Outside child
    is_active.set(true);
    harness.rebuild();

    // Active (not hovering): RED
    let style = harness.get_computed_style(child_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::RED),
        "Active (not hovering) should be RED, got {:?}",
        style.get(Background)
    );

    // Hover while active: still BLUE (hover overrides)
    harness.pointer_move(25.0, 25.0);
    let style = harness.get_computed_style(child_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Hover while active should be BLUE, got {:?}",
        style.get(Background)
    );
}

/// Test deeply nested views all using parent's theme colors in selectors.
#[test]
fn test_deep_nesting_with_theme_selectors() {
    let custom_theme = TestTheme {
        primary_bg: palette::css::RED,
        secondary_bg: palette::css::GREEN,
        primary_color: palette::css::BLUE,
        secondary_color: palette::css::YELLOW,
    };

    // Create a deep hierarchy where the leaf uses theme in hover
    let leaf = Empty::new().style(|s| {
        s.size(20.0, 20.0).with_test_theme(|s, theme| {
            s.background(theme.secondary_bg)
                .hover(|s| s.background(theme.primary_bg))
        })
    });
    let leaf_id = leaf.view_id();

    let level3 = Container::new(leaf).style(|s| s.size(40.0, 40.0));
    let level2 = Container::new(level3).style(|s| s.size(60.0, 60.0));
    let level1 = Container::new(level2).style(|s| s.size(80.0, 80.0));

    // Theme is set at root, 4 levels above the leaf
    let root = Container::new(level1)
        .style(move |s| s.size(100.0, 100.0).set(TestThemeProp, custom_theme));

    let mut harness = HeadlessHarness::new_with_size(root, 100.0, 100.0);

    // Initial: GREEN from parent theme (inherited through 4 levels)
    let style = harness.get_computed_style(leaf_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Leaf initial should be GREEN from ancestor theme, got {:?}",
        style.get(Background)
    );

    // Hover on leaf: RED from parent theme
    harness.pointer_move(10.0, 10.0);
    let style = harness.get_computed_style(leaf_id);
    assert!(
        matches!(style.get(Background), Some(Brush::Solid(c)) if c == palette::css::RED),
        "Leaf hover should be RED from ancestor theme, got {:?}",
        style.get(Background)
    );
}

/// Test that sibling views each get correct theme colors in their selectors.
#[test]
fn test_siblings_with_theme_selectors() {
    let custom_theme = TestTheme {
        primary_bg: palette::css::RED,
        secondary_bg: palette::css::GREEN,
        primary_color: palette::css::BLUE,
        secondary_color: palette::css::YELLOW,
    };

    let child1 = Empty::new().style(|s| {
        s.size(40.0, 40.0).with_test_theme(|s, theme| {
            s.background(theme.secondary_bg) // GREEN
                .hover(|s| s.background(theme.primary_bg)) // RED
        })
    });
    let child1_id = child1.view_id();

    let child2 = Empty::new().style(|s| {
        s.size(40.0, 40.0).with_test_theme(|s, theme| {
            s.background(theme.secondary_color) // YELLOW
                .hover(|s| s.background(theme.primary_color)) // BLUE
        })
    });
    let child2_id = child2.view_id();

    let container = Stack::new((child1, child2)).style(move |s| {
        s.size(100.0, 100.0)
            .flex_direction(floem::style::FlexDirection::Row)
            .set(TestThemeProp, custom_theme)
    });

    let mut harness = HeadlessHarness::new_with_size(container, 100.0, 100.0);

    // Initial: child1=GREEN, child2=YELLOW
    let style1 = harness.get_computed_style(child1_id);
    let style2 = harness.get_computed_style(child2_id);
    assert!(
        matches!(style1.get(Background), Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Child1 initial should be GREEN, got {:?}",
        style1.get(Background)
    );
    assert!(
        matches!(style2.get(Background), Some(Brush::Solid(c)) if c == palette::css::YELLOW),
        "Child2 initial should be YELLOW, got {:?}",
        style2.get(Background)
    );

    // Hover on child1 (at x=20, which is in child1's area)
    harness.pointer_move(20.0, 20.0);
    let style1 = harness.get_computed_style(child1_id);
    let style2 = harness.get_computed_style(child2_id);
    assert!(
        matches!(style1.get(Background), Some(Brush::Solid(c)) if c == palette::css::RED),
        "Child1 hover should be RED, got {:?}",
        style1.get(Background)
    );
    assert!(
        matches!(style2.get(Background), Some(Brush::Solid(c)) if c == palette::css::YELLOW),
        "Child2 should still be YELLOW (not hovered), got {:?}",
        style2.get(Background)
    );

    // Hover on child2 (at x=60, which is in child2's area)
    harness.pointer_move(60.0, 20.0);
    let style1 = harness.get_computed_style(child1_id);
    let style2 = harness.get_computed_style(child2_id);
    assert!(
        matches!(style1.get(Background), Some(Brush::Solid(c)) if c == palette::css::GREEN),
        "Child1 should be GREEN (not hovered), got {:?}",
        style1.get(Background)
    );
    assert!(
        matches!(style2.get(Background), Some(Brush::Solid(c)) if c == palette::css::BLUE),
        "Child2 hover should be BLUE, got {:?}",
        style2.get(Background)
    );
}
