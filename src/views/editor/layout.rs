use std::ops::Range;

use crate::{peniko::Color, text::TextLayout};
use floem_editor_core::buffer::rope_text::RopeText;

use super::{phantom_text::PhantomTextLine, visual_line::TextLayoutProvider};

#[derive(Clone, Debug)]
pub struct LineExtraStyle {
    pub x: f64,
    pub y: f64,
    pub width: Option<f64>,
    pub height: f64,
    pub bg_color: Option<Color>,
    pub under_line: Option<Color>,
    pub wave_line: Option<Color>,
}

#[derive(Clone)]
pub struct TextLayoutLine {
    /// Extra styling that should be applied to the text
    /// (x0, x1 or line display end, style)
    pub extra_style: Vec<LineExtraStyle>,
    pub text: TextLayout,
    pub whitespaces: Option<Vec<(char, (f64, f64))>>,
    pub indent: f64,
    pub phantom_text: PhantomTextLine,
}

/// Check if a text range within `full_text` contains non-whitespace characters.
/// Returns false for out-of-bounds or empty ranges.
fn has_visible_content(full_text: &str, range: &Range<usize>) -> bool {
    if range.end > full_text.len() || range.start >= range.end {
        return false;
    }
    full_text.as_bytes()[range.start..range.end]
        .iter()
        .any(|&b| !b.is_ascii_whitespace())
}

impl TextLayoutLine {
    /// The number of line breaks in the text layout. Always at least `1`.
    /// Only counts non-empty visual lines (matching old relevant_layouts behavior).
    pub fn line_count(&self) -> usize {
        self.relevant_layout_count().max(1)
    }

    /// Count of visual lines that contain non-whitespace content.
    /// Parley's whitespace-only lines are always trailing, so we scan
    /// backwards to find the last visible line rather than filtering all lines.
    pub fn relevant_layout_count(&self) -> usize {
        let count = self.text.visual_line_count();
        let full_text = self.text.text();
        (0..count)
            .rev()
            .find(|&i| {
                self.text
                    .visual_line_text_range(i)
                    .is_some_and(|r| has_visible_content(full_text, &r))
            })
            .map_or(0, |last| last + 1)
    }

    /// Iterator over the (start, end) columns of the relevant layouts.
    pub fn layout_cols<'a>(
        &'a self,
        text_prov: impl TextLayoutProvider + 'a,
        line: usize,
    ) -> impl Iterator<Item = (usize, usize)> + 'a {
        let visual_line_count = self.text.visual_line_count();
        let full_text = self.text.text();

        // Check if there's a single paragraph with all whitespace-only visual lines
        let mut prefix = None;
        if self.text.lines_range().len() == 1 && visual_line_count > 0 {
            let line_start = self.text.lines_range()[0].start;
            let all_whitespace = (0..visual_line_count).all(|i| {
                self.text
                    .visual_line_text_range(i)
                    .is_none_or(|r| !has_visible_content(full_text, &r))
            });
            if all_whitespace {
                prefix = Some((line_start, line_start));
            }
        }

        let line_v = line;

        // Collect visual line text ranges that have non-whitespace content
        let visual_ranges: Vec<_> = (0..visual_line_count)
            .filter_map(|i| {
                let range = self.text.visual_line_text_range(i)?;
                if has_visible_content(full_text, &range) {
                    Some(range)
                } else {
                    None
                }
            })
            .collect();

        let iter = visual_ranges.into_iter().map(move |text_range| {
            let start_idx = text_range.start;
            let mut end_idx = text_range.end.min(full_text.len());

            // Strip trailing whitespace from byte range (matching old behavior)
            while end_idx > start_idx {
                let ch = full_text.as_bytes().get(end_idx - 1).copied().unwrap_or(0);
                if ch == b' ' || ch == b'\t' || ch == b'\n' || ch == b'\r' {
                    end_idx -= 1;
                } else {
                    break;
                }
            }

            let start = start_idx;
            let end = end_idx;

            let text = text_prov.rope_text();
            let pre_end = text_prov.before_phantom_col(line_v, end);
            let line_offset = text.offset_of_line(line);
            let line_end = text.line_end_col(line, true);

            let end = if pre_end <= line_end {
                let after = text.slice_to_cow(line_offset + pre_end..line_offset + line_end);
                if after.starts_with(' ') && !after.starts_with("  ") {
                    end + 1
                } else {
                    end
                }
            } else {
                end
            };

            (start, end)
        });

        prefix.into_iter().chain(iter)
    }

    /// Iterator over the start columns of the relevant layouts
    pub fn start_layout_cols<'a>(
        &'a self,
        text_prov: impl TextLayoutProvider + 'a,
        line: usize,
    ) -> impl Iterator<Item = usize> + 'a {
        self.layout_cols(text_prov, line).map(|(start, _)| start)
    }

    /// Get the baseline y position of the given visual line index
    pub fn get_layout_y(&self, nth: usize) -> Option<f32> {
        self.text.visual_line_y(nth)
    }

    /// Get the (start x, end x) positions of the given visual line index
    pub fn get_layout_x(&self, nth: usize) -> Option<(f32, f32)> {
        let text_range = self.text.visual_line_text_range(nth)?;
        let full_text = self.text.text();

        if text_range.is_empty() || text_range.end > full_text.len() {
            return Some((0.0, 0.0));
        }

        let start_hit = self.text.hit_position(text_range.start);
        // For end, find last non-whitespace char
        let mut end_byte = text_range.end;
        while end_byte > text_range.start {
            let ch = full_text.as_bytes().get(end_byte - 1).copied().unwrap_or(0);
            if ch == b' ' || ch == b'\t' || ch == b'\n' || ch == b'\r' {
                end_byte -= 1;
            } else {
                break;
            }
        }
        let end_hit = self.text.hit_position(end_byte);

        Some((start_hit.point.x as f32, end_hit.point.x as f32))
    }
}
