use floem::HasViewId;
use floem::headless::{HeadlessHarness, TestRoot};
use floem::style::{ObjectFit, Style};
use floem::views::{Decorators, Stack, img};
use image::{ColorType, ImageEncoder, RgbaImage, codecs::png::PngEncoder};
use peniko::kurbo::Rect;
use std::io::Cursor;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let img = RgbaImage::from_fn(width, height, |_x, _y| [0, 0, 0, 255].into());
    let mut bytes = Vec::new();
    let mut cursor = Cursor::new(&mut bytes);
    let encoder = PngEncoder::new(&mut cursor);
    let raw = img.as_raw();
    encoder
        .write_image(raw, width, height, ColorType::Rgba8.into())
        .unwrap();
    bytes
}

/// Returns the layout content rect for an img view inside a fixed-size Stack.
fn layout_rect(
    natural_w: u32,
    natural_h: u32,
    style: impl Fn(Style) -> Style + 'static,
    container_size: (f32, f32),
) -> Rect {
    let img_bytes = png_bytes(natural_w, natural_h);
    let root = TestRoot::new();
    let img_view = img(move || img_bytes.clone()).style(style);
    let img_id = img_view.view_id();
    let container = Stack::new((img_view,))
        .style(move |s| s.items_start().size(container_size.0, container_size.1));
    let mut harness = HeadlessHarness::new_with_size(root, container, 300.0, 300.0);
    harness.rebuild();
    img_id.get_content_rect_local()
}

fn assert_rect(actual: Rect, expected: Rect) {
    let epsilon = 0.01f64;
    assert!(
        (actual.x0 - expected.x0).abs() < epsilon
            && (actual.y0 - expected.y0).abs() < epsilon
            && (actual.x1 - expected.x1).abs() < epsilon
            && (actual.y1 - expected.y1).abs() < epsilon,
        "actual={actual:?}, expected={expected:?}"
    );
}

// Convenience: build a content rect anchored at origin.
fn box_rect(w: f64, h: f64) -> Rect {
    Rect::new(0.0, 0.0, w, h)
}

// ---------------------------------------------------------------------------
// Layout tests
//
// These assert what Taffy computes for the layout box. object-fit is a
// paint-time concept and NEVER affects layout — all object-fit variants
// with the same explicit size must produce the same layout rect.
// ---------------------------------------------------------------------------

// --- Intrinsic sizing (no explicit width or height) -----------------------

#[test]
fn layout_no_explicit_size_uses_natural_size() {
    // CSS: img with auto width & height → intrinsic dimensions
    let r = layout_rect(40, 25, |s| s, (300.0, 300.0));
    assert_rect(r, box_rect(40.0, 25.0));
}

#[test]
fn layout_no_explicit_size_object_fit_does_not_change_layout() {
    // object-fit never affects layout — all variants should yield natural size
    // when no explicit dimensions are given.
    for fit in [
        ObjectFit::Fill,
        ObjectFit::Contain,
        ObjectFit::Cover,
        ObjectFit::None,
        ObjectFit::ScaleDown,
    ] {
        let r = layout_rect(40, 25, move |s| s.object_fit(fit), (300.0, 300.0));
        assert_rect(r, box_rect(40.0, 25.0));
    }
}

// --- Aspect-ratio preservation -------------------------------------------

#[test]
fn layout_width_only_preserves_aspect_ratio() {
    // CSS: explicit width, auto height → height = width / AR
    let r = layout_rect(4, 2, |s| s.width(40.0), (300.0, 300.0));
    assert_rect(r, box_rect(40.0, 20.0));
}

#[test]
fn layout_height_only_preserves_aspect_ratio() {
    // CSS: explicit height, auto width → width = height * AR
    let r = layout_rect(4, 2, |s| s.height(40.0), (300.0, 300.0));
    assert_rect(r, box_rect(80.0, 40.0));
}

#[test]
fn layout_width_only_non_integer_aspect_ratio() {
    // 3:2 image, explicit width 90 → height 60
    let r = layout_rect(3, 2, |s| s.width(90.0), (300.0, 300.0));
    assert_rect(r, box_rect(90.0, 60.0));
}

#[test]
fn layout_height_only_non_integer_aspect_ratio() {
    // 3:2 image, explicit height 60 → width 90
    let r = layout_rect(3, 2, |s| s.height(60.0), (300.0, 300.0));
    assert_rect(r, box_rect(90.0, 60.0));
}

// --- Explicit both dimensions --------------------------------------------

