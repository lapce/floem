use std::{cell::RefCell, rc::Rc};

use peniko::kurbo::{Point, Rect};

use crate::{
    ViewId,
    context::Phases,
    custom_event,
    event::Event,
    style::{NoWrapOverflow, TextOverflow},
    view::{FinalizeFn, MeasureFn},
};

use super::{Alignment, Attrs, AttrsList, Cursor, TextLayout, TextWrapMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AvailableLayoutKind {
    Aligned,
    Wrapped,
    Ellipsis { byte_end: usize },
}

/// Event fired when a text view's overflow state changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextOverflowChanged {
    /// Whether the text is currently overflowing its bounds
    pub is_overflowing: bool,
}
custom_event!(TextOverflowChanged);

/// Shared text layout state used by text-based views.
#[derive(Clone)]
pub struct TextLayoutData {
    text_layout: Option<TextLayout>,
    available_width: Option<f32>,
    available_text_layout: Option<TextLayout>,
    available_layout_kind: Option<AvailableLayoutKind>,
    attrs_list: AttrsList,
    text_align: Option<Alignment>,
    text_overflow: TextOverflow,
    last_overflow_state: Option<bool>,
    view_id: Option<ViewId>,
}

impl TextLayoutData {
    const SELECTION_X_PAD: f64 = 0.35;

    pub fn new(view_id: Option<ViewId>) -> Self {
        Self {
            text_layout: None,
            available_width: None,
            available_text_layout: None,
            available_layout_kind: None,
            attrs_list: AttrsList::new(Attrs::new()),
            text_align: None,
            text_overflow: TextOverflow::NoWrap(NoWrapOverflow::Clip),
            last_overflow_state: None,
            view_id,
        }
    }

    pub fn set_text(&mut self, text: &str, attrs_list: AttrsList, text_align: Option<Alignment>) {
        self.attrs_list = attrs_list.clone();
        self.text_align = text_align;

        let mut text_layout = TextLayout::new();
        self.apply_text_overflow(&mut text_layout);
        text_layout.set_text(text, attrs_list, text_align);
        self.text_layout = Some(text_layout);
        self.clear_overflow_state();
    }

    pub fn set_text_overflow(&mut self, text_overflow: TextOverflow) {
        if self.text_overflow != text_overflow {
            self.text_overflow = text_overflow;
            if let Some(current_text) = self.text().map(str::to_owned) {
                let mut text_layout = TextLayout::new();
                self.apply_text_overflow(&mut text_layout);
                text_layout.set_text(&current_text, self.attrs_list.clone(), self.text_align);
                self.text_layout = Some(text_layout);
            }
            self.clear_overflow_state();
        }
    }

    pub fn text(&self) -> Option<&str> {
        self.text_layout.as_ref().map(TextLayout::text)
    }

    pub fn get_effective_text_layout(&self) -> Option<&TextLayout> {
        self.available_text_layout
            .as_ref()
            .or(self.text_layout.as_ref())
    }

    pub fn with_effective_text_layout<O>(&self, with: impl FnOnce(&TextLayout) -> O) -> O {
        if let Some(layout) = self.available_text_layout.as_ref() {
            with(layout)
        } else if let Some(layout) = self.text_layout.as_ref() {
            with(layout)
        } else {
            let layout = TextLayout::new();
            with(&layout)
        }
    }

    fn build_layout(&self, text: &str) -> TextLayout {
        let mut layout = TextLayout::new();
        self.apply_text_overflow(&mut layout);
        layout.set_text(text, self.attrs_list.clone(), self.text_align);
        layout
    }

    fn apply_text_overflow(&self, layout: &mut TextLayout) {
        match self.text_overflow {
            TextOverflow::NoWrap(_) => layout.set_text_wrap_mode(TextWrapMode::NoWrap),
            TextOverflow::Wrap { overflow_wrap, .. } => {
                layout.set_text_wrap_mode(TextWrapMode::Wrap);
                layout.set_overflow_wrap(overflow_wrap);
            }
        }
    }

    fn ellipsis_layout(&self) -> TextLayout {
        self.build_layout("...")
    }

