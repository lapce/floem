use std::sync::LazyLock;

use crate::text::AttrsList;
use cosmic_text::{
    Buffer, BufferLine, Cursor, FontSystem, LayoutCursor, LayoutRunIter, LineEnding, LineIter,
    Metrics, Scroll, Shaping, Wrap,
};
use parking_lot::Mutex;
use peniko::kurbo::{Point, Size};

pub static FONT_SYSTEM: LazyLock<Mutex<FontSystem>> =
    LazyLock::new(|| Mutex::new(FontSystem::new()));

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
}

#[derive(Clone)]
pub struct TextLayout(Buffer);

impl Default for TextLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLayout {
    pub fn new() -> Self {
        TextLayout(Buffer::new_empty(Metrics::new(1.0, 1.0)))
    }

    pub fn set_text(&mut self, text: &str, attrs_list: AttrsList) {
        self.0.lines.clear();
        for (range, ending) in LineIter::new(text) {
            let line_attrs = attrs_list
                .0
                .clone()
                .split_off(range.len() + ending.as_str().len());
            self.0.lines.push(BufferLine::new(
                &text[range],
                ending,
                line_attrs,
                Shaping::Advanced,
            ));
        }
        if self.0.lines.is_empty() {
            self.0.lines.push(BufferLine::new(
                "",
                LineEnding::default(),
                attrs_list.0,
                Shaping::Advanced,
            ));
        }
        self.0.set_scroll(Scroll::default());
        let mut font_system = FONT_SYSTEM.lock();
        self.0.shape_until_scroll(&mut font_system, false);
    }

    pub fn set_wrap(&mut self, wrap: Wrap) {
        let mut font_system = FONT_SYSTEM.lock();
        self.0.set_wrap(&mut font_system, wrap);
    }

    pub fn set_tab_width(&mut self, tab_width: usize) {
        let mut font_system = FONT_SYSTEM.lock();
        self.0.set_tab_width(&mut font_system, tab_width as u16);
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        let mut font_system = FONT_SYSTEM.lock();
        self.0.set_size(&mut font_system, Some(width), Some(height));
    }

    pub fn lines(&self) -> &[BufferLine] {
        &self.0.lines
    }

    pub fn layout_runs(&self) -> LayoutRunIter {
        self.0.layout_runs()
    }

    pub fn layout_cursor(&mut self, cursor: Cursor) -> LayoutCursor {
        let line = cursor.line;
        let mut font_system = FONT_SYSTEM.lock();
        self.0
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
        for (line, run) in self.0.layout_runs().enumerate() {
            if run.line_i > last_line {
                last_line = run.line_i;
                offset += last_end + 1;
            }
            for glyph in run.glyphs {
                if glyph.start + offset > idx {
                    last_position.point.x += last_glyph_width as f64;
                    return last_position;
                }
                last_end = glyph.end;
                last_glyph_width = glyph.w;
                last_position = HitPosition {
                    line,
                    point: Point::new(glyph.x as f64, run.line_y as f64),
                    glyph_ascent: run.line_y as f64,
                    glyph_descent: (run.line_height - run.line_y) as f64,
                };
                if (glyph.start + offset..glyph.end + offset).contains(&idx) {
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
        if let Some(cursor) = self.0.hit(point.x as f32, point.y as f32) {
            HitPoint {
                line: cursor.line,
                index: cursor.index,
                is_inside: match cursor.affinity {
                    cosmic_text::Affinity::Before => true,
                    cosmic_text::Affinity::After => false,
                },
            }
        } else {
            HitPoint {
                line: 0,
                index: 0,
                is_inside: false,
            }
        }
    }

    pub fn size(&self) -> Size {
        self.0
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
