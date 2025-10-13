use std::{ops::Range, sync::LazyLock};

use crate::text::AttrsList;
use cosmic_text::{
    Affinity, Align, Buffer, BufferLine, Cursor, FontSystem, LayoutCursor, LayoutGlyph, LineEnding,
    LineIter, Metrics, Scroll, Shaping, Wrap,
};
use parking_lot::Mutex;
use peniko::kurbo::{Point, Size};
use unicode_segmentation::UnicodeSegmentation;

pub static FONT_SYSTEM: LazyLock<Mutex<FontSystem>> = LazyLock::new(|| {
    let mut font_system = FontSystem::new();
    #[cfg(target_os = "macos")]
    font_system.db_mut().set_sans_serif_family("Helvetica Neue");
    #[cfg(target_os = "windows")]
    font_system.db_mut().set_sans_serif_family("Segoe UI");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    font_system.db_mut().set_sans_serif_family("Noto Sans");
    Mutex::new(font_system)
});

/// A line of visible text for rendering
#[derive(Debug)]
pub struct LayoutRun<'a> {
    /// The index of the original text line
    pub line_i: usize,
    /// The original text line
    pub text: &'a str,
    /// True if the original paragraph direction is RTL
    pub rtl: bool,
    /// The array of layout glyphs to draw
    pub glyphs: &'a [LayoutGlyph],
    /// Maximum ascent of the glyphs in line
    pub max_ascent: f32,
    /// Maximum descent of the glyphs in line
    pub max_descent: f32,
    /// Y offset to baseline of line
    pub line_y: f32,
    /// Y offset to top of line
    pub line_top: f32,
    /// Y offset to next line
    pub line_height: f32,
    /// Width of line
    pub line_w: f32,
}

impl LayoutRun<'_> {
    /// Return the pixel span `Some((x_left, x_width))` of the highlighted area between `cursor_start`
    /// and `cursor_end` within this run, or None if the cursor range does not intersect this run.
    /// This may return widths of zero if `cursor_start == cursor_end`, if the run is empty, or if the
    /// region's left start boundary is the same as the cursor's end boundary or vice versa.
    pub fn highlight(&self, cursor_start: Cursor, cursor_end: Cursor) -> Option<(f32, f32)> {
        let mut x_start = None;
        let mut x_end = None;
        let rtl_factor = if self.rtl { 1. } else { 0. };
        let ltr_factor = 1. - rtl_factor;
        for glyph in self.glyphs.iter() {
            let cursor = self.cursor_from_glyph_left(glyph);
            if cursor >= cursor_start && cursor <= cursor_end {
                if x_start.is_none() {
                    x_start = Some(glyph.x + glyph.w * rtl_factor);
                }
                x_end = Some(glyph.x + glyph.w * rtl_factor);
            }
            let cursor = self.cursor_from_glyph_right(glyph);
            if cursor >= cursor_start && cursor <= cursor_end {
                if x_start.is_none() {
                    x_start = Some(glyph.x + glyph.w * ltr_factor);
                }
                x_end = Some(glyph.x + glyph.w * ltr_factor);
            }
        }
        if let Some(x_start) = x_start {
            let x_end = x_end.expect("end of cursor not found");
            let (x_start, x_end) = if x_start < x_end {
                (x_start, x_end)
            } else {
                (x_end, x_start)
            };
            Some((x_start, x_end - x_start))
        } else {
            None
        }
    }

    fn cursor_from_glyph_left(&self, glyph: &LayoutGlyph) -> Cursor {
        if self.rtl {
            Cursor::new_with_affinity(self.line_i, glyph.end, Affinity::Before)
        } else {
            Cursor::new_with_affinity(self.line_i, glyph.start, Affinity::After)
        }
    }

    pub fn cursor_from_glyph_right(&self, glyph: &LayoutGlyph) -> Cursor {
        if self.rtl {
            Cursor::new_with_affinity(self.line_i, glyph.start, Affinity::After)
        } else {
            Cursor::new_with_affinity(self.line_i, glyph.end, Affinity::Before)
        }
    }
}

