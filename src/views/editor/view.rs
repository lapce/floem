use std::{collections::HashMap, ops::RangeInclusive, rc::Rc};

use crate::{
    action::{set_ime_allowed, set_ime_cursor_area},
    context::{LayoutCx, PaintCx, UpdateCx},
    cosmic_text::{Attrs, AttrsList, TextLayout},
    event::{Event, EventListener, EventPropagation},
    id::ViewId,
    keyboard::{Key, Modifiers, NamedKey},
    kurbo::{BezPath, Line, Point, Rect, Size, Vec2},
    peniko::Color,
    reactive::{batch, create_effect, create_memo, create_rw_signal, Memo, RwSignal, Scope},
    style::{CursorStyle, Style},
    style_class,
    taffy::tree::NodeId,
    view::{IntoView, View},
    views::{scroll, stack, Decorators},
    Renderer,
};
use floem_editor_core::{
    cursor::{ColPosition, CursorAffinity, CursorMode},
    mode::{Mode, VisualMode},
};

use crate::views::editor::{
    command::CommandExecuted,
    gutter::editor_gutter_view,
    keypress::{key::KeyInput, press::KeyPress},
    layout::LineExtraStyle,
    visual_line::{RVLine, VLineInfo},
};

use super::{Editor, CHAR_WIDTH};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DiffSectionKind {
    NoCode,
    Added,
    Removed,
}

#[derive(Clone, PartialEq)]
pub struct DiffSection {
    /// The y index that the diff section is at.  
    /// This is multiplied by the line height to get the y position.  
    /// So this can roughly be considered as the `VLine of the start of this diff section, but it
    /// isn't necessarily convertable to a `VLine` due to jumping over empty code sections.
    pub y_idx: usize,
    pub height: usize,
    pub kind: DiffSectionKind,
}

// TODO(minor): We have diff sections in screen lines because Lapce uses them, but
// we don't really have support for diffs in floem-editor! Is there a better design for this?
// Possibly we should just move that out to a separate field on Lapce's editor.
#[derive(Clone, PartialEq)]
pub struct ScreenLines {
    pub lines: Rc<Vec<RVLine>>,
    /// Guaranteed to have an entry for each `VLine` in `lines`  
    /// You should likely use accessor functions rather than this directly.
    pub info: Rc<HashMap<RVLine, LineInfo>>,
    pub diff_sections: Option<Rc<Vec<DiffSection>>>,
    /// The base y position that all the y positions inside `info` are relative to.  
    /// This exists so that if a text layout is created outside of the view, we don't have to
    /// completely recompute the screen lines (or do somewhat intricate things to update them)
    /// we simply have to update the `base_y`.
    pub base: RwSignal<ScreenLinesBase>,
}
impl ScreenLines {
    pub fn new(cx: Scope, viewport: Rect) -> ScreenLines {
        ScreenLines {
            lines: Default::default(),
            info: Default::default(),
            diff_sections: Default::default(),
            base: cx.create_rw_signal(ScreenLinesBase {
                active_viewport: viewport,
            }),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn clear(&mut self, viewport: Rect) {
        self.lines = Default::default();
        self.info = Default::default();
        self.diff_sections = Default::default();
        self.base.set(ScreenLinesBase {
            active_viewport: viewport,
        });
    }

    /// Get the line info for the given rvline.  
    pub fn info(&self, rvline: RVLine) -> Option<LineInfo> {
        let info = self.info.get(&rvline)?;
        let base = self.base.get();

        Some(info.clone().with_base(base))
    }

    pub fn vline_info(&self, rvline: RVLine) -> Option<VLineInfo<()>> {
        self.info.get(&rvline).map(|info| info.vline_info)
    }

    pub fn rvline_range(&self) -> Option<(RVLine, RVLine)> {
        self.lines.first().copied().zip(self.lines.last().copied())
    }

    /// Iterate over the line info, copying them with the full y positions.  
    pub fn iter_line_info(&self) -> impl Iterator<Item = LineInfo> + '_ {
        self.lines.iter().map(|rvline| self.info(*rvline).unwrap())
    }

    /// Iterate over the line info within the range, copying them with the full y positions.  
    /// If the values are out of range, it is clamped to the valid lines within.
    pub fn iter_line_info_r(
        &self,
        r: RangeInclusive<RVLine>,
    ) -> impl Iterator<Item = LineInfo> + '_ {
        // We search for the start/end indices due to not having a good way to iterate over
        // successive rvlines without the view.
        // This should be good enough due to lines being small.
        let start_idx = self.lines.binary_search(r.start()).ok().or_else(|| {
            if self.lines.first().map(|l| r.start() < l).unwrap_or(false) {
                Some(0)
            } else {
                // The start is past the start of our lines
                None
            }
        });

        let end_idx = self.lines.binary_search(r.end()).ok().or_else(|| {
            if self.lines.last().map(|l| r.end() > l).unwrap_or(false) {
                Some(self.lines.len() - 1)
            } else {
                // The end is before the end of our lines but not available
                None
            }
        });

        if let (Some(start_idx), Some(end_idx)) = (start_idx, end_idx) {
            self.lines.get(start_idx..=end_idx)
        } else {
            // Hacky method to get an empty iterator of the same type
            self.lines.get(0..0)
        }
        .into_iter()
        .flatten()
        .copied()
        .map(|rvline| self.info(rvline).unwrap())
    }

