use floem_reactive::create_effect;
use floem_renderer::{
    text::FONT_SYSTEM,
    usvg::{self, Tree},
    Renderer,
};
use peniko::kurbo::Size;
use sha2::{Digest, Sha256};

use crate::{id::ViewId, prop_extractor, style::TextColor, style_class, view::View};

use super::Decorators;

style_class!(pub SvgClass);

prop_extractor!(SvgProps {
    text_color: TextColor,
});

pub struct Svg {
    id: ViewId,
    props: SvgProps,
    svg_tree: Option<Tree>,
    svg_hash: Option<Vec<u8>>,
}

impl Svg {
    pub fn update_value<S: Into<String>>(self, svg_str: impl Fn() -> S + 'static) -> Self {
        let id = self.id;
        create_effect(move |_| {
            let new_svg_str = svg_str();
            id.update_state(new_svg_str.into());
        });
        self
    }
}

pub fn svg(svg_str: impl Into<String> + 'static) -> Svg {
    let id = ViewId::new();
    id.update_state(svg_str.into());
    Svg {
        id,
        svg_tree: None,
        svg_hash: None,
        props: Default::default(),
    }
    .class(SvgClass)
}

impl View for Svg {
    fn id(&self) -> ViewId {
        self.id
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        self.props.read(cx);
    }

    fn update(&mut self, _cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<String>() {
            let text = &*state;
            let font_system = FONT_SYSTEM.lock();
            let font_db = font_system.db();
            self.svg_tree = Tree::from_str(text, &usvg::Options::default(), font_db).ok();

            let mut hasher = Sha256::new();
            hasher.update(text);
            let hash = hasher.finalize().to_vec();
            self.svg_hash = Some(hash);

            self.id.request_layout();
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if let Some(tree) = self.svg_tree.as_ref() {
            let hash = self.svg_hash.as_ref().unwrap();
            let layout = self.id.get_layout().unwrap_or_default();
            let rect = Size::new(layout.size.width as f64, layout.size.height as f64).to_rect();
            let color = self.props.text_color();
            cx.draw_svg(floem_renderer::Svg { tree, hash }, rect, color);
        }
    }
}