    fn ellipsis_text(base_text: &str, byte_end: usize) -> String {
        if byte_end == 0 {
            String::new()
        } else {
            format!("{}...", &base_text[..byte_end])
        }
    }

    pub fn selection_rects_for_cursors(
        &self,
        start: &Cursor,
        end: &Cursor,
        text_origin: Point,
        mut f: impl FnMut(Rect),
    ) {
        self.with_effective_text_layout(|layout| {
            layout.selection_for_cursors_with_line_metrics(start, end, |x0, y0, x1, y1| {
                let rect = Rect::new(
                    x0 + text_origin.x - Self::SELECTION_X_PAD,
                    y0 + text_origin.y,
                    x1 + text_origin.x + Self::SELECTION_X_PAD,
                    y1 + text_origin.y,
                );
                if rect.width() > 0.0 && rect.height() > 0.0 {
                    f(rect);
                }
            });
        });
    }

    pub fn selection_rects_for_byte_range(
        &self,
        start: usize,
        end: usize,
        text_origin: Point,
        mut f: impl FnMut(Rect),
    ) {
        self.with_effective_text_layout(|layout| {
            layout.selection_geometry_with_line_metrics(start, end, |x0, y0, x1, y1| {
                let rect = Rect::new(
                    x0 + text_origin.x - Self::SELECTION_X_PAD,
                    y0 + text_origin.y,
                    x1 + text_origin.x + Self::SELECTION_X_PAD,
                    y1 + text_origin.y,
                );
                if rect.width() > 0.0 && rect.height() > 0.0 {
                    f(rect);
                }
            });
        });
    }

    pub fn centered_text_origin(&self, content_rect: Rect) -> Point {
        let mut origin = content_rect.origin();
        self.with_effective_text_layout(|layout| {
            let (min_y, max_y) = layout
                .centering_bounds_y()
                .map(|(min_y, max_y)| (min_y as f64, max_y as f64))
                .unwrap_or((0.0, layout.size().height));
            let text_height = (max_y - min_y).max(0.0);
            let y_offset = ((content_rect.height() - text_height).max(0.0)) * 0.5 - min_y;
            origin.y += y_offset;
        });
        origin
    }

    pub fn clear_overflow_state(&mut self) {
        self.available_width = None;
        self.available_text_layout = None;
        self.available_layout_kind = None;
    }

    pub fn get_text_layout(&self) -> Option<&TextLayout> {
        self.text_layout.as_ref()
    }

    pub fn compute_overflow_size(
        &mut self,
        width_constraint: Option<f32>,
        text_overflow: TextOverflow,
    ) -> peniko::kurbo::Size {
        let Some(text_layout) = self.text_layout.as_ref() else {
            return peniko::kurbo::Size::new(0.0, 14.0);
        };

        let Some(available_width) = width_constraint else {
            return text_layout.size();
        };

        match text_overflow {
            TextOverflow::NoWrap(NoWrapOverflow::Ellipsis) => {
                let dots_width = self.ellipsis_layout().size().width as f32;
                let width_left = available_width - dots_width;
                let byte_end = text_layout
                    .hit_test(Point::new(width_left as f64, 0.0))
                    .map(|cursor| text_layout.cursor_to_byte_index(&cursor))
                    .unwrap_or(0);
                self.build_layout(&Self::ellipsis_text(text_layout.text(), byte_end))
                    .size()
            }
            TextOverflow::Wrap { .. } => {
                let mut layout = text_layout.clone();
                layout.set_size(available_width, f32::MAX);
                layout.size()
            }
            _ => peniko::kurbo::Size::new(available_width as f64, text_layout.size().height),
        }
    }