/// An iterator of visible text lines, see [`LayoutRun`]
#[derive(Debug)]
pub struct LayoutRunIter<'b> {
    text_layout: &'b TextLayout,
    line_i: usize,
    layout_i: usize,
    total_height: f32,
    line_top: f32,
}

impl<'b> LayoutRunIter<'b> {
    pub fn new(text_layout: &'b TextLayout) -> Self {
        Self {
            text_layout,
            line_i: text_layout.buffer.scroll().line,
            layout_i: 0,
            total_height: 0.0,
            line_top: 0.0,
        }
    }
}

impl<'b> Iterator for LayoutRunIter<'b> {
    type Item = LayoutRun<'b>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(line) = self.text_layout.buffer.lines.get(self.line_i) {
            let shape = line.shape_opt()?;
            let layout = line.layout_opt()?;
            while let Some(layout_line) = layout.get(self.layout_i) {
                self.layout_i += 1;

                let line_height = layout_line
                    .line_height_opt
                    .unwrap_or(self.text_layout.buffer.metrics().line_height);
                self.total_height += line_height;

                let line_top = self.line_top - self.text_layout.buffer.scroll().vertical;
                let glyph_height = layout_line.max_ascent + layout_line.max_descent;
                let centering_offset = (line_height - glyph_height) / 2.0;
                let line_y = line_top + centering_offset + layout_line.max_ascent;
                if let Some(height) = self.text_layout.height_opt {
                    if line_y > height {
                        return None;
                    }
                }
                self.line_top += line_height;
                if line_y < 0.0 {
                    continue;
                }

                return Some(LayoutRun {
                    line_i: self.line_i,
                    text: line.text(),
                    rtl: shape.rtl,
                    glyphs: &layout_line.glyphs,
                    max_ascent: layout_line.max_ascent,
                    max_descent: layout_line.max_descent,
                    line_y,
                    line_top,
                    line_height,
                    line_w: layout_line.w,
                });
            }
            self.line_i += 1;
            self.layout_i = 0;
        }

        None
    }
}

pub struct HitPosition {
    /// Text line the cursor is on
    pub line: usize,
    /// Point of the cursor
    pub point: Point,
    /// ascent of glyph
    pub glyph_ascent: f64,
    /// descent of glyph
    pub glyph_descent: f64,
}

pub struct HitPoint {
    /// Text line the cursor is on
    pub line: usize,
    /// First-byte-index of glyph at cursor (will insert behind this glyph)
    pub index: usize,
    /// Whether or not the point was inside the bounds of the layout object.
    ///
    /// A click outside the layout object will still resolve to a position in the
    /// text; for instance a click to the right edge of a line will resolve to the
    /// end of that line, and a click below the last line will resolve to a
    /// position in that line.
    pub is_inside: bool,
    pub affinity: Affinity,
}

#[derive(Clone, Debug)]
pub struct TextLayout {
    buffer: Buffer,
    lines_range: Vec<Range<usize>>,
    width_opt: Option<f32>,
    height_opt: Option<f32>,
}

impl Default for TextLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLayout {
    pub fn new() -> Self {
        TextLayout {
            buffer: Buffer::new_empty(Metrics::new(16.0, 16.0)),
            lines_range: Vec::new(),
            width_opt: None,
            height_opt: None,
        }
    }

    pub fn new_with_text(text: &str, attrs_list: AttrsList, align: Option<Align>) -> Self {
        let mut layout = Self::new();
        layout.set_text(text, attrs_list, align);
        layout
    }

