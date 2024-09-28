use std::borrow::Cow;

use crate::{
    peniko::Color,
    text::{Attrs, AttrsList},
};
use floem_editor_core::cursor::CursorAffinity;
use smallvec::SmallVec;

/// `PhantomText` is for text that is not in the actual document, but should be rendered with it.
///
/// Ex: Inlay hints, IME text, error lens' diagnostics, etc
#[derive(Debug, Clone)]
pub struct PhantomText {
    /// The kind is currently used for sorting the phantom text on a line
    pub kind: PhantomTextKind,
    /// Column on the line that the phantom text should be displayed at
    pub col: usize,
    /// the affinity of cursor, e.g. for completion phantom text,
    /// we want the cursor always before the phantom text
    pub affinity: Option<CursorAffinity>,
    pub text: String,
    pub font_size: Option<usize>,
    // font_family: Option<FontFamily>,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub under_line: Option<Color>,
}

#[derive(Debug, Clone, Copy, Ord, Eq, PartialEq, PartialOrd)]
pub enum PhantomTextKind {
    /// Input methods
    Ime,
    Placeholder,
    /// Completion lens / Inline completion
    Completion,
    /// Inlay hints supplied by an LSP/PSP (like type annotations)
    InlayHint,
    /// Error lens
    Diagnostic,
}

/// Information about the phantom text on a specific line.
///
/// This has various utility functions for transforming a coordinate (typically a column) into the
/// resulting coordinate after the phantom text is combined with the line's real content.
#[derive(Debug, Default, Clone)]
pub struct PhantomTextLine {
    /// This uses a smallvec because most lines rarely have more than a couple phantom texts
    pub text: SmallVec<[PhantomText; 6]>,
}

impl PhantomTextLine {
    /// Translate a column position into the text into what it would be after combining
    pub fn col_at(&self, pre_col: usize) -> usize {
        let mut last = pre_col;
        for (col_shift, size, col, _) in self.offset_size_iter() {
            if pre_col >= col {
                last = pre_col + col_shift + size;
            }
        }

        last
    }

    /// Translate a column position into the text into what it would be after combining
    ///
    /// If `before_cursor` is false and the cursor is right at the start then it will stay there
    /// (Think 'is the phantom text before the cursor')
    pub fn col_after(&self, pre_col: usize, before_cursor: bool) -> usize {
        let mut last = pre_col;
        for (col_shift, size, col, text) in self.offset_size_iter() {
            let before_cursor = match text.affinity {
                Some(CursorAffinity::Forward) => true,
                Some(CursorAffinity::Backward) => false,
                None => before_cursor,
            };

            if pre_col > col || (pre_col == col && before_cursor) {
                last = pre_col + col_shift + size;
            }
        }

        last
    }

    /// Translate a column position into the text into what it would be after combining
    ///
    /// it only takes `before_cursor` in the params without considering the
    /// cursor affinity in phantom text
    pub fn col_after_force(&self, pre_col: usize, before_cursor: bool) -> usize {
        let mut last = pre_col;
        for (col_shift, size, col, _) in self.offset_size_iter() {
            if pre_col > col || (pre_col == col && before_cursor) {
                last = pre_col + col_shift + size;
            }
        }

        last
    }

    /// Translate a column position into the text into what it would be after combining

    /// If `before_cursor` is false and the cursor is right at the start then it will stay there
    ///
    /// (Think 'is the phantom text before the cursor')
    ///
    /// This accepts a `PhantomTextKind` to ignore. Primarily for IME due to it needing to put the
    /// cursor in the middle.
    pub fn col_after_ignore(
        &self,
        pre_col: usize,
        before_cursor: bool,
        skip: impl Fn(&PhantomText) -> bool,
    ) -> usize {
        let mut last = pre_col;
        for (col_shift, size, col, phantom) in self.offset_size_iter() {
            if skip(phantom) {
                continue;
            }

            if pre_col > col || (pre_col == col && before_cursor) {
                last = pre_col + col_shift + size;
            }
        }

        last
    }

    /// Translate a column position into the position it would be before combining
    pub fn before_col(&self, col: usize) -> usize {
        let mut last = col;
        for (col_shift, size, hint_col, _) in self.offset_size_iter() {
            let shifted_start = hint_col + col_shift;
            let shifted_end = shifted_start + size;
            if col >= shifted_start {
                if col >= shifted_end {
                    last = col - col_shift - size;
                } else {
                    last = hint_col;
                }
            }
        }
        last
    }

    /// Insert the hints at their positions in the text
    pub fn combine_with_text<'a>(&self, text: &'a str) -> Cow<'a, str> {
        let mut text = Cow::Borrowed(text);
        let mut col_shift = 0;

        for phantom in self.text.iter() {
            let location = phantom.col + col_shift;

            // Stop iterating if the location is bad
            if text.get(location..).is_none() {
                return text;
            }

            let mut text_o = text.into_owned();
            text_o.insert_str(location, &phantom.text);
            text = Cow::Owned(text_o);

            col_shift += phantom.text.len();
        }

        text
    }

    /// Iterator over (col_shift, size, hint, pre_column)
    /// Note that this only iterates over the ordered text, since those depend on the text for where
    /// they'll be positioned
    pub fn offset_size_iter(
        &self,
    ) -> impl Iterator<Item = (usize, usize, usize, &PhantomText)> + '_ {
        let mut col_shift = 0;

        self.text.iter().map(move |phantom| {
            let pre_col_shift = col_shift;
            col_shift += phantom.text.len();
            (
                pre_col_shift,
                col_shift - pre_col_shift,
                phantom.col,
                phantom,
            )
        })
    }

    pub fn apply_attr_styles(&self, default: Attrs, attrs_list: &mut AttrsList) {
        for (offset, size, col, phantom) in self.offset_size_iter() {
            let start = col + offset;
            let end = start + size;

            let mut attrs = default;
            if let Some(fg) = phantom.fg {
                attrs = attrs.color(fg);
            }
            if let Some(phantom_font_size) = phantom.font_size {
                attrs = attrs.font_size((phantom_font_size as f32).min(attrs.font_size));
            }

            attrs_list.add_span(start..end, attrs);
        }
    }
}
