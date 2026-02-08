//! Movement logic for the editor.

use floem_editor_core::{
    buffer::rope_text::{RopeText, RopeTextVal},
    command::MultiSelectionCommand,
    cursor::{ColPosition, Cursor, CursorAffinity, CursorMode},
    mode::{Mode, MotionMode, VisualMode},
    movement::{LinePosition, Movement},
    register::Register,
    selection::{SelRegion, Selection},
    soft_tab::{SnapDirection, snap_to_soft_tab},
};

use super::{
    Editor,
    actions::CommonAction,
    visual_line::{RVLine, VLineInfo},
};

/// Move a selection region by a given movement.
///
/// Much of the time, this will just be a matter of moving the cursor, but
/// some movements may depend on the current selection.
fn move_region(
    view: &Editor,
    region: &SelRegion,
    count: usize,
    modify: bool,
    movement: &Movement,
    mode: Mode,
) -> SelRegion {
    let (count, region) = if count >= 1 && !modify && !region.is_caret() {
        // If we're not a caret, and we are moving left/up or right/down, we want to move
        // the cursor to the left or right side of the selection.
        // Ex: `|abc|` -> left/up arrow key -> `|abc`
        // Ex: `|abc|` -> right/down arrow key -> `abc|`
        // and it doesn't matter which direction the selection is going, so we use min/max
        match movement {
            Movement::Left | Movement::Up => {
                let leftmost = region.min();
                (
                    count - 1,
                    SelRegion::new(leftmost, leftmost, region.affinity, region.horiz),
                )
            }
            Movement::Right | Movement::Down => {
                let rightmost = region.max();
                (
                    count - 1,
                    SelRegion::new(rightmost, rightmost, region.affinity, region.horiz),
                )
            }
            _ => (count, *region),
        }
    } else {
        (count, *region)
    };

    let mut affinity = region.affinity;

    let (end, horiz) = move_offset(
        view,
        region.end,
        region.horiz.as_ref(),
        &mut affinity,
        count,
        movement,
        mode,
    );
    let start = match modify {
        true => region.start,
        false => end,
    };
    SelRegion::new(start, end, affinity, horiz)
}

pub fn move_selection(
    view: &Editor,
    selection: &Selection,
    count: usize,
    modify: bool,
    movement: &Movement,
    mode: Mode,
) -> Selection {
    let mut new_selection = Selection::new();
    for region in selection.regions() {
        new_selection.add_region(move_region(view, region, count, modify, movement, mode));
    }
    new_selection
}

// TODO: It would probably fit the overall logic better if affinity was immutable and it just returned the new affinity!
pub fn move_offset(
    view: &Editor,
    offset: usize,
    horiz: Option<&ColPosition>,
    affinity: &mut CursorAffinity,
    count: usize,
    movement: &Movement,
    mode: Mode,
) -> (usize, Option<ColPosition>) {
    let (new_offset, horiz) = match movement {
        Movement::Left => {
            let new_offset = move_left(view, offset, affinity, mode, count);

            (new_offset, None)
        }
        Movement::Right => {
            let new_offset = move_right(view, offset, affinity, mode, count);

            (new_offset, None)
        }
        Movement::Up => {
            let (new_offset, horiz) = move_up(view, offset, affinity, horiz.cloned(), mode, count);

            (new_offset, Some(horiz))
        }
        Movement::Down => {
            let (new_offset, horiz) =
                move_down(view, offset, affinity, horiz.cloned(), mode, count);

            (new_offset, Some(horiz))
        }
        Movement::DocumentStart => {
            // Put it before any inlay hints at the very start
            *affinity = CursorAffinity::Backward;
            (0, Some(ColPosition::Start))
        }
        Movement::DocumentEnd => {
            let (new_offset, horiz) = document_end(view.rope_text(), affinity, mode);

            (new_offset, Some(horiz))
        }
        Movement::FirstNonBlank => {
            let (new_offset, horiz) = first_non_blank(view, affinity, offset);

            (new_offset, Some(horiz))
        }
        Movement::StartOfLine => {
            let (new_offset, horiz) = start_of_line(view, affinity, offset);

            (new_offset, Some(horiz))
        }
        Movement::EndOfLine => {
            let (new_offset, horiz) = end_of_line(view, affinity, offset, mode);

            (new_offset, Some(horiz))
        }
        Movement::Line(position) => {
            let (new_offset, horiz) = to_line(view, offset, horiz.cloned(), mode, position);

            (new_offset, Some(horiz))
        }
        Movement::Offset(offset) => {
            let new_offset = view.text().prev_grapheme_offset(*offset + 1).unwrap();
            (new_offset, None)
        }
        Movement::WordEndForward => {
            let new_offset =
                view.rope_text()
                    .move_n_wordends_forward(offset, count, mode == Mode::Insert);
            (new_offset, None)
        }
        Movement::WordForward => {
            let new_offset = view.rope_text().move_n_words_forward(offset, count);
            (new_offset, None)
        }
        Movement::WordBackward => {
            let new_offset = view.rope_text().move_n_words_backward(offset, count, mode);
            (new_offset, None)
        }
        Movement::NextUnmatched(char) => {
            let new_offset = view.doc().find_unmatched(offset, false, *char);

            (new_offset, None)
        }
        Movement::PreviousUnmatched(char) => {
            let new_offset = view.doc().find_unmatched(offset, true, *char);

            (new_offset, None)
        }
        Movement::MatchPairs => {
            let new_offset = view.doc().find_matching_pair(offset);

            (new_offset, None)
        }
        Movement::ParagraphForward => {
            let new_offset = view.rope_text().move_n_paragraphs_forward(offset, count);

            (new_offset, None)
        }
        Movement::ParagraphBackward => {
            let new_offset = view.rope_text().move_n_paragraphs_backward(offset, count);

            (new_offset, None)
        }
    };

    let new_offset = correct_crlf(&view.rope_text(), new_offset);

    (new_offset, horiz)
}

