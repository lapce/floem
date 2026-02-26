use floem::peniko::Brush;
use floem::prelude::*;
use floem::responsive::ScreenSize;
use floem::style::Background;
use floem_test::prelude::*;

#[test]
fn test_responsive_api_compatibility() {
    let root = TestRoot::new();
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .background(palette::css::GRAY)
            .responsive(ScreenSize::SM | ScreenSize::MD, |s| {
                s.background(palette::css::GREEN)
            })
    });
    let id = view.view_id();

    let harness_sm = HeadlessHarness::new_with_size(root, view, 600.0, 100.0);
    let style_sm = harness_sm.get_computed_style(id);
    assert!(
        matches!(style_sm.get(Background), Some(Brush::Solid(c)) if c == palette::css::GREEN)
    );
}

#[test]
fn test_min_window_width_selector() {
    let root = TestRoot::new();
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .background(palette::css::GRAY)
            .min_window_width(768.0, |s| s.background(palette::css::BLUE))
    });
    let id = view.view_id();

    let harness_md = HeadlessHarness::new_with_size(root, view, 1000.0, 100.0);
    let style_md = harness_md.get_computed_style(id);
    assert!(
        matches!(style_md.get(Background), Some(Brush::Solid(c)) if c == palette::css::BLUE)
    );
}

#[test]
fn test_window_width_range_selector() {
    let root = TestRoot::new();
    let view = Empty::new().style(|s| {
        s.size(100.0, 100.0)
            .background(palette::css::GRAY)
            .window_width_range(576.0, 1199.0, |s| s.background(palette::css::ORANGE))
    });
    let id = view.view_id();

    let harness_sm = HeadlessHarness::new_with_size(root, view, 700.0, 100.0);
    let style_sm = harness_sm.get_computed_style(id);
    assert!(
        matches!(style_sm.get(Background), Some(Brush::Solid(c)) if c == palette::css::ORANGE)
    );
}
