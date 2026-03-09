use std::{cell::RefCell, num::NonZeroU8, ops::Range, sync::LazyLock};

use floem_renderer::text::{AttrsList, TextBrush, TextGlyphsProps, TextLine, TextRun};
use parking_lot::Mutex;
use parley::{
    Affinity, Alignment, Cursor, FontContext, LayoutContext,
    layout::{AlignmentOptions, Glyph, Layout},
    style::{OverflowWrap, StyleProperty, TextWrapMode},
};
use peniko::{
    Fill,
    kurbo::{Affine, Point, Size},
};

pub static FONT_CONTEXT: LazyLock<Mutex<FontContext>> =
    LazyLock::new(|| Mutex::new(FontContext::new()));

thread_local! {
    static LAYOUT_CONTEXT: RefCell<LayoutContext<TextBrush>> =
        RefCell::new(LayoutContext::new());
}

#[derive(Clone, Debug)]
struct TabInfo {
    display_text: String,
    tabs: Vec<(usize, usize)>,
}

impl TabInfo {
    fn orig_to_display(&self, pos: usize) -> usize {
        let mut shift = 0usize;
        for &(tab_orig, tab_len) in &self.tabs {
            if tab_orig >= pos {
                break;
            }
            shift += tab_len - 1;
        }
        pos + shift
    }

    fn display_to_orig(&self, pos: usize) -> usize {
        let mut shift = 0usize;
        for &(tab_orig, tab_len) in &self.tabs {
            let tab_display = tab_orig + shift;
            if tab_display >= pos {
                break;
            }
            if pos < tab_display + tab_len {
                return tab_orig;
            }
            shift += tab_len - 1;
        }
        pos - shift
    }
}

fn expand_tabs(text: &str, tab_width: usize) -> Option<TabInfo> {
    if !text.as_bytes().contains(&b'\t') {
        return None;
    }

    let mut display = String::with_capacity(text.len());
    let mut tabs = Vec::new();
    let mut col = 0usize;

    for (i, c) in text.char_indices() {
        if c == '\t' {
            let spaces = tab_width - (col % tab_width);
            tabs.push((i, spaces));
            display.extend(std::iter::repeat_n(' ', spaces));
            col += spaces;
        } else {
            display.push(c);
            col = if c == '\n' || c == '\r' { 0 } else { col + 1 };
        }
    }

    Some(TabInfo {
        display_text: display,
        tabs,
    })
}

pub struct HitPosition {
    pub line: usize,
    pub point: Point,
    pub glyph_ascent: f64,
    pub glyph_descent: f64,
}

pub struct HitPoint {
    pub index: usize,
    pub is_inside: bool,
    pub affinity: Affinity,
}

#[derive(Clone)]
pub struct TextLayout {
    layout: Layout<TextBrush>,
    text: String,
    alignment: Option<Alignment>,
    text_wrap_mode: TextWrapMode,
    overflow_wrap: OverflowWrap,
    width_opt: Option<f32>,
    height_opt: Option<f32>,
    tab_width: Option<NonZeroU8>,
    tab_info: Option<TabInfo>,
}

impl std::fmt::Debug for TextLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextLayout")
            .field("text", &self.text)
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

pub struct LayoutLine<'a> {
    line: parley::layout::Line<'a, TextBrush>,
    origin: Point,
}

pub struct LayoutRun<'a> {
    glyph_run: parley::layout::GlyphRun<'a, TextBrush>,
    origin: Point,
}

impl TextRun for LayoutRun<'_> {
    fn props(&self) -> TextGlyphsProps<'_> {
        let run = self.glyph_run.run();
        let synthesis = run.synthesis();
        let glyph_transform = synthesis
            .skew()
            .map(|angle| Affine::skew((angle as f64).to_radians().tan(), 0.0));

        TextGlyphsProps::new(run.font())
            .font_size(run.font_size())
            .hint(false)
            .normalized_coords(run.normalized_coords())
            .style(Fill::NonZero)
            .brush(self.glyph_run.style().brush.0)
            .transform(Affine::translate((self.origin.x, self.origin.y)))
            .glyph_transform(glyph_transform)
    }

    fn glyphs(&self) -> impl Iterator<Item = Glyph> + Clone + '_ {
        self.glyph_run.positioned_glyphs()
    }
}

impl TextLine for LayoutLine<'_> {
    type Run<'a>
        = LayoutRun<'a>
    where
        Self: 'a;

    fn runs(&self) -> impl Iterator<Item = Self::Run<'_>> + Clone + '_ {
        self.line.items().filter_map(move |item| {
            let parley::layout::PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                return None;
            };

            Some(LayoutRun {
                glyph_run,
                origin: self.origin,
            })
        })
    }
}

