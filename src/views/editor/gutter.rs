use std::{cell::RefCell, rc::Rc};

use crate::{
    Renderer,
    context::PaintCx,
    peniko::kurbo::Point,
    prop, prop_extractor,
    style::TextColor,
    style_class,
    text::{Attrs, AttrsList, TextLayout},
    view::{LayoutNodeCx, MeasureFn, View, ViewId},
    views::Decorators,
};
use floem_editor_core::{cursor::CursorMode, mode::Mode};
use floem_reactive::{RwSignal, SignalGet, SignalWith};
use peniko::Color;
use peniko::color::palette;
use peniko::kurbo::Rect;

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
        self.accent_color().unwrap_or(palette::css::BLACK)
    }

    fn gs_dim_color(&self) -> Color {
        self.dim_color().unwrap_or(self.gs_accent_color())
    }
}

pub struct EditorGutterView {
    id: ViewId,
    editor: RwSignal<Editor>,
    full_width: Rc<RefCell<f64>>,
    text_width: f64,
    gutter_style: GutterStyle,
    layout_node: Option<taffy::NodeId>,
}

style_class!(pub GutterClass);

pub fn editor_gutter_view(editor: RwSignal<Editor>) -> EditorGutterView {
    let id = ViewId::new();

    let mut gutter = EditorGutterView {
        id,
        editor,
        full_width: Rc::new(RefCell::new(0.)),
        text_width: 0.0,
        gutter_style: Default::default(),
        layout_node: None,
    }
    .class(GutterClass);
    gutter.set_taffy_layout();
    gutter
}

impl View for EditorGutterView {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Editor Gutter View".into()
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        let mut transitioning = false;
        if self.gutter_style.read(cx, &mut transitioning) {
            cx.window_state.request_paint(self.id());
        }
        if transitioning {
            cx.request_transition();
        }
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
        let attrs_list = AttrsList::new(attrs.clone());
        let current_line_attrs_list = AttrsList::new(attrs.color(accent_color));
        let show_relative = editor.es.with_untracked(|es| es.modal())
            && editor.es.with_untracked(|es| es.modal_relative_line())
            && mode != Mode::Insert;

        self.text_width = Self::compute_widest_text_width(self.editor, &attrs_list);

        editor.screen_lines.with_untracked(|screen_lines| {
            if let Some(current_line_color) = self.gutter_style.current_line_color() {
                cursor.with_untracked(|cursor| {
                    let highlight_current_line = match cursor.mode {
                        // TODO: check if shis should be 0 or 1
                        CursorMode::Normal { offset: size, .. } => size == 0,
                        CursorMode::Insert(ref sel) => sel.is_caret(),
                        CursorMode::Visual { .. } => false,
                    };

                    // Highlight the current line
                    if highlight_current_line {
                        for (_, end, affinity) in cursor.regions_iter() {
                            // TODO: unsure if this is correct for wrapping lines
                            let rvline = editor.rvline_of_offset(end, affinity);

                            if let Some(info) = screen_lines.info(rvline) {
                                let line_height = editor.line_height(info.vline_info.rvline.line);
                                // the extra 1px is for a small line that appears between
                                let rect = Rect::from_origin_size(
                                    (viewport.x0, info.vline_y - viewport.y0),
                                    (*self.full_width.borrow() + 1.1, f64::from(line_height)),
                                );

                                cx.fill(&rect, current_line_color, 0.0);
                            }
                        }
                    }
                })
            }

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
                    text_layout.set_text(&text, current_line_attrs_list.clone(), None);
                } else {
                    text_layout.set_text(&text, attrs_list.clone(), None);
                }
                let size = text_layout.size();
                let height = size.height;

                let pos = Point::new(
                    (*self.full_width.borrow() - (size.width) - self.gutter_style.right_padding())
                        .max(0.0),
                    y + (line_height - height) / 2.0 - viewport.y0,
                );

                text_layout.draw(cx, pos);
            }
        });
    }
}

impl EditorGutterView {
    fn set_taffy_layout(&mut self) {
        let taffy_node = self.id.taffy_node();
        let taffy = self.id.taffy();
        let mut taffy = taffy.borrow_mut();

        let gutter_node = taffy.new_leaf(taffy::Style::DEFAULT).unwrap();

        let editor_sig = self.editor;
        let gutter_style = self.gutter_style.clone();

        let layout_fn: Box<MeasureFn> = Box::new(
            move |known_dimensions, _available_space, node_id, _style, measure_ctx| {
                use taffy::*;

                measure_ctx.needs_finalization(node_id);

                let editor = editor_sig.get_untracked();
                let edid = editor.id();
                let style = editor.style();

                // Get font attrs for measuring
                let family = style.font_family(edid, 0);
                let attrs = Attrs::new()
                    .family(&family)
                    .font_size(style.font_size(edid, 0) as f32);
                let attrs_list = AttrsList::new(attrs);

                // Compute the width of gutter text content
                let text_width = Self::compute_widest_text_width(editor_sig, &attrs_list);

                let width = match known_dimensions.width {
                    Some(w) => w,
                    None => {
                        let total_width =
                            gutter_style.left_padding() + text_width + gutter_style.right_padding();
                        total_width as f32
                    }
                };

                // Height is determined by editor content
                let line_height = f64::from(editor.line_height(0));
                let last_line_height = line_height * (editor.last_vline().get() + 1) as f64;
                let margin_bottom = if editor.es.with_untracked(|es| es.scroll_beyond_last_line()) {
                    let parent_size = editor.parent_size.get_untracked();
                    parent_size.height().min(last_line_height) - line_height
                } else {
                    0.0
                };
                let height = (last_line_height + margin_bottom) as f32;

                Size {
                    width,
                    height: known_dimensions.height.unwrap_or(height),
                }
            },
        );

        let full_width = self.full_width.clone();
        let finalize_fn = Box::new(move |_node_id, layout: &taffy::Layout| {
            *full_width.borrow_mut() = layout.size.width as f64;
        });

        self.layout_node = Some(gutter_node);

        taffy
            .set_node_context(
                gutter_node,
                Some(LayoutNodeCx::Custom {
                    measure: layout_fn,
                    finalize: Some(finalize_fn),
                }),
            )
            .unwrap();

        taffy.set_children(taffy_node, &[gutter_node]).unwrap();
    }

    fn compute_widest_text_width(editor: RwSignal<Editor>, attrs_list: &AttrsList) -> f64 {
        let last_line = editor.get_untracked().last_line() + 1;
        let mut text = TextLayout::new();
        text.set_text(&last_line.to_string(), attrs_list.clone(), None);
        text.size().width
    }
}
