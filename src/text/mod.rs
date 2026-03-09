use std::ops::Range;

mod layout;
mod layout_data;

pub use floem_renderer::text::{
    Attrs, AttrsList, AttrsOwned, FamilyOwned, FontStyle, FontWeight, FontWidth, Glyph, GlyphRun,
    Line, LineHeightValue, NormalizedCoord, TextGlyphsProps, TextLine, TextRun,
};
pub use layout::{FONT_CONTEXT, HitPoint, HitPosition, TextLayout};
pub use layout_data::{TextLayoutData, TextOverflowChanged};
pub use parley::Alignment;
pub use parley::layout::{Affinity, Cursor};
pub use parley::style::{OverflowWrap, TextWrapMode, WordBreakStrength};

/// Returns the byte ranges of the source text's logical paragraphs.
///
/// This splits the original string on line-ending boundaries (`\n`, `\r\n`, or `\r`)
/// and yields the content ranges between those separators. The returned ranges do not
/// include the line-ending bytes themselves.
///
/// This is a source-text helper, not a layout helper:
/// - use this when you need width-independent paragraph/logical-line ranges from the
///   raw text buffer
/// - do not use this when you need wrapped or shaped visual lines from Parley layout
///
/// In other words, this answers "how is the input text structurally split?" rather than
/// "how did the text lay out on screen?".
///
/// Typical uses:
/// - editor/document logic that works in terms of source paragraphs
/// - fallback handling for single-paragraph text
/// - debug or inspection code that wants the original paragraph segmentation
///
/// Prefer `TextLayout`/Parley queries when you care about:
/// - wrapping
/// - alignment
/// - hit testing
/// - visual line geometry
pub fn paragraph_ranges(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
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