impl TextLayout {
    #[inline]
    fn display_byte_index(&self, idx: usize) -> usize {
        if let Some(tab_info) = self.tab_info.as_ref() {
            tab_info.orig_to_display(idx)
        } else {
            idx
        }
    }

    fn selection_from_byte_range(
        &self,
        start_byte: usize,
        end_byte: usize,
    ) -> parley::editing::Selection {
        let anchor = parley::editing::Cursor::from_byte_index(
            &self.layout,
            self.display_byte_index(start_byte),
            Affinity::Downstream,
        );
        let focus = parley::editing::Cursor::from_byte_index(
            &self.layout,
            self.display_byte_index(end_byte),
            Affinity::Upstream,
        );
        parley::editing::Selection::new(anchor, focus)
    }

    fn reflow(&mut self, width: Option<f32>) {
        let max_advance = if self.text_wrap_mode == TextWrapMode::NoWrap {
            None
        } else {
            width
        };
        self.layout.break_all_lines(max_advance);

        if let Some(align) = self.alignment
            && let Some(width) = width
        {
            self.layout
                .align(Some(width), align, AlignmentOptions::default());
        }
    }

    pub fn new() -> Self {
        Self {
            layout: Layout::new(),
            text: String::new(),
            alignment: None,
            text_wrap_mode: TextWrapMode::Wrap,
            overflow_wrap: OverflowWrap::Normal,
            width_opt: None,
            height_opt: None,
            tab_width: None,
            tab_info: None,
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
        self.tab_info = self
            .tab_width
            .and_then(|w| expand_tabs(text, w.get() as usize));

        let layout_text = self
            .tab_info
            .as_ref()
            .map_or(text, |ti| ti.display_text.as_str());

        {
            let mut font_cx = FONT_CONTEXT.lock();
            LAYOUT_CONTEXT.with(|lc| {
                let mut layout_cx = lc.borrow_mut();
                let mut builder = layout_cx.ranged_builder(&mut font_cx, layout_text, 1.0, true);

                if let Some(ref ti) = self.tab_info {
                    let defaults = attrs_list.defaults();
                    defaults.apply_defaults(&mut builder);
                    for (range, attrs_owned) in attrs_list.spans() {
                        let display_range =
                            ti.orig_to_display(range.start)..ti.orig_to_display(range.end);
                        attrs_owned
                            .as_attrs()
                            .apply_range(&mut builder, display_range, &defaults);
                    }
                } else {
                    attrs_list.apply_to_builder(&mut builder);
                }

                builder.push_default(StyleProperty::TextWrapMode(self.text_wrap_mode));
                builder.push_default(StyleProperty::OverflowWrap(self.overflow_wrap));
                builder.build_into(&mut self.layout, layout_text);
            });
        }

        self.reflow(self.width_opt);
    }

    pub fn set_text_wrap_mode(&mut self, text_wrap_mode: TextWrapMode) {
        self.text_wrap_mode = text_wrap_mode;
    }

    pub fn set_overflow_wrap(&mut self, overflow_wrap: OverflowWrap) {
        self.overflow_wrap = overflow_wrap;
    }

    pub fn set_tab_width(&mut self, tab_width: usize) {
        self.tab_width = NonZeroU8::new(tab_width as u8);
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        let old_width = self.width_opt;
        self.width_opt = Some(width);
        self.height_opt = Some(height);
        if old_width != Some(width) {
            self.reflow(Some(width));
        }
    }

    pub fn clear_size(&mut self) {
        self.width_opt = None;
        self.height_opt = None;
        self.reflow(None);
    }

    pub fn set_align(&mut self, align: Option<Alignment>) {
        if self.alignment != align {
            self.alignment = align;
            self.reflow(self.width_opt);
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn parley_layout(&self) -> &Layout<TextBrush> {
        &self.layout
    }

    pub fn visual_line_count(&self) -> usize {
        self.layout.len()
    }

    pub fn size(&self) -> Size {
        Size::new(self.layout.full_width() as f64, self.layout.height() as f64)
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<Cursor> {
        if self.text.is_empty() {
            return Some(Cursor::from_byte_index(
                &self.layout,
                0,
                Affinity::default(),
            ));
        }
        Some(Cursor::from_point(&self.layout, x, y))
    }

    pub fn hit_position(&self, idx: usize) -> HitPosition {
        self.hit_position_aff(idx, Affinity::Upstream)
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

        let display_idx = if let Some(tab_info) = self.tab_info.as_ref() {
            tab_info.orig_to_display(idx)
        } else {
            idx
        };
        let pcursor = parley::editing::Cursor::from_byte_index(&self.layout, display_idx, affinity);
        let bbox = pcursor.geometry(&self.layout, 0.0);

        let cursor_y = bbox.y0 as f32;
        let line_count = self.layout.len();
        let visual_line = if line_count <= 1 {
            0
        } else {
            let mut lo = 0usize;
            let mut hi = line_count;
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                let exceeds = self
                    .layout
                    .get(mid)
                    .is_none_or(|l| l.metrics().max_coord <= cursor_y);
                if exceeds {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
            lo.min(line_count - 1)
        };

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
            HitPoint {
                index: self.cursor_to_byte_index(&cursor),
                is_inside: point.x <= size.width && point.y <= size.height,
                affinity: cursor.affinity(),
            }
        } else {
            HitPoint {
                index: 0,
                is_inside: false,
                affinity: Affinity::default(),
            }
        }
    }

    pub fn cursor_to_byte_index(&self, cursor: &Cursor) -> usize {
        let idx = cursor.index();
        if let Some(tab_info) = self.tab_info.as_ref() {
            tab_info.display_to_orig(idx)
        } else {
            idx
        }
    }

    pub fn selection_geometry_with(
        &self,
        start_byte: usize,
        end_byte: usize,
        mut f: impl FnMut(f64, f64, f64, f64),
    ) {
        let selection = self.selection_from_byte_range(start_byte, end_byte);
        selection.geometry_with(&self.layout, |bbox, _| {
            f(bbox.x0, bbox.y0, bbox.x1, bbox.y1);
        });
    }

    pub fn selection_geometry_with_line_metrics(
        &self,
        start_byte: usize,
        end_byte: usize,
        mut f: impl FnMut(f64, f64, f64, f64),
    ) {
        let selection = self.selection_from_byte_range(start_byte, end_byte);
        selection.geometry_with(&self.layout, |bbox, line_idx| {
            if let Some(line) = self.layout.get(line_idx) {
                let m = line.metrics();
                f(bbox.x0, m.min_coord as f64, bbox.x1, m.max_coord as f64);
            } else {
                f(bbox.x0, bbox.y0, bbox.x1, bbox.y1);
            }
        });
    }

    pub fn selection_for_cursors(
        &self,
        start: &Cursor,
        end: &Cursor,
        mut f: impl FnMut(f64, f64, f64, f64),
    ) {
        let selection = parley::editing::Selection::new(*start, *end);
        selection.geometry_with(&self.layout, |bbox, _| {
            f(bbox.x0, bbox.y0, bbox.x1, bbox.y1);
        });
    }

    pub fn selection_for_cursors_with_line_metrics(
        &self,
        start: &Cursor,
        end: &Cursor,
        mut f: impl FnMut(f64, f64, f64, f64),
    ) {
        let selection = parley::editing::Selection::new(*start, *end);
        selection.geometry_with(&self.layout, |bbox, line_idx| {
            if let Some(line) = self.layout.get(line_idx) {
                let m = line.metrics();
                f(bbox.x0, m.min_coord as f64, bbox.x1, m.max_coord as f64);
            } else {
                f(bbox.x0, bbox.y0, bbox.x1, bbox.y1);
            }
        });
    }

    pub fn visual_line_y(&self, nth: usize) -> Option<f32> {
        self.layout.get(nth).map(|l| l.metrics().baseline)
    }

    pub fn visual_line_text_range(&self, nth: usize) -> Option<Range<usize>> {
        self.layout.get(nth).map(|l| {
            let r = l.text_range();
            match self.tab_info {
                Some(ref ti) => ti.display_to_orig(r.start)..ti.display_to_orig(r.end),
                None => r,
            }
        })
    }

    pub fn visual_bounds_y(&self) -> Option<(f32, f32)> {
        if self.layout.is_empty() {
            return None;
        }

        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for i in 0..self.layout.len() {
            if let Some(line) = self.layout.get(i) {
                let m = line.metrics();
                min_y = min_y.min(m.min_coord);
                max_y = max_y.max(m.max_coord);
            }
        }

        (min_y.is_finite() && max_y.is_finite()).then_some((min_y, max_y))
    }

    pub fn centering_bounds_y(&self) -> Option<(f32, f32)> {
        if self.layout.is_empty() {
            return None;
        }

        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for i in 0..self.layout.len() {
            if let Some(line) = self.layout.get(i) {
                let m = line.metrics();
                min_y = min_y.min(m.baseline - m.ascent);
                max_y = max_y.max(m.baseline);
            }
        }

        (min_y.is_finite() && max_y.is_finite()).then_some((min_y, max_y))
    }

    pub fn layout_lines(
        &self,
        origin: impl Into<Point>,
    ) -> impl Iterator<Item = LayoutLine<'_>> + Clone {
        let origin = origin.into();
        self.layout
            .lines()
            .map(move |line| LayoutLine { line, origin })
    }
}
