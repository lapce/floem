use std::{cell::RefCell, ops::Range, sync::LazyLock};

use crate::text::{Affinity, Alignment, AttrsList, TextBrush, Wrap};
use parking_lot::Mutex;
use parley::{
    layout::{AlignmentOptions, Layout},
    style::StyleProperty,
    FontContext, LayoutContext,
};
use peniko::kurbo::{Point, Size};

pub static FONT_CONTEXT: LazyLock<Mutex<FontContext>> =
    LazyLock::new(|| Mutex::new(FontContext::new()));

thread_local! {
    static LAYOUT_CONTEXT: RefCell<LayoutContext<TextBrush>> =
        RefCell::new(LayoutContext::new());
}

pub struct HitPosition {
    /// Text line the cursor is on.
    pub line: usize,
    /// Point of the cursor.
    pub point: Point,
    /// ascent of glyph.
    pub glyph_ascent: f64,
    /// descent of glyph.
    pub glyph_descent: f64,
}

pub struct HitPoint {
    /// Text line the cursor is on.
    pub line: usize,
    /// First-byte-index of glyph at cursor (will insert behind this glyph).
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

#[derive(Clone)]
pub struct TextLayout {
    layout: Layout<TextBrush>,
    text: String,
    lines_range: Vec<Range<usize>>,
    alignment: Option<Alignment>,
    wrap: Wrap,
    width_opt: Option<f32>,
    height_opt: Option<f32>,
    default_line_height: f32,
}

impl std::fmt::Debug for TextLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextLayout")
            .field("text", &self.text)
            .field("lines_range", &self.lines_range)
            .field("width_opt", &self.width_opt)
            .field("height_opt", &self.height_opt)
            .finish()
    }
}

impl Default for TextLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLayout {
    pub fn new() -> Self {
        TextLayout {
            layout: Layout::new(),
            text: String::new(),
            lines_range: Vec::new(),
            alignment: None,
            wrap: Wrap::Word,
            width_opt: None,
            height_opt: None,
            default_line_height: 16.0,
        }
    }

    pub fn new_with_text(text: &str, attrs_list: AttrsList, align: Option<Alignment>) -> Self {
        let mut layout = Self::new();
        layout.set_text(text, attrs_list, align);
        layout
    }

    pub fn set_text(&mut self, text: &str, attrs_list: AttrsList, align: Option<Alignment>) {
        self.text = text.to_string();
        self.alignment = align;
        self.lines_range.clear();

        // Compute per-paragraph byte ranges
        let mut start = 0;
        let bytes = text.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        while i < len {
            if bytes[i] == b'\r' {
                if i + 1 < len && bytes[i + 1] == b'\n' {
                    self.lines_range.push(start..i);
                    i += 2;
                    start = i;
                } else {
                    self.lines_range.push(start..i);
                    i += 1;
                    start = i;
                }
            } else if bytes[i] == b'\n' {
                self.lines_range.push(start..i);
                i += 1;
                start = i;
            } else {
                i += 1;
            }
        }
        self.lines_range.push(start..len);

        // Store default line height from attrs
        self.default_line_height = attrs_list.defaults().effective_line_height();

        // Build Parley layout (font context only needed during shaping)
        {
            let mut font_cx = FONT_CONTEXT.lock();
            LAYOUT_CONTEXT.with(|lc| {
                let mut layout_cx = lc.borrow_mut();
                let mut builder = layout_cx.ranged_builder(&mut font_cx, text, 1.0, true);

                // Apply attributes
                attrs_list.apply_to_builder(&mut builder);

                // Apply wrap mode
                match self.wrap {
                    Wrap::None => {
                        builder.push_default(StyleProperty::TextWrapMode(
                            parley::style::TextWrapMode::NoWrap,
                        ));
                    }
                    Wrap::Glyph => {
                        builder.push_default(StyleProperty::OverflowWrap(
                            parley::style::OverflowWrap::BreakWord,
                        ));
                    }
                    Wrap::Word => {}
                    Wrap::WordOrGlyph => {
                        builder.push_default(StyleProperty::OverflowWrap(
                            parley::style::OverflowWrap::BreakWord,
                        ));
                    }
                }

                builder.build_into(&mut self.layout, text);
            });
        }

        // Line breaking (no font context needed)
        let max_advance = if self.wrap == Wrap::None {
            None
        } else {
            self.width_opt
        };
        self.layout.break_all_lines(max_advance);

        // Two-pass alignment for non-left alignment without width constraint
        let needs_two_pass =
            align.is_some() && align != Some(Alignment::Left) && self.width_opt.is_none();

        if needs_two_pass {
            let measured_width = self.layout.full_width();
            if measured_width > 0.0 {
                self.layout.align(
                    Some(measured_width),
                    align.unwrap().into(),
                    AlignmentOptions::default(),
                );
            }
        } else if let Some(align) = align {
            self.layout
                .align(self.width_opt, align.into(), AlignmentOptions::default());
        }
    }

    pub fn set_wrap(&mut self, wrap: Wrap) {
        self.wrap = wrap;
    }