/// If the offset is at `\r|\n` then move it back.
fn correct_crlf(text: &RopeTextVal, offset: usize) -> usize {
    if offset == 0 || offset == text.len() {
        return offset;
    }

    let mut cursor = lapce_xi_rope::Cursor::new(text.text(), offset);
    if cursor.peek_next_codepoint() == Some('\n') && cursor.prev_codepoint() == Some('\r') {
        return offset - 1;
    }

    offset
}

fn atomic_soft_tab_width_for_offset(ed: &Editor, offset: usize) -> Option<usize> {
    let line = ed.line_of_offset(offset);
    let style = ed.style();
    if style.atomic_soft_tabs(ed.id(), line) {
        Some(style.tab_width(ed.id(), line))
    } else {
        None
    }
}

/// Move the offset to the left by `count` amount.
///
/// If `soft_tab_width` is `Some` (and greater than 1) then the offset will snap to the soft tab.
fn move_left(
    ed: &Editor,
    offset: usize,
    affinity: &mut CursorAffinity,
    mode: Mode,
    count: usize,
) -> usize {
    let rope_text = ed.rope_text();
    let mut new_offset = rope_text.move_left(offset, mode, count);

    if let Some(soft_tab_width) = atomic_soft_tab_width_for_offset(ed, offset)
        && soft_tab_width > 1
    {
        new_offset = snap_to_soft_tab(
            rope_text.text(),
            new_offset,
            SnapDirection::Left,
            soft_tab_width,
        );
    }

    *affinity = CursorAffinity::Forward;

    new_offset
}

/// Move the offset to the right by `count` amount.
/// If `soft_tab_width` is `Some` (and greater than 1) then the offset will snap to the soft tab.
fn move_right(
    view: &Editor,
    offset: usize,
    affinity: &mut CursorAffinity,
    mode: Mode,
    count: usize,
) -> usize {
    let rope_text = view.rope_text();
    let mut new_offset = rope_text.move_right(offset, mode, count);

    if let Some(soft_tab_width) = atomic_soft_tab_width_for_offset(view, offset)
        && soft_tab_width > 1
    {
        new_offset = snap_to_soft_tab(
            rope_text.text(),
            new_offset,
            SnapDirection::Right,
            soft_tab_width,
        );
    }

    *affinity = CursorAffinity::Backward;

    new_offset
}

