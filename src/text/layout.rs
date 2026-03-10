use std::{
    cell::RefCell,
    collections::HashMap,
    num::NonZeroU8,
    ops::{Deref, DerefMut, Range},
    sync::LazyLock,
};

use floem_renderer::Renderer as _;
use floem_renderer::text::{AttrsList, GlyphRunProps, TextBrush};
use parking_lot::Mutex;
use parley::swash::{FontRef, scale::ScaleContext, zeno};
use parley::{
    Affinity, Alignment, Cursor, FontContext, LayoutContext, Selection,
    layout::{AlignmentOptions, Layout},
    style::{OverflowWrap, StyleProperty, TextWrapMode},
};
use peniko::{
    Fill, FontData,
    kurbo::{Affine, Point, Size},
};

use crate::paint::Renderer;

/// Shared Parley font context used by Floem text layout construction.
///
/// This is exposed so callers that need to register or inspect fonts can work
/// with the same font database used by [`TextLayout`].
pub static FONT_CONTEXT: LazyLock<Mutex<FontContext>> =
    LazyLock::new(|| Mutex::new(FontContext::new()));

thread_local! {
    static LAYOUT_CONTEXT: RefCell<LayoutContext<TextBrush>> =
        RefCell::new(LayoutContext::new());
    static OUTLINE_SCALE_CONTEXT: RefCell<ScaleContext> =
        RefCell::new(ScaleContext::new());
    static GLYPH_OUTLINE_BOUNDS_CACHE: RefCell<HashMap<GlyphOutlineBoundsKey, Option<zeno::Bounds>>> =
        RefCell::new(HashMap::new());
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct GlyphOutlineBoundsKey {
    font_blob_id: u64,
    font_index: u32,
    glyph_id: u16,
    font_size_bits: u32,
    normalized_coords: Vec<i16>,
    skew_bits: u32,
}

/// A `TextLayout`-owned selection value.
///
/// This wraps Parley's selection type for APIs that depend on `TextLayout`'s
/// selection-geometry invariants and cache lifecycle.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TextSelection {
    selection: Selection,
}

impl TextSelection {
    fn new(selection: Selection) -> Self {
        Self { selection }
    }
}

impl Deref for TextSelection {
    type Target = Selection;

    fn deref(&self) -> &Self::Target {
        &self.selection
    }
}

impl DerefMut for TextSelection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.selection
    }
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

#[derive(Clone)]
/// A Floem wrapper around a Parley text layout.
///
/// This type owns the source text, shaping result, wrapping configuration, and
/// the tab-expansion mapping Floem uses for editor-style tab handling.
///
/// Use this when you need:
/// - shaping and wrapping text for painting
/// - hit testing and cursor geometry
/// - visual-line text ranges and metrics
///
/// This is a higher-level Floem wrapper, not a direct re-export of Parley's
/// layout type.
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

impl TextLayout {
    /// Creates a selection without clearing the cached selection outline
    /// bounds.
    ///
    /// Use this when updating an existing selection rather than beginning a
    /// new one.
    pub fn selection(&self, anchor: Cursor, focus: Cursor) -> TextSelection {
        TextSelection::new(Selection::new(anchor, focus))
    }

    /// Clears the cached glyph outline bounds used for selection geometry.
    ///
    /// Floem uses this cache to avoid repeatedly scaling glyph outlines when
    /// selection rectangles need ink-tight horizontal bounds. Callers that
    /// treat selection as a short-lived interaction can clear it when starting
    /// a new selection to keep the cache from growing across unrelated
    /// gestures.
    pub fn clear_selection_geometry_cache() {
        GLYPH_OUTLINE_BOUNDS_CACHE.with_borrow_mut(|cache| cache.clear());
    }

    /// Creates a new selection and clears the cached selection outline bounds.
    ///
    /// This is the preferred entry point when beginning a new user selection.
    /// It makes the outline-bounds cache invalidation explicit and local to the
    /// selection-construction API instead of requiring callers to remember a
    /// separate cache-management step.
    pub fn begin_selection(&self, anchor: Cursor, focus: Cursor) -> TextSelection {
        Self::clear_selection_geometry_cache();
        TextSelection::new(Selection::new(anchor, focus))
    }

