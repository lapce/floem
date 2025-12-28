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
