use crate::{
    style_class,
    views::{self, Decorators},
    View, ViewId,
};
use core::ops::FnMut;
use std::fmt::Display;

style_class!(pub ButtonClass);

pub fn dyn_button<S: Display + 'static>(label: impl Fn() -> S + 'static) -> Button {
    Button::new_dyn(label)
}
pub fn button<S: Display + 'static>(label: S) -> Button {
    Button::new(label)
}

pub struct Button {
    id: ViewId,
}
impl View for Button {
    fn id(&self) -> ViewId {
        self.id
    }
}
impl Button {
    pub fn new_dyn<S: Display + 'static>(label: impl Fn() -> S + 'static) -> Self {
        let id = ViewId::new();
        let text = views::label(label);
        id.add_child(Box::new(text));
        Button { id }.keyboard_navigatable().class(ButtonClass)
    }
    pub fn new<S: Display>(label: S) -> Self {
        let id = ViewId::new();
        let text = views::text(label);
        id.add_child(Box::new(text));
        Button { id }.keyboard_navigatable().class(ButtonClass)
    }

    pub fn action(self, mut on_press: impl FnMut() + 'static) -> Self {
        self.on_click_stop(move |_| {
            on_press();
        })
    }
}

pub trait ButtonExt {
    fn button(self) -> Button;
}
impl<T: std::fmt::Display + 'static> ButtonExt for T {
    fn button(self) -> Button {
        button(self)
    }
}
