//! Tests for translate transform behavior.
//!
//! CSS-style translate percentages should be relative to the element's own size:
//! - translate_x(50%) moves the element by 50% of its own width
//! - translate_y(-50%) moves the element up by 50% of its own height
//!
//! This is commonly used for centering:
//! ```css
//! position: absolute;
//! left: 50%;
//! top: 50%;
//! transform: translate(-50%, -50%);
//! ```

use floem::headless::HeadlessHarness;
use floem::unit::Pct;
use floem::views::{Decorators, Empty, stack};
use floem::HasViewId;

// ============================================================================
// Pixel-based translate tests (baseline)
// ============================================================================

#[test]
fn test_translate_x_pixels() {
    // Test that translate_x with pixels moves the element by the exact pixel amount.
    let element = Empty::new().style(|s| {
        s.absolute()
            .inset_left(0.0)
            .inset_top(0.0)
            .size(50.0, 50.0)
            .translate_x(20.0) // Move right 20px
    });
    let element_id = element.view_id();

    let view = stack((element,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    // The transform should include a 20px x translation
    let transform = element_id.get_transform();

    // Extract translation from the affine transform
    let coeffs = transform.as_coeffs();
    let translate_x = coeffs[4]; // e coefficient is x translation

    assert!(
        (translate_x - 20.0).abs() < 0.1,
        "translate_x(20.0) should translate by 20px, got {}",
        translate_x
    );
}

#[test]
fn test_translate_y_pixels() {
    // Test that translate_y with pixels moves the element by the exact pixel amount.
    let element = Empty::new().style(|s| {
        s.absolute()
            .inset_left(0.0)
            .inset_top(0.0)
            .size(50.0, 50.0)
            .translate_y(30.0) // Move down 30px
    });
    let element_id = element.view_id();

    let view = stack((element,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let transform = element_id.get_transform();

    let coeffs = transform.as_coeffs();
    let translate_y = coeffs[5]; // f coefficient is y translation

    assert!(
        (translate_y - 30.0).abs() < 0.1,
        "translate_y(30.0) should translate by 30px, got {}",
        translate_y
    );
}

// ============================================================================
// Percentage-based translate tests (CSS semantics)
// ============================================================================

#[test]
fn test_translate_x_percentage() {
    // Test that translate_x with percentage moves the element by that percentage of its own WIDTH.
    // For a 100px wide element, translate_x(50%) should move it 50px to the right.
    let element = Empty::new().style(|s| {
        s.absolute()
            .inset_left(0.0)
            .inset_top(0.0)
            .size(100.0, 50.0) // 100px wide
            .translate_x(Pct(50.0)) // Move right by 50% of width = 50px
    });
    let element_id = element.view_id();

    let view = stack((element,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let transform = element_id.get_transform();

    let coeffs = transform.as_coeffs();
    let translate_x = coeffs[4];

    // 50% of 100px width = 50px
    assert!(
        (translate_x - 50.0).abs() < 0.1,
        "translate_x(50%) on 100px wide element should translate by 50px, got {}",
        translate_x
    );
}

#[test]
fn test_translate_y_percentage() {
    // Test that translate_y with percentage moves the element by that percentage of its own HEIGHT.
    // For a 80px tall element, translate_y(25%) should move it 20px down.
    let element = Empty::new().style(|s| {
        s.absolute()
            .inset_left(0.0)
            .inset_top(0.0)
            .size(50.0, 80.0) // 80px tall
            .translate_y(Pct(25.0)) // Move down by 25% of height = 20px
    });
    let element_id = element.view_id();

    let view = stack((element,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let transform = element_id.get_transform();

    let coeffs = transform.as_coeffs();
    let translate_y = coeffs[5];

    // 25% of 80px height = 20px
    assert!(
        (translate_y - 20.0).abs() < 0.1,
        "translate_y(25%) on 80px tall element should translate by 20px, got {}",
        translate_y
    );
}

#[test]
fn test_translate_negative_percentage() {
    // Test that negative percentage translates in the opposite direction.
    // For a 100px wide element, translate_x(-50%) should move it 50px to the left.
    let element = Empty::new().style(|s| {
        s.absolute()
            .inset_left(100.0)
            .inset_top(0.0)
            .size(100.0, 50.0)
            .translate_x(Pct(-50.0)) // Move left by 50% of width = -50px
    });
    let element_id = element.view_id();

    let view = stack((element,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let transform = element_id.get_transform();

    let coeffs = transform.as_coeffs();
    let translate_x = coeffs[4];

    // -50% of 100px width = -50px
    assert!(
        (translate_x - (-50.0)).abs() < 0.1,
        "translate_x(-50%) on 100px wide element should translate by -50px, got {}",
        translate_x
    );
}

// ============================================================================
// Centering with translate (common CSS pattern)
// ============================================================================

#[test]
fn test_center_with_translate() {
    // Test the common CSS centering pattern:
    // position: absolute; left: 50%; top: 50%; transform: translate(-50%, -50%);
    //
    // For a 100x60 element in a 200x200 container:
    // - left: 50% puts the left edge at x=100
    // - translate_x(-50%) moves it left by 50px (half its width)
    // - Result: element centered at x=50 (left edge), center at x=100
    let element = Empty::new().style(|s| {
        s.absolute()
            .inset_left(Pct(50.0)) // Left edge at 50% = 100px
            .inset_top(Pct(50.0)) // Top edge at 50% = 100px
            .size(100.0, 60.0) // 100x60 element
            .translate_x(Pct(-50.0)) // Move left by half width
            .translate_y(Pct(-50.0)) // Move up by half height
    });
    let element_id = element.view_id();

    let view = stack((element,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
    harness.rebuild();

    let transform = element_id.get_transform();

    let coeffs = transform.as_coeffs();
    let translate_x = coeffs[4];
    let translate_y = coeffs[5];

    // translate_x(-50%) on 100px wide = -50px
    // translate_y(-50%) on 60px tall = -30px
    assert!(
        (translate_x - (-50.0)).abs() < 0.1,
        "translate_x(-50%) on 100px element should be -50px, got {}",
        translate_x
    );
    assert!(
        (translate_y - (-30.0)).abs() < 0.1,
        "translate_y(-50%) on 60px element should be -30px, got {}",
        translate_y
    );
}

// ============================================================================
// Translate with different element sizes
// ============================================================================

#[test]
fn test_translate_percentage_scales_with_element_size() {
    // Test that the same percentage translates by different amounts for different sized elements.
    // 50% on a 200px element = 100px
    // 50% on a 50px element = 25px

    let large_element = Empty::new().style(|s| {
        s.absolute()
            .inset_left(0.0)
            .inset_top(0.0)
            .size(200.0, 50.0)
            .translate_x(Pct(50.0))
    });
    let large_id = large_element.view_id();

    let small_element = Empty::new().style(|s| {
        s.absolute()
            .inset_left(0.0)
            .inset_top(100.0)
            .size(50.0, 50.0)
            .translate_x(Pct(50.0))
    });
    let small_id = small_element.view_id();

    let view = stack((large_element, small_element)).style(|s| s.size(300.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 300.0, 200.0);
    harness.rebuild();

    let large_transform = large_id.get_transform();
    let small_transform = small_id.get_transform();

    let large_translate_x = large_transform.as_coeffs()[4];
    let small_translate_x = small_transform.as_coeffs()[4];

    // 50% of 200px = 100px
    assert!(
        (large_translate_x - 100.0).abs() < 0.1,
        "50% on 200px element should be 100px, got {}",
        large_translate_x
    );

    // 50% of 50px = 25px
    assert!(
        (small_translate_x - 25.0).abs() < 0.1,
        "50% on 50px element should be 25px, got {}",
        small_translate_x
    );
}
