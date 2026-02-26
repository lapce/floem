use std::{cell::RefCell, ops::Range, sync::LazyLock};

use crate::text::{Affinity, Alignment, AttrsList, TextBrush, Wrap};
use parking_lot::Mutex;
use parley::{
    layout::{AlignmentOptions, Layout},
    style::StyleProperty,
    FontContext, LayoutContext,
};
use peniko::kurbo::{Point, Size};

/// Global shared font context used for font discovery and shaping.
///
/// Parley requires a [`FontContext`] during layout building. This static provides
/// a single shared instance behind a [`Mutex`].
/// The lock is only held for the duration of a [`TextLayout::set_text`] call.
pub static FONT_CONTEXT: LazyLock<Mutex<FontContext>> =
    LazyLock::new(|| Mutex::new(FontContext::new()));

thread_local! {
    /// Thread-local layout context that caches shaping data between layouts.
    ///
    /// [`LayoutContext`] is not `Send`, so each thread keeps its own instance.
    /// It is borrowed mutably for the duration of a [`TextLayout::set_text`] call.
    static LAYOUT_CONTEXT: RefCell<LayoutContext<TextBrush>> =
        RefCell::new(LayoutContext::new());
}

/// The geometric position of a cursor at a given byte index in a [`TextLayout`].
///
/// Returned by [`TextLayout::hit_position`] and [`TextLayout::hit_position_aff`].
/// Contains the cursor's (x, baseline-y) coordinates plus the ascent and descent
/// of the glyph at that position, which together define the cursor's visible height.
pub struct HitPosition {
    /// Visual line index the cursor is on.
    pub line: usize,
    /// Cursor position where `x` is the horizontal offset and `y` is the baseline.
    pub point: Point,
    /// Ascent of the glyph at the cursor (distance above the baseline).
    pub glyph_ascent: f64,
    /// Descent of the glyph at the cursor (distance below the baseline).
    pub glyph_descent: f64,
}

/// Result of hit-testing a point against a [`TextLayout`].
///
/// Returned by [`TextLayout::hit_point`]. Maps a pixel coordinate to the
/// nearest text position, indicating which paragraph line and byte offset
/// the point maps to and whether it fell within the layout bounds.
pub struct HitPoint {
    /// Paragraph line index the resolved position is on.
    pub line: usize,
    /// Byte offset within the paragraph line (insert position).
    pub index: usize,
    /// Whether the queried point was inside the layout bounds.
    ///
    /// A click outside the layout still resolves to a text position:
    /// - click to the right of a line resolves to the end of that line
    /// - click below the last line resolves to a position in that line
    pub is_inside: bool,
    /// Cursor affinity at the resolved position.
    pub affinity: Affinity,
}

/// Shaped and positioned text ready for rendering.
///
/// `TextLayout` wraps a Parley [`Layout`] and adds paragraph-aware cursor
/// mapping, hit-testing, selection geometry, and line-breaking control.
///
/// # Lifecycle
///
/// 1. Create with [`TextLayout::new`] (empty) or [`TextLayout::new_with_text`].
/// 2. Optionally call [`set_wrap`](Self::set_wrap) before setting text.
/// 3. Set or update text and attributes via [`set_text`](Self::set_text).
/// 4. Constrain dimensions with [`set_size`](Self::set_size) to trigger reflow.
/// 5. Query the result: [`size`](Self::size), [`hit`](Self::hit),
///    [`hit_position`](Self::hit_position), [`selection_geometry_with`](Self::selection_geometry_with), etc.
///
/// # Example
///
/// ```no_run
/// use floem_renderer::text::{Attrs, AttrsList, TextLayout, Wrap};
///
/// let mut layout = TextLayout::new();
/// layout.set_wrap(Wrap::Word);
/// layout.set_text("Hello, world!", AttrsList::new(Attrs::new()), None);
/// layout.set_size(200.0, f32::MAX);
///
/// let size = layout.size();
/// ```
#[derive(Clone)]
pub struct TextLayout {
    /// The underlying Parley layout containing shaped glyph runs.
    layout: Layout<TextBrush>,
    /// The full text content.
    text: String,
    /// Byte ranges for each paragraph line (split on `\n`, `\r\n`, or `\r`).
    lines_range: Vec<Range<usize>>,
    /// Text alignment (left, center, right, justified), or `None` for the default (left).
    alignment: Option<Alignment>,
    /// Word/glyph wrapping strategy.
    wrap: Wrap,
    /// Maximum width constraint in pixels, or `None` if unconstrained.
    width_opt: Option<f32>,
    /// Maximum height constraint in pixels, or `None` if unconstrained.
    height_opt: Option<f32>,
    /// Default line height computed from the attrs at [`set_text`](Self::set_text) time.
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
    /// Creates an empty `TextLayout` with word wrapping and no text.
    ///
    /// Call [`set_text`](Self::set_text) afterwards to populate it.
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

