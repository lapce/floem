#![deny(missing_docs)]

use crate::{
    view::ViewId,
    view::{IntoView, View},
};

/// A simple wrapper around another View.
///
/// Wrapping a [View](crate::view::View) with a [`Container`] allows using another
/// set of styles completely separate from the child View that is being wrapped.
pub struct Container {
    id: ViewId,
}

impl Container {
    /// Creates a new container wrapping the given child view.
    ///
    /// ## Example
    /// ```rust
    /// use floem::views::{Container, Label};
    ///
    /// let wrapped = Container::new(Label::new("Content"));
    /// ```
    pub fn new(child: impl IntoView) -> Self {
        let id = ViewId::new();
        id.set_children([child.into_view()]);
        Container { id }
    }

    /// Creates a new container with a specific ViewId wrapping the given child view.
    ///
    /// This is useful when you need to control the ViewId for lazy view creation.
    pub fn with_id(id: ViewId, child: impl IntoView) -> Self {
        id.set_children([child.into_view()]);
        Container { id }
    }
}

/// A simple wrapper around another View
///
/// Wrapping a [View](crate::view::View) with a [`Container`] allows using another
/// set of styles completely separate from the child View that is being wrapped.
#[deprecated(since = "0.2.0", note = "Use Container::new() instead")]
pub fn container<V: IntoView + 'static>(child: V) -> Container {
    Container::new(child)
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
        Container::new(self)
    }
}
