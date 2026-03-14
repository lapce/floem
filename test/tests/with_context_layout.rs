//! Tests for layout properties inside `with_context` closures.
//!
//! These tests verify that layout properties (size, position, etc.) set inside
//! `with_context` closures are properly applied during layout computation.
//!
//! KNOWN ISSUE: Layout properties inside `with_context` may not work correctly
//! because context mappings are resolved at style time, but layout props need
//! to be available during taffy style computation.

use floem::peniko::Color;
use floem::prelude::*;
use floem::prop;
use floem::style::{ContextRef, ExprStyle, Style};
use floem_test::prelude::*;
use serial_test::serial;

// Define a simple theme prop for testing
prop!(
    pub TestThemeProp: TestTheme { inherited } = TestTheme::default()
);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TestTheme {
    pub primary: Color,
    pub size: f64,
}

impl Default for TestTheme {
    fn default() -> Self {
        Self {
            primary: palette::css::BLUE,
            size: 100.0,
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
    fn with_test_theme(
        self,
        f: impl Fn(ExprStyle, ContextRef<TestThemeProp>) -> ExprStyle + 'static,
    ) -> Self
    where
        Self: Sized;
}

impl TestThemeExt for Style {
    fn with_test_theme(
        self,
        f: impl Fn(ExprStyle, ContextRef<TestThemeProp>) -> ExprStyle + 'static,
    ) -> Self {
        self.with::<TestThemeProp>(f)
    }
}

// =============================================================================
// Baseline tests - layout props work outside with_context
// =============================================================================

/// Baseline test: size applied directly works.
#[test]
#[serial]
fn test_size_applied_directly() {
    let view = Empty::new().style(|s| s.size(100.0, 50.0));
    let id = view.view_id();

    let root = TestRoot::new();
    let mut harness = HeadlessHarness::new_with_size(root, view, 200.0, 200.0);
    harness.rebuild();

    let layout = id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "Width should be 100.0, got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 50.0).abs() < 0.1,
        "Height should be 50.0, got {}",
        layout.size.height
    );
}

/// Baseline test: absolute positioning works outside with_context.
#[test]
#[serial]
fn test_absolute_positioning_directly() {
    let element = Empty::new().style(|s| {
        s.absolute()
            .inset_left(20.0)
            .inset_top(30.0)
            .size(50.0, 50.0)
    });
    let element_id = element.view_id();

    let view = Stack::new((element,)).style(|s| s.size(200.0, 200.0));

    let root = TestRoot::new();
    let mut harness = HeadlessHarness::new_with_size(root, view, 200.0, 200.0);
    harness.rebuild();

    let layout = element_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.location.x - 20.0).abs() < 0.1,
        "Left should be 20.0, got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 30.0).abs() < 0.1,
        "Top should be 30.0, got {}",
        layout.location.y
    );
}

// =============================================================================
// Tests for layout props inside with_context
// =============================================================================

/// Test that size set inside with_context is applied.
///
/// KNOWN ISSUE: This test may fail because layout properties inside
/// `with_context` are not properly applied during taffy style computation.
#[test]
#[serial]
fn test_size_inside_with_context() {
    let theme = TestTheme::default();
    let view = Empty::new().style(move |s| {
        s.set(TestThemeProp, theme)
            .with_test_theme(|s, t| s.size(t.def(|t| t.size), t.def(|t| t.size / 2.)))
    });
    let id = view.view_id();

    let root = TestRoot::new();
    let mut harness = HeadlessHarness::new_with_size(root, view, 200.0, 200.0);
    harness.rebuild();

    let layout = id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "Width should be 100.0 when set inside with_context, got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 50.0).abs() < 0.1,
        "Height should be 50.0 when set inside with_context, got {}",
        layout.size.height
    );
}

// =============================================================================
// Mixed tests - layout props both inside and outside with_context
// =============================================================================

/// Test that layout props outside with_context and color inside work together.
#[test]
#[serial]
fn test_layout_outside_color_inside_with_context() {
    let theme = TestTheme::default();
    let view = Empty::new().style(move |s| {
        s.size(100.0, 50.0)
            .set(TestThemeProp, theme)
            .with_test_theme(|s, t| s.background(t.def(|t| t.primary)))
    });
    let id = view.view_id();

    let root = TestRoot::new();
    let mut harness = HeadlessHarness::new_with_size(root, view, 200.0, 200.0);
    harness.rebuild();

    // Layout should work (set outside with_context)
    let layout = id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "Width should be 100.0, got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 50.0).abs() < 0.1,
        "Height should be 50.0, got {}",
        layout.size.height
    );

    // Color should also work (set inside with_context)
    let style = harness.get_computed_style(id);
    let bg = style.get(floem::style::Background);
    assert!(bg.is_some(), "Background should be set from with_context");
}

/// Test using theme values for sizing.
#[test]
#[serial]
fn test_theme_value_for_size() {
    let theme = TestTheme {
        primary: palette::css::BLUE,
        size: 80.0,
    };
    let view = Empty::new().style(move |s| {
        s.set(TestThemeProp, theme)
            .with_test_theme(|s, t| s.width(t.def(|t| t.size)).height(t.def(|t| t.size / 2.0)))
    });
    let id = view.view_id();

    let root = TestRoot::new();
    let mut harness = HeadlessHarness::new_with_size(root, view, 200.0, 200.0);
    harness.rebuild();

    let layout = id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 80.0).abs() < 0.1,
        "Width should be 80.0 from theme, got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 40.0).abs() < 0.1,
        "Height should be 40.0 from theme, got {}",
        layout.size.height
    );
}