    pub fn set_text(&mut self, text: &str, attrs_list: AttrsList, align: Option<Align>) {
        self.buffer.lines.clear();
        self.lines_range.clear();
        let mut attrs_list = attrs_list.0;
        for (range, ending) in LineIter::new(text) {
            self.lines_range.push(range.clone());
            let line_text = &text[range];
            let new_attrs = attrs_list
                .clone()
                .split_off(line_text.len() + ending.as_str().len());
            let mut line =
                BufferLine::new(line_text, ending, attrs_list.clone(), Shaping::Advanced);
            line.set_align(align);
            self.buffer.lines.push(line);
            attrs_list = new_attrs;
        }
        if self.buffer.lines.is_empty() {
            let mut line =
                BufferLine::new("", LineEnding::default(), attrs_list, Shaping::Advanced);
            line.set_align(align);
            self.buffer.lines.push(line);
            self.lines_range.push(0..0)
        }
        self.buffer.set_scroll(Scroll::default());

        let mut font_system = FONT_SYSTEM.lock();

        // two-pass layout for alignment to work properly
        let needs_two_pass =
            align.is_some() && align != Some(Align::Left) && self.width_opt.is_none();
        if needs_two_pass {
            // first pass: shape and layout without width constraint to measure natural width
            self.buffer.shape_until_scroll(&mut font_system, false);

            // measure the actual width
            let measured_width = self
                .buffer
                .layout_runs()
                .fold(0.0f32, |width, run| width.max(run.line_w));

            // second pass: set the measured width and layout again
            if measured_width > 0.0 {
                self.buffer
                    .set_size(&mut font_system, Some(measured_width), self.height_opt);
                // shape again after size change
                self.buffer.shape_until_scroll(&mut font_system, false);
            }
        } else {
            // For left-aligned text, single pass is sufficient
            self.buffer.shape_until_scroll(&mut font_system, false);
        }
    }

    pub fn set_wrap(&mut self, wrap: Wrap) {
        let mut font_system = FONT_SYSTEM.lock();
        self.buffer.set_wrap(&mut font_system, wrap);
    }

    pub fn set_tab_width(&mut self, tab_width: usize) {
        let mut font_system = FONT_SYSTEM.lock();
        self.buffer
            .set_tab_width(&mut font_system, tab_width as u16);
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        let mut font_system = FONT_SYSTEM.lock();
        self.width_opt = Some(width);
        self.height_opt = Some(height);
        self.buffer
            .set_size(&mut font_system, Some(width), Some(height));
    }

    pub fn metrics(&self) -> Metrics {
        self.buffer.metrics()
    }

    pub fn lines(&self) -> &[BufferLine] {
        &self.buffer.lines
    }

    pub fn lines_range(&self) -> &[Range<usize>] {
        &self.lines_range
    }

