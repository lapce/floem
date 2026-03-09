use std::ops::Range;

#[path = "text_layout.rs"]
mod text_layout;

pub use floem_renderer::text::{
    Attrs, AttrsList, AttrsOwned, FamilyOwned, FontStyle, FontWeight, FontWidth, Glyph, GlyphRun,
    Line, LineHeightValue, NormalizedCoord, TextGlyphsProps, TextLine, TextRun,
};
pub use parley::Alignment;
pub use parley::layout::{Affinity, Cursor};
pub use parley::style::{OverflowWrap, TextWrapMode, WordBreakStrength};
pub use text_layout::{FONT_CONTEXT, HitPoint, HitPosition, TextLayout};

pub fn line_ranges(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut i = 0;

    std::iter::from_fn(move || {
        if start > bytes.len() {
            return None;
        }

        while i < bytes.len() {
            match bytes[i] {
                b'\r' => {
                    let end = i;
                    i += if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                        2
                    } else {
                        1
                    };
                    let range = start..end;
                    start = i;
                    return Some(range);
                }
                b'\n' => {
                    let end = i;
                    i += 1;
                    let range = start..end;
                    start = i;
                    return Some(range);
                }
                _ => i += 1,
            }
        }

        let end = bytes.len();
        let range = start..end;
        start = bytes.len() + 1;
        Some(range)
    })
}