    /// Creates a new selection from a source-text byte range and clears the
    /// cached selection outline bounds.
    ///
    /// Use this when a new selection starts from byte offsets rather than from
    /// existing Parley cursors.
    pub fn begin_selection_from_byte_range(
        &self,
        start_byte: usize,
        end_byte: usize,
    ) -> TextSelection {
        Self::clear_selection_geometry_cache();
        self.selection_from_byte_range(start_byte, end_byte)
    }

    /// Creates a new collapsed selection at the given cursor.
    pub fn collapsed_selection(&self, cursor: Cursor) -> TextSelection {
        self.selection(cursor, cursor)
    }

    fn glyph_outline_bounds(
        font: &FontData,
        font_size: f32,
        normalized_coords: &[i16],
        skew: Option<f32>,
        glyph_id: u16,
    ) -> Option<zeno::Bounds> {
        let key = GlyphOutlineBoundsKey {
            font_blob_id: font.data.id(),
            font_index: font.index,
            glyph_id,
            font_size_bits: font_size.to_bits(),
            normalized_coords: normalized_coords.to_vec(),
            skew_bits: skew.unwrap_or(0.0).to_bits(),
        };

        if let Some(bounds) =
            GLYPH_OUTLINE_BOUNDS_CACHE.with_borrow(|cache| cache.get(&key).cloned())
        {
            return bounds;
        }

        let font_ref = FontRef::from_index(font.data.data(), font.index as usize)?;
        let bounds = OUTLINE_SCALE_CONTEXT.with_borrow_mut(|context| {
            let mut scaler = context
                .builder(font_ref)
                .size(font_size)
                .hint(true)
                .normalized_coords(normalized_coords)
                .build();
            let mut outline = scaler.scale_outline(glyph_id)?;
            if let Some(angle) = skew {
                outline.transform(&zeno::Transform::skew(
                    zeno::Angle::from_degrees(angle),
                    zeno::Angle::ZERO,
                ));
            }
            let bounds = outline.bounds();
            (!bounds.is_empty()).then_some(bounds)
        });

        GLYPH_OUTLINE_BOUNDS_CACHE.with_borrow_mut(|cache| {
            cache.insert(key, bounds);
        });
        bounds
    }

    #[inline]
    fn display_byte_index(&self, idx: usize) -> usize {
        if let Some(tab_info) = self.tab_info.as_ref() {
            tab_info.orig_to_display(idx)
        } else {
            idx
        }
    }

    /// Creates a Parley selection from a source-text byte range.
    pub fn selection_from_byte_range(&self, start_byte: usize, end_byte: usize) -> TextSelection {
        let anchor = Cursor::from_byte_index(
            &self.layout,
            self.display_byte_index(start_byte),
            Affinity::Downstream,
        );
        let focus = Cursor::from_byte_index(
            &self.layout,
            self.display_byte_index(end_byte),
            Affinity::Upstream,
        );
        TextSelection::new(Selection::new(anchor, focus))
    }

    /// Creates a selection from source-text byte positions while preserving
    /// anchor/focus direction.
    pub fn selection_from_byte_positions(
        &self,
        anchor_byte: usize,
        focus_byte: usize,
    ) -> Option<TextSelection> {
        if anchor_byte == focus_byte {
            return None;
        }
        let start = anchor_byte.min(focus_byte);
        let end = anchor_byte.max(focus_byte);
        let selection = self.selection_from_byte_range(start, end);
        Some(if anchor_byte <= focus_byte {
            selection
        } else {
            TextSelection::new(Selection::new(selection.focus(), selection.anchor()))
        })
    }

    /// Creates a new selection from source-text byte positions while
    /// preserving anchor/focus direction and clearing the cached selection
    /// outline bounds.
    pub fn begin_selection_from_byte_positions(
        &self,
        anchor_byte: usize,
        focus_byte: usize,
    ) -> Option<TextSelection> {
        if anchor_byte == focus_byte {
            return None;
        }
        Self::clear_selection_geometry_cache();
        self.selection_from_byte_positions(anchor_byte, focus_byte)
    }