    pub fn layout_runs(&self) -> LayoutRunIter<'_> {
        LayoutRunIter::new(self)
    }

    pub fn layout_cursor(&mut self, cursor: Cursor) -> LayoutCursor {
        let line = cursor.line;
        let mut font_system = FONT_SYSTEM.lock();
        self.buffer
            .layout_cursor(&mut font_system, cursor)
            .unwrap_or_else(|| LayoutCursor::new(line, 0, 0))
    }

    pub fn hit_position(&self, idx: usize) -> HitPosition {
        let mut last_line = 0;
        let mut last_end: usize = 0;
        let mut offset = 0;
        let mut last_glyph_width = 0.0;
        let mut last_position = HitPosition {
            line: 0,
            point: Point::ZERO,
            glyph_ascent: 0.0,
            glyph_descent: 0.0,
        };
        for (line, run) in self.layout_runs().enumerate() {
            if run.line_i > last_line {
                last_line = run.line_i;
                offset += last_end + 1;
            }
            for glyph in run.glyphs {
                last_end = glyph.end;
                last_glyph_width = glyph.w;
                last_position = HitPosition {
                    line,
                    point: Point::new(glyph.x as f64, run.line_y as f64),
                    glyph_ascent: run.max_ascent as f64,
                    glyph_descent: run.max_descent as f64,
                };
                if (glyph.start + offset..=glyph.end + offset).contains(&idx) {
                    // possibly inside ligature, need to resolve glyph internal offset

                    let glyph_str = &run.text[glyph.start..glyph.end];
                    let relative_idx = idx - offset - glyph.start;
                    let mut total_graphemes = 0;
                    let mut grapheme_i = 0;

                    for (i, _) in glyph_str.grapheme_indices(true) {
                        if relative_idx > i {
                            grapheme_i += 1;
                        }

                        total_graphemes += 1;
                    }

                    if glyph.level.is_rtl() {
                        grapheme_i = total_graphemes - grapheme_i;
                    }

                    last_position.point.x +=
                        (grapheme_i as f64 / total_graphemes as f64) * glyph.w as f64;

                    return last_position;
                }
            }
        }

        if idx > 0 {
            last_position.point.x += last_glyph_width as f64;
            return last_position;
        }

        HitPosition {
            line: 0,
            point: Point::ZERO,
            glyph_ascent: 0.0,
            glyph_descent: 0.0,
        }
    }

    pub fn hit_point(&self, point: Point) -> HitPoint {
        if let Some(cursor) = self.hit(point.x as f32, point.y as f32) {
            let size = self.size();
            let is_inside = point.x <= size.width && point.y <= size.height;
            HitPoint {
                line: cursor.line,
                index: cursor.index,
                is_inside,
                affinity: cursor.affinity,
            }
        } else {
            HitPoint {
                line: 0,
                index: 0,
                is_inside: false,
                affinity: Affinity::Before,
            }
        }
    }

    /// Convert x, y position to Cursor (hit detection)
    pub fn hit(&self, x: f32, y: f32) -> Option<Cursor> {
        self.buffer.hit(x, y)
    }

    pub fn line_col_position(&self, line: usize, col: usize) -> HitPosition {
        let mut last_glyph: Option<&LayoutGlyph> = None;
        let mut last_line = 0;
        let mut last_line_y = 0.0;
        let mut last_glyph_ascent = 0.0;
        let mut last_glyph_descent = 0.0;
        for (current_line, run) in self.layout_runs().enumerate() {
            for glyph in run.glyphs {
                match run.line_i.cmp(&line) {
                    std::cmp::Ordering::Equal => {
                        if glyph.start > col {
                            return HitPosition {
                                line: last_line,
                                point: Point::new(
                                    last_glyph.map(|g| (g.x + g.w) as f64).unwrap_or(0.0),
                                    last_line_y as f64,
                                ),
                                glyph_ascent: last_glyph_ascent as f64,
                                glyph_descent: last_glyph_descent as f64,
                            };
                        }
                        if (glyph.start..glyph.end).contains(&col) {
                            return HitPosition {
                                line: current_line,
                                point: Point::new(glyph.x as f64, run.line_y as f64),
                                glyph_ascent: run.max_ascent as f64,
                                glyph_descent: run.max_descent as f64,
                            };
                        }
                    }
                    std::cmp::Ordering::Greater => {
                        return HitPosition {
                            line: last_line,
                            point: Point::new(
                                last_glyph.map(|g| (g.x + g.w) as f64).unwrap_or(0.0),
                                last_line_y as f64,
                            ),
                            glyph_ascent: last_glyph_ascent as f64,
                            glyph_descent: last_glyph_descent as f64,
                        };
                    }
                    std::cmp::Ordering::Less => {}
                };
                last_glyph = Some(glyph);
            }
            last_line = current_line;
            last_line_y = run.line_y;
            last_glyph_ascent = run.max_ascent;
            last_glyph_descent = run.max_descent;
        }

        HitPosition {
            line: last_line,
            point: Point::new(
                last_glyph.map(|g| (g.x + g.w) as f64).unwrap_or(0.0),
                last_line_y as f64,
            ),
            glyph_ascent: last_glyph_ascent as f64,
            glyph_descent: last_glyph_descent as f64,
        }
    }

    pub fn size(&self) -> Size {
        self.buffer
            .layout_runs()
            .fold(Size::new(0.0, 0.0), |mut size, run| {
                let new_width = run.line_w as f64;
                if new_width > size.width {
                    size.width = new_width;
                }

                size.height += run.line_height as f64;

                size
            })
    }
}
