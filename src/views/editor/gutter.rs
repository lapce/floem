use crate::{
    context::PaintCx,
    cosmic_text::{Attrs, AttrsList, TextLayout},
    id::ViewId,
    peniko::kurbo::Point,
    prop, prop_extractor,
    style::{Style, TextColor},
    style_class,
    view::View,
    views::Decorators,
    Renderer,
};
use floem_editor_core::{cursor::CursorMode, mode::Mode};
use floem_reactive::RwSignal;
use peniko::kurbo::Rect;
use peniko::Color;

use super::{CurrentLineColor, Editor};

prop!(pub LeftOfCenterPadding: f64 {} = 25.);
prop!(pub RightOfCenterPadding: f64 {} = 30.);
prop!(pub DimColor: Option<Color> {} = None);

prop_extractor! {
    GutterStyle {
        accent_color: TextColor,
        dim_color: DimColor,
        left_padding: LeftOfCenterPadding,
        right_padding: RightOfCenterPadding,
        current_line_color: CurrentLineColor,
    }
}
impl GutterStyle {
    fn gs_accent_color(&self) -> Color {
        self.accent_color().unwrap_or(Color::BLACK)
    }

    fn gs_dim_color(&self) -> Color {
        self.dim_color().unwrap_or(self.gs_accent_color())
    }
}

pub struct EditorGutterView {
    id: ViewId,
    editor: RwSignal<Editor>,
    full_width: f64,
    text_width: f64,
    gutter_style: GutterStyle,
}

style_class!(pub GutterClass);

pub fn editor_gutter_view(editor: RwSignal<Editor>) -> EditorGutterView {
    let id = ViewId::new();

    EditorGutterView {
        id,
        editor,
        full_width: 0.0,
        text_width: 0.0,
        gutter_style: Default::default(),
    }
    .class(GutterClass)
}

impl View for EditorGutterView {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Editor Gutter View".into()
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.gutter_style.read(cx) {
            cx.app_state_mut().request_paint(self.id());
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::NodeId {
        cx.layout_node(self.id(), true, |_cx| {
            let (width, height) = (self.text_width, 10.0);
            let layout_node = self
                .id
                .taffy()
                .borrow_mut()
                .new_leaf(taffy::style::Style::DEFAULT)
                .unwrap();

            let style = Style::new()
                .width(self.gutter_style.left_padding() + width + self.gutter_style.right_padding())
                .height(height)
                .to_taffy_style();
            let _ = self.id.taffy().borrow_mut().set_style(layout_node, style);
            vec![layout_node]
        })
    }

    fn compute_layout(&mut self, _cx: &mut crate::context::ComputeLayoutCx) -> Option<Rect> {
        if let Some(width) = self.id.get_layout().map(|l| l.size.width as f64) {
            self.full_width = width;
        }

        let editor = self.editor.get_untracked();
        let edid = editor.id();
        let style = editor.style();
        // TODO: don't assume font family is constant for each line
        let family = style.font_family(edid, 0);
        let attrs = Attrs::new()
            .family(&family)
            .font_size(style.font_size(edid, 0) as f32);

        let attrs_list = AttrsList::new(attrs);

        let widest_text_width = self.compute_widest_text_width(&attrs_list);
        if (self.full_width
            - widest_text_width
            - self.gutter_style.left_padding()
            - self.gutter_style.right_padding())
        .abs()
            > 1e-2
        {
            self.text_width = widest_text_width;
            self.id.request_layout();
        }
        None
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let editor = self.editor.get_untracked();
        let edid = editor.id();

        let viewport = editor.viewport.get_untracked();
        let cursor = editor.cursor;
        let style = editor.style.get_untracked();

        let (offset, mode) = cursor.with_untracked(|c| (c.offset(), c.get_mode()));
        let last_line = editor.last_line();
        let current_line = editor.line_of_offset(offset);

        // TODO: don't assume font family is constant for each line
        let family = style.font_family(edid, 0);
        let accent_color = self.gutter_style.gs_accent_color();
        let dim_color = self.gutter_style.gs_dim_color();
        let attrs = Attrs::new()
            .family(&family)
            .color(dim_color)
            .font_size(style.font_size(edid, 0) as f32);
        let attrs_list = AttrsList::new(attrs);
        let current_line_attrs_list = AttrsList::new(attrs.color(accent_color));
        let show_relative = editor.es.with_untracked(|es| es.modal())
            && editor.es.with_untracked(|es| es.modal_relative_line())
            && mode != Mode::Insert;

        self.text_width = self.compute_widest_text_width(&attrs_list);

        editor.screen_lines.with_untracked(|screen_lines| {
            for (line, y) in screen_lines.iter_lines_y() {
                // If it ends up outside the bounds of the file, stop trying to display line numbers
                if line > last_line {
                    break;
                }

                let line_height = f64::from(style.line_height(edid, line));

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
                    if let Some(current_line_color) = self.gutter_style.current_line_color() {
                        cursor.with_untracked(|cursor| {
                            let highlight_current_line = match cursor.mode {
                                // TODO: check if shis should be 0 or 1
                                CursorMode::Normal(size) => size == 0,
                                CursorMode::Insert(ref sel) => sel.is_caret(),
                                CursorMode::Visual { .. } => false,
                            };

                            // Highlight the current line
                            if highlight_current_line {
                                for (_, end) in cursor.regions_iter() {
                                    // TODO: unsure if this is correct for wrapping lines
                                    let rvline = editor.rvline_of_offset(end, cursor.affinity);

                                    if let Some(info) = screen_lines.info(rvline) {
                                        let line_height =
                                            editor.line_height(info.vline_info.rvline.line);
                                        // the extra 1px is for a small line that appears between
                                        let rect = Rect::from_origin_size(
                                            (viewport.x0, info.vline_y - viewport.y0),
                                            (self.full_width + 1.1, f64::from(line_height)),
                                        );

                                        cx.fill(&rect, current_line_color, 0.0);
                                    }
                                }
                            }
                        })
                    }
                } else {
                    text_layout.set_text(&text, attrs_list.clone());
                }
                let size = text_layout.size();
                let height = size.height;

                let pos = Point::new(
                    (self.full_width - (size.width) - self.gutter_style.right_padding()).max(0.0),
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