    /// Converts a Parley selection into a byte range in the original source text.
    pub fn selection_text_range(&self, selection: &TextSelection) -> Range<usize> {
        let range = selection.selection.text_range();
        match self.tab_info {
            Some(ref ti) => ti.display_to_orig(range.start)..ti.display_to_orig(range.end),
            None => range,
        }
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

    /// Creates an empty text layout with Floem's default wrapping settings.
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

    /// Creates a new layout and immediately sets its text and attributes.
    pub fn new_with_text(text: &str, attrs_list: AttrsList, align: Option<Alignment>) -> Self {
        let mut layout = Self::new();
        layout.set_text(text, attrs_list, align);
        layout
    }

    /// Replaces the text content and style spans for this layout.
    ///
    /// This rebuilds the underlying Parley layout and reapplies the current
    /// wrapping configuration.
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

    /// Sets the primary wrap mode used when reflowing the layout.
    pub fn set_text_wrap_mode(&mut self, text_wrap_mode: TextWrapMode) {
        self.text_wrap_mode = text_wrap_mode;
    }

    /// Sets the emergency wrap behavior used when wrapping is enabled.
    pub fn set_overflow_wrap(&mut self, overflow_wrap: OverflowWrap) {
        self.overflow_wrap = overflow_wrap;
    }

    /// Sets the visual width of tab characters in spaces.
    ///
    /// When set, Floem expands tabs before shaping and keeps a mapping between
    /// original and display byte indices so hit-testing still refers back to
    /// the source text.
    pub fn set_tab_width(&mut self, tab_width: usize) {
        self.tab_width = NonZeroU8::new(tab_width as u8);
    }

    /// Sets the layout bounds used for reflow.
    ///
    /// Width changes trigger line breaking and optional alignment.
    pub fn set_size(&mut self, width: f32, height: f32) {
        let old_width = self.width_opt;
        self.width_opt = Some(width);
        self.height_opt = Some(height);
        if old_width != Some(width) {
            self.reflow(Some(width));
        }
    }

    /// Removes any explicit layout size and reflows without a width constraint.
    pub fn clear_size(&mut self) {
        self.width_opt = None;
        self.height_opt = None;
        self.reflow(None);
    }

    /// Sets horizontal alignment for wrapped text.
    pub fn set_align(&mut self, align: Option<Alignment>) {
        if self.alignment != align {
            self.alignment = align;
            self.reflow(self.width_opt);
        }
    }

    /// Returns the original source text for this layout.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the underlying Parley layout.
    ///
    /// This is an escape hatch for code that genuinely needs direct Parley
    /// access and should be used sparingly.
    pub fn parley_layout(&self) -> &Layout<TextBrush> {
        &self.layout
    }

    /// Returns the number of visual lines currently in the layout.
    pub fn visual_line_count(&self) -> usize {
        self.layout.len()
    }

    /// Returns the overall layout size in layout coordinates.
    pub fn size(&self) -> Size {
        Size::new(self.layout.full_width() as f64, self.layout.height() as f64)
    }

    /// Performs hit testing and returns the nearest Parley cursor.
    ///
    /// The returned cursor remains in display/layout coordinates. Use
    /// [`cursor_to_byte_index`](Self::cursor_to_byte_index) when you need a byte
    /// index in the original source text.
    pub fn hit_test(&self, point: Point) -> Option<Cursor> {
        if self.text.is_empty() {
            return Some(Cursor::from_byte_index(
                &self.layout,
                0,
                Affinity::default(),
            ));
        }
        Some(Cursor::from_point(
            &self.layout,
            point.x as f32,
            point.y as f32,
        ))
    }

    fn line_index_for_cursor_y(&self, cursor_y: f32) -> usize {
        // Mirrors Parley's internal `Layout::line_for_offset` behavior so our
        // cursor-line lookup stays consistent until Parley exposes that API.
        let line_count = self.layout.len();
        if line_count <= 1 {
            return 0;
        }

        if cursor_y < 0.0 {
            return 0;
        }

        let mut lo = 0usize;
        let mut hi = line_count;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let ordering = self
                .layout
                .get(mid)
                .map_or(std::cmp::Ordering::Greater, |line| {
                    let metrics = line.metrics();
                    if cursor_y < metrics.min_coord {
                        std::cmp::Ordering::Greater
                    } else if cursor_y >= metrics.max_coord {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Equal
                    }
                });

            match ordering {
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Equal => return mid,
            }
        }

        lo.saturating_sub(1).min(line_count - 1)
    }