    pub fn finalize_for_width(&mut self, final_width: f32) {
        let Some(text_layout) = self.text_layout.as_ref() else {
            return;
        };

        let natural_width = text_layout.size().width as f32;
        let overflows = natural_width > final_width;
        let overflow_changed = self.last_overflow_state != Some(overflows);
        self.last_overflow_state = Some(overflows);

        if !overflows {
            if self.text_align.is_some() {
                let needs_rebuild = self.available_width != Some(final_width)
                    || self.available_text_layout.is_none()
                    || self.available_layout_kind != Some(AvailableLayoutKind::Aligned);
                if needs_rebuild {
                    let mut layout = text_layout.clone();
                    layout.set_size(final_width, f32::MAX);
                    self.available_text_layout = Some(layout);
                    self.available_layout_kind = Some(AvailableLayoutKind::Aligned);
                    self.available_width = Some(final_width);
                }
            } else {
                self.clear_overflow_state();
            }
        } else {
            if self.available_width == Some(final_width) {
                return;
            }

            match self.text_overflow {
                TextOverflow::NoWrap(NoWrapOverflow::Ellipsis) => {
                    let dots_width = self.ellipsis_layout().size().width as f32;
                    let width_left = final_width - dots_width;
                    let byte_end = text_layout
                        .hit_test(Point::new(width_left as f64, 0.0))
                        .map(|cursor| text_layout.cursor_to_byte_index(&cursor))
                        .unwrap_or(0);
                    let next_kind = AvailableLayoutKind::Ellipsis { byte_end };
                    if self.available_layout_kind != Some(next_kind) {
                        let new_text = Self::ellipsis_text(text_layout.text(), byte_end);
                        let layout = self.build_layout(&new_text);
                        self.available_text_layout = Some(layout);
                        self.available_layout_kind = Some(next_kind);
                    }
                    self.available_width = Some(final_width);
                }
                TextOverflow::Wrap { .. } => {
                    let mut layout = text_layout.clone();
                    layout.set_size(final_width, f32::MAX);
                    self.available_text_layout = Some(layout);
                    self.available_layout_kind = Some(AvailableLayoutKind::Wrapped);
                    self.available_width = Some(final_width);
                }
                _ => {
                    self.clear_overflow_state();
                }
            }
        }

        if overflow_changed && let Some(id) = self.view_id {
            id.route_event(
                Event::new_custom(TextOverflowChanged {
                    is_overflowing: overflows,
                }),
                crate::event::RouteKind::Directed {
                    target: id.get_element_id(),
                    phases: Phases::TARGET,
                },
            );
        }
    }

    pub fn create_taffy_layout_fn(layout_data: Rc<RefCell<Self>>) -> Box<MeasureFn> {
        Box::new(
            move |known_dimensions, available_space, node_id, _style, measure_ctx| {
                use taffy::*;

                measure_ctx.needs_finalization(node_id);

                let (has_text_layout, text_overflow) = {
                    let layout_data = layout_data.borrow();
                    let has_text = layout_data.text_layout.is_some();
                    (has_text, layout_data.text_overflow)
                };

                if !has_text_layout {
                    return Size {
                        width: known_dimensions.width.unwrap_or(0.0),
                        height: known_dimensions.height.unwrap_or(14.0),
                    };
                }

                let width_constraint: Option<f32> =
                    known_dimensions.width.or(match available_space.width {
                        AvailableSpace::Definite(w) => Some(w),
                        AvailableSpace::MinContent => match text_overflow {
                            TextOverflow::Wrap { .. } => Some(5.),
                            TextOverflow::NoWrap(NoWrapOverflow::Ellipsis) => Some(5.),
                            TextOverflow::NoWrap(NoWrapOverflow::Clip) => None,
                        },
                        AvailableSpace::MaxContent => None,
                    });

                let text_size = {
                    let mut layout_data = layout_data.borrow_mut();
                    layout_data.compute_overflow_size(width_constraint, text_overflow)
                };

                Size {
                    width: known_dimensions.width.unwrap_or(text_size.width as f32) + 1.,
                    height: known_dimensions.height.unwrap_or(text_size.height as f32),
                }
            },
        )
    }

    pub fn create_finalize_fn(layout_data: Rc<RefCell<Self>>) -> Box<FinalizeFn> {
        Box::new(move |_node_id, layout| {
            let mut layout_data = layout_data.borrow_mut();
            layout_data.finalize_for_width(layout.content_box_width());
        })
    }
}
