use crate::{peniko::Color, text::TextLayout};
use cosmic_text::LayoutLine;
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

impl TextLayoutLine {
    /// The number of line breaks in the text layout. Always at least `1`.
    pub fn line_count(&self) -> usize {
        self.relevant_layouts().count().max(1)
    }

    /// Iterate over all the layouts that are nonempty.
    /// Note that this may be empty if the line is completely empty, like the last line
    pub fn relevant_layouts(&self) -> impl Iterator<Item = &'_ LayoutLine> + '_ {
        // Even though we only have one hard line (and thus only one `lines` entry) typically, for
        // normal buffer lines, we can have more than one due to multiline phantom text. So we have
        // to sum over all of the entries line counts.
        self.text
            .lines()
            .iter()
            .flat_map(|l| l.layout_opt())
            .flat_map(|ls| ls.iter())
            .filter(|l| !l.glyphs.is_empty())
    }

    /// Iterator over the (start, end) columns of the relevant layouts.
    pub fn layout_cols<'a>(
        &'a self,
        text_prov: impl TextLayoutProvider + 'a,
        line: usize,
    ) -> impl Iterator<Item = (usize, usize)> + 'a {
        let mut prefix = None;
        // Include an entry if there is nothing
        if self.text.lines().len() == 1 {
            let line_start = self.text.lines_range()[0].start;
            if let Some(layouts) = self.text.lines()[0].layout_opt() {
                // Do we need to require !layouts.is_empty()?
                if !layouts.is_empty() && layouts.iter().all(|l| l.glyphs.is_empty()) {
                    // We assume the implicit glyph start is zero
                    prefix = Some((line_start, line_start));
                }
            }
        }

        let line_v = line;
        let iter = self
            .text
            .lines()
            .iter()
            .zip(self.text.lines_range().iter())
            .filter_map(|(line, line_range)| line.layout_opt().map(|ls| (line, line_range, ls)))
            .flat_map(|(line, line_range, ls)| ls.iter().map(move |l| (line, line_range, l)))
            .filter(|(_, _, l)| !l.glyphs.is_empty())
            .map(move |(tl_line, line_range, l)| {
                let line_start = line_range.start;
                tl_line.align();

                let start = line_start + l.glyphs[0].start;
                let end = line_start + l.glyphs.last().unwrap().end;

                let text = text_prov.rope_text();
                // We can't just use the original end, because the *true* last glyph on the line
                // may be a space, but it isn't included in the layout! Though this only happens
                // for single spaces, for some reason.
                let pre_end = text_prov.before_phantom_col(line_v, end);
                let line_offset = text.offset_of_line(line);

                // TODO(minor): We don't really need the entire line, just the two characters after
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

    /// Get the top y position of the given line index
    pub fn get_layout_y(&self, nth: usize) -> Option<f32> {
        self.text.layout_runs().nth(nth).map(|run| run.line_y)
    }

    /// Get the (start x, end x) positions of the given line index
    pub fn get_layout_x(&self, nth: usize) -> Option<(f32, f32)> {
        self.text.layout_runs().nth(nth).map(|run| {
            (
                run.glyphs.first().map(|g| g.x).unwrap_or(0.0),
                run.glyphs.last().map(|g| g.x + g.w).unwrap_or(0.0),
            )
        })
    }
}