    /// Returns the cursor's visual point for a source-text byte index.
    ///
    /// The resulting point is in layout coordinates, with `x` at the cursor
    /// position and `y` at the line baseline.
    pub fn cursor_point(&self, idx: usize, affinity: Affinity) -> Point {
        if self.text.is_empty() || self.layout.is_empty() {
            return Point::ZERO;
        }

        let display_idx = if let Some(tab_info) = self.tab_info.as_ref() {
            tab_info.orig_to_display(idx)
        } else {
            idx
        };
        let pcursor = Cursor::from_byte_index(&self.layout, display_idx, affinity);
        let bbox = pcursor.geometry(&self.layout, 0.0);
        let baseline = self
            .line_metrics_at(idx, affinity)
            .map(|metrics| metrics.baseline as f64)
            .unwrap_or(0.0);
        Point::new(bbox.x0, baseline)
    }

    /// Returns the visual line metrics for the line containing the given byte index.
    ///
    /// The returned metrics are copied from Parley because Parley's public line
    /// wrapper does not allow us to return a borrowed `&LineMetrics` directly
    /// from this wrapper type.
    pub fn line_metrics_at(
        &self,
        idx: usize,
        affinity: Affinity,
    ) -> Option<parley::layout::LineMetrics> {
        if self.text.is_empty() || self.layout.is_empty() {
            return None;
        }

        let display_idx = if let Some(tab_info) = self.tab_info.as_ref() {
            tab_info.orig_to_display(idx)
        } else {
            idx
        };
        let pcursor = Cursor::from_byte_index(&self.layout, display_idx, affinity);
        let bbox = pcursor.geometry(&self.layout, 0.0);
        let visual_line = self.line_index_for_cursor_y(bbox.y0 as f32);
        let line = self.layout.get(visual_line)?;
        Some(*line.metrics())
    }

    /// Converts a Parley cursor back into a byte index in the original source text.
    ///
    /// This reverses Floem's internal tab-expansion mapping when tab handling is
    /// enabled.
    pub fn cursor_to_byte_index(&self, cursor: &Cursor) -> usize {
        let idx = cursor.index();
        if let Some(tab_info) = self.tab_info.as_ref() {
            tab_info.display_to_orig(idx)
        } else {
            idx
        }
    }

    /// Iterates selection rectangles for a Parley selection using raw cursor geometry.
    pub fn selection_geometry_with(
        &self,
        selection: &TextSelection,
        mut f: impl FnMut(f64, f64, f64, f64),
    ) {
        selection.selection.geometry_with(&self.layout, |bbox, _| {
            f(bbox.x0, bbox.y0, bbox.x1, bbox.y1);
        });
    }