fn find_prev_rvline(view: &Editor, start: RVLine, count: usize) -> Option<RVLine> {
    if count == 0 {
        return Some(start);
    }

    // We can't just directly subtract count because of multi-line phantom text.
    // As just subtracting count wouldn't properly skip over the phantom lines.
    // So we have to search backwards for the previous line that has real content.
    let mut info = None;
    let mut found_count = 0;
    for prev_info in view.iter_rvlines(true, start).skip(1) {
        if prev_info.is_empty_phantom() {
            // We skip any phantom text lines in our consideration
            continue;
        }

        // Otherwise we found a real line.
        found_count += 1;

        if found_count == count {
            // If we've completed all the count instances then we're done
            info = Some(prev_info);
            break;
        }
        // Otherwise we continue on to find the previous line with content before that.
    }

    info.map(|info| info.rvline)
}

/// Move the offset up by `count` amount.
///
/// `count` may be zero, because moving up in a selection just jumps to the start of the selection.
fn move_up(
    view: &Editor,
    offset: usize,
    affinity: &mut CursorAffinity,
    horiz: Option<ColPosition>,
    mode: Mode,
    count: usize,
) -> (usize, ColPosition) {
    let rvline = view.rvline_of_offset(offset, *affinity);
    if rvline.line == 0 && rvline.line_index == 0 {
        // Zeroth line
        let horiz = horiz
            .unwrap_or_else(|| ColPosition::Col(view.line_point_of_offset(offset, *affinity).x));

        *affinity = CursorAffinity::Backward;

        return (0, horiz);
    }

    let Some(rvline) = find_prev_rvline(view, rvline, count) else {
        // Zeroth line
        let horiz = horiz
            .unwrap_or_else(|| ColPosition::Col(view.line_point_of_offset(offset, *affinity).x));

        *affinity = CursorAffinity::Backward;

        return (0, horiz);
    };

    let horiz =
        horiz.unwrap_or_else(|| ColPosition::Col(view.line_point_of_offset(offset, *affinity).x));
    let col = view.rvline_horiz_col(rvline, &horiz, mode != Mode::Normal);
    let new_offset = view.offset_of_line_col(rvline.line, col);

    let info = view.rvline_info(rvline);

    *affinity = if new_offset == info.interval.start {
        CursorAffinity::Forward
    } else {
        CursorAffinity::Backward
    };

    (new_offset, horiz)
}

/// Move down for when the cursor is on the last visual line.
fn move_down_last_rvline(
    view: &Editor,
    offset: usize,
    affinity: &mut CursorAffinity,
    horiz: Option<ColPosition>,
    mode: Mode,
) -> (usize, ColPosition) {
    let rope_text = view.rope_text();

    let last_line = rope_text.last_line();
    let new_offset = rope_text.line_end_offset(last_line, mode != Mode::Normal);

    // We should appear after any phantom text at the very end of the line.
    *affinity = CursorAffinity::Forward;

    let horiz =
        horiz.unwrap_or_else(|| ColPosition::Col(view.line_point_of_offset(offset, *affinity).x));

    (new_offset, horiz)
}

fn find_next_rvline_info(
    view: &Editor,
    offset: usize,
    start: RVLine,
    count: usize,
) -> Option<VLineInfo<()>> {
    // We can't just directly add count because of multi-line phantom text.
    // These lines are 'not there' and also don't have any position that can be moved into
    // (unlike phantom text that is mixed with real text)
    // So we have to search forward for the next line that has real content.
    // The typical iteration count for this is 1, and even after that it is usually only a handful.
    let mut found_count = 0;
    for next_info in view.iter_rvlines(false, start) {
        if count == 0 {
            return Some(next_info);
        }

        if next_info.is_empty_phantom() {
            // We skip any phantom text lines in our consideration
            // TODO: Would this skip over an empty line?
            continue;
        }

        if next_info.interval.start < offset || next_info.rvline == start {
            // If we're on or before our current visual line then we skip it
            continue;
        }

        // Otherwise we found a real line.
        found_count += 1;

        if found_count == count {
            // If we've completed all the count instances then we're done
            return Some(next_info);
        }
        // Otherwise we continue on to find the next line with content after that.
    }

    None
}

