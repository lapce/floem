use crate::{
    style_class,
    view::ViewBuilder,
    views::{self, container, Decorators},
};

style_class!(pub TooltipClass);

pub fn tooltip<V: ViewBuilder + 'static, T: ViewBuilder + 'static>(
    child: V,
    tip: impl Fn() -> T + 'static,
) -> impl ViewBuilder {
    views::tooltip(child, move || container(tip()).class(TooltipClass))
}
