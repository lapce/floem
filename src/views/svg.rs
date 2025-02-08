use floem_reactive::create_effect;
use floem_renderer::{
    usvg::{self, Tree},
    Renderer,
};
use peniko::{kurbo::Size, Brush};
use sha2::{Digest, Sha256};

use crate::{id::ViewId, prop, prop_extractor, style::TextColor, style_class, view::View};

use super::Decorators;

prop!(pub SvgColor: Option<Brush> {} = None);

prop_extractor! {
    SvgStyle {
        svg_color: SvgColor,
        text_color: TextColor,
    }
}

pub struct Svg {
    id: ViewId,
    svg_tree: Option<Tree>,
    svg_hash: Option<Vec<u8>>,
    svg_style: SvgStyle,
}

style_class!(pub SvgClass);

pub struct SvgStrFn {
    str_fn: Box<dyn Fn() -> String>,
}

impl<T, F> From<F> for SvgStrFn
where
    F: Fn() -> T + 'static,
    T: Into<String>,
{
    fn from(value: F) -> Self {
        SvgStrFn {
            str_fn: Box::new(move || value().into()),
        }
    }
}

impl From<String> for SvgStrFn {
    fn from(value: String) -> Self {
        SvgStrFn {
            str_fn: Box::new(move || value.clone()),
        }
    }
}

impl From<&str> for SvgStrFn {
    fn from(value: &str) -> Self {
        let value = value.to_string();
        SvgStrFn {
            str_fn: Box::new(move || value.clone()),
        }
    }
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

pub fn svg(svg_str_fn: impl Into<SvgStrFn> + 'static) -> Svg {
    let id = ViewId::new();
    let svg_str_fn: SvgStrFn = svg_str_fn.into();
    create_effect(move |_| {
        let new_svg_str = (svg_str_fn.str_fn)();
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

    fn view_style(&self) -> Option<crate::style::Style> {
        Some(crate::style::Style::new().pointer_events_auto())
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        self.svg_style.read(cx);
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

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if let Some(tree) = self.svg_tree.as_ref() {
            let hash = self.svg_hash.as_ref().unwrap();
            let layout = self.id.get_layout().unwrap_or_default();
            let rect = Size::new(layout.size.width as f64, layout.size.height as f64).to_rect();
            let color = if let Some(brush) = self.svg_style.svg_color() {
                Some(brush)
            } else {
                self.svg_style.text_color().map(Brush::Solid)
            };
            cx.draw_svg(floem_renderer::Svg { tree, hash }, rect, color.as_ref());
        }
    }
}