/// Move the offset down by `count` amount.
///
/// `count` may be zero, because moving down in a selection just jumps to the end of the selection.
fn move_down(
    view: &Editor,
    offset: usize,
    affinity: &mut CursorAffinity,
    horiz: Option<ColPosition>,
    mode: Mode,
    count: usize,
) -> (usize, ColPosition) {
    let rvline = view.rvline_of_offset(offset, *affinity);

    let Some(info) = find_next_rvline_info(view, offset, rvline, count) else {
        // There was no next entry, this typically means that we would go past the end if we went
        // further
        return move_down_last_rvline(view, offset, affinity, horiz, mode);
    };

    // TODO(minor): is this the right affinity?
    let horiz =
        horiz.unwrap_or_else(|| ColPosition::Col(view.line_point_of_offset(offset, *affinity).x));

    let col = view.rvline_horiz_col(info.rvline, &horiz, mode != Mode::Normal);

    let new_offset = view.offset_of_line_col(info.rvline.line, col);

    *affinity = if new_offset == info.interval.start {
        // The column was zero so we shift it to be at the line itself.
        // This lets us move down to an empty - for example - next line and appear at the
        // start of that line without coinciding with the offset at the end of the previous line.
        CursorAffinity::Forward
    } else {
        CursorAffinity::Backward
    };

    (new_offset, horiz)
}

fn document_end(
    rope_text: impl RopeText,
    affinity: &mut CursorAffinity,
    mode: Mode,
) -> (usize, ColPosition) {
    let last_offset = rope_text.offset_line_end(rope_text.len(), mode != Mode::Normal);

    // Put it past any inlay hints directly at the end
    *affinity = CursorAffinity::Forward;

    (last_offset, ColPosition::End)
}

fn first_non_blank(
    view: &Editor,
    affinity: &mut CursorAffinity,
    offset: usize,
) -> (usize, ColPosition) {
    let info = view.rvline_info_of_offset(offset, *affinity);
    let non_blank_offset = info.first_non_blank_character(&view.text_prov());
    let start_line_offset = info.interval.start;
    // TODO: is this always the correct affinity? It might be desirable for the very first character on a wrapped line?
    *affinity = CursorAffinity::Forward;

    if offset > non_blank_offset {
        // Jump to the first non-whitespace character if we're strictly after it
        (non_blank_offset, ColPosition::FirstNonBlank)
    } else {
        // If we're at the start of the line, also jump to the first not blank
        if start_line_offset == offset {
            (non_blank_offset, ColPosition::FirstNonBlank)
        } else {
            // Otherwise, jump to the start of the line
            (start_line_offset, ColPosition::Start)
        }
    }
}

fn start_of_line(
    view: &Editor,
    affinity: &mut CursorAffinity,
    offset: usize,
) -> (usize, ColPosition) {
    let rvline = view.rvline_of_offset(offset, *affinity);
    let new_offset = view.offset_of_rvline(rvline);
    // TODO(minor): if the line has zero characters, it should probably be forward affinity but
    // other cases might be better as backwards?
    *affinity = CursorAffinity::Forward;

    (new_offset, ColPosition::Start)
}

fn end_of_line(
    view: &Editor,
    affinity: &mut CursorAffinity,
    offset: usize,
    mode: Mode,
) -> (usize, ColPosition) {
    let info = view.rvline_info_of_offset(offset, *affinity);
    let new_col = info.last_col(&view.text_prov(), mode != Mode::Normal);
    *affinity = if new_col == 0 {
        CursorAffinity::Forward
    } else {
        CursorAffinity::Backward
    };

    let new_offset = view.offset_of_line_col(info.rvline.line, new_col);

    (new_offset, ColPosition::End)
}

fn to_line(
    view: &Editor,
    offset: usize,
    horiz: Option<ColPosition>,
    mode: Mode,
    position: &LinePosition,
) -> (usize, ColPosition) {
    let rope_text = view.rope_text();

    // TODO(minor): Should this use rvline?
    let line = match position {
        LinePosition::Line(line) => (line - 1).min(rope_text.last_line()),
        LinePosition::First => 0,
        LinePosition::Last => rope_text.last_line(),
    };
    // TODO(minor): is this the best affinity?
    let horiz = horiz.unwrap_or_else(|| {
        ColPosition::Col(
            view.line_point_of_offset(offset, CursorAffinity::Backward)
                .x,
        )
    });
    let col = view.line_horiz_col(line, &horiz, mode != Mode::Normal);
    let new_offset = rope_text.offset_of_line_col(line, col);

    (new_offset, horiz)
}

