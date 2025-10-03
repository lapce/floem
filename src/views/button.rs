use crate::{style_class, views::Decorators, IntoView, View, ViewId};
use core::ops::FnMut;

style_class!(
    /// The style class that is applied to buttons.
    pub ButtonClass
);

/// Creates a new simple [Button] view with the default [ButtonClass] style.
pub fn button<V: IntoView + 'static>(child: V) -> Button {
    Button::new(child)
}

/// A simple Button view. See [`button`].
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
        Button { id }.keyboard_navigable().class(ButtonClass)
    }

    pub fn action(self, mut on_press: impl FnMut() + 'static) -> Self {
        self.on_click_stop(move |_| {
            on_press();
        })
    }
}

/// A trait that adds a `button` method to any type that implements `IntoView`.
pub trait ButtonExt {
    fn button(self) -> Button;
}
impl<T: IntoView + 'static> ButtonExt for T {
    fn button(self) -> Button {
        button(self)
    }
}