    pub fn iter_vline_info(&self) -> impl Iterator<Item = VLineInfo<()>> + '_ {
        self.lines
            .iter()
            .map(|vline| &self.info[vline].vline_info)
            .copied()
    }

    pub fn iter_vline_info_r(
        &self,
        r: RangeInclusive<RVLine>,
    ) -> impl Iterator<Item = VLineInfo<()>> + '_ {
        // TODO(minor): this should probably skip tracking?
        self.iter_line_info_r(r).map(|x| x.vline_info)
    }

    /// Iter the real lines underlying the visual lines on the screen
    pub fn iter_lines(&self) -> impl Iterator<Item = usize> + '_ {
        // We can just assume that the lines stored are contiguous and thus just get the first
        // buffer line and then the last buffer line.
        let start_vline = self.lines.first().copied().unwrap_or_default();
        let end_vline = self.lines.last().copied().unwrap_or_default();

        let start_line = self.info(start_vline).unwrap().vline_info.rvline.line;
        let end_line = self.info(end_vline).unwrap().vline_info.rvline.line;

        start_line..=end_line
    }

    /// Iterate over the real lines underlying the visual lines on the screen with the y position
    /// of their layout.  
    /// (line, y)  
    pub fn iter_lines_y(&self) -> impl Iterator<Item = (usize, f64)> + '_ {
        let mut last_line = None;
        self.lines.iter().filter_map(move |vline| {
            let info = self.info(*vline).unwrap();

            let line = info.vline_info.rvline.line;

            if last_line == Some(line) {
                // We've already considered this line.
                return None;
            }

            last_line = Some(line);

            Some((line, info.y))
        })
    }

    /// Get the earliest line info for a given line.
    pub fn info_for_line(&self, line: usize) -> Option<LineInfo> {
        self.info(self.first_rvline_for_line(line)?)
    }

    /// Get the earliest rvline for the given line
    pub fn first_rvline_for_line(&self, line: usize) -> Option<RVLine> {
        self.lines
            .iter()
            .find(|rvline| rvline.line == line)
            .copied()
    }

    /// Get the latest rvline for the given line
    pub fn last_rvline_for_line(&self, line: usize) -> Option<RVLine> {
        self.lines
            .iter()
            .rfind(|rvline| rvline.line == line)
            .copied()
    }

    /// Ran on [LayoutEvent::CreatedLayout](super::visual_line::LayoutEvent::CreatedLayout) to update  [`ScreenLinesBase`] &
    /// the viewport if necessary.
    ///
    /// Returns `true` if [`ScreenLines`] needs to be completely updated in response
    pub fn on_created_layout(&self, ed: &Editor, line: usize) -> bool {
        // The default creation is empty, force an update if we're ever like this since it should
        // not happen.
        if self.is_empty() {
            return true;
        }

        let base = self.base.get_untracked();
        let vp = ed.viewport.get_untracked();

        let is_before = self
            .iter_vline_info()
            .next()
            .map(|l| line < l.rvline.line)
            .unwrap_or(false);

        // If the line is created before the current screenlines, we can simply shift the
        // base and viewport forward by the number of extra wrapped lines,
        // without needing to recompute the screen lines.
        if is_before {
            // TODO: don't assume line height is constant
            let line_height = f64::from(ed.line_height(0));

            // We could use `try_text_layout` here, but I believe this guards against a rare
            // crash (though it is hard to verify) wherein the style id has changed and so the
            // layouts get cleared.
            // However, the original trigger of the layout event was when a layout was created
            // and it expects it to still exist. So we create it just in case, though we of course
            // don't trigger another layout event.
            let layout = ed.text_layout_trigger(line, false);

            // One line was already accounted for by treating it as an unwrapped line.
            let new_lines = layout.line_count() - 1;

            let new_y0 = base.active_viewport.y0 + new_lines as f64 * line_height;
            let new_y1 = new_y0 + vp.height();
            let new_viewport = Rect::new(vp.x0, new_y0, vp.x1, new_y1);

            batch(|| {
                self.base.set(ScreenLinesBase {
                    active_viewport: new_viewport,
                });
                ed.viewport.set(new_viewport);
            });

            // Ensure that it is created even after the base/viewport signals have been updated.
            // (We need the `text_layout` to still have the layout)
            // But we have to trigger an event still if it is created because it *would* alter the
            // screenlines.
            // TODO: this has some risk for infinite looping if we're unlucky.
            let _layout = ed.text_layout_trigger(line, true);

            return false;
        }

        let is_after = self
            .iter_vline_info()
            .last()
            .map(|l| line > l.rvline.line)
            .unwrap_or(false);

        // If the line created was after the current view, we don't need to update the screenlines
        // at all, since the new line is not visible and has no effect on y positions
        if is_after {
            return false;
        }

        // If the line is created within the current screenlines, we need to update the
        // screenlines to account for the new line.
        // That is handled by the caller.
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScreenLinesBase {
    /// The current/previous viewport.  
    /// Used for determining whether there were any changes, and the `y0` serves as the
    /// base for positioning the lines.
    pub active_viewport: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineInfo {
    // font_size: usize,
    // line_height: f64,
    // x: f64,
    /// The starting y position of the overall line that this vline
    /// is a part of.
    pub y: f64,
    /// The y position of the visual line
    pub vline_y: f64,
    pub vline_info: VLineInfo<()>,
}

impl LineInfo {
    pub fn with_base(mut self, base: ScreenLinesBase) -> Self {
        self.y += base.active_viewport.y0;
        self.vline_y += base.active_viewport.y0;
        self
    }
}

pub struct EditorView {
    id: ViewId,
    editor: RwSignal<Editor>,
    is_active: Memo<bool>,
    inner_node: Option<NodeId>,
}

impl EditorView {
    #[allow(clippy::too_many_arguments)]
    fn paint_normal_selection(
        cx: &mut PaintCx,
        ed: &Editor,
        color: Color,
        screen_lines: &ScreenLines,
        start_offset: usize,
        end_offset: usize,
        affinity: CursorAffinity,
    ) {
        // TODO: selections should have separate start/end affinity
        let (start_rvline, start_col) = ed.rvline_col_of_offset(start_offset, affinity);
        let (end_rvline, end_col) = ed.rvline_col_of_offset(end_offset, affinity);

        for LineInfo {
            vline_y,
            vline_info: info,
            ..
        } in screen_lines.iter_line_info_r(start_rvline..=end_rvline)
        {
            let rvline = info.rvline;
            let line = rvline.line;

            let left_col = if rvline == start_rvline {
                start_col
            } else {
                ed.first_col(info)
            };
            let right_col = if rvline == end_rvline {
                end_col
            } else {
                ed.last_col(info, true)
            };

            // Skip over empty selections
            if !info.is_empty_phantom() && left_col == right_col {
                continue;
            }

            // TODO: What affinity should these use?
            let x0 = ed
                .line_point_of_line_col(line, left_col, CursorAffinity::Forward, true)
                .x;
            let x1 = ed
                .line_point_of_line_col(line, right_col, CursorAffinity::Backward, true)
                .x;
            // TODO(minor): Should this be line != end_line?
            let x1 = if rvline != end_rvline {
                x1 + CHAR_WIDTH
            } else {
                x1
            };

            let (x0, width) = if info.is_empty_phantom() {
                let text_layout = ed.text_layout(line);
                let width = text_layout
                    .get_layout_x(rvline.line_index)
                    .map(|(_, x1)| x1)
                    .unwrap_or(0.0)
                    .into();
                (0.0, width)
            } else {
                (x0, x1 - x0)
            };

            let line_height = ed.line_height(line);
            let rect = Rect::from_origin_size((x0, vline_y), (width, f64::from(line_height)));
            cx.fill(&rect, color, 0.0);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint_linewise_selection(
        cx: &mut PaintCx,
        ed: &Editor,
        color: Color,
        screen_lines: &ScreenLines,
        start_offset: usize,
        end_offset: usize,
        affinity: CursorAffinity,
    ) {
        let viewport = ed.viewport.get_untracked();

        let (start_rvline, _) = ed.rvline_col_of_offset(start_offset, affinity);
        let (end_rvline, _) = ed.rvline_col_of_offset(end_offset, affinity);
        // Linewise selection is by *line* so we move to the start/end rvlines of the line
        let start_rvline = screen_lines
            .first_rvline_for_line(start_rvline.line)
            .unwrap_or(start_rvline);
        let end_rvline = screen_lines
            .last_rvline_for_line(end_rvline.line)
            .unwrap_or(end_rvline);

        for LineInfo {
            vline_info: info,
            vline_y,
            ..
        } in screen_lines.iter_line_info_r(start_rvline..=end_rvline)
        {
            let rvline = info.rvline;
            let line = rvline.line;

            // The left column is always 0 for linewise selections.
            let right_col = ed.last_col(info, true);

            // TODO: what affinity to use?
            let x1 = ed
                .line_point_of_line_col(line, right_col, CursorAffinity::Backward, true)
                .x
                + CHAR_WIDTH;

            let line_height = ed.line_height(line);
            let rect = Rect::from_origin_size(
                (viewport.x0, vline_y),
                (x1 - viewport.x0, f64::from(line_height)),
            );
            cx.fill(&rect, color, 0.0);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint_blockwise_selection(
        cx: &mut PaintCx,
        ed: &Editor,
        color: Color,
        screen_lines: &ScreenLines,
        start_offset: usize,
        end_offset: usize,
        affinity: CursorAffinity,
        horiz: Option<ColPosition>,
    ) {
        let (start_rvline, start_col) = ed.rvline_col_of_offset(start_offset, affinity);
        let (end_rvline, end_col) = ed.rvline_col_of_offset(end_offset, affinity);
        let left_col = start_col.min(end_col);
        let right_col = start_col.max(end_col) + 1;

        let lines = screen_lines
            .iter_line_info_r(start_rvline..=end_rvline)
            .filter_map(|line_info| {
                let max_col = ed.last_col(line_info.vline_info, true);
                (max_col > left_col).then_some((line_info, max_col))
            });

        for (line_info, max_col) in lines {
            let line = line_info.vline_info.rvline.line;
            let right_col = if let Some(ColPosition::End) = horiz {
                max_col
            } else {
                right_col.min(max_col)
            };

            // TODO: what affinity to use?
            let x0 = ed
                .line_point_of_line_col(line, left_col, CursorAffinity::Forward, true)
                .x;
            let x1 = ed
                .line_point_of_line_col(line, right_col, CursorAffinity::Backward, true)
                .x;

            let line_height = ed.line_height(line);
            let rect =
                Rect::from_origin_size((x0, line_info.vline_y), (x1 - x0, f64::from(line_height)));
            cx.fill(&rect, color, 0.0);
        }
    }

    fn paint_cursor(cx: &mut PaintCx, ed: &Editor, is_active: bool, screen_lines: &ScreenLines) {
        let cursor = ed.cursor;

        let viewport = ed.viewport.get_untracked();

        let current_line_color = ed.es.with_untracked(|es| es.current_line());

        cursor.with_untracked(|cursor| {
            let highlight_current_line = match cursor.mode {
                // TODO: check if shis should be 0 or 1
                CursorMode::Normal(size) => size == 0,
                CursorMode::Insert(ref sel) => sel.is_caret(),
                CursorMode::Visual { .. } => false,
            };

            if let Some(current_line_color) = current_line_color {
                // Highlight the current line
                if highlight_current_line {
                    for (_, end) in cursor.regions_iter() {
                        // TODO: unsure if this is correct for wrapping lines
                        let rvline = ed.rvline_of_offset(end, cursor.affinity);

                        if let Some(info) = screen_lines.info(rvline) {
                            let line_height = ed.line_height(info.vline_info.rvline.line);
                            let rect = Rect::from_origin_size(
                                (viewport.x0, info.vline_y),
                                (viewport.width(), f64::from(line_height)),
                            );

                            cx.fill(&rect, current_line_color, 0.0);
                        }
                    }
                }
            }

            EditorView::paint_selection(cx, ed, screen_lines);

            EditorView::paint_cursor_caret(cx, ed, is_active, screen_lines);
        });
    }

    pub fn paint_selection(cx: &mut PaintCx, ed: &Editor, screen_lines: &ScreenLines) {
        let cursor = ed.cursor;

        let selection_color = ed.es.with_untracked(|es| es.selection());

        cursor.with_untracked(|cursor| match cursor.mode {
            CursorMode::Normal(_) => {}
            CursorMode::Visual {
                start,
                end,
                mode: VisualMode::Normal,
            } => {
                let start_offset = start.min(end);
                let end_offset = ed.move_right(start.max(end), Mode::Insert, 1);

                EditorView::paint_normal_selection(
                    cx,
                    ed,
                    selection_color,
                    screen_lines,
                    start_offset,
                    end_offset,
                    cursor.affinity,
                );
            }
            CursorMode::Visual {
                start,
                end,
                mode: VisualMode::Linewise,
            } => {
                EditorView::paint_linewise_selection(
                    cx,
                    ed,
                    selection_color,
                    screen_lines,
                    start.min(end),
                    start.max(end),
                    cursor.affinity,
                );
            }
            CursorMode::Visual {
                start,
                end,
                mode: VisualMode::Blockwise,
            } => {
                EditorView::paint_blockwise_selection(
                    cx,
                    ed,
                    selection_color,
                    screen_lines,
                    start.min(end),
                    start.max(end),
                    cursor.affinity,
                    cursor.horiz,
                );
            }
            CursorMode::Insert(_) => {
                for (start, end) in cursor.regions_iter().filter(|(start, end)| start != end) {
                    EditorView::paint_normal_selection(
                        cx,
                        ed,
                        selection_color,
                        screen_lines,
                        start.min(end),
                        start.max(end),
                        cursor.affinity,
                    );
                }
            }
        });
    }

    pub fn paint_cursor_caret(
        cx: &mut PaintCx,
        ed: &Editor,
        is_active: bool,
        screen_lines: &ScreenLines,
    ) {
        let cursor = ed.cursor;
        let hide_cursor = ed.cursor_info.hidden;
        let caret_color = ed.es.with_untracked(|es| es.ed_caret());

        if !is_active || hide_cursor.get_untracked() {
            return;
        }

        cursor.with_untracked(|cursor| {
            let style = ed.style();
            for (_, end) in cursor.regions_iter() {
                let is_block = match cursor.mode {
                    CursorMode::Normal(_) | CursorMode::Visual { .. } => true,
                    CursorMode::Insert(_) => false,
                };
                let LineRegion { x, width, rvline } =
                    cursor_caret(ed, end, is_block, cursor.affinity);

                if let Some(info) = screen_lines.info(rvline) {
                    if !style.paint_caret(ed.id(), rvline.line) {
                        continue;
                    }

                    let line_height = ed.line_height(info.vline_info.rvline.line);
                    let rect =
                        Rect::from_origin_size((x, info.vline_y), (width, f64::from(line_height)));
                    cx.fill(&rect, caret_color, 0.0);
                }
            }
        });
    }

    pub fn paint_wave_line(cx: &mut PaintCx, width: f64, point: Point, color: Color) {
        let radius = 2.0;
        let origin = Point::new(point.x, point.y + radius);
        let mut path = BezPath::new();
        path.move_to(origin);

        let mut x = 0.0;
        let mut direction = -1.0;
        while x < width {
            let point = origin + (x, 0.0);
            let p1 = point + (radius, -radius * direction);
            let p2 = point + (radius * 2.0, 0.0);
            path.quad_to(p1, p2);
            x += radius * 2.0;
            direction *= -1.0;
        }

        cx.stroke(&path, color, 1.0);
    }

    pub fn paint_extra_style(
        cx: &mut PaintCx,
        extra_styles: &[LineExtraStyle],
        y: f64,
        viewport: Rect,
    ) {
        for style in extra_styles {
            let height = style.height;
            if let Some(bg) = style.bg_color {
                let width = style.width.unwrap_or_else(|| viewport.width());
                let base = if style.width.is_none() {
                    viewport.x0
                } else {
                    0.0
                };
                let x = style.x + base;
                let y = y + style.y;
                cx.fill(
                    &Rect::ZERO
                        .with_size(Size::new(width, height))
                        .with_origin(Point::new(x, y)),
                    bg,
                    0.0,
                );
            }

            if let Some(color) = style.under_line {
                let width = style.width.unwrap_or_else(|| viewport.width());
                let base = if style.width.is_none() {
                    viewport.x0
                } else {
                    0.0
                };
                let x = style.x + base;
                let y = y + style.y + height;
                cx.stroke(
                    &Line::new(Point::new(x, y), Point::new(x + width, y)),
                    color,
                    1.0,
                );
            }

            if let Some(color) = style.wave_line {
                let width = style.width.unwrap_or_else(|| viewport.width());
                let y = y + style.y + height;
                EditorView::paint_wave_line(cx, width, Point::new(style.x, y), color);
            }
        }
    }

    pub fn paint_text(cx: &mut PaintCx, ed: &Editor, viewport: Rect, screen_lines: &ScreenLines) {
        let edid = ed.id();
        let style = ed.style();

        // TODO: cache indent text layout width
        let indent_unit = ed.es.with_untracked(|es| es.indent_style()).as_str();
        // TODO: don't assume font family is the same for all lines?
        let family = style.font_family(edid, 0);
        let attrs = Attrs::new()
            .family(&family)
            .font_size(style.font_size(edid, 0) as f32);
        let attrs_list = AttrsList::new(attrs);

        let mut indent_text = TextLayout::new();
        indent_text.set_text(&format!("{indent_unit}a"), attrs_list);
        let indent_text_width = indent_text.hit_position(indent_unit.len()).point.x;

        for (line, y) in screen_lines.iter_lines_y() {
            let text_layout = ed.text_layout(line);

            EditorView::paint_extra_style(cx, &text_layout.extra_style, y, viewport);

            if let Some(whitespaces) = &text_layout.whitespaces {
                let family = style.font_family(edid, line);
                let font_size = style.font_size(edid, line) as f32;
                let attrs = Attrs::new()
                    .color(ed.es.with_untracked(|es| es.visible_whitespace()))
                    .family(&family)
                    .font_size(font_size);
                let attrs_list = AttrsList::new(attrs);
                let mut space_text = TextLayout::new();
                space_text.set_text("·", attrs_list.clone());
                let mut tab_text = TextLayout::new();
                tab_text.set_text("→", attrs_list);

                for (c, (x0, _x1)) in whitespaces.iter() {
                    match *c {
                        '\t' => {
                            cx.draw_text(&tab_text, Point::new(*x0, y));
                        }
                        ' ' => {
                            cx.draw_text(&space_text, Point::new(*x0, y));
                        }
                        _ => {}
                    }
                }
            }

            if ed.es.with(|s| s.show_indent_guide()) {
                let line_height = f64::from(ed.line_height(line));
                let mut x = 0.0;
                while x + 1.0 < text_layout.indent {
                    cx.stroke(
                        &Line::new(Point::new(x, y), Point::new(x, y + line_height)),
                        ed.es.with(|es| es.indent_guide()),
                        1.0,
                    );
                    x += indent_text_width;
                }
            }

            cx.draw_text(&text_layout.text, Point::new(0.0, y));
        }
    }
}

impl View for EditorView {
    fn id(&self) -> ViewId {
        self.id
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        self.editor.with_untracked(|ed| {
            ed.es.update(|s| {
                if s.read(cx) {
                    ed.floem_style_id.update(|val| *val += 1);
                    cx.app_state_mut().request_paint(self.id());
                }
            })
        });
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Editor View".into()
    }

    fn update(&mut self, _cx: &mut UpdateCx, _state: Box<dyn std::any::Any>) {}

    fn layout(&mut self, cx: &mut LayoutCx) -> crate::taffy::tree::NodeId {
        cx.layout_node(self.id, true, |_cx| {
            let editor = self.editor.get_untracked();

            let parent_size = editor.parent_size.get_untracked();

            if self.inner_node.is_none() {
                self.inner_node = Some(self.id.new_taffy_node());
            }

            let screen_lines = editor.screen_lines.get_untracked();
            for (line, _) in screen_lines.iter_lines_y() {
                // fill in text layout cache so that max width is correct.
                editor.text_layout(line);
            }

            let inner_node = self.inner_node.unwrap();

            // TODO: don't assume there's a constant line height
            let line_height = f64::from(editor.line_height(0));

            let width = editor.max_line_width().max(parent_size.width());
            let last_line_height = line_height * (editor.last_vline().get() + 1) as f64;
            let height = last_line_height.max(parent_size.height());

            let margin_bottom = if editor.es.with_untracked(|es| es.scroll_beyond_last_line()) {
                parent_size.height().min(last_line_height) - line_height
            } else {
                0.0
            };

            let style = Style::new()
                .width(width)
                .height(height)
                .margin_bottom(margin_bottom)
                .to_taffy_style();
            let _ = self.id.taffy().borrow_mut().set_style(inner_node, style);

            vec![inner_node]
        })
    }

    fn compute_layout(&mut self, cx: &mut crate::context::ComputeLayoutCx) -> Option<Rect> {
        let editor = self.editor.get_untracked();

        let viewport = cx.current_viewport();
        if editor.viewport.with_untracked(|v| v != &viewport) {
            editor.viewport.set(viewport);
        }

        if let Some(parent) = self.id.parent() {
            let parent_size = parent.layout_rect();
            if editor.parent_size.with_untracked(|ps| ps != &parent_size) {
                editor.parent_size.set(parent_size);
            }
        }
        None
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let ed = self.editor.get_untracked();
        let viewport = ed.viewport.get_untracked();

        // We repeatedly get the screen lines because we don't currently carefully manage the
        // paint functions to avoid potentially needing to recompute them, which could *maybe*
        // make them invalid.
        // TODO: One way to get around the above issue would be to more careful, since we
        // technically don't need to stop it from *recomputing* just stop any possible changes, but
        // avoiding recomputation seems easiest/clearest.
        // I expect that most/all of the paint functions could restrict themselves to only what is
        // within the active screen lines without issue.
        let screen_lines = ed.screen_lines.get_untracked();
        EditorView::paint_cursor(cx, &ed, self.is_active.get_untracked(), &screen_lines);
        let screen_lines = ed.screen_lines.get_untracked();
        EditorView::paint_text(cx, &ed, viewport, &screen_lines);
    }
}

style_class!(pub EditorViewClass);

pub fn editor_view(
    editor: RwSignal<Editor>,
    is_active: impl Fn(bool) -> bool + 'static + Copy,
) -> EditorView {
    let id = ViewId::new();
    let is_active = create_memo(move |_| is_active(true));

    let ed = editor.get_untracked();

    let doc = ed.doc;
    let style = ed.style;
    let lines = ed.screen_lines;
    create_effect(move |_| {
        doc.track();
        style.track();
        lines.track();
        id.request_layout();
    });

    let hide_cursor = ed.cursor_info.hidden;
    create_effect(move |_| {
        hide_cursor.track();
        id.request_paint();
    });

    let editor_window_origin = ed.window_origin;
    let cursor = ed.cursor;
    let ime_allowed = ed.ime_allowed;
    let editor_viewport = ed.viewport;
    create_effect(move |_| {
        let active = is_active.get();
        if active {
            if !cursor.with(|c| c.is_insert()) {
                if ime_allowed.get_untracked() {
                    ime_allowed.set(false);
                    set_ime_allowed(false);
                }
            } else {
                if !ime_allowed.get_untracked() {
                    ime_allowed.set(true);
                    set_ime_allowed(true);
                }
                let (offset, affinity) = cursor.with(|c| (c.offset(), c.affinity));
                let (_, point_below) = ed.points_of_offset(offset, affinity);
                let window_origin = editor_window_origin.get();
                let viewport = editor_viewport.get();
                let pos =
                    window_origin + (point_below.x - viewport.x0, point_below.y - viewport.y0);
                set_ime_cursor_area(pos, Size::new(800.0, 600.0));
            }
        }
    });

    EditorView {
        id,
        editor,
        is_active,
        inner_node: None,
    }
    .keyboard_navigatable()
    .on_event(EventListener::ImePreedit, move |event| {
        if !is_active.get_untracked() {
            return EventPropagation::Continue;
        }

        if let Event::ImePreedit { text, cursor } = event {
            editor.with_untracked(|ed| {
                if text.is_empty() {
                    ed.clear_preedit();
                } else {
                    let offset = ed.cursor.with_untracked(|c| c.offset());
                    ed.set_preedit(text.clone(), *cursor, offset);
                }
            });
        }
        EventPropagation::Stop
    })
    .on_event(EventListener::ImeCommit, move |event| {
        if !is_active.get_untracked() {
            return EventPropagation::Continue;
        }

        if let Event::ImeCommit(text) = event {
            editor.with_untracked(|ed| {
                ed.clear_preedit();
                ed.receive_char(text);
            });
        }
        EventPropagation::Stop
    })
    .class(EditorViewClass)
}

#[derive(Clone, Debug)]
pub struct LineRegion {
    pub x: f64,
    pub width: f64,
    pub rvline: RVLine,
}

/// Get the render information for a caret cursor at the given `offset`.  
pub fn cursor_caret(
    ed: &Editor,
    offset: usize,
    block: bool,
    affinity: CursorAffinity,
) -> LineRegion {
    let info = ed.rvline_info_of_offset(offset, affinity);
    let (_, col) = ed.offset_to_line_col(offset);
    let after_last_char = col == ed.line_end_col(info.rvline.line, true);

    let doc = ed.doc();
    let preedit_start = doc
        .preedit()
        .preedit
        .with_untracked(|preedit| {
            preedit.as_ref().and_then(|preedit| {
                let preedit_line = ed.line_of_offset(preedit.offset);
                preedit.cursor.map(|x| (preedit_line, x))
            })
        })
        .filter(|(preedit_line, _)| *preedit_line == info.rvline.line)
        .map(|(_, (start, _))| start);

    let (_, col) = ed.offset_to_line_col(offset);

    let point = ed.line_point_of_line_col(info.rvline.line, col, CursorAffinity::Forward, false);

    let rvline = if preedit_start.is_some() {
        // If there's an IME edit, then we need to use the point's y to get the actual y position
        // that the IME cursor is at. Since it could be in the middle of the IME phantom text
        let y = point.y;

        // TODO: I don't think this is handling varying line heights properly
        let line_height = ed.line_height(info.rvline.line);

        let line_index = (y / f64::from(line_height)).floor() as usize;
        RVLine::new(info.rvline.line, line_index)
    } else {
        info.rvline
    };

    let x0 = point.x;
    if block {
        let x0 = ed
            .line_point_of_line_col(info.rvline.line, col, CursorAffinity::Forward, true)
            .x;
        let new_offset = ed.move_right(offset, Mode::Insert, 1);
        let (_, new_col) = ed.offset_to_line_col(new_offset);
        let width = if after_last_char {
            CHAR_WIDTH
        } else {
            let x1 = ed
                .line_point_of_line_col(info.rvline.line, new_col, CursorAffinity::Backward, true)
                .x;
            x1 - x0
        };

        LineRegion {
            x: x0,
            width,
            rvline,
        }
    } else {
        LineRegion {
            x: x0 - 1.0,
            width: 2.0,
            rvline,
        }
    }
}

pub fn editor_container_view(
    editor: RwSignal<Editor>,
    is_active: impl Fn(bool) -> bool + 'static + Copy,
    handle_key_event: impl Fn(&KeyPress, Modifiers) -> CommandExecuted + 'static,
) -> impl IntoView {
    stack((
        editor_gutter(editor),
        editor_content(editor, is_active, handle_key_event),
    ))
    .style(|s| s.absolute().size_pct(100.0, 100.0))
    .on_cleanup(move || {
        // TODO: should we have some way for doc to tell us if we're allowed to cleanup the editor?
        let editor = editor.get_untracked();
        editor.cx.get().dispose();
    })
}

/// Default editor gutter
/// Simply shows line numbers
pub fn editor_gutter(editor: RwSignal<Editor>) -> impl IntoView {
    let ed = editor.get_untracked();

    let scroll_delta = ed.scroll_delta;

    let gutter_rect = create_rw_signal(Rect::ZERO);

    editor_gutter_view(editor)
        .on_resize(move |rect| {
            gutter_rect.set(rect);
        })
        .on_event_stop(EventListener::PointerWheel, move |event| {
            if let Event::PointerWheel(pointer_event) = event {
                scroll_delta.set(pointer_event.delta);
            }
        })
}

fn editor_content(
    editor: RwSignal<Editor>,
    is_active: impl Fn(bool) -> bool + 'static + Copy,
    handle_key_event: impl Fn(&KeyPress, Modifiers) -> CommandExecuted + 'static,
) -> impl IntoView {
    let ed = editor.get_untracked();
    let cursor = ed.cursor;
    let scroll_delta = ed.scroll_delta;
    let scroll_to = ed.scroll_to;
    let window_origin = ed.window_origin;
    let viewport = ed.viewport;

    scroll({
        let editor_content_view =
            editor_view(editor, is_active).style(move |s| s.absolute().cursor(CursorStyle::Text));

        let id = editor_content_view.id();
        ed.editor_view_id.set(Some(id));

        editor_content_view
            .on_event_cont(EventListener::FocusGained, move |_| {
                editor.with_untracked(|ed| ed.editor_view_focused.notify())
            })
            .on_event_cont(EventListener::FocusLost, move |_| {
                editor.with_untracked(|ed| ed.editor_view_focus_lost.notify())
            })
            .on_event_cont(EventListener::PointerDown, move |event| {
                if let Event::PointerDown(pointer_event) = event {
                    id.request_active();
                    id.request_focus();
                    editor.get_untracked().pointer_down(pointer_event);
                }
            })
            .on_event_cont(EventListener::PointerMove, move |event| {
                if let Event::PointerMove(pointer_event) = event {
                    editor.get_untracked().pointer_move(pointer_event);
                }
            })
            .on_event_cont(EventListener::PointerUp, move |event| {
                if let Event::PointerUp(pointer_event) = event {
                    editor.get_untracked().pointer_up(pointer_event);
                }
            })
            .on_event_stop(EventListener::KeyDown, move |event| {
                let Event::KeyDown(key_event) = event else {
                    return;
                };

                let Ok(keypress) = KeyPress::try_from(key_event) else {
                    return;
                };

                handle_key_event(&keypress, key_event.modifiers);

                let mut mods = key_event.modifiers;
                mods.set(Modifiers::SHIFT, false);
                mods.set(Modifiers::ALTGR, false);
                #[cfg(target_os = "macos")]
                mods.set(Modifiers::ALT, false);

                if mods.is_empty() {
                    if let KeyInput::Keyboard(Key::Character(c), _) = keypress.key {
                        editor.get_untracked().receive_char(&c);
                    } else if let KeyInput::Keyboard(Key::Named(NamedKey::Space), _) = keypress.key
                    {
                        editor.get_untracked().receive_char(" ");
                    }
                }
            })
    })
    .on_move(move |point| {
        window_origin.set(point);
    })
    .scroll_to(move || scroll_to.get().map(Vec2::to_point))
    .scroll_delta(move || scroll_delta.get())
    .ensure_visible(move || {
        let editor = editor.get_untracked();
        let cursor = cursor.get();
        let offset = cursor.offset();
        editor.doc.track();
        // TODO:?
        // editor.kind.track();

        let LineRegion { x, width, rvline } =
            cursor_caret(&editor, offset, !cursor.is_insert(), cursor.affinity);

        // TODO: don't assume line-height is constant
        let line_height = f64::from(editor.line_height(0));

        // TODO: is there a good way to avoid the calculation of the vline here?
        let vline = editor.vline_of_rvline(rvline);
        let rect =
            Rect::from_origin_size((x, vline.get() as f64 * line_height), (width, line_height))
                .inflate(10.0, 1.0);

        let viewport = viewport.get_untracked();
        let smallest_distance = (viewport.y0 - rect.y0)
            .abs()
            .min((viewport.y1 - rect.y0).abs())
            .min((viewport.y0 - rect.y1).abs())
            .min((viewport.y1 - rect.y1).abs());
        let biggest_distance = (viewport.y0 - rect.y0)
            .abs()
            .max((viewport.y1 - rect.y0).abs())
            .max((viewport.y0 - rect.y1).abs())
            .max((viewport.y1 - rect.y1).abs());
        let jump_to_middle =
            biggest_distance > viewport.height() && smallest_distance > viewport.height() / 2.0;

        if jump_to_middle {
            rect.inflate(0.0, viewport.height() / 2.0)
        } else {
            let mut rect = rect;
            let cursor_surrounding_lines = editor.es.with(|s| s.cursor_surrounding_lines()) as f64;
            rect.y0 -= cursor_surrounding_lines * line_height;
            rect.y1 += cursor_surrounding_lines * line_height;
            rect
        }
    })
    .style(|s| s.size_pct(100.0, 100.0))
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, rc::Rc};

    use floem_reactive::create_rw_signal;
    use peniko::kurbo::Rect;

    use crate::views::editor::{
        view::LineInfo,
        visual_line::{RVLine, VLineInfo},
    };

    use super::{ScreenLines, ScreenLinesBase};

    #[test]
    fn iter_line_info_range() {
        let lines = vec![
            RVLine::new(10, 0),
            RVLine::new(10, 1),
            RVLine::new(10, 2),
            RVLine::new(10, 3),
        ];
        let mut info = HashMap::new();
        for rv in lines.iter() {
            info.insert(
                *rv,
                LineInfo {
                    // The specific values don't really matter
                    y: 0.0,
                    vline_y: 0.0,
                    vline_info: VLineInfo::new(0..0, *rv, 4, ()),
                },
            );
        }
        let sl = ScreenLines {
            lines: Rc::new(lines),
            info: Rc::new(info),
            diff_sections: None,
            base: create_rw_signal(ScreenLinesBase {
                active_viewport: Rect::ZERO,
            }),
        };

        // Completely outside range should be empty
        assert_eq!(
            sl.iter_line_info_r(RVLine::new(0, 0)..=RVLine::new(1, 5))
                .collect::<Vec<_>>(),
            Vec::new()
        );
        // Should include itself
        assert_eq!(
            sl.iter_line_info_r(RVLine::new(10, 0)..=RVLine::new(10, 0))
                .count(),
            1
        );
        // Typical case
        assert_eq!(
            sl.iter_line_info_r(RVLine::new(10, 0)..=RVLine::new(10, 2))
                .count(),
            3
        );
        assert_eq!(
            sl.iter_line_info_r(RVLine::new(10, 0)..=RVLine::new(10, 3))
                .count(),
            4
        );
        // Should only include what is within the interval
        assert_eq!(
            sl.iter_line_info_r(RVLine::new(10, 0)..=RVLine::new(10, 5))
                .count(),
            4
        );
        assert_eq!(
            sl.iter_line_info_r(RVLine::new(0, 0)..=RVLine::new(10, 5))
                .count(),
            4
        );
    }
}
