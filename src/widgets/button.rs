use crate::{
    style_class,
    view::ViewBuilder,
    views::{self, container, Decorators},
};
use std::fmt::Display;

style_class!(pub ButtonClass);

pub fn button<S: Display + 'static>(label: impl Fn() -> S + 'static) -> impl ViewBuilder {
    container(views::label(label))
        .keyboard_navigatable()
        .class(ButtonClass)
}
