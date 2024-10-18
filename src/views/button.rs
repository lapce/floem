use crate::{style_class, views::Decorators, IntoView, View, ViewId};
use core::ops::FnMut;

style_class!(
    /// A Style class for buttons
    pub ButtonClass
);

/// A wrapper around a view that adds a button style class and provides an `action` method to add a callback to the button.
pub fn button<V: IntoView + 'static>(child: V) -> Button {
    Button::new(child)
}

/// A wrapper around a view that adds a button style class and provides an `action` method to add a callback to the button.
pub struct Button {
    id: ViewId,
}
impl View for Button {
    fn id(&self) -> ViewId {
        self.id
    }
}
impl Button {
    /// Create a new button with the given child view.
    pub fn new(child: impl IntoView) -> Self {
        let id = ViewId::new();
        id.add_child(Box::new(child.into_view()));
        Button { id }.keyboard_navigatable().class(ButtonClass)
    }

    /// Add a callback to the button that will be called when the button is pressed.
    pub fn action(self, mut on_press: impl FnMut() + 'static) -> Self {
        self.on_click_stop(move |_| {
            on_press();
        })
    }
}

/// A trait that adds a `button` method to any type that implements `IntoView`.
pub trait ButtonExt {
    /// Wrap the view in a button.
    fn button(self) -> Button;
}
impl<T: IntoView + 'static> ButtonExt for T {
    fn button(self) -> Button {
        button(self)
    }
}
