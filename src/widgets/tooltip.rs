use crate::{
    style_class,
    view::View,
    views::{self, container, Decorators},
};

style_class!(pub TooltipClass);

pub fn tooltip<V: View + 'static, T: View + 'static>(child: V, tip: T) -> impl View {
    views::tooltip(child, container(tip).class(TooltipClass))
}