#[test]
fn layout_explicit_width_and_height_ignores_aspect_ratio() {
    // CSS: both explicit → layout box is exactly those dimensions
    let r = layout_rect(4, 2, |s| s.width(100.0).height(60.0), (300.0, 300.0));
    assert_rect(r, box_rect(100.0, 60.0));
}

#[test]
fn layout_explicit_size_same_for_all_object_fit_variants() {
    // object-fit NEVER affects the layout box when explicit size is set.
    for fit in [
        ObjectFit::Fill,
        ObjectFit::Contain,
        ObjectFit::Cover,
        ObjectFit::None,
        ObjectFit::ScaleDown,
    ] {
        let r = layout_rect(
            4,
            3,
            move |s| s.width(100.0).height(80.0).object_fit(fit),
            (300.0, 300.0),
        );
        assert_rect(r, box_rect(100.0, 80.0));
    }
}

// ---------------------------------------------------------------------------
// Paint dest-rect tests  (object-fit rendering geometry)
//
// These create an img view directly and call object_fit_dest_rect_with(),
// passing object_fit explicitly — no harness, no style pass needed.
//
// CSS object-position defaults to 50% 50% (centered), which is what the
// implementation uses. content_rect is anchored at (0,0) in all cases.
// ---------------------------------------------------------------------------

fn dest_rect(natural_w: u32, natural_h: u32, object_fit: ObjectFit, content_rect: Rect) -> Rect {
    let img_bytes = png_bytes(natural_w, natural_h);
    let view = img(move || img_bytes.clone());
    view.object_fit_dest_rect_with(content_rect, object_fit)
}

// --- Fill -----------------------------------------------------------------

#[test]
fn paint_fill_stretches_to_box() {
    // Fill always maps the image exactly onto the box, ignoring aspect ratio.
    let dest = dest_rect(4, 3, ObjectFit::Fill, box_rect(100.0, 80.0));
    assert_rect(dest, box_rect(100.0, 80.0));
}

#[test]
fn paint_fill_square_box() {
    let dest = dest_rect(4, 3, ObjectFit::Fill, box_rect(50.0, 50.0));
    assert_rect(dest, box_rect(50.0, 50.0));
}

// --- Contain --------------------------------------------------------------

#[test]
fn paint_contain_wide_image_in_square_box_letterboxed() {
    // 4:3 image in 120×120 box → scale to fit width → 120×90, centered vertically
    // y offset = (120 - 90) / 2 = 15
    let dest = dest_rect(4, 3, ObjectFit::Contain, box_rect(120.0, 120.0));
    assert_rect(dest, Rect::new(0.0, 15.0, 120.0, 105.0));
}

#[test]
fn paint_contain_tall_image_in_square_box_pillarboxed() {
    // 3:4 image in 120×120 box → scale to fit height → 90×120, centered horizontally
    // x offset = (120 - 90) / 2 = 15
    let dest = dest_rect(3, 4, ObjectFit::Contain, box_rect(120.0, 120.0));
    assert_rect(dest, Rect::new(15.0, 0.0, 105.0, 120.0));
}

#[test]
fn paint_contain_exact_aspect_ratio_fills_box() {
    // Image AR matches box AR → no letterbox/pillarbox
    let dest = dest_rect(4, 3, ObjectFit::Contain, box_rect(120.0, 90.0));
    assert_rect(dest, box_rect(120.0, 90.0));
}

#[test]
fn paint_contain_small_image_scales_up_to_fit() {
    // 2×1 image in 100×100 box → scale up to 100×50, centered vertically
    // y offset = (100 - 50) / 2 = 25
    let dest = dest_rect(2, 1, ObjectFit::Contain, box_rect(100.0, 100.0));
    assert_rect(dest, Rect::new(0.0, 25.0, 100.0, 75.0));
}

// --- Cover ----------------------------------------------------------------

#[test]
fn paint_cover_wide_image_in_square_box_cropped_sides() {
    // 4:3 image in 120×120 → scale to fill height → 160×120, centered
    // x offset = (120 - 160) / 2 = -20  (overflows; clipped by caller)
    let dest = dest_rect(4, 3, ObjectFit::Cover, box_rect(120.0, 120.0));
    assert_rect(dest, Rect::new(-20.0, 0.0, 140.0, 120.0));
}

