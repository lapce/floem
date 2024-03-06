use crate::{
    context::PaintCx,
    cosmic_text::{Attrs, AttrsList, TextLayout},
    id::Id,
    peniko::kurbo::Point,
    style::Style,
    view::{AnyWidget, View, ViewData, Widget},
    Renderer,
};
use floem_editor_core::mode::Mode;
use floem_reactive::RwSignal;
use kurbo::Rect;

use super::{color::EditorColor, Editor};

pub struct EditorGutterView {
    data: ViewData,
    editor: RwSignal<Editor>,
    full_width: f64,
    text_width: f64,
    padding_left: f64,
    padding_right: f64,
}

pub fn editor_gutter_view(editor: RwSignal<Editor>) -> EditorGutterView {
    let id = Id::next();

    EditorGutterView {
        data: ViewData::new(id),
        editor,
        full_width: 0.0,
        text_width: 0.0,
        // TODO: these are probably tuned for lapce?
        padding_left: 25.0,
        padding_right: 30.0,
    }
}

impl View for EditorGutterView {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn build(self) -> AnyWidget {
        Box::new(self)
    }
}
impl Widget for EditorGutterView {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::NodeId {
        cx.layout_node(self.id(), true, |cx| {
            let (width, height) = (self.text_width, 10.0);
            let layout_node = cx
                .app_state_mut()
                .taffy
                .new_leaf(taffy::style::Style::DEFAULT)
                .unwrap();

            let style = Style::new()
                .width(self.padding_left + width + self.padding_right)
                .height(height)
                .to_taffy_style();
            let _ = cx.app_state_mut().taffy.set_style(layout_node, style);
            vec![layout_node]
        })
    }
    fn compute_layout(&mut self, cx: &mut crate::context::ComputeLayoutCx) -> Option<Rect> {
        if let Some(width) = cx.get_layout(self.data.id()).map(|l| l.size.width as f64) {
            self.full_width = width;
        }

        let style = self.editor.get_untracked().style.get_untracked();
        // TODO: don't assume font family is constant for each line
        let family = style.font_family(0);
        let attrs = Attrs::new()
            .family(&family)
            .color(style.color(EditorColor::Dim))
            .font_size(style.font_size(0) as f32);

        let attrs_list = AttrsList::new(attrs);

        let widest_text_width = self.compute_widest_text_width(&attrs_list);
        if (self.full_width - widest_text_width - self.padding_left - self.padding_right).abs()
            > 1e-2
        {
            self.text_width = widest_text_width;
            cx.app_state_mut().request_layout(self.id());
        }
        None
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let editor = self.editor.get_untracked();

        let viewport = editor.viewport.get_untracked();
        let cursor = editor.cursor;
        let style = editor.style.get_untracked();

        let (offset, mode) = cursor.with_untracked(|c| (c.offset(), c.get_mode()));
        let last_line = editor.last_line();
        let current_line = editor.line_of_offset(offset);

        // TODO: don't assume font family is constant for each line
        let family = style.font_family(0);
        let attrs = Attrs::new()
            .family(&family)
            .color(style.color(EditorColor::Dim))
            .font_size(style.font_size(0) as f32);
        let attrs_list = AttrsList::new(attrs);
        let current_line_attrs_list =
            AttrsList::new(attrs.color(style.color(EditorColor::Foreground)));
        let show_relative = editor.modal.get_untracked()
            && editor.modal_relative_line_numbers.get_untracked()
            && mode != Mode::Insert;

        self.text_width = self.compute_widest_text_width(&attrs_list);

        editor.screen_lines.with_untracked(|screen_lines| {
            for (line, y) in screen_lines.iter_lines_y() {
                // If it ends up outside the bounds of the file, stop trying to display line numbers
                if line > last_line {
                    break;
                }

                let line_height = f64::from(style.line_height(line));

                let text = if show_relative {
                    if line == current_line {
                        line + 1
                    } else {
                        line.abs_diff(current_line)
                    }
                } else {
                    line + 1
                }
                .to_string();

                let mut text_layout = TextLayout::new();
                if line == current_line {
                    text_layout.set_text(&text, current_line_attrs_list.clone());
                } else {
                    text_layout.set_text(&text, attrs_list.clone());
                }
                let size = text_layout.size();
                let height = size.height;

                let pos = Point::new(
                    (self.full_width - (size.width) - self.padding_right).max(0.0),
                    y + (line_height - height) / 2.0 - viewport.y0,
                );

                cx.draw_text(&text_layout, pos);
            }
        });
    }
}

impl EditorGutterView {
    fn compute_widest_text_width(&mut self, attrs_list: &AttrsList) -> f64 {
        let last_line = self.editor.get_untracked().last_line() + 1;
        let mut text = TextLayout::new();
        text.set_text(&last_line.to_string(), attrs_list.clone());
        text.size().width
    }
}