    /// Iterates selection rectangles for a Parley selection using full visual-line metrics.
    ///
    /// This is the geometry helper to use for painted selection backgrounds.
    ///
    /// Compared with [`selection_geometry_with`](Self::selection_geometry_with), this method:
    /// - expands each rectangle vertically to the containing line's full `min_coord..max_coord`
    /// - expands horizontal bounds to the actual glyph outline bounds of the selected text
    ///
    /// The vertical expansion is important for consistent full-line selection backgrounds.
    /// The horizontal expansion is important because cursor/advance-based selection geometry
    /// is not always wide enough to cover italic or otherwise overhanging glyph outlines.
    ///
    /// The x extents are computed by:
    /// - starting from Parley's selection geometry for the selected line fragment
    /// - scanning the selected glyphs on that line
    /// - unioning in cached Swash outline bounds for those glyphs
    ///
    /// Complexity:
    /// - baseline Parley selection coverage is proportional to the number of selected line fragments
    /// - the additional Floem work here is proportional to the number of selected runs/clusters/glyphs
    ///   on the affected lines
    /// - outline extraction itself is cached by font blob, font index, glyph id, size,
    ///   normalized coords, and skew, so repeated paints normally pay lookup cost rather than
    ///   outline-scaling cost
    ///
    /// In practice, this is more expensive than raw cursor-box geometry but is only intended for
    /// selection painting, where correct ink coverage matters more than the absolute minimum amount
    /// of work.
    pub fn selection_geometry_with_line_metrics(
        &self,
        selection: &TextSelection,
        mut f: impl FnMut(f64, f64, f64, f64),
    ) {
        let selection_range = selection.selection.text_range();
        selection
            .selection
            .geometry_with(&self.layout, |bbox, line_idx| {
                if let Some(line) = self.layout.get(line_idx) {
                    let m = line.metrics();
                    let mut min_x = bbox.x0;
                    let mut max_x = bbox.x1;

                    // Parley's selection geometry is based on cursor/advance coverage, which can be
                    // narrower than the actual ink for italic or otherwise overhanging outlines.
                    // We union in cached outline bounds for the selected glyphs on this line so the
                    // painted selection fully covers the rendered text.
                    let mut run_offset = m.offset as f64;
                    for run in line.runs() {
                        let run_range = run.text_range();
                        if run_range.end <= selection_range.start
                            || run_range.start >= selection_range.end
                        {
                            run_offset += run.advance() as f64;
                            continue;
                        }

                        let mut cluster_offset = run_offset;
                        for cluster in run.visual_clusters() {
                            let cluster_range = cluster.text_range();
                            let cluster_advance = cluster.advance() as f64;
                            if cluster_range.end > selection_range.start
                                && cluster_range.start < selection_range.end
                            {
                                for glyph in cluster.glyphs() {
                                    if let Some(bounds) = Self::glyph_outline_bounds(
                                        run.font(),
                                        run.font_size(),
                                        run.normalized_coords(),
                                        run.synthesis().skew(),
                                        glyph.id as u16,
                                    ) {
                                        min_x = min_x.min(
                                            cluster_offset + glyph.x as f64 + bounds.min.x as f64,
                                        );
                                        max_x = max_x.max(
                                            cluster_offset + glyph.x as f64 + bounds.max.x as f64,
                                        );
                                    }
                                }
                            }
                            cluster_offset += cluster_advance;
                        }

                        run_offset += run.advance() as f64;
                    }

                    f(min_x, m.min_coord as f64, max_x, m.max_coord as f64);
                } else {
                    f(bbox.x0, bbox.y0, bbox.x1, bbox.y1);
                }
            });
    }

    /// Returns the baseline y coordinate for the `nth` visual line.
    pub fn visual_line_y(&self, nth: usize) -> Option<f32> {
        self.layout.get(nth).map(|l| l.metrics().baseline)
    }

    /// Returns the source-text byte range covered by the `nth` visual line.
    pub fn visual_line_text_range(&self, nth: usize) -> Option<Range<usize>> {
        self.layout.get(nth).map(|l| {
            let r = l.text_range();
            match self.tab_info {
                Some(ref ti) => ti.display_to_orig(r.start)..ti.display_to_orig(r.end),
                None => r,
            }
        })
    }

    /// Returns the top and bottom extents of the visual line boxes.
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

    /// Returns the vertical bounds used when visually centering this layout.
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

    /// Draws the layout at the given origin using Floem's renderer wrapper.
    pub fn draw(&self, renderer: &mut Renderer, origin: impl Into<Point>) {
        let origin = origin.into();
        for line in self.layout.lines() {
            for item in line.items() {
                let parley::layout::PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };

                let run = glyph_run.run();
                let synthesis = run.synthesis();
                let glyph_transform = synthesis
                    .skew()
                    .map(|angle| Affine::skew((angle as f64).to_radians().tan(), 0.0));

                let props = GlyphRunProps::new(run.font())
                    .font_size(run.font_size())
                    .hint(false)
                    .normalized_coords(run.normalized_coords())
                    .style(Fill::NonZero)
                    .brush(glyph_run.style().brush.0)
                    .glyph_transform(glyph_transform);

                renderer.draw_glyphs(origin, &props, glyph_run.positioned_glyphs());
            }
        }
    }
}
