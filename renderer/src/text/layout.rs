use std::{cell::RefCell, num::NonZeroU8, ops::Range, sync::LazyLock};

use crate::text::{Affinity, Alignment, AttrsList, TextBrush, Wrap};
use parking_lot::Mutex;
use parley::{
    layout::{AlignmentOptions, Layout},
    style::StyleProperty,
    FontContext, LayoutContext,
};
use peniko::kurbo::{Point, Size};

/// Byte-offset mapping between original text (with `\t`) and display text (tabs expanded to spaces).
///
/// Created by [`expand_tabs`] when a [`TextLayout`] has a nonzero tab width and the
/// text contains tab characters. The mapping is used by [`TextLayout`] methods to
/// translate between original byte indices (used by callers) and display byte
/// indices (used by Parley internally).
#[derive(Clone, Debug)]
struct TabInfo {
    /// The text with every `\t` replaced by the appropriate number of spaces.
    display_text: String,
    /// For each tab in order: `(original_byte_position, number_of_spaces)`.
    tabs: Vec<(usize, usize)>,
}

impl TabInfo {
    /// Converts an original byte index to the corresponding display byte index.
    ///
    /// Positions before all tabs return unchanged. Positions after one or more
    /// tabs are shifted forward by the cumulative extra bytes those tabs added.
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

    /// Converts a display byte index back to the corresponding original byte index.
    ///
    /// Display positions that fall inside a tab expansion (i.e. on one of the
    /// inserted spaces) map to the original `\t` byte position.
    fn display_to_orig(&self, pos: usize) -> usize {
        let mut shift = 0usize;
        for &(tab_orig, tab_len) in &self.tabs {
            let tab_display = tab_orig + shift;
            if tab_display >= pos {
                break;
            }
            if pos < tab_display + tab_len {
                // Inside a tab expansion — map to the tab's original position.
                return tab_orig;
            }
            shift += tab_len - 1;
        }
        pos - shift
    }
}

