use floem_renderer::{usvg, usvg::Tree, Renderer};
use glazier::kurbo::Size;
use leptos_reactive::create_effect;
use sha2::{Digest, Sha256};

use crate::{
    app_handle::AppContext,
    id::Id,
    view::{ChangeFlags, View},
};

pub struct Svg {
    id: Id,
    svg_tree: Option<Tree>,
    svg_hash: Option<Vec<u8>>,
}

pub fn svg(cx: AppContext, svg_str: impl Fn() -> String + 'static) -> Svg {
    let id = cx.new_id();
    create_effect(cx.scope, move |_| {
        let new_svg_str = svg_str();
        AppContext::update_state(id, new_svg_str, false);
    });
    Svg {
        id,
        svg_tree: None,
        svg_hash: None,
    }
}

impl View for Svg {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&mut self, _id: Id) -> Option<&mut dyn View> {
        None
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

    fn compute_layout(&mut self, _cx: &mut crate::context::LayoutCx) {}

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
