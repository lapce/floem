use std::any::Any;

use floem_reactive::create_effect;
use floem_renderer::{cosmic_text::TextLayout, Renderer};
use kurbo::{Point, Rect};
use taffy::tree::NodeId;

use crate::{
    context::UpdateCx,
    style::{Style, TextOverflow},
    unit::PxPct,
    view::View,
    view_storage::ViewId,
};

pub struct RichText {
    id: ViewId,
    text_layout: TextLayout,
    text_node: Option<NodeId>,
    text_overflow: TextOverflow,
    available_width: Option<f32>,
    available_text_layout: Option<TextLayout>,
}

pub fn rich_text(text_layout: impl Fn() -> TextLayout + 'static) -> RichText {
    let id = ViewId::new();
    let text = text_layout();
    create_effect(move |_| {
        let new_text_layout = text_layout();
        id.update_state(new_text_layout);
    });
    RichText {
        id,
        text_layout: text,
        text_node: None,
        text_overflow: TextOverflow::Wrap,
        available_width: None,
        available_text_layout: None,
    }
}

impl View for RichText {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        format!(
            "RichText: {:?}",
            self.text_layout
                .lines
                .iter()
                .map(|text| text.text())
                .collect::<String>()
        )
        .into()
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) {
        if let Ok(state) = state.downcast() {
            self.text_layout = *state;
            self.available_width = None;
            self.available_text_layout = None;
            self.id.request_layout();
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::tree::NodeId {
        cx.layout_node(self.id(), true, |cx| {
            let size = self.text_layout.size();
            let width = size.width as f32;
            let mut height = size.height as f32;

            if let Some(t) = self.available_text_layout.as_ref() {
                height = height.max(t.size().height as f32);
            }

            if self.text_node.is_none() {
                self.text_node = Some(
                    cx.app_state_mut()
                        .taffy
                        .new_leaf(taffy::style::Style::DEFAULT)
                        .unwrap(),
                );
            }
            let text_node = self.text_node.unwrap();

            let style = Style::new().width(width).height(height).to_taffy_style();
            let _ = cx.app_state_mut().taffy.set_style(text_node, style);
            vec![text_node]
        })
    }

    fn compute_layout(&mut self, cx: &mut crate::context::ComputeLayoutCx) -> Option<Rect> {
        let layout = self.id.get_layout().unwrap_or_default();
        let view_state = self.id.state();
        let style = view_state.borrow().combined_style.builtin();
        let padding_left = match style.padding_left() {
            PxPct::Px(padding) => padding as f32,
            PxPct::Pct(pct) => pct as f32 * layout.size.width,
        };
        let padding_right = match style.padding_right() {
            PxPct::Px(padding) => padding as f32,
            PxPct::Pct(pct) => pct as f32 * layout.size.width,
        };

        self.text_overflow = style.text_overflow();

        let padding = padding_left + padding_right;
        let width = self.text_layout.size().width as f32;
        let available_width = layout.size.width - padding;
        if self.text_overflow == TextOverflow::Wrap {
            if width > available_width {
                if self.available_width != Some(available_width) {
                    let mut text_layout = self.text_layout.clone();
                    text_layout.set_size(available_width, f32::MAX);
                    self.available_text_layout = Some(text_layout);
                    self.available_width = Some(available_width);
                    self.id.request_layout();
                }
            } else {
                if self.available_text_layout.is_some() {
                    self.id.request_layout();
                }
                self.available_text_layout = None;
                self.available_width = None;
            }
        }

        None
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let text_node = self.text_node.unwrap();
        let location = cx.app_state.taffy.layout(text_node).unwrap().location;
        let point = Point::new(location.x as f64, location.y as f64);
        if let Some(text_layout) = self.available_text_layout.as_ref() {
            cx.draw_text(text_layout, point);
        } else {
            cx.draw_text(&self.text_layout, point);
        }
    }
}