/// Replaces every `\t` in `text` with the right number of spaces to reach the
/// next tab stop, returning the expanded text and a per-tab record.
///
/// Returns `None` when the text contains no tab characters, avoiding an
/// unnecessary allocation. Tab stops are aligned to multiples of `tab_width`
/// characters. The column counter resets at every `\n` or `\r` so that each
/// paragraph line has independent tab-stop positions.
fn expand_tabs(text: &str, tab_width: usize) -> Option<TabInfo> {
    // Fast path: skip allocation when there are no tabs.
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
/// nearest text position, indicating the flat byte offset in the original
/// text and whether the point fell within the layout bounds.
pub struct HitPoint {
    /// Flat byte offset into the original text (insert position).
    pub index: usize,
    /// Whether the queried point was inside the layout bounds.
    ///
    /// A click outside the layout still resolves to a text position:
    /// - click to the right of a line resolves to the end of that line.
    /// - click below the last line resolves to a position in that line.
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
    /// Tab width in spaces, or `None` to leave `\t` as-is (Parley default: near-zero width).
    tab_width: Option<NonZeroU8>,
    /// Tab expansion mapping, present when `tab_width` is set and text contains `\t`.
    tab_info: Option<TabInfo>,
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
            tab_width: None,
            tab_info: None,
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

        // Expand tabs to spaces if configured
        self.tab_info = self
            .tab_width
            .and_then(|w| expand_tabs(text, w.get() as usize));

        // Text that Parley will shape: expanded if tabs were substituted, original otherwise
        let layout_text = self
            .tab_info
            .as_ref()
            .map_or(text, |ti| ti.display_text.as_str());

        // Build Parley layout (font context only needed during shaping)
        {
            let mut font_cx = FONT_CONTEXT.lock();
            LAYOUT_CONTEXT.with(|lc| {
                let mut layout_cx = lc.borrow_mut();
                let mut builder = layout_cx.ranged_builder(&mut font_cx, layout_text, 1.0, true);

                // Apply attributes — remap span ranges when tabs are expanded
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

                builder.build_into(&mut self.layout, layout_text);
            });
        }

        // Line breaking (no font context needed)
        let max_advance = if self.wrap == Wrap::None {
            None
        } else {
            self.width_opt
        };
        self.layout.break_all_lines(max_advance);

        if let Some(align) = self.alignment {
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

    /// Sets the tab width in number of spaces.
    ///
    /// When nonzero, every `\t` in the text passed to [`set_text`](Self::set_text)
    /// is expanded to spaces aligned to the next tab stop (a multiple of
    /// `tab_width` columns). A value of `0` disables expansion.
    ///
    /// Must be called **before** [`set_text`](Self::set_text).
    pub fn set_tab_width(&mut self, tab_width: usize) {
        self.tab_width = NonZeroU8::new(tab_width as u8);
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

    /// Sets text alignment without changing text content or attributes.
    ///
    /// This updates the stored alignment and recomputes line layout so the new
    /// alignment takes effect for the current width/wrap constraints.
    pub fn set_align(&mut self, align: Option<Alignment>) {
        if self.alignment == align {
            return;
        }

        self.alignment = align;

        let max_advance = if self.wrap == Wrap::None {
            None
        } else {
            self.width_opt
        };
        self.layout.break_all_lines(max_advance);

        if let Some(align) = self.alignment {
            self.layout
                .align(self.width_opt, align, AlignmentOptions::default());
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

    /// Converts pixel coordinates to a [`Cursor`] (hit detection).
    ///
    /// Returns `Some(Cursor)` with the display-space byte index nearest to
    /// `(x, y)`. The returned cursor can be passed directly to
    /// [`cursor_to_byte_index`](Self::cursor_to_byte_index) to obtain the
    /// original byte offset, or to [`selection_for_cursors`](Self::selection_for_cursors)
    /// for selection painting.
    pub fn hit(&self, x: f32, y: f32) -> Option<parley::Cursor> {
        if self.text.is_empty() {
            return Some(parley::Cursor::from_byte_index(
                &self.layout,
                0,
                Affinity::default(),
            ));
        }
        Some(parley::Cursor::from_point(&self.layout, x, y))
    }

    /// Returns the geometric position of the cursor at the given flat byte index.
    ///
    /// Uses [`Affinity::Before`] by default. See [`hit_position_aff`](Self::hit_position_aff)
    /// for explicit affinity control.
    pub fn hit_position(&self, idx: usize) -> HitPosition {
        self.hit_position_aff(idx, Affinity::Upstream)
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

        // Map original byte index to display space for Parley.
        let display_idx = match self.tab_info {
            Some(ref ti) => ti.orig_to_display(idx),
            None => idx,
        };
        let pcursor = parley::editing::Cursor::from_byte_index(&self.layout, display_idx, affinity);
        let bbox = pcursor.geometry(&self.layout, 0.0);

        // Find which visual line this cursor is on using binary search on
        // line y-coordinates. This correctly accounts for affinity at wrapped-line
        // boundaries (where the byte index belongs to both lines).
        let cursor_y = bbox.y0 as f32;
        let line_count = self.layout.len();
        let visual_line = if line_count <= 1 {
            0
        } else {
            // Binary search: find the first line whose max_coord > cursor_y.
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

    /// Hit-tests a point and returns a [`HitPoint`] with the nearest text position.
    ///
    /// Unlike [`hit`](Self::hit), this returns a flat original byte index and
    /// reports whether the point was inside the layout bounds via
    /// [`HitPoint::is_inside`].
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

    /// Converts a [`Cursor`] (display-space) to a flat byte index into the
    /// original text, accounting for tab expansion when present.
    pub fn cursor_to_byte_index(&self, cursor: &parley::Cursor) -> usize {
        match self.tab_info {
            Some(ref ti) => ti.display_to_orig(cursor.index()),
            None => cursor.index(),
        }
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
        // Map original byte indices to display space for Parley.
        let (display_start, display_end) = match self.tab_info {
            Some(ref ti) => (ti.orig_to_display(start_byte), ti.orig_to_display(end_byte)),
            None => (start_byte, end_byte),
        };
        let anchor = parley::editing::Cursor::from_byte_index(
            &self.layout,
            display_start,
            parley::layout::Affinity::Downstream,
        );
        let focus = parley::editing::Cursor::from_byte_index(
            &self.layout,
            display_end,
            parley::layout::Affinity::Upstream,
        );
        let selection = parley::editing::Selection::new(anchor, focus);
        selection.geometry_with(&self.layout, |bbox, _line_idx| {
            f(bbox.x0, bbox.y0, bbox.x1, bbox.y1);
        });
    }

    /// Computes selection highlight rectangles between two flat byte indices,
    /// using per-line fragment metrics for vertical bounds.
    pub fn selection_geometry_with_line_metrics(
        &self,
        start_byte: usize,
        end_byte: usize,
        mut f: impl FnMut(f64, f64, f64, f64),
    ) {
        // Map original byte indices to display space for Parley.
        let (display_start, display_end) = match self.tab_info {
            Some(ref ti) => (ti.orig_to_display(start_byte), ti.orig_to_display(end_byte)),
            None => (start_byte, end_byte),
        };
        let anchor = parley::editing::Cursor::from_byte_index(
            &self.layout,
            display_start,
            parley::layout::Affinity::Downstream,
        );
        let focus = parley::editing::Cursor::from_byte_index(
            &self.layout,
            display_end,
            parley::layout::Affinity::Upstream,
        );
        let selection = parley::editing::Selection::new(anchor, focus);
        selection.geometry_with(&self.layout, |bbox, line_idx| {
            if let Some(line) = self.layout.get(line_idx) {
                let m = line.metrics();
                f(bbox.x0, m.min_coord as f64, bbox.x1, m.max_coord as f64);
            } else {
                f(bbox.x0, bbox.y0, bbox.x1, bbox.y1);
            }
        });
    }

    /// Computes selection highlight rectangles between two display-space
    /// [`Cursor`] values, avoiding the byte-index round-trip.
    ///
    /// Calls `f(x0, y0, x1, y1)` once for each visual line that the selection
    /// spans. Prefer this over [`selection_geometry_with`](Self::selection_geometry_with)
    /// when you already hold cursors from [`hit`](Self::hit).
    pub fn selection_for_cursors(
        &self,
        start: &parley::Cursor,
        end: &parley::Cursor,
        mut f: impl FnMut(f64, f64, f64, f64),
    ) {
        let selection = parley::editing::Selection::new(*start, *end);
        selection.geometry_with(&self.layout, |bbox, _| {
            f(bbox.x0, bbox.y0, bbox.x1, bbox.y1);
        });
    }

    /// Computes selection highlight rectangles using per-line fragment metrics
    /// for vertical bounds, which generally looks closer to native selection.
    ///
    /// Calls `f(x0, y0, x1, y1)` once for each visual line the selection spans.
    pub fn selection_for_cursors_with_line_metrics(
        &self,
        start: &parley::Cursor,
        end: &parley::Cursor,
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

    /// Returns the baseline y-position (in pixels) of the `nth` visual line,
    /// or `None` if `nth` is out of bounds.
    pub fn visual_line_y(&self, nth: usize) -> Option<f32> {
        self.layout.get(nth).map(|l| l.metrics().baseline)
    }

    /// Returns the text byte range covered by the `nth` visual line,
    /// or `None` if `nth` is out of bounds.
    ///
    /// When tab expansion is active the returned range is in **original**
    /// byte space (with `\t` characters), not the display-expanded space.
    pub fn visual_line_text_range(&self, nth: usize) -> Option<Range<usize>> {
        self.layout.get(nth).map(|l| {
            let r = l.text_range();
            match self.tab_info {
                Some(ref ti) => ti.display_to_orig(r.start)..ti.display_to_orig(r.end),
                None => r,
            }
        })
    }

    /// Returns `true` if the `nth` visual line has no glyph items (or does not exist).
    pub fn visual_line_is_empty(&self, nth: usize) -> bool {
        self.layout.get(nth).is_none_or(|l| l.is_empty())
    }

    /// Returns the vertical bounds of all visual lines as `(min_y, max_y)`.
    ///
    /// Values are in layout-local coordinates and are derived from per-line
    /// metrics (`min_coord` / `max_coord`).
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

        if min_y.is_finite() && max_y.is_finite() {
            Some((min_y, max_y))
        } else {
            None
        }
    }

    /// Returns vertical bounds suitable for optical centering that exclude descenders.
    ///
    /// Values are in layout-local coordinates. The top bound is derived from
    /// each line's ascent, while the bottom bound is clamped to the baseline
    /// instead of the full line box so descenders do not affect centering.
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

        if min_y.is_finite() && max_y.is_finite() {
            Some((min_y, max_y))
        } else {
            None
        }
    }

    pub fn clear_size(&mut self) {
        self.width_opt = None;
        self.height_opt = None;
        self.layout.break_all_lines(None);
    }
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;
    use crate::text::{Attrs, AttrsList, FamilyOwned};
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

    #[test]
    fn centering_bounds_exclude_descenders() {
        let layout = make("gy");
        let (visual_min, visual_max) = layout.visual_bounds_y().unwrap();
        let (center_min, center_max) = layout.centering_bounds_y().unwrap();

        assert!(center_min >= visual_min);
        assert!(center_min < center_max);
        assert!(center_max < visual_max);
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
        assert_eq!(layout.cursor_to_byte_index(&cursor), 0);
    }

    #[test]
    fn hit_origin_returns_start() {
        let layout = make("hello");
        let cursor = layout.hit(0.0, 0.0).unwrap();
        assert_eq!(layout.cursor_to_byte_index(&cursor), 0);
    }

    #[test]
    fn hit_far_right_returns_line_end() {
        let layout = make("hello");
        let cursor = layout.hit(9999.0, 0.0).unwrap();
        let idx = layout.cursor_to_byte_index(&cursor);
        assert!(idx >= 4, "expected near end, got {idx}");
    }

    #[test]
    fn hit_second_paragraph() {
        let layout = make("aaa\nbbb");
        let y = layout.visual_line_y(1).unwrap_or(20.0);
        let cursor = layout.hit(0.0, y).unwrap();
        let idx = layout.cursor_to_byte_index(&cursor);
        // "aaa\n" = 4 bytes, so the second paragraph starts at byte 4.
        assert!(
            idx >= 4,
            "should resolve to second paragraph, got byte {idx}"
        );
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
        let before = layout.hit_position_aff(3, Affinity::Upstream);
        let after = layout.hit_position_aff(3, Affinity::Downstream);
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
    }

    #[test]
    fn hit_point_outside() {
        let layout = make("hello");
        let hp = layout.hit_point(Point::new(9999.0, 9999.0));
        assert!(!hp.is_inside);
    }

    // ========================== Cursor conversion ==========================

    #[test]
    fn cursor_to_byte_index_start() {
        let layout = make("hello\nworld");
        let cursor =
            parley::Cursor::from_byte_index(layout.parley_layout(), 0, Affinity::Downstream);
        assert_eq!(layout.cursor_to_byte_index(&cursor), 0);
    }

    #[test]
    fn cursor_to_byte_index_mid_text() {
        let layout = make("hello\nworld");
        // Byte 8 = 'r' in "world" (6 + 2).
        let cursor =
            parley::Cursor::from_byte_index(layout.parley_layout(), 8, Affinity::Downstream);
        assert_eq!(layout.cursor_to_byte_index(&cursor), 8);
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

    // ========================== Tab helpers ==========================

    fn make_tab(text: &str, tab_width: usize) -> TextLayout {
        ensure_font();
        let mut layout = TextLayout::new();
        layout.set_tab_width(tab_width);
        layout.set_text(text, default_attrs(), None);
        layout
    }

    fn make_tab_wrapped(text: &str, tab_width: usize, width: f32) -> TextLayout {
        ensure_font();
        let mut layout = TextLayout::new();
        layout.set_tab_width(tab_width);
        layout.set_text(text, default_attrs(), None);
        layout.set_size(width, f32::MAX);
        layout
    }

    // ========================== expand_tabs unit tests ==========================

    #[test]
    fn expand_tabs_no_tabs() {
        assert!(expand_tabs("hello", 4).is_none(), "no tabs → None");
    }

    #[test]
    fn expand_tabs_single_tab() {
        // "a\tb" with tab_width=4: 'a' at col 0, tab at col 1 → 3 spaces → "a   b".
        let info = expand_tabs("a\tb", 4).unwrap();
        assert_eq!(info.display_text, "a   b");
        assert_eq!(info.tabs, vec![(1, 3)]);
    }

    #[test]
    fn expand_tabs_multiple_tabs() {
        // "\t\t" with tab_width=4: first tab at col 0 → 4 spaces, second tab at col 4 → 4 spaces.
        let info = expand_tabs("\t\t", 4).unwrap();
        assert_eq!(info.display_text, "        ");
        assert_eq!(info.tabs, vec![(0, 4), (1, 4)]);
    }

    #[test]
    fn expand_tabs_tab_at_boundary() {
        // "abcd\te" with tab_width=4: tab at col 4 → 4 spaces (next stop at 8).
        let info = expand_tabs("abcd\te", 4).unwrap();
        assert_eq!(info.display_text, "abcd    e");
        assert_eq!(info.tabs, vec![(4, 4)]);
    }

    #[test]
    fn expand_tabs_after_newline() {
        // "ab\t\ncd\te": column resets after \n.
        // Line 1: 'a'=0, 'b'=1, tab at col 2 → 2 spaces, '\n' resets.
        // Line 2: 'c'=0, 'd'=1, tab at col 2 → 2 spaces.
        let info = expand_tabs("ab\t\ncd\te", 4).unwrap();
        assert_eq!(info.display_text, "ab  \ncd  e");
        assert_eq!(info.tabs, vec![(2, 2), (6, 2)]);
    }

    #[test]
    fn expand_tabs_width_1() {
        // tab_width=1: every tab becomes exactly 1 space (always at a stop).
        let info = expand_tabs("a\tb\tc", 1).unwrap();
        assert_eq!(info.display_text, "a b c");
        assert_eq!(info.tabs, vec![(1, 1), (3, 1)]);
    }

    #[test]
    fn expand_tabs_width_2() {
        // "a\tb" with tab_width=2: 'a' at col 0, tab at col 1 → 1 space.
        let info = expand_tabs("a\tb", 2).unwrap();
        assert_eq!(info.display_text, "a b");
        assert_eq!(info.tabs, vec![(1, 1)]);
    }

    // ========================== Offset mapping unit tests ==========================

    #[test]
    fn orig_to_display_with_tabs() {
        // "a\tb\tc" tab_width=4 → "a   b   c"
        // tabs: [(1, 3), (3, 3)]
        let info = expand_tabs("a\tb\tc", 4).unwrap();
        assert_eq!(info.orig_to_display(0), 0); // 'a'
        assert_eq!(info.orig_to_display(1), 1); // '\t' → start of expansion
        assert_eq!(info.orig_to_display(2), 4); // 'b' (shift = 3-1 = 2)
        assert_eq!(info.orig_to_display(3), 5); // second '\t'
        assert_eq!(info.orig_to_display(4), 8); // 'c' (shift = 2+2 = 4)
    }

    #[test]
    fn display_to_orig_with_tabs() {
        // "a\tb\tc" tab_width=4 → "a   b   c"
        let info = expand_tabs("a\tb\tc", 4).unwrap();
        assert_eq!(info.display_to_orig(0), 0); // 'a'
        assert_eq!(info.display_to_orig(1), 1); // first space of tab 1
        assert_eq!(info.display_to_orig(2), 1); // inside tab expansion
        assert_eq!(info.display_to_orig(3), 1); // inside tab expansion
        assert_eq!(info.display_to_orig(4), 2); // 'b'
        assert_eq!(info.display_to_orig(5), 3); // first space of tab 2
        assert_eq!(info.display_to_orig(6), 3); // inside tab expansion
        assert_eq!(info.display_to_orig(7), 3); // inside tab expansion
        assert_eq!(info.display_to_orig(8), 4); // 'c'
    }

    #[test]
    fn display_to_orig_inside_expansion() {
        // Positions inside a tab expansion map to the tab's original byte.
        let info = expand_tabs("\thello", 4).unwrap();
        // Tab at col 0 → 4 spaces. "    hello"
        assert_eq!(info.display_to_orig(0), 0); // first space → tab at byte 0
        assert_eq!(info.display_to_orig(1), 0);
        assert_eq!(info.display_to_orig(2), 0);
        assert_eq!(info.display_to_orig(3), 0);
        assert_eq!(info.display_to_orig(4), 1); // 'h'
    }

    #[test]
    fn roundtrip_orig_display_orig() {
        let info = expand_tabs("a\tb\tc", 4).unwrap();
        // For non-tab original positions, roundtrip should be identity.
        for orig_pos in [0, 2, 4] {
            let display_pos = info.orig_to_display(orig_pos);
            let back = info.display_to_orig(display_pos);
            assert_eq!(back, orig_pos, "roundtrip failed for orig_pos={orig_pos}");
        }
    }

    // ========================== TextLayout tab integration tests ==========================

    #[test]
    fn tab_text_has_positive_width() {
        let with_tab = make_tab("a\tb", 4);
        let without = make("ab");
        assert!(
            with_tab.size().width > without.size().width,
            "tab layout ({}) should be wider than plain ({})",
            with_tab.size().width,
            without.size().width
        );
    }

    #[test]
    fn tab_text_returns_original() {
        let layout = make_tab("a\tb", 4);
        assert_eq!(
            layout.text(),
            "a\tb",
            "text() should return original with \\t"
        );
    }

    #[test]
    fn hit_position_after_tab() {
        let layout = make_tab("a\tb", 4);
        // Byte 2 in original = 'b'. It should be well to the right.
        let pos = layout.hit_position(2);
        let pos_a = layout.hit_position(0);
        assert!(
            pos.point.x > pos_a.point.x + 10.0,
            "b should be far right of a: b.x={}, a.x={}",
            pos.point.x,
            pos_a.point.x
        );
    }

    #[test]
    fn hit_position_at_tab() {
        let layout = make_tab("a\tb", 4);
        // Byte 1 = the tab character. Position should be right after 'a'.
        let pos_tab = layout.hit_position(1);
        let pos_a = layout.hit_position(0);
        assert!(
            pos_tab.point.x > pos_a.point.x,
            "tab position should be right of a"
        );
    }

    #[test]
    fn hit_roundtrip_with_tabs() {
        let layout = make_tab("a\tb", 4);
        // Hit at the position of 'b' (byte 2 in original) and check roundtrip.
        let b_pos = layout.hit_position(2);
        let cursor = layout
            .hit(b_pos.point.x as f32 + 1.0, b_pos.point.y as f32)
            .unwrap();
        let byte_idx = layout.cursor_to_byte_index(&cursor);
        assert_eq!(byte_idx, 2, "roundtrip should land on 'b' at byte 2");
    }

    #[test]
    fn hit_far_right_tabs() {
        let layout = make_tab("a\tb", 4);
        let cursor = layout.hit(9999.0, 0.0).unwrap();
        let idx = layout.cursor_to_byte_index(&cursor);
        // Should be at or near the end (byte 3 in original "a\tb").
        assert!(idx >= 2, "far right should be near end, got {idx}");
    }

    #[test]
    fn selection_geometry_spanning_tab() {
        let with_tab = make_tab("a\tb", 4);
        let without = make("ab");

        let mut tab_rects = Vec::new();
        with_tab.selection_geometry_with(0, 3, |x0, y0, x1, y1| {
            tab_rects.push((x0, y0, x1, y1));
        });

        let mut plain_rects = Vec::new();
        without.selection_geometry_with(0, 2, |x0, y0, x1, y1| {
            plain_rects.push((x0, y0, x1, y1));
        });

        assert!(!tab_rects.is_empty());
        assert!(!plain_rects.is_empty());
        let tab_width = tab_rects[0].2 - tab_rects[0].0;
        let plain_width = plain_rects[0].2 - plain_rects[0].0;
        assert!(
            tab_width > plain_width,
            "tab selection ({tab_width}) should be wider than plain ({plain_width})"
        );
    }

    #[test]
    fn visual_line_text_range_with_tabs() {
        let layout = make_tab("\ta", 4);
        let range = layout.visual_line_text_range(0).unwrap();
        // Should be in original space: 0..2 (tab + 'a'), not 0..5 (spaces + 'a').
        assert_eq!(range.start, 0);
        assert!(
            range.end <= 2,
            "range should be in original space, got {range:?}"
        );
    }

    #[test]
    fn lines_range_unaffected_by_tabs() {
        let layout = make_tab("a\tb\nc\td", 4);
        assert_eq!(layout.lines_range(), &[0..3, 4..7]);
    }

    #[test]
    fn attrs_span_remapped_across_tab() {
        ensure_font();
        let family = vec![FamilyOwned::Name("DejaVu Serif".into())];
        let mut attrs_list = AttrsList::new(Attrs::new().font_size(16.0).family(&family));
        // Span covering bytes 2..4 in "a\tbcd" = "bc".
        attrs_list.add_span(2..4, Attrs::new().font_size(32.0).family(&family));

        let mut layout = TextLayout::new();
        layout.set_tab_width(4);
        layout.set_text("a\tbcd", attrs_list, None);

        // If attrs were not remapped, the span would apply to the wrong bytes
        // in display space. The layout should not panic and should have positive size.
        assert!(layout.size().width > 0.0);
        assert!(layout.size().height > 0.0);
    }

    #[test]
    fn no_tab_expansion_when_width_zero() {
        let layout = make_tab("a\tb", 0);
        // tab_width=0 → no expansion. text() still original.
        assert_eq!(layout.text(), "a\tb");
        // Layout should still work (Parley gives tabs near-zero width).
        assert!(layout.size().width >= 0.0);
    }

    #[test]
    fn wrapped_tabs() {
        // Wide tab content should wrap when constrained.
        let layout = make_tab_wrapped("\t\t\t\thello world", 4, 100.0);
        assert!(
            layout.visual_line_count() >= 1,
            "wrapped tab content should produce visual lines"
        );
    }

    // ========================== Tab edge cases ==========================

    #[test]
    fn empty_text_with_tab_width() {
        let layout = make_tab("", 4);
        assert_eq!(layout.text(), "");
        assert!(layout.size().width >= 0.0);
    }

    #[test]
    fn only_tabs() {
        let layout = make_tab("\t\t\t", 4);
        assert_eq!(layout.text(), "\t\t\t");
        assert!(layout.size().width > 0.0, "three tabs should have width");
        // hit_position at each original byte should not panic.
        for i in 0..=3 {
            let _ = layout.hit_position(i);
        }
    }

    #[test]
    fn tab_at_end() {
        let layout = make_tab("abc\t", 4);
        assert_eq!(layout.text(), "abc\t");
        // Cursor at end of text (byte 4) should work.
        let pos = layout.hit_position(4);
        assert!(pos.point.x > 0.0);
    }

    #[test]
    fn tab_with_multibyte_chars() {
        // UTF-8 multi-byte chars: 'é' = 2 bytes, 'à' = 2 bytes.
        let layout = make_tab("é\tà", 4);
        assert_eq!(layout.text(), "é\tà");
        // 'é' takes 1 column, tab at col 1 → 3 spaces, 'à' at col 4.
        assert!(layout.size().width > 0.0);
        // 'à' starts at original byte 4 (2 bytes for é + 1 for \t + 0 offset = 3, wait...)
        // Actually: 'é' = bytes 0..2, '\t' = byte 2, 'à' = bytes 3..5.
        let pos_a_grave = layout.hit_position(3);
        let pos_e_acute = layout.hit_position(0);
        assert!(
            pos_a_grave.point.x > pos_e_acute.point.x,
            "à should be right of é"
        );
    }

    #[test]
    fn set_size_reflow_with_tabs() {
        ensure_font();
        let mut layout = TextLayout::new();
        layout.set_tab_width(4);
        layout.set_text(
            "\t\thello world this is a long line with tabs",
            default_attrs(),
            None,
        );
        layout.set_size(1000.0, f32::MAX);
        let wide = layout.visual_line_count();

        layout.set_size(100.0, f32::MAX);
        let narrow = layout.visual_line_count();
        assert!(
            narrow >= wide,
            "narrower width should produce >= lines: wide={wide}, narrow={narrow}"
        );
    }
}
