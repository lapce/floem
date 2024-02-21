use std::any::Any;

use floem_reactive::create_effect;
use floem_renderer::{cosmic_text::TextLayout, Renderer};
use kurbo::{Point, Rect};
use taffy::tree::NodeId;

use crate::{
    context::UpdateCx,
    id::Id,
    style::{Style, TextOverflow},
    unit::PxPct,
    view::{View, ViewData, Widget},
};

pub struct RichText {
    data: ViewData,
    text_layout: TextLayout,
    text_node: Option<NodeId>,
    text_overflow: TextOverflow,
    available_width: f32,
}

pub fn rich_text(text_layout: impl Fn() -> TextLayout + 'static) -> RichText {
    let id = Id::next();
    let text = text_layout();
    create_effect(move |_| {
        let new_text_layout = text_layout();
        id.update_state(new_text_layout);
    });
    RichText {
        data: ViewData::new(id),
        text_layout: text,
        text_node: None,
        text_overflow: TextOverflow::Wrap,
        available_width: 0.0,
    }
}

impl View for RichText {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn build(self) -> Box<dyn Widget> {
        Box::new(self)
    }
}

impl Widget for RichText {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
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
            let mut text_layout: TextLayout = *state;
            if self.text_overflow == TextOverflow::Wrap && self.available_width > 0.0 {
                text_layout.set_size(self.available_width, f32::MAX);
            }

            self.text_layout = text_layout;
            cx.request_layout(self.id());
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::tree::NodeId {
        cx.layout_node(self.id(), true, |cx| {
            let size = self.text_layout.size();
            let width = size.width as f32;
            let height = size.height as f32;

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
        let layout = cx.get_layout(self.id()).unwrap();
        let style = cx.app_state_mut().get_builtin_style(self.id());
        let padding_left = match style.padding_left() {
            PxPct::Px(padding) => padding as f32,
            PxPct::Pct(pct) => pct as f32 * layout.size.width,
        };
        let padding_right = match style.padding_right() {
            PxPct::Px(padding) => padding as f32,
            PxPct::Pct(pct) => pct as f32 * layout.size.width,
        };
        let padding = padding_left + padding_right;
        let available_width = layout.size.width - padding;
        self.text_overflow = style.text_overflow();
        if self.text_overflow == TextOverflow::Wrap && self.available_width != available_width {
            self.available_width = available_width;
            self.text_layout.set_size(self.available_width, f32::MAX);
            cx.app_state_mut().request_layout(self.id());
        }

        None
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let text_node = self.text_node.unwrap();
        let location = cx.app_state.taffy.layout(text_node).unwrap().location;
        let point = Point::new(location.x as f64, location.y as f64);
        cx.draw_text(&self.text_layout, point);
    }
}
