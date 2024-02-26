use floem_reactive::create_effect;
use floem_renderer::{
    usvg::Tree,
    usvg::{self, TreeParsing},
    Renderer,
};
use kurbo::Size;
use sha2::{Digest, Sha256};

use crate::{
    id::Id,
    style_class,
    view::{View, ViewData, Widget},
};

use super::Decorators;

pub struct Svg {
    data: ViewData,
    svg_tree: Option<Tree>,
    svg_hash: Option<Vec<u8>>,
}

style_class!(pub SvgClass);

pub fn svg(svg_str: impl Fn() -> String + 'static) -> Svg {
    let id = Id::next();
    create_effect(move |_| {
        let new_svg_str = svg_str();
        id.update_state(new_svg_str);
    });
    Svg {
        data: ViewData::new(id),
        svg_tree: None,
        svg_hash: None,
    }
    .class(SvgClass)
}

impl View for Svg {
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

impl Widget for Svg {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Svg".into()
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<String>() {
            let text = &*state;
            self.svg_tree = Tree::from_str(text, &usvg::Options::default()).ok();

            let mut hasher = Sha256::new();
            hasher.update(text);
            let hash = hasher.finalize().to_vec();
            self.svg_hash = Some(hash);

            cx.request_layout(self.id());
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if let Some(tree) = self.svg_tree.as_ref() {
            let hash = self.svg_hash.as_ref().unwrap();
            let layout = cx.get_layout(self.id()).unwrap();
            let rect = Size::new(layout.size.width as f64, layout.size.height as f64).to_rect();
            let color = cx.app_state.get_builtin_style(self.id()).color();
            cx.draw_svg(floem_renderer::Svg { tree, hash }, rect, color);
        }
    }
}
