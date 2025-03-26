#![deny(missing_docs)]

use crate::{
    id::ViewId,
    view::{IntoView, View},
};

/// A simple wrapper around another View. See [`container`].
pub struct Container {
    id: ViewId,
}

/// A simple wrapper around another View
///
/// A [`Container`] is useful for wrapping another [View](crate::view::View). This is often useful for allowing another
/// set of styles completely separate from the child View that is being wrapped.
pub fn container<V: IntoView + 'static>(child: V) -> Container {
    let id = ViewId::new();
    id.set_children([child.into_view()]);

    Container { id }
}

impl View for Container {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Container".into()
    }
}

/// A trait that adds a `container` method to any type that implements `IntoView`.
pub trait ContainerExt {
    /// Wrap the view in a container.
    fn container(self) -> Container;
}

impl<T: IntoView + 'static> ContainerExt for T {
    fn container(self) -> Container {
        container(self)
    }
}
