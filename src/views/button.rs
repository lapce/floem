use crate::{style_class, views::Decorators, IntoView, View, ViewId};
use core::ops::FnMut;

style_class!(pub ButtonClass);

pub fn button<V: IntoView + 'static>(child: V) -> Button {
    Button::new(child)
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
    pub fn new(child: impl IntoView) -> Self {
        let id = ViewId::new();
        id.add_child(Box::new(child.into_view()));
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
impl<T: IntoView + 'static> ButtonExt for T {
    fn button(self) -> Button {
        button(self)
    }
}