#[test]
fn paint_cover_tall_image_in_square_box_cropped_top_bottom() {
    // 3:4 image in 120×120 → scale to fill width → 120×160, centered
    // y offset = (120 - 160) / 2 = -20
    let dest = dest_rect(3, 4, ObjectFit::Cover, box_rect(120.0, 120.0));
    assert_rect(dest, Rect::new(0.0, -20.0, 120.0, 140.0));
}

#[test]
fn paint_cover_exact_aspect_ratio_fills_box() {
    let dest = dest_rect(4, 3, ObjectFit::Cover, box_rect(120.0, 90.0));
    assert_rect(dest, box_rect(120.0, 90.0));
}

#[test]
fn paint_cover_small_image_scales_up_to_cover() {
    // 1×1 image in 100×50 box → cover picks larger scale: max(100/1, 50/1) = 100
    // → 100×100, centered vertically: y offset = (50 - 100) / 2 = -25
    let dest = dest_rect(1, 1, ObjectFit::Cover, box_rect(100.0, 50.0));
    assert_rect(dest, Rect::new(0.0, -25.0, 100.0, 75.0));
}

// --- None -----------------------------------------------------------------

#[test]
fn paint_none_uses_natural_size_centered() {
    // Natural size, centered in box. 40×20 image in 100×80 box.
    // x = (100 - 40) / 2 = 30, y = (80 - 20) / 2 = 30
    let dest = dest_rect(40, 20, ObjectFit::None, box_rect(100.0, 80.0));
    assert_rect(dest, Rect::new(30.0, 30.0, 70.0, 50.0));
}

#[test]
fn paint_none_large_image_overflows_box() {
    // 200×100 image in 80×80 box → natural size, centered → clipped by caller
    // x = (80 - 200) / 2 = -60, y = (80 - 100) / 2 = -10
    // x1 = -60 + 200 = 140, y1 = -10 + 100 = 90
    let dest = dest_rect(200, 100, ObjectFit::None, box_rect(80.0, 80.0));
    assert_rect(dest, Rect::new(-60.0, -10.0, 140.0, 90.0));
}

#[test]
fn paint_none_exact_fit_no_offset() {
    // Natural size matches box exactly → no offset
    let dest = dest_rect(40, 30, ObjectFit::None, box_rect(40.0, 30.0));
    assert_rect(dest, box_rect(40.0, 30.0));
}

// --- ScaleDown ------------------------------------------------------------

#[test]
fn paint_scale_down_large_image_acts_like_contain() {
    // Image larger than box → scale down. Must match Contain result exactly.
    let contain = dest_rect(120, 90, ObjectFit::Contain, box_rect(80.0, 80.0));
    let sd = dest_rect(120, 90, ObjectFit::ScaleDown, box_rect(80.0, 80.0));
    assert_rect(sd, contain);
}

#[test]
fn paint_scale_down_small_image_acts_like_none() {
    // Image smaller than box → do NOT scale up. Must match None result exactly.
    let none = dest_rect(40, 30, ObjectFit::None, box_rect(200.0, 200.0));
    let sd = dest_rect(40, 30, ObjectFit::ScaleDown, box_rect(200.0, 200.0));
    assert_rect(sd, none);
}

#[test]
fn paint_scale_down_exact_fit_no_scaling() {
    // Natural size exactly matches box → no scaling, no offset
    let dest = dest_rect(40, 30, ObjectFit::ScaleDown, box_rect(40.0, 30.0));
    assert_rect(dest, box_rect(40.0, 30.0));
}

#[test]
fn paint_scale_down_never_scales_above_natural_size() {
    // Box larger than image → natural size, centered. Must NOT upscale.
    // 40×20 image in 120×80 box: x = (120-40)/2 = 40, y = (80-20)/2 = 30
    let dest = dest_rect(40, 20, ObjectFit::ScaleDown, box_rect(120.0, 80.0));
    assert_rect(dest, Rect::new(40.0, 30.0, 80.0, 50.0));
}

// --- Edge cases -----------------------------------------------------------

#[test]
fn paint_zero_natural_size_returns_content_rect() {
    // 1×1 image with zero-size box — degenerate box hits the fallback return.
    let dest = dest_rect(1, 1, ObjectFit::Contain, box_rect(0.0, 0.0));
    assert_rect(dest, box_rect(0.0, 0.0));
}

#[test]
fn paint_zero_box_size_returns_content_rect() {
    // Degenerate box (0×0) → fall back to content rect unchanged
    let dest = dest_rect(40, 30, ObjectFit::Cover, box_rect(0.0, 0.0));
    assert_rect(dest, box_rect(0.0, 0.0));
}
