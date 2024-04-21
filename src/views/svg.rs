use floem_reactive::create_effect;
use floem_renderer::{
    usvg::{self, Tree},
    Renderer,
};
use peniko::{kurbo::Size, Brush, Color};
use sha2::{Digest, Sha256};

use crate::{id::ViewId, prop, prop_extractor, style_class, view::View};

use super::Decorators;

prop!(pub SvgColor: Option<Brush> {} = None);

prop_extractor! {
    SvgStyle {
        svg_color: SvgColor,
    }
}

pub struct Svg {
    id: ViewId,
    svg_tree: Option<Tree>,
    svg_hash: Option<Vec<u8>>,
    svg_style: SvgStyle,
}

style_class!(pub SvgClass);

pub fn svg(svg_str: impl Fn() -> String + 'static) -> Svg {
    let id = ViewId::new();
    create_effect(move |_| {
        let new_svg_str = svg_str();
        id.update_state(new_svg_str);
    });
    Svg {
        id,
        svg_tree: None,
        svg_hash: None,
        svg_style: Default::default(),
    }
    .class(SvgClass)
}

impl View for Svg {
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, _cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<String>() {
            let text = &*state;
            self.svg_tree = Tree::from_str(text, &usvg::Options::default()).ok();

            let mut hasher = Sha256::new();
            hasher.update(text);
            let hash = hasher.finalize().to_vec();
            self.svg_hash = Some(hash);

            self.id.request_layout();
        }
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.svg_style.read(cx) {
            self.id.request_paint();
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if let Some(tree) = self.svg_tree.as_ref() {
            let hash = self.svg_hash.as_ref().unwrap();
            let layout = self.id.get_layout().unwrap_or_default();
            let rect = Size::new(layout.size.width as f64, layout.size.height as f64).to_rect();
            let color = if let Some(brush) = self.svg_style.svg_color() {
                Some(brush)
            } else {
                Some(Brush::Solid(
                    self.id
                        .state()
                        .borrow()
                        .combined_style
                        .builtin()
                        .color()
                        .unwrap_or(Color::BLACK),
                ))
            };
            cx.draw_svg(floem_renderer::Svg { tree, hash }, rect, color.as_ref());
        }
    }
}
