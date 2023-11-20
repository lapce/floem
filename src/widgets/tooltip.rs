use crate::{
    style_class,
    view::View,
    views::{self, container, Decorators},
};

style_class!(pub TooltipClass);

pub fn tooltip<V: View + 'static, T: View + 'static>(
    child: V,
    tip: impl Fn() -> T + 'static,
) -> impl View {
    views::tooltip(child, move || container(tip()).class(TooltipClass))
}
