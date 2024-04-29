use crate::{
    style_class,
    view::IntoView,
    views::{self, container, Decorators},
};

style_class!(pub TooltipClass);

pub fn tooltip<V: IntoView + 'static, T: IntoView + 'static>(
    child: V,
    tip: impl Fn() -> T + 'static,
) -> impl IntoView {
    views::tooltip(child, move || container(tip()).class(TooltipClass))
}