    pub fn set_tab_width(&mut self, _tab_width: usize) {
        // TODO!: Parley doesn't have a direct tab width setting
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        let old_width = self.width_opt;
        self.width_opt = Some(width);
        self.height_opt = Some(height);

        // Skip reflow if width hasn't changed (height doesn't affect line breaking)
        if old_width == Some(width) {
            return;
        }

        let max_advance = if self.wrap == Wrap::None {
            None
        } else {
            Some(width)
        };
        self.layout.break_all_lines(max_advance);

        if let Some(align) = self.alignment {
            self.layout
                .align(Some(width), align.into(), AlignmentOptions::default());
        }
    }

    pub fn lines_range(&self) -> &[Range<usize>] {
        &self.lines_range
    }

    /// Full text content.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Direct access to the Parley layout for renderers and consumers.
    pub fn parley_layout(&self) -> &Layout<TextBrush> {
        &self.layout
    }

    /// Count of visual lines in the layout.
    pub fn visual_line_count(&self) -> usize {
        self.layout.len()
    }

    pub fn size(&self) -> Size {
        let width = self.layout.full_width() as f64;
        let height = self.layout.height() as f64;
        Size::new(width, height)
    }

    /// Convert x, y position to Cursor (hit detection)
    pub fn hit(&self, x: f32, y: f32) -> Option<crate::text::Cursor> {
        if self.text.is_empty() {
            return Some(crate::text::Cursor::new(0, 0));
        }

        let pcursor = parley::editing::Cursor::from_point(&self.layout, x, y);
        let flat_idx = pcursor.index();
        let affinity: Affinity = pcursor.affinity().into();

        // Convert flat byte index to (line, index_within_line)
        for (line_i, range) in self.lines_range.iter().enumerate() {
            if flat_idx <= range.end || line_i == self.lines_range.len() - 1 {
                let local_idx = flat_idx.saturating_sub(range.start);
                return Some(crate::text::Cursor::new_with_affinity(
                    line_i, local_idx, affinity,
                ));
            }
        }

        Some(crate::text::Cursor::new(0, 0))
    }

    pub fn hit_position(&self, idx: usize) -> HitPosition {
        self.hit_position_aff(idx, Affinity::Before)
    }

    pub fn hit_position_aff(&self, idx: usize, affinity: Affinity) -> HitPosition {
        if self.text.is_empty() || self.layout.is_empty() {
            return HitPosition {
                line: 0,
                point: Point::ZERO,
                glyph_ascent: 0.0,
                glyph_descent: 0.0,
            };
        }

        let pcursor = parley::editing::Cursor::from_byte_index(&self.layout, idx, affinity.into());
        let bbox = pcursor.geometry(&self.layout, 0.0);

        // Find which visual line this cursor is on
        let mut visual_line = 0;
        for (i, line) in self.layout.lines().enumerate() {
            let range = line.text_range();
            if idx <= range.end {
                visual_line = i;
                break;
            }
            visual_line = i;
        }

        let metrics = self
            .layout
            .get(visual_line)
            .map(|l| *l.metrics())
            .unwrap_or_default();

        HitPosition {
            line: visual_line,
            point: Point::new(bbox.x0, metrics.baseline as f64),
            glyph_ascent: metrics.ascent as f64,
            glyph_descent: metrics.descent as f64,
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

    /// Convert a floem Cursor (paragraph_line, index_within_line) to a flat byte index.
    pub fn cursor_to_byte_index(&self, cursor: &crate::text::Cursor) -> usize {
        self.lines_range
            .get(cursor.line)
            .map(|r| r.start + cursor.index)
            .unwrap_or(0)
    }

    /// Compute selection highlight rectangles between two byte indices.
    /// Calls `f` with (x0, y0, x1, y1) for each visual line's selection rectangle.
    pub fn selection_geometry_with(
        &self,
        start_byte: usize,
        end_byte: usize,
        mut f: impl FnMut(f64, f64, f64, f64),
    ) {
        let anchor = parley::editing::Cursor::from_byte_index(
            &self.layout,
            start_byte,
            parley::layout::Affinity::Downstream,
        );
        let focus = parley::editing::Cursor::from_byte_index(
            &self.layout,
            end_byte,
            parley::layout::Affinity::Upstream,
        );
        let selection = parley::editing::Selection::new(anchor, focus);
        selection.geometry_with(&self.layout, |bbox, _line_idx| {
            f(bbox.x0, bbox.y0, bbox.x1, bbox.y1);
        });
    }

    /// Get the baseline y position of the nth visual line.
    pub fn visual_line_y(&self, nth: usize) -> Option<f32> {
        self.layout.get(nth).map(|l| l.metrics().baseline)
    }

    /// Get the text byte range of the nth visual line.
    pub fn visual_line_text_range(&self, nth: usize) -> Option<Range<usize>> {
        self.layout.get(nth).map(|l| l.text_range())
    }

    /// Check if the nth visual line is empty (has no items).
    pub fn visual_line_is_empty(&self, nth: usize) -> bool {
        self.layout.get(nth).is_none_or(|l| l.is_empty())
    }
}
