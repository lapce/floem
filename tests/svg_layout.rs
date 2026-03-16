use floem::HasViewId;
use floem::headless::{HeadlessHarness, TestRoot};
use floem::style::Style;
use floem::views::{Container, Decorators, svg};

const ICON_PATH: &str = r#"M9 12.75L11.25 15 15 9.75M21 12c0 1.268-.63 2.39-1.593 3.068a3.745 3.745 0 01-1.043 3.296 3.745 3.745 0 01-3.296 1.043A3.745 3.745 0 0112 21c-1.268 0-2.39-.63-3.068-1.593a3.746 3.746 0 01-3.296-1.043 3.745 3.745 0 01-1.043-3.296A3.745 3.745 0 013 12c0-1.268.63-2.39 1.593-3.068a3.745 3.745 0 011.043-3.296 3.746 3.746 0 013.296-1.043A3.746 3.746 0 0112 3c1.268 0 2.39.63 3.068 1.593a3.746 3.746 0 013.296 1.043 3.746 3.746 0 011.043 3.296A3.745 3.745 0 0121 12z"#;

fn svg_markup(view_box: &str, width: Option<&str>, height: Option<&str>) -> String {
    let mut attrs =
        format!(r#"xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="{view_box}""#);

    if let Some(width) = width {
        attrs.push_str(&format!(r#" width="{width}""#));
    }
    if let Some(height) = height {
        attrs.push_str(&format!(r#" height="{height}""#));
    }

    format!(
        "<svg {attrs} stroke-width=\"1.5\" stroke=\"#000\">
      <path stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"{ICON_PATH}\" />
    </svg>"
    )
}

fn layout_for_svg(svg_str: String, style: impl Fn(Style) -> Style + 'static) -> (f32, f32) {
    let root = TestRoot::new();
    let svg_view = svg(svg_str).style(style);
    let svg_id = svg_view.view_id();
    let container = Container::new(svg_view).style(|s| s.items_start().size(300.0, 300.0));
    let mut harness = HeadlessHarness::new_with_size(root, container, 300.0, 300.0);
    harness.rebuild();

    let layout = svg_id.get_layout().expect("SVG layout should be computed");
    (layout.size.width, layout.size.height)
}

fn assert_size(actual: (f32, f32), expected: (f32, f32)) {
    let (actual_width, actual_height) = actual;
    let (expected_width, expected_height) = expected;

    assert!((actual_width - expected_width).abs() < 0.01);
    assert!((actual_height - expected_height).abs() < 0.01);
}

#[test]
fn svg_defaults_to_viewbox_dimensions_when_style_unspecified() {
    let size = layout_for_svg(svg_markup("0 0 24 24", None, None), |s| s);
    assert_size(size, (24.0, 24.0));
}

#[test]
fn svg_respects_width_height_from_svg_markup() {
    let size = layout_for_svg(svg_markup("0 0 24 24", Some("40"), Some("16")), |s| s);
    assert_size(size, (40.0, 16.0));
}

#[test]
fn svg_style_width_only_uses_svg_aspect_ratio() {
    let size = layout_for_svg(svg_markup("0 0 24 24", None, None), |s| s.width(50.0));
    assert_size(size, (50.0, 50.0));
}

#[test]
fn svg_style_height_only_uses_svg_aspect_ratio() {
    let size = layout_for_svg(svg_markup("0 0 24 24", None, None), |s| s.height(50.0));
    assert_size(size, (50.0, 50.0));
}

#[test]
fn svg_style_width_and_height_override_marked_size() {
    let size = layout_for_svg(svg_markup("0 0 24 24", None, None), |s| {
        s.width(120.0).height(30.0)
    });
    assert_size(size, (120.0, 30.0));
}

#[test]
fn svg_style_width_and_aspect_ratio_override_height() {
    let size = layout_for_svg(svg_markup("0 0 24 24", None, None), |s| {
        s.width(100.0).aspect_ratio(2.0)
    });
    assert_size(size, (100.0, 50.0));
}

#[test]
fn svg_style_height_and_aspect_ratio_override_width() {
    let size = layout_for_svg(svg_markup("0 0 24 24", None, None), |s| {
        s.height(80.0).aspect_ratio(0.5)
    });
    assert_size(size, (40.0, 80.0));
}

#[test]
fn svg_style_width_and_height_keeps_aspect_ratio_unrelated() {
    let size = layout_for_svg(svg_markup("0 0 24 24", None, None), |s| {
        s.width(100.0).height(60.0).aspect_ratio(0.2)
    });
    assert_size(size, (100.0, 60.0));
}

#[test]
fn svg_uses_non_square_viewbox_ratio_when_unstyled() {
    let size = layout_for_svg(svg_markup("0 0 100 50", None, None), |s| s);
    assert_size(size, (100.0, 50.0));
}

#[test]
fn svg_non_square_viewbox_width_only_uses_ratio() {
    let size = layout_for_svg(svg_markup("0 0 100 50", None, None), |s| s.width(80.0));
    assert_size(size, (80.0, 40.0));
}

#[test]
fn svg_non_square_viewbox_height_only_uses_ratio() {
    let size = layout_for_svg(svg_markup("0 0 100 50", None, None), |s| s.height(40.0));
    assert_size(size, (80.0, 40.0));
}

#[test]
fn svg_non_square_viewbox_width_only_and_aspect_ratio_override() {
    let size = layout_for_svg(svg_markup("0 0 100 50", None, None), |s| {
        s.width(80.0).aspect_ratio(4.0)
    });
    assert_size(size, (80.0, 20.0));
}
