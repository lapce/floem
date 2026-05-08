//! Floem's high-level text API.
//!
//! This module exposes the text types used by Floem views and editor code:
//! - styling attributes and font vocabulary defined here
//! - Parley alignment, cursor, selection, and wrapping vocabulary used directly by Floem
//! - [`TextLayout`], Floem's layout wrapper around Parley
//! - [`TextLayoutState`], shared view state for overflow-aware text layout
//!
//! `TextLayout` deliberately hides Parley's concrete layout type from most
//! callers while still using Parley's lower-level vocabulary types.

use std::ops::Range;

mod attrs;
mod layout;
mod layout_state;

pub use attrs::{Attrs, AttrsList, AttrsOwned, FamilyOwned, LineHeightValue};
pub use fontique::{FontStyle, FontWeight, FontWidth};
pub use imaging::{GlyphRunRef, NormalizedCoord};
pub use layout::{FONT_CONTEXT, TextBrushTransformSpec, TextLayout, TextSelection};
pub use layout_state::{TextLayoutState, TextOverflowChanged};
pub use parley::Alignment;
pub use parley::layout::Glyph;
pub use parley::layout::{Affinity, Cursor, Selection};
pub use parley::style::{OverflowWrap, TextWrapMode, WordBreakStrength};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextBrush(pub usize);

pub trait GlyphDrawer {
    fn draw_glyphs<'a>(
        &mut self,
        origin: peniko::kurbo::Point,
        run: &GlyphRunRef<'a>,
        glyphs: impl Iterator<Item = Glyph> + 'a,
    );
}

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
