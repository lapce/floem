//! Unit tests for the style system.

use super::{Padding, PaddingBottom, PaddingLeft, PaddingRight, Style};
use crate::unit::PxPct;

#[test]
fn style_override() {
    let style1 = Style::new().padding_left(32.0);
    let style2 = Style::new().padding_left(64.0);

    let style = style1.apply(style2);

    // Check that the combined padding has the expected left value
    assert_eq!(style.get(PaddingLeft), PxPct::Px(64.0));

    let style1 = Style::new().padding_left(32.0).padding_bottom(45.0);
    let style2 = Style::new().padding_left(64.0);

    let style = style1.apply(style2);

    assert_eq!(style.get(PaddingLeft), PxPct::Px(64.0));
    assert_eq!(style.get(PaddingBottom), PxPct::Px(45.0)); // Should be preserved from style1

    // Test with explicit combined padding struct
    let style1 = Style::new().apply_padding(Padding::new().left(32.0).bottom(45.0));
    let style2 = Style::new().apply_padding(Padding::new().left(64.0));

    let style = style1.apply(style2);

    assert_eq!(style.get(PaddingLeft), PxPct::Px(64.0));
    assert_eq!(style.get(PaddingBottom), PxPct::Px(45.));

    // Test that individual methods work correctly within a single style
    let style1 = Style::new().padding_left(32.0).padding_bottom(45.0);

    assert_eq!(style1.get(PaddingLeft), PxPct::Px(32.0));
    assert_eq!(style1.get(PaddingBottom), PxPct::Px(45.0)); // Both values are preserved in same style

    // Test aggregate helper on top of split props
    let custom_padding = Padding::new().left(100.0).right(200.0);
    let style1 = Style::new().apply_padding(custom_padding);

    assert_eq!(style1.get(PaddingLeft), PxPct::Px(100.0));
    assert_eq!(style1.get(PaddingRight), PxPct::Px(200.0));
}