/// Move the current cursor.
///
/// This will signal-update the document for some motion modes.
pub fn move_cursor(
    ed: &Editor,
    action: &dyn CommonAction,
    cursor: &mut Cursor,
    movement: &Movement,
    count: usize,
    modify: bool,
    register: &mut Register,
) {
    match cursor.mode {
        CursorMode::Normal {
            offset,
            mut affinity,
        } => {
            let count = if let Some(motion_mode) = cursor.motion_mode.as_ref() {
                count.max(motion_mode.count())
            } else {
                count
            };
            let (new_offset, horiz) = move_offset(
                ed,
                offset,
                cursor.horiz.as_ref(),
                &mut affinity,
                count,
                movement,
                Mode::Normal,
            );
            if let Some(motion_mode) = cursor.motion_mode.clone() {
                let (moved_new_offset, _) = move_offset(
                    ed,
                    new_offset,
                    None,
                    &mut affinity,
                    1,
                    &Movement::Right,
                    Mode::Insert,
                );
                let range = match movement {
                    Movement::EndOfLine | Movement::WordEndForward => offset..moved_new_offset,
                    Movement::MatchPairs => {
                        if new_offset > offset {
                            offset..moved_new_offset
                        } else {
                            moved_new_offset..new_offset
                        }
                    }
                    _ => offset..new_offset,
                };
                action.exec_motion_mode(
                    ed,
                    cursor,
                    motion_mode,
                    range,
                    movement.is_vertical(),
                    register,
                );
                cursor.motion_mode = None;
            } else {
                cursor.mode = CursorMode::Normal {
                    offset: new_offset,
                    affinity,
                };
                cursor.horiz = horiz;
            }
        }
        CursorMode::Visual {
            start,
            end,
            mode,
            mut affinity,
        } => {
            let (new_offset, horiz) = move_offset(
                ed,
                end,
                cursor.horiz.as_ref(),
                &mut affinity,
                count,
                movement,
                Mode::Visual(VisualMode::Normal),
            );
            cursor.mode = CursorMode::Visual {
                start,
                end: new_offset,
                mode,
                affinity,
            };
            cursor.horiz = horiz;
        }
        CursorMode::Insert(ref selection) => {
            let selection = move_selection(ed, selection, count, modify, movement, Mode::Insert);
            cursor.set_insert(selection);
        }
    }
}

pub fn do_multi_selection(view: &Editor, cursor: &mut Cursor, cmd: &MultiSelectionCommand) {
    use MultiSelectionCommand::*;
    let rope_text = view.rope_text();

    match cmd {
        SelectUndo => {
            if let CursorMode::Insert(_) = cursor.mode.clone() {
                if let Some(selection) = cursor.history_selections.last().cloned() {
                    cursor.mode = CursorMode::Insert(selection);
                }
                cursor.history_selections.pop();
            }
        }
        InsertCursorAbove => {
            if let CursorMode::Insert(mut selection) = cursor.mode.clone() {
                let (offset, mut affinity) = selection
                    .first()
                    .map(|s| (s.end, s.affinity))
                    .unwrap_or((0, CursorAffinity::Backward));

                let (new_offset, _) = move_offset(
                    view,
                    offset,
                    cursor.horiz.as_ref(),
                    &mut affinity,
                    1,
                    &Movement::Up,
                    Mode::Insert,
                );
                if new_offset != offset {
                    selection.add_region(SelRegion::new(new_offset, new_offset, affinity, None));
                }
                cursor.set_insert(selection);
            }
        }
        InsertCursorBelow => {
            if let CursorMode::Insert(mut selection) = cursor.mode.clone() {
                let (offset, mut affinity) = selection
                    .last()
                    .map(|s| (s.end, s.affinity))
                    .unwrap_or((0, CursorAffinity::Backward));

                let (new_offset, _) = move_offset(
                    view,
                    offset,
                    cursor.horiz.as_ref(),
                    &mut affinity,
                    1,
                    &Movement::Down,
                    Mode::Insert,
                );
                if new_offset != offset {
                    selection.add_region(SelRegion::new(new_offset, new_offset, affinity, None));
                }
                cursor.set_insert(selection);
            }
        }
        InsertCursorEndOfLine => {
            if let CursorMode::Insert(selection) = cursor.mode.clone() {
                let mut new_selection = Selection::new();
                for region in selection.regions() {
                    let (start_line, _) = rope_text.offset_to_line_col(region.min());
                    let (end_line, end_col) = rope_text.offset_to_line_col(region.max());
                    for line in start_line..end_line + 1 {
                        let offset = if line == end_line {
                            rope_text.offset_of_line_col(line, end_col)
                        } else {
                            rope_text.line_end_offset(line, true)
                        };
                        new_selection.add_region(SelRegion::new(
                            offset,
                            offset,
                            CursorAffinity::Backward,
                            None,
                        ));
                    }
                }
                cursor.set_insert(new_selection);
            }
        }
        SelectCurrentLine => {
            if let CursorMode::Insert(selection) = cursor.mode.clone() {
                let mut new_selection = Selection::new();
                for region in selection.regions() {
                    let start_line = rope_text.line_of_offset(region.min());
                    let start = rope_text.offset_of_line(start_line);
                    let end_line = rope_text.line_of_offset(region.max());
                    let end = rope_text.offset_of_line(end_line + 1);
                    new_selection.add_region(SelRegion::new(
                        start,
                        end,
                        CursorAffinity::Backward,
                        None,
                    ));
                }
                cursor.set_insert(new_selection);
            }
        }
        SelectAllCurrent | SelectNextCurrent | SelectSkipCurrent => {
            // TODO: How should we handle these?
            // The specific common editor behavior is to use the editor's find
            // to do these finds and use it for the selections.
            // However, we haven't included a `find` in floem-editor
        }
        SelectAll => {
            let new_selection = Selection::region(0, rope_text.len(), CursorAffinity::Forward);
            cursor.set_insert(new_selection);
        }
    }
}

