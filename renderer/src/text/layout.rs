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
                    align.unwrap().into(),
                    AlignmentOptions::default(),
                );
            }
        } else if let Some(align) = align {
            self.layout
                .align(self.width_opt, align.into(), AlignmentOptions::default());
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
                .align(Some(width), align.into(), AlignmentOptions::default());
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
