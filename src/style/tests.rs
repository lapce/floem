//! Unit tests for the style system.

use super::{Padding, PaddingProp, Style, StyleValue};
use crate::unit::PxPct;

#[test]
fn style_override() {
    let style1 = Style::new().padding_left(32.0);
    let style2 = Style::new().padding_left(64.0);

    let style = style1.apply(style2);

    // Check that the combined padding has the expected left value
    let padding = style.get(PaddingProp);
    assert_eq!(padding.left, Some(PxPct::Px(64.0)));

    let style1 = Style::new().padding_left(32.0).padding_bottom(45.0);
    let style2 = Style::new().padding_left(64.0);

    let style = style1.apply(style2);

    let padding = style.get(PaddingProp);
    assert_eq!(padding.left, Some(PxPct::Px(64.0)));
    assert_eq!(padding.bottom, Some(PxPct::Px(45.0))); // Should be preserved from style1

    // Test with explicit combined padding struct
    let style1 = Style::new().apply_padding(Padding::new().left(32.0).bottom(45.0));
    let style2 = Style::new().apply_padding(Padding::new().left(64.0));

    let style = style1.apply(style2);

    let padding = style.get(PaddingProp);
    assert_eq!(padding.left, Some(PxPct::Px(64.0)));
    assert_eq!(padding.bottom, Some(PxPct::Px(45.)));

    // Test that individual methods work correctly within a single style
    let style1 = Style::new().padding_left(32.0).padding_bottom(45.0);

    let padding = style1.get(PaddingProp);
    assert_eq!(padding.left, Some(PxPct::Px(32.0)));
    assert_eq!(padding.bottom, Some(PxPct::Px(45.0))); // Both values are preserved in same style

    // Test with StyleValue manipulation on combined struct
    let custom_padding = Padding::new().left(100.0).right(200.0);
    let style1 = Style::new().set_style_value(PaddingProp, StyleValue::Val(custom_padding));

    let padding = style1.get(PaddingProp);
    assert_eq!(padding.left, Some(PxPct::Px(100.0)));
    assert_eq!(padding.right, Some(PxPct::Px(200.0)));
    assert_eq!(padding.top, None);
    assert_eq!(padding.bottom, None);
}