pub fn do_motion_mode(
    ed: &Editor,
    action: &dyn CommonAction,
    cursor: &mut Cursor,
    motion_mode: MotionMode,
    register: &mut Register,
) {
    if let Some(cached_motion_mode) = cursor.motion_mode.take() {
        // If it's the same MotionMode discriminant, continue, count is cached in the old motion_mode.
        if core::mem::discriminant(&cached_motion_mode) == core::mem::discriminant(&motion_mode) {
            let offset = cursor.offset();
            action.exec_motion_mode(
                ed,
                cursor,
                cached_motion_mode,
                offset..offset,
                true,
                register,
            );
        }
    } else {
        cursor.motion_mode = Some(motion_mode);
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use floem_editor_core::{
        buffer::rope_text::{RopeText, RopeTextVal},
        cursor::{ColPosition, CursorAffinity},
        mode::Mode,
    };
    use floem_reactive::{Scope, SignalUpdate};
    use lapce_xi_rope::Rope;
    use peniko::kurbo::{Rect, Size};

    use crate::views::editor::{
        movement::{correct_crlf, end_of_line, move_down, move_up},
        text::SimpleStyling,
        text_document::TextDocument,
    };

    use super::Editor;

    fn make_ed(text: &str) -> Editor {
        let cx = Scope::new();
        let doc = Rc::new(TextDocument::new(cx, text));
        let style = Rc::new(SimpleStyling::new());
        let editor = Editor::new(cx, doc, style, false);
        editor
            .viewport
            .set(Rect::ZERO.with_size(Size::new(f64::MAX, f64::MAX)));
        editor
    }

    // Tests for movement logic.
    // Many of the locations that use affinity are unsure of the specifics, and should only be
    // assumed to be mostly kinda correct.

    #[test]
    fn test_correct_crlf() {
        let text = Rope::from("hello\nworld");
        let text = RopeTextVal::new(text);
        assert_eq!(correct_crlf(&text, 0), 0);
        assert_eq!(correct_crlf(&text, 5), 5);
        assert_eq!(correct_crlf(&text, 6), 6);
        assert_eq!(correct_crlf(&text, text.len()), text.len());

        let text = Rope::from("hello\r\nworld");
        let text = RopeTextVal::new(text);
        assert_eq!(correct_crlf(&text, 0), 0);
        assert_eq!(correct_crlf(&text, 5), 5);
        assert_eq!(correct_crlf(&text, 6), 5);
        assert_eq!(correct_crlf(&text, 7), 7);
        assert_eq!(correct_crlf(&text, text.len()), text.len());
    }

    #[test]
    fn test_end_of_line() {
        let ed = make_ed("abc\ndef\nghi");
        let mut aff = CursorAffinity::Backward;
        assert_eq!(end_of_line(&ed, &mut aff, 0, Mode::Insert).0, 3);
        assert_eq!(aff, CursorAffinity::Backward);
        assert_eq!(end_of_line(&ed, &mut aff, 1, Mode::Insert).0, 3);
        assert_eq!(aff, CursorAffinity::Backward);
        assert_eq!(end_of_line(&ed, &mut aff, 3, Mode::Insert).0, 3);
        assert_eq!(aff, CursorAffinity::Backward);

        assert_eq!(end_of_line(&ed, &mut aff, 4, Mode::Insert).0, 7);
        assert_eq!(end_of_line(&ed, &mut aff, 5, Mode::Insert).0, 7);
        assert_eq!(end_of_line(&ed, &mut aff, 7, Mode::Insert).0, 7);

        let ed = make_ed("abc\r\ndef\r\nghi");
        let mut aff = CursorAffinity::Forward;
        assert_eq!(end_of_line(&ed, &mut aff, 0, Mode::Insert).0, 3);
        assert_eq!(aff, CursorAffinity::Backward);

        assert_eq!(end_of_line(&ed, &mut aff, 1, Mode::Insert).0, 3);
        assert_eq!(aff, CursorAffinity::Backward);
        assert_eq!(end_of_line(&ed, &mut aff, 3, Mode::Insert).0, 3);
        assert_eq!(aff, CursorAffinity::Backward);

        assert_eq!(end_of_line(&ed, &mut aff, 5, Mode::Insert).0, 8);
        assert_eq!(end_of_line(&ed, &mut aff, 6, Mode::Insert).0, 8);
        assert_eq!(end_of_line(&ed, &mut aff, 7, Mode::Insert).0, 8);
        assert_eq!(end_of_line(&ed, &mut aff, 8, Mode::Insert).0, 8);

        let ed = make_ed("testing\r\nAbout\r\nblah");
        let mut aff = CursorAffinity::Backward;
        assert_eq!(end_of_line(&ed, &mut aff, 0, Mode::Insert).0, 7);
    }

    #[test]
    fn test_move_down() {
        let ed = make_ed("abc\n\n\ndef\n\nghi");

        let mut aff = CursorAffinity::Forward;

        assert_eq!(move_down(&ed, 0, &mut aff, None, Mode::Insert, 1).0, 4);

        let (offset, horiz) = move_down(&ed, 1, &mut aff, None, Mode::Insert, 1);
        assert_eq!(offset, 4);
        assert!(matches!(horiz, ColPosition::Col(_)));
        let (offset, horiz) = move_down(&ed, 4, &mut aff, Some(horiz), Mode::Insert, 1);
        assert_eq!(offset, 5);
        assert!(matches!(horiz, ColPosition::Col(_)));
        let (offset, _) = move_down(&ed, 5, &mut aff, Some(horiz), Mode::Insert, 1);
        // Moving down with a horiz starting from position 1 on first line will put cursor at
        // (approximately) position 1 on the next line with content they arrive at
        assert_eq!(offset, 7);
    }

    #[test]
    fn test_move_up() {
        let ed = make_ed("abc\n\n\ndef\n\nghi");

        let mut aff = CursorAffinity::Forward;

        assert_eq!(move_up(&ed, 0, &mut aff, None, Mode::Insert, 1).0, 0);

        let (offset, horiz) = move_up(&ed, 7, &mut aff, None, Mode::Insert, 1);
        assert_eq!(offset, 5);
        assert!(matches!(horiz, ColPosition::Col(_)));
        let (offset, horiz) = move_up(&ed, 5, &mut aff, Some(horiz), Mode::Insert, 1);
        assert_eq!(offset, 4);
        assert!(matches!(horiz, ColPosition::Col(_)));
        let (offset, _) = move_up(&ed, 4, &mut aff, Some(horiz), Mode::Insert, 1);
        // Moving up with a horiz starting from position 1 on first line will put cursor at
        // (approximately) position 1 on the next line with content they arrive at
        assert_eq!(offset, 1);
    }
}
