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
use floem::style::Style;
use floem_test::prelude::*;

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
    fn with_test_theme(self, f: impl Fn(Self, &TestTheme) -> Self + 'static) -> Self
    where
        Self: Sized;
}

impl TestThemeExt for Style {
    fn with_test_theme(self, f: impl Fn(Self, &TestTheme) -> Self + 'static) -> Self {
        self.with_context::<TestThemeProp>(f)
    }
}

// =============================================================================
// Baseline tests - layout props work outside with_context
// =============================================================================

/// Baseline test: size applied directly works.
#[test]
fn test_size_applied_directly() {
    let view = Empty::new().style(|s| s.size(100.0, 50.0));
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
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
fn test_absolute_positioning_directly() {
    let element = Empty::new().style(|s| {
        s.absolute()
            .inset_left(20.0)
            .inset_top(30.0)
            .size(50.0, 50.0)
    });
    let element_id = element.view_id();

    let view = Stack::new((element,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
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
fn test_size_inside_with_context() {
    let theme = TestTheme::default();
    let view = Empty::new().style(move |s| {
        s.set(TestThemeProp, theme)
            .with_test_theme(|s, _t| s.size(100.0, 50.0))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
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

/// Test that width set inside with_context is applied.
#[test]
fn test_width_inside_with_context() {
    let theme = TestTheme::default();
    let view = Empty::new().style(move |s| {
        s.set(TestThemeProp, theme)
            .with_test_theme(|s, _t| s.width(150.0).height(50.0))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let layout = id.get_layout().expect("Layout should exist");
    assert!(
        (layout.size.width - 150.0).abs() < 0.1,
        "Width should be 150.0 when set inside with_context, got {}",
        layout.size.width
    );
}

/// Test that absolute positioning inside with_context is applied.
#[test]
fn test_absolute_inside_with_context() {
    let theme = TestTheme::default();
    let element = Empty::new().style(move |s| {
        s.set(TestThemeProp, theme).with_test_theme(|s, _t| {
            s.absolute()
                .inset_left(20.0)
                .inset_top(30.0)
                .size(50.0, 50.0)
        })
    });
    let element_id = element.view_id();

    let view = Stack::new((element,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let layout = element_id.get_layout().expect("Layout should exist");
    assert!(
        (layout.location.x - 20.0).abs() < 0.1,
        "Left should be 20.0 when set inside with_context, got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 30.0).abs() < 0.1,
        "Top should be 30.0 when set inside with_context, got {}",
        layout.location.y
    );
}

/// Test that flex properties inside with_context are applied.
#[test]
fn test_flex_inside_with_context() {
    let theme = TestTheme::default();
    let view = Empty::new().style(move |s| {
        s.set(TestThemeProp, theme)
            .with_test_theme(|s, _t| s.flex_grow(1.0).height(50.0))
    });
    let id = view.view_id();

    // Put it in a flex container with fixed width
    let container = Stack::new((view,)).style(|s| s.flex_row().size(200.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(container, 200.0, 200.0);
    harness.rebuild();

    let layout = id.get_layout().expect("Layout should exist");
    // With flex_grow(1.0), the element should expand to fill the container
    assert!(
        (layout.size.width - 200.0).abs() < 0.1,
        "Width should be 200.0 with flex_grow(1.0), got {}",
        layout.size.width
    );
}

/// Test that padding inside with_context is applied.
#[test]
fn test_padding_inside_with_context() {
    let theme = TestTheme::default();
    let child = Empty::new().style(|s| s.size(50.0, 50.0));
    let child_id = child.view_id();

    let container = floem::views::Container::new(child).style(move |s| {
        s.set(TestThemeProp, theme)
            .with_test_theme(|s, _t| s.padding(10.0))
    });

    let view = Stack::new((container,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    // Child should be offset by padding
    assert!(
        (layout.location.x - 10.0).abs() < 0.1,
        "Child x should be 10.0 with padding(10.0), got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 10.0).abs() < 0.1,
        "Child y should be 10.0 with padding(10.0), got {}",
        layout.location.y
    );
}

/// Test that margin inside with_context is applied.
#[test]
fn test_margin_inside_with_context() {
    let theme = TestTheme::default();
    let element = Empty::new().style(move |s| {
        s.set(TestThemeProp, theme)
            .with_test_theme(|s, _t| s.margin(15.0).size(50.0, 50.0))
    });
    let element_id = element.view_id();

    let view = Stack::new((element,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let layout = element_id.get_layout().expect("Layout should exist");
    // Element should be offset by margin
    assert!(
        (layout.location.x - 15.0).abs() < 0.1,
        "Element x should be 15.0 with margin(15.0), got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 15.0).abs() < 0.1,
        "Element y should be 15.0 with margin(15.0), got {}",
        layout.location.y
    );
}

/// Test that gap inside with_context is applied.
#[test]
fn test_gap_inside_with_context() {
    let theme = TestTheme::default();

    let child1 = Empty::new().style(|s| s.size(50.0, 50.0));
    let child2 = Empty::new().style(|s| s.size(50.0, 50.0));
    let child2_id = child2.view_id();

    let container = Stack::new((child1, child2)).style(move |s| {
        s.set(TestThemeProp, theme)
            .with_test_theme(|s, _t| s.flex_row().gap(20.0))
    });

    let view = Stack::new((container,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let layout = child2_id.get_layout().expect("Layout should exist");
    // Second child should be offset by first child width + gap
    // First child is 50px, gap is 20px, so second child starts at 70px
    assert!(
        (layout.location.x - 70.0).abs() < 0.1,
        "Second child x should be 70.0 (50 + 20 gap), got {}",
        layout.location.x
    );
}

// =============================================================================
// Mixed tests - layout props both inside and outside with_context
// =============================================================================

/// Test that layout props outside with_context and color inside work together.
#[test]
fn test_layout_outside_color_inside_with_context() {
    let theme = TestTheme::default();
    let view = Empty::new().style(move |s| {
        s.size(100.0, 50.0)
            .set(TestThemeProp, theme)
            .with_test_theme(|s, t| s.background(t.primary))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
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

/// Test that some layout props outside and some inside work correctly.
#[test]
fn test_mixed_layout_inside_outside_with_context() {
    let theme = TestTheme::default();
    let element = Empty::new().style(move |s| {
        // Width and absolute outside, height inside
        s.absolute()
            .width(100.0)
            .set(TestThemeProp, theme)
            .with_test_theme(|s, _t| s.height(50.0).inset_left(20.0).inset_top(30.0))
    });
    let element_id = element.view_id();

    let view = Stack::new((element,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let layout = element_id.get_layout().expect("Layout should exist");

    // Width (set outside) should work
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "Width should be 100.0, got {}",
        layout.size.width
    );

    // Height (set inside with_context) should also work
    assert!(
        (layout.size.height - 50.0).abs() < 0.1,
        "Height should be 50.0 when set inside with_context, got {}",
        layout.size.height
    );

    // Position (set inside with_context) should work
    assert!(
        (layout.location.x - 20.0).abs() < 0.1,
        "Left should be 20.0 when set inside with_context, got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 30.0).abs() < 0.1,
        "Top should be 30.0 when set inside with_context, got {}",
        layout.location.y
    );
}

/// Test using theme values for sizing.
#[test]
fn test_theme_value_for_size() {
    let theme = TestTheme {
        primary: palette::css::BLUE,
        size: 80.0,
    };
    let view = Empty::new().style(move |s| {
        s.set(TestThemeProp, theme)
            .with_test_theme(|s, t| s.size(t.size, t.size / 2.0))
    });
    let id = view.view_id();

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
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

// =============================================================================
// Tests with INHERITED context (theme set on parent)
// =============================================================================

/// Test that size set inside with_context works when theme is INHERITED from parent.
/// This more closely matches the Dialog scenario where theme is set at app root.
///
/// KNOWN ISSUE: This test demonstrates the bug where layout properties inside
/// with_context don't work when the context value is inherited from parent.
#[test]
fn test_size_with_inherited_context() {
    let theme = TestTheme::default();

    // Child uses with_context to apply layout - should get theme from parent
    let child = Empty::new().style(|s| s.with_test_theme(|s, _t| s.size(100.0, 50.0)));
    let child_id = child.view_id();

    // Parent sets the theme context
    let parent =
        Stack::new((child,)).style(move |s| s.size(300.0, 200.0).set(TestThemeProp, theme));

    let mut harness = HeadlessHarness::new_with_size(parent, 300.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    eprintln!(
        "Child layout with inherited context: size={}x{}, location=({}, {})",
        layout.size.width, layout.size.height, layout.location.x, layout.location.y
    );

    // KNOWN ISSUE: Layout props in with_context with inherited theme don't work
    assert!(
        (layout.size.width - 100.0).abs() < 0.1,
        "Width should be 100.0 when set inside with_context (inherited), got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 50.0).abs() < 0.1,
        "Height should be 50.0 when set inside with_context (inherited), got {}",
        layout.size.height
    );
}

/// Test absolute positioning with inherited context.
#[test]
fn test_absolute_with_inherited_context() {
    let theme = TestTheme::default();

    // Child uses with_context for layout with inherited theme
    let child = Empty::new().style(|s| {
        s.with_test_theme(|s, _t| {
            s.absolute()
                .inset_left(20.0)
                .inset_top(30.0)
                .size(50.0, 50.0)
        })
    });
    let child_id = child.view_id();

    // Parent sets theme
    let parent =
        Stack::new((child,)).style(move |s| s.size(200.0, 200.0).set(TestThemeProp, theme));

    let mut harness = HeadlessHarness::new_with_size(parent, 200.0, 200.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    eprintln!(
        "Child with inherited context: location=({}, {}), size={}x{}",
        layout.location.x, layout.location.y, layout.size.width, layout.size.height
    );

    // These should be the expected values if with_context worked correctly
    assert!(
        (layout.location.x - 20.0).abs() < 0.1,
        "Left should be 20.0 with inherited context, got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 30.0).abs() < 0.1,
        "Top should be 30.0 with inherited context, got {}",
        layout.location.y
    );
}

/// Test with Container::derived (like Dialog uses)
#[test]
fn test_container_derived_with_inherited_context() {
    let theme = TestTheme::default();

    // Container::derived like Dialog uses
    let child = floem::views::Container::derived(|| Empty::new().style(|s| s.size(50.0, 30.0)))
        .style(|s| {
            s.with_test_theme(|s, _t| {
                s.absolute()
                    .inset_left(100.0)
                    .inset_top(75.0)
                    .size(200.0, 100.0)
            })
        });
    let child_id = child.view_id();

    // Parent sets theme
    let parent =
        Stack::new((child,)).style(move |s| s.size(400.0, 300.0).set(TestThemeProp, theme));

    let mut harness = HeadlessHarness::new_with_size(parent, 400.0, 300.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    eprintln!(
        "Container::derived with inherited context: location=({}, {}), size={}x{}",
        layout.location.x, layout.location.y, layout.size.width, layout.size.height
    );

    // Expected: positioned at (100, 75) with size 200x100
    assert!(
        (layout.location.x - 100.0).abs() < 0.1,
        "x should be 100.0 with inherited context, got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 75.0).abs() < 0.1,
        "y should be 75.0 with inherited context, got {}",
        layout.location.y
    );
    assert!(
        (layout.size.width - 200.0).abs() < 0.1,
        "width should be 200.0, got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 100.0).abs() < 0.1,
        "height should be 100.0, got {}",
        layout.size.height
    );
}

/// Test layout props when theme is NOT set anywhere (uses default value).
/// This is the exact scenario that fails in floem-shadcn's test.
#[test]
fn test_layout_with_context_no_theme_set() {
    // Theme is NOT set anywhere - with_context will use default value
    let child = Empty::new().style(|s| {
        s.with_test_theme(|s, _t| {
            s.absolute()
                .inset_left(100.0)
                .inset_top(75.0)
                .size(200.0, 100.0)
        })
    });
    let child_id = child.view_id();

    // Parent does NOT set theme - this is the key difference!
    let parent = Stack::new((child,)).style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(parent, 400.0, 300.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    eprintln!(
        "With context (no theme set): location=({}, {}), size={}x{}",
        layout.location.x, layout.location.y, layout.size.width, layout.size.height
    );

    // This should work even when theme is not set (uses default value)
    assert!(
        (layout.location.x - 100.0).abs() < 0.1,
        "x should be 100.0 even without theme set, got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 75.0).abs() < 0.1,
        "y should be 75.0 even without theme set, got {}",
        layout.location.y
    );
    assert!(
        (layout.size.width - 200.0).abs() < 0.1,
        "width should be 200.0 even without theme set, got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 100.0).abs() < 0.1,
        "height should be 100.0 even without theme set, got {}",
        layout.size.height
    );
}

/// Test with Container::derived and no theme set (exactly like floem-shadcn's failing test)
#[test]
fn test_container_derived_no_theme_set() {
    // Container::derived like Dialog uses, with NO theme set anywhere
    let child = floem::views::Container::derived(|| Empty::new().style(|s| s.size(50.0, 30.0)))
        .style(|s| {
            s.with_test_theme(|s, _t| {
                s.absolute()
                    .inset_left(100.0)
                    .inset_top(75.0)
                    .size(200.0, 100.0)
            })
        });
    let child_id = child.view_id();

    // Parent does NOT set theme!
    let parent = Stack::new((child,)).style(|s| s.size(400.0, 300.0));

    let mut harness = HeadlessHarness::new_with_size(parent, 400.0, 300.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    eprintln!(
        "Container::derived (no theme set): location=({}, {}), size={}x{}",
        layout.location.x, layout.location.y, layout.size.width, layout.size.height
    );

    // Expected: positioned at (100, 75) with size 200x100
    assert!(
        (layout.location.x - 100.0).abs() < 0.1,
        "x should be 100.0 without theme set, got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 75.0).abs() < 0.1,
        "y should be 75.0 without theme set, got {}",
        layout.location.y
    );
    assert!(
        (layout.size.width - 200.0).abs() < 0.1,
        "width should be 200.0, got {}",
        layout.size.width
    );
    assert!(
        (layout.size.height - 100.0).abs() < 0.1,
        "height should be 100.0, got {}",
        layout.size.height
    );
}

/// Test the dialog centering pattern with inherited context
#[test]
fn test_centering_with_inherited_context() {
    use floem::unit::Pct;

    let theme = TestTheme::default();

    // Centered element like Dialog content
    let child = Empty::new().style(|s| {
        s.with_test_theme(|s, _t| {
            s.absolute()
                .inset_left(Pct(50.0))
                .inset_top(Pct(50.0))
                .translate_x(Pct(-50.0))
                .translate_y(Pct(-50.0))
                .size(200.0, 100.0)
        })
    });
    let child_id = child.view_id();

    // Parent sets theme
    let parent =
        Stack::new((child,)).style(move |s| s.size(400.0, 300.0).set(TestThemeProp, theme));

    let mut harness = HeadlessHarness::new_with_size(parent, 400.0, 300.0);
    harness.rebuild();

    let layout = child_id.get_layout().expect("Layout should exist");
    let transform = child_id.get_transform();
    let coeffs = transform.as_coeffs();

    eprintln!(
        "Centered with inherited context: location=({}, {}), size={}x{}, transform=({}, {})",
        layout.location.x,
        layout.location.y,
        layout.size.width,
        layout.size.height,
        coeffs[4],
        coeffs[5]
    );

    // Expected: positioned at 50% of parent (200, 150)
    // with transform offset of -50% of self (-100, -50)
    assert!(
        (layout.location.x - 200.0).abs() < 0.1,
        "x should be 200.0 (50% of 400), got {}",
        layout.location.x
    );
    assert!(
        (layout.location.y - 150.0).abs() < 0.1,
        "y should be 150.0 (50% of 300), got {}",
        layout.location.y
    );
}
