use floem_reactive::create_effect;
use floem_renderer::{
    usvg::Tree,
    usvg::{self, TreeParsing},
    Renderer,
};
use kurbo::Size;
use peniko::Color;
use sha2::{Digest, Sha256};

use crate::{
    id::Id,
    view::{ChangeFlags, View},
    views::Decorators,
};

pub struct Svg {
    id: Id,
    svg_tree: Option<Tree>,
    svg_hash: Option<Vec<u8>>,
}

pub fn svg(svg_str: impl Fn() -> String + 'static) -> Svg {
    let id = Id::next();
    create_effect(move |_| {
        let new_svg_str = svg_str();
        id.update_state(new_svg_str, false);
    });
    Svg {
        id,
        svg_tree: None,
        svg_hash: None,
    }
}

/// Renders a checkbox using an svg and the provided checked signal.
/// Can be combined with a label and a stack with a click event (as in `examples/widget-gallery`).
pub fn checkbox(checked: crate::reactive::ReadSignal<bool>) -> Svg {
    const CHECKBOX_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-2 -2 16 16"><polygon points="5.19,11.83 0.18,7.44 1.82,5.56 4.81,8.17 10,1.25 12,2.75" /></svg>"#;
    let svg_str = move || if checked.get() { CHECKBOX_SVG } else { "" }.to_string();

    svg(svg_str)
        .style(|base| {
            base.width(20.)
                .height(20.)
                .border_color(Color::BLACK)
                .border(1.)
                .border_radius(5.)
                .margin_right(5.)
        })
        .keyboard_navigatable()
}

impl View for Svg {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&self, _id: Id) -> Option<&dyn View> {
        None
    }

    fn child_mut(&mut self, _id: Id) -> Option<&mut dyn View> {
        None
    }

    fn children(&self) -> Vec<&dyn View> {
        Vec::new()
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        Vec::new()
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Svg".into()
    }

    fn update(
        &mut self,
        cx: &mut crate::context::UpdateCx,
        state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        if let Ok(state) = state.downcast::<String>() {
            let text = &*state;
            self.svg_tree = Tree::from_str(text, &usvg::Options::default()).ok();

            let mut hasher = Sha256::new();
            hasher.update(text);
            let hash = hasher.finalize().to_vec();
            self.svg_hash = Some(hash);

            cx.request_layout(self.id());
            ChangeFlags::LAYOUT
        } else {
            ChangeFlags::empty()
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, false, |_| Vec::new())
    }

    fn event(
        &mut self,
        _cx: &mut crate::context::EventCx,
        _id_path: Option<&[Id]>,
        _event: crate::event::Event,
    ) -> bool {
        false
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if let Some(tree) = self.svg_tree.as_ref() {
            let hash = self.svg_hash.as_ref().unwrap();
            let layout = cx.get_layout(self.id).unwrap();
            let rect = Size::new(layout.size.width as f64, layout.size.height as f64).to_rect();
            let color = cx.app_state.get_computed_style(self.id).color;
            cx.draw_svg(floem_renderer::Svg { tree, hash }, rect, color);
        }
    }
}