    /// Creates a `TextLayout` pre-populated with the given text, attributes, and alignment.
    ///
    /// This is a convenience for `TextLayout::new()` followed by
    /// [`set_text`](Self::set_text).
    pub fn new_with_text(text: &str, attrs_list: AttrsList, align: Option<Alignment>) -> Self {
        let mut layout = Self::new();
        layout.set_text(text, attrs_list, align);
        layout
    }

    /// Sets or replaces the text content, attributes, and alignment.
    ///
    /// This performs the full layout pipeline: paragraph splitting, Parley
    /// shaping (under [`FONT_CONTEXT`] lock), line breaking, and alignment.
    /// If a width constraint was previously set via [`set_size`](Self::set_size),
    /// it is applied during line breaking.
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
                    align.unwrap_or_default(),
                    AlignmentOptions::default(),
                );
            }
        } else if let Some(align) = align {
            self.layout
                .align(self.width_opt, align, AlignmentOptions::default());
        }
    }

    /// Sets the text wrapping strategy.
    ///
    /// Must be called **before** [`set_text`](Self::set_text) for the wrap mode
    /// to take effect during shaping.
    pub fn set_wrap(&mut self, wrap: Wrap) {
        self.wrap = wrap;
    }

    /// Sets the tab width in number of spaces (currently a no-op).
    ///
    /// Parley does not yet expose a direct tab-width setting.
    pub fn set_tab_width(&mut self, _tab_width: usize) {
        // TODO!
    }

    /// Sets the layout width and height constraints in pixels.
    ///
    /// If the width changes (and wrapping is enabled), the layout is reflowed:
    /// lines are re-broken and alignment is re-applied. Height changes alone
    /// do not trigger reflow since height does not affect line breaking.
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
                .align(Some(width), align, AlignmentOptions::default());
        }
    }

    /// Returns the byte ranges of each paragraph line.
    ///
    /// Paragraph lines are split on `\n`, `\r\n`, or `\r`. Each range covers
    /// the text content without the line ending.
    pub fn lines_range(&self) -> &[Range<usize>] {
        &self.lines_range
    }

    /// Returns the full text content.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns direct access to the underlying Parley [`Layout`].
    ///
    /// This is intended for renderers that need to iterate glyph runs.
    /// Application code should prefer the higher-level methods on `TextLayout`.
    pub fn parley_layout(&self) -> &Layout<TextBrush> {
        &self.layout
    }

    /// Returns the number of visual lines (after wrapping) in the layout.
    pub fn visual_line_count(&self) -> usize {
        self.layout.len()
    }

    /// Returns the total size of the laid-out text as a `(width, height)` pair.
    pub fn size(&self) -> Size {
        let width = self.layout.full_width() as f64;
        let height = self.layout.height() as f64;
        Size::new(width, height)
    }

    /// Converts pixel coordinates to a [`Cursor`](crate::text::Cursor) (hit detection).
    ///
    /// Returns `Some(Cursor)` with the paragraph line and byte offset nearest
    /// to `(x, y)`. Returns a zero cursor for empty text.
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

    /// Returns the geometric position of the cursor at the given flat byte index.
    ///
    /// Uses [`Affinity::Before`] by default. See [`hit_position_aff`](Self::hit_position_aff)
    /// for explicit affinity control.
    pub fn hit_position(&self, idx: usize) -> HitPosition {
        self.hit_position_aff(idx, Affinity::Before)
    }

    /// Returns the geometric position of the cursor at the given flat byte index
    /// with explicit affinity.
    ///
    /// The returned [`HitPosition`] contains the cursor's pixel coordinates and
    /// the ascent/descent of the glyph at that position.
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

        // Find which visual line this cursor is on using the geometry's y
        // coordinate. This correctly accounts for affinity at wrapped-line
        // boundaries (where the byte index belongs to both lines).
        let cursor_y = bbox.y0 as f32;
        let mut visual_line = 0;
        for (i, line) in self.layout.lines().enumerate() {
            let m = line.metrics();
            if cursor_y >= m.min_coord && cursor_y < m.max_coord {
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

    /// Hit-tests a point and returns a [`HitPoint`] with the nearest text position.
    ///
    /// Unlike [`hit`](Self::hit), this also reports whether the point was inside
    /// the layout bounds via [`HitPoint::is_inside`].
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

    /// Converts a [`Cursor`](crate::text::Cursor) (paragraph line + offset) to a flat byte index
    /// into the full text.
    pub fn cursor_to_byte_index(&self, cursor: &crate::text::Cursor) -> usize {
        self.lines_range
            .get(cursor.line)
            .map(|r| r.start + cursor.index)
            .unwrap_or(0)
    }

    /// Computes selection highlight rectangles between two flat byte indices.
    ///
    /// Calls `f(x0, y0, x1, y1)` once for each visual line that the selection
    /// spans. This avoids allocating a `Vec` of rectangles.
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

    /// Returns the baseline y-position (in pixels) of the `nth` visual line,
    /// or `None` if `nth` is out of bounds.
    pub fn visual_line_y(&self, nth: usize) -> Option<f32> {
        self.layout.get(nth).map(|l| l.metrics().baseline)
    }

    /// Returns the text byte range covered by the `nth` visual line,
    /// or `None` if `nth` is out of bounds.
    pub fn visual_line_text_range(&self, nth: usize) -> Option<Range<usize>> {
        self.layout.get(nth).map(|l| l.text_range())
    }

    /// Returns `true` if the `nth` visual line has no glyph items (or does not exist).
    pub fn visual_line_is_empty(&self, nth: usize) -> bool {
        self.layout.get(nth).is_none_or(|l| l.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{Attrs, AttrsList, Cursor, FamilyOwned};
    use std::sync::Once;

    const DEJAVU_SERIF: &[u8] = include_bytes!("../../../examples/webgpu/fonts/DejaVuSerif.ttf");

    static FONT_INIT: Once = Once::new();

    fn ensure_font() {
        FONT_INIT.call_once(|| {
            let mut font_cx = FONT_CONTEXT.lock();
            font_cx
                .collection
                .register_fonts(DEJAVU_SERIF.to_vec().into(), None);
        });
    }

    fn default_attrs() -> AttrsList {
        let family = vec![FamilyOwned::Name("DejaVu Serif".into())];
        AttrsList::new(Attrs::new().font_size(16.0).family(&family))
    }

    fn make(text: &str) -> TextLayout {
        ensure_font();
        TextLayout::new_with_text(text, default_attrs(), None)
    }

    fn make_wrapped(text: &str, width: f32) -> TextLayout {
        ensure_font();
        let mut layout = TextLayout::new();
        layout.set_text(text, default_attrs(), None);
        layout.set_size(width, f32::MAX);
        layout
    }

    // ========================== Construction ==========================

    #[test]
    fn new_layout_is_empty() {
        let layout = TextLayout::new();
        assert!(layout.text().is_empty());
        assert_eq!(layout.visual_line_count(), 0);
        assert!(layout.lines_range().is_empty());
    }

    #[test]
    fn new_with_text_populates() {
        let layout = make("hello");
        assert_eq!(layout.text(), "hello");
        assert!(layout.visual_line_count() >= 1);
    }

    #[test]
    fn set_text_replaces_content() {
        ensure_font();
        let mut layout = TextLayout::new();
        layout.set_text("first", default_attrs(), None);
        assert_eq!(layout.text(), "first");

        layout.set_text("second", default_attrs(), None);
        assert_eq!(layout.text(), "second");
    }

    // ========================== Paragraph lines ==========================

    #[test]
    fn lines_range_single_line() {
        let layout = make("hello world");
        assert_eq!(layout.lines_range(), &[0..11]);
    }

    #[test]
    fn lines_range_lf() {
        let layout = make("aaa\nbbb\nccc");
        assert_eq!(layout.lines_range(), &[0..3, 4..7, 8..11]);
    }

    #[test]
    fn lines_range_crlf() {
        let layout = make("aaa\r\nbbb\r\nccc");
        assert_eq!(layout.lines_range(), &[0..3, 5..8, 10..13]);
    }

    #[test]
    fn lines_range_cr() {
        let layout = make("aaa\rbbb");
        assert_eq!(layout.lines_range(), &[0..3, 4..7]);
    }

    #[test]
    fn lines_range_trailing_newline() {
        let layout = make("aaa\n");
        assert_eq!(layout.lines_range(), &[0..3, 4..4]);
    }

    #[test]
    fn lines_range_empty_text() {
        let layout = make("");
        // Even empty text produces one (empty) paragraph.
        assert_eq!(layout.lines_range(), &[0..0]);
    }

    #[test]
    fn lines_range_multiple_empty_lines() {
        let layout = make("\n\n");
        assert_eq!(layout.lines_range(), &[0..0, 1..1, 2..2]);
    }

    // ========================== Measurement ==========================

    #[test]
    fn size_nonempty_has_positive_dimensions() {
        let layout = make("Hello, world!");
        let size = layout.size();
        assert!(size.width > 0.0, "width should be positive");
        assert!(size.height > 0.0, "height should be positive");
    }

    #[test]
    fn size_empty_text() {
        let layout = make("");
        let size = layout.size();
        // Parley may report a small width for the empty paragraph's line metrics.
        // Height may be zero or one line-height depending on implementation.
        // Main invariant: non-negative.
        assert!(size.width >= 0.0);
        assert!(size.height >= 0.0);
    }

    #[test]
    fn size_grows_with_text_length() {
        let short = make("Hi");
        let long = make("Hello, world! This is a longer sentence.");
        assert!(long.size().width > short.size().width);
    }

    // ========================== Visual lines ==========================

    #[test]
    fn visual_line_count_single_line() {
        let layout = make("short");
        assert_eq!(layout.visual_line_count(), 1);
    }

    #[test]
    fn visual_line_count_with_wrapping() {
        let layout = make_wrapped(
            "The quick brown fox jumps over the lazy dog and keeps running around",
            100.0,
        );
        assert!(
            layout.visual_line_count() > 1,
            "wrapped text should produce multiple visual lines, got {}",
            layout.visual_line_count()
        );
    }

    #[test]
    fn visual_line_count_multiline() {
        let layout = make("line1\nline2\nline3");
        // At least 3 visual lines (one per paragraph), possibly more with trailing empty.
        assert!(layout.visual_line_count() >= 3);
    }

    #[test]
    fn visual_line_y_first_is_some() {
        let layout = make("hello");
        assert!(layout.visual_line_y(0).is_some());
    }

    #[test]
    fn visual_line_y_out_of_bounds() {
        let layout = make("hello");
        assert!(layout.visual_line_y(999).is_none());
    }

    #[test]
    fn visual_line_y_increases() {
        let layout = make("line1\nline2\nline3");
        let y0 = layout.visual_line_y(0).unwrap();
        let y1 = layout.visual_line_y(1).unwrap();
        let y2 = layout.visual_line_y(2).unwrap();
        assert!(y1 > y0, "y should increase: y0={y0}, y1={y1}");
        assert!(y2 > y1, "y should increase: y1={y1}, y2={y2}");
    }

    #[test]
    fn visual_line_text_range_covers_text() {
        let layout = make("abcdef");
        let range = layout.visual_line_text_range(0).unwrap();
        // Range should cover at least some of the text.
        assert!(range.start == 0);
        assert!(range.end >= 6);
    }

    #[test]
    fn visual_line_text_range_out_of_bounds() {
        let layout = make("hello");
        assert!(layout.visual_line_text_range(999).is_none());
    }

    #[test]
    fn visual_line_is_empty_on_text() {
        let layout = make("hello");
        assert!(!layout.visual_line_is_empty(0));
    }

    #[test]
    fn visual_line_is_empty_out_of_bounds() {
        let layout = make("hello");
        assert!(layout.visual_line_is_empty(999));
    }

    // ========================== Hit testing ==========================

    #[test]
    fn hit_empty_text() {
        let layout = make("");
        let cursor = layout.hit(0.0, 0.0).unwrap();
        assert_eq!(cursor.line, 0);
        assert_eq!(cursor.index, 0);
    }

    #[test]
    fn hit_origin_returns_start() {
        let layout = make("hello");
        let cursor = layout.hit(0.0, 0.0).unwrap();
        assert_eq!(cursor.line, 0);
        assert_eq!(cursor.index, 0);
    }

    #[test]
    fn hit_far_right_returns_line_end() {
        let layout = make("hello");
        let cursor = layout.hit(9999.0, 0.0).unwrap();
        assert_eq!(cursor.line, 0);
        // Index should be at or near the end of the text.
        assert!(cursor.index >= 4, "expected near end, got {}", cursor.index);
    }

    #[test]
    fn hit_second_paragraph() {
        let layout = make("aaa\nbbb");
        let y = layout.visual_line_y(1).unwrap_or(20.0);
        let cursor = layout.hit(0.0, y).unwrap();
        assert_eq!(cursor.line, 1, "should resolve to second paragraph line");
    }

    // ========================== Hit position ==========================

    #[test]
    fn hit_position_start() {
        let layout = make("hello");
        let pos = layout.hit_position(0);
        assert_eq!(pos.line, 0);
        assert!(
            pos.point.x < 5.0,
            "x at start should be near 0, got {}",
            pos.point.x
        );
    }

    #[test]
    fn hit_position_end() {
        let layout = make("hello");
        let pos = layout.hit_position(5);
        assert!(
            pos.point.x > 10.0,
            "x at end should be well past 0, got {}",
            pos.point.x
        );
    }

    #[test]
    fn hit_position_has_glyph_metrics() {
        let layout = make("hello");
        let pos = layout.hit_position(0);
        assert!(pos.glyph_ascent > 0.0, "ascent should be positive");
        // Descent is typically positive (distance below baseline).
        assert!(pos.glyph_descent > 0.0, "descent should be positive");
    }

    #[test]
    fn hit_position_empty() {
        let layout = make("");
        let pos = layout.hit_position(0);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.point, Point::ZERO);
    }

    #[test]
    fn hit_position_aff_before_and_after() {
        let layout = make("hello");
        let before = layout.hit_position_aff(3, Affinity::Before);
        let after = layout.hit_position_aff(3, Affinity::After);
        // On a single non-wrapping line, both affinities should give similar positions.
        assert!(
            (before.point.x - after.point.x).abs() < 2.0,
            "single-line affinities should be close: before={}, after={}",
            before.point.x,
            after.point.x
        );
    }

    // ========================== Hit point ==========================

    #[test]
    fn hit_point_inside() {
        let layout = make("hello world");
        let size = layout.size();
        let hp = layout.hit_point(Point::new(size.width / 2.0, size.height / 2.0));
        assert!(hp.is_inside);
        assert_eq!(hp.line, 0);
    }

    #[test]
    fn hit_point_outside() {
        let layout = make("hello");
        let hp = layout.hit_point(Point::new(9999.0, 9999.0));
        assert!(!hp.is_inside);
    }

    // ========================== Cursor conversion ==========================

    #[test]
    fn cursor_to_byte_index_first_line() {
        let layout = make("hello\nworld");
        let idx = layout.cursor_to_byte_index(&Cursor::new(0, 3));
        assert_eq!(idx, 3);
    }

    #[test]
    fn cursor_to_byte_index_second_line() {
        let layout = make("hello\nworld");
        let idx = layout.cursor_to_byte_index(&Cursor::new(1, 2));
        // "hello\n" = 6 bytes, then index 2 within "world" = 8.
        assert_eq!(idx, 8);
    }

    #[test]
    fn cursor_to_byte_index_out_of_bounds_line() {
        let layout = make("hello");
        let idx = layout.cursor_to_byte_index(&Cursor::new(99, 0));
        assert_eq!(idx, 0, "out-of-bounds line should return 0");
    }

    #[test]
    fn hit_then_cursor_to_byte_roundtrip() {
        let layout = make("hello\nworld");
        let cursor = layout.hit(0.0, 0.0).unwrap();
        let idx = layout.cursor_to_byte_index(&cursor);
        assert_eq!(idx, 0);
    }

    // ========================== Selection geometry ==========================

    #[test]
    fn selection_geometry_single_line() {
        let layout = make("hello world");
        let mut rects = Vec::new();
        layout.selection_geometry_with(2, 8, |x0, y0, x1, y1| {
            rects.push((x0, y0, x1, y1));
        });
        assert!(
            !rects.is_empty(),
            "should produce at least one selection rect"
        );
        let (x0, _y0, x1, _y1) = rects[0];
        assert!(x1 > x0, "selection rect should have positive width");
    }

    #[test]
    fn selection_geometry_multi_line() {
        let layout = make("aaa\nbbb\nccc");
        let mut rects = Vec::new();
        // Select from middle of first line to middle of last.
        layout.selection_geometry_with(1, 10, |x0, y0, x1, y1| {
            rects.push((x0, y0, x1, y1));
        });
        assert!(
            rects.len() >= 2,
            "multi-line selection should span multiple rects, got {}",
            rects.len()
        );
    }

    #[test]
    fn selection_geometry_empty_range() {
        let layout = make("hello");
        let mut count = 0;
        layout.selection_geometry_with(3, 3, |_, _, _, _| count += 1);
        // Empty range may or may not produce a zero-width rect — just ensure no panic.
    }

    #[test]
    fn selection_geometry_full_text() {
        let layout = make("hello");
        let mut rects = Vec::new();
        layout.selection_geometry_with(0, 5, |x0, y0, x1, y1| {
            rects.push((x0, y0, x1, y1));
        });
        assert!(!rects.is_empty());
    }

    // ========================== Wrapping ==========================

    #[test]
    fn set_wrap_none_no_breaking() {
        ensure_font();
        let mut layout = TextLayout::new();
        layout.set_wrap(Wrap::None);
        layout.set_text(
            "a very long line that should not wrap at all even with a narrow width constraint",
            default_attrs(),
            None,
        );
        layout.set_size(50.0, f32::MAX);
        assert_eq!(layout.visual_line_count(), 1);
    }

    #[test]
    fn set_wrap_word_breaks_at_words() {
        let layout = make_wrapped("one two three four five six seven eight", 80.0);
        assert!(layout.visual_line_count() > 1);
    }

    // ========================== set_size reflow ==========================

    #[test]
    fn set_size_reflow_changes_lines() {
        ensure_font();
        let mut layout = TextLayout::new();
        layout.set_text(
            "The quick brown fox jumps over the lazy dog",
            default_attrs(),
            None,
        );
        layout.set_size(1000.0, f32::MAX);
        let wide = layout.visual_line_count();

        layout.set_size(80.0, f32::MAX);
        let narrow = layout.visual_line_count();
        assert!(narrow > wide, "narrower width should produce more lines");
    }

    #[test]
    fn set_size_same_width_is_noop() {
        let mut layout = make_wrapped("hello world", 200.0);
        let count_before = layout.visual_line_count();
        // Calling set_size with the same width should not change anything.
        layout.set_size(200.0, f32::MAX);
        assert_eq!(layout.visual_line_count(), count_before);
    }

    // ========================== Parley access ==========================

    #[test]
    fn parley_layout_accessible() {
        let layout = make("hello");
        let parley = layout.parley_layout();
        assert_eq!(parley.len(), layout.visual_line_count());
    }

    // ========================== Default / Debug ==========================

    #[test]
    fn default_matches_new() {
        let a = TextLayout::default();
        let b = TextLayout::new();
        assert_eq!(a.text(), b.text());
        assert_eq!(a.visual_line_count(), b.visual_line_count());
    }

    #[test]
    fn debug_does_not_panic() {
        let layout = make("hello");
        let _s = format!("{layout:?}");
    }
}
