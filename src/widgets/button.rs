use crate::{
    style_class,
    view::View,
    views::{self, container, Decorators},
};
use std::fmt::Display;

style_class!(pub ButtonClass);
style_class!(pub ButtonLabelClass);

pub fn button<S: Display + 'static>(label: impl Fn() -> S + 'static) -> impl View {
    container(views::label(label).class(ButtonLabelClass))
        .keyboard_navigatable()
        .class(ButtonClass)
}
