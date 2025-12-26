#![deny(missing_docs)]

use floem_reactive::{Scope, UpdaterEffect};

use crate::{
    context::UpdateCx,
    view::ViewId,
    view::{IntoView, View},
};

/// A simple wrapper around another View.
///
/// Wrapping a [View](crate::view::View) with a [`Container`] allows using another
/// set of styles completely separate from the child View that is being wrapped.
pub struct Container {
    id: ViewId,
    child_scope: Scope,
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
        Container {
            id,
            child_scope: Scope::current(),
        }
    }

    /// Creates a new container with derived (reactive) content.
    ///
    /// The content function is called initially and re-called whenever
    /// its reactive dependencies change, replacing the container's child.
    ///
    /// ## Example
    /// ```rust
    /// use floem::prelude::*;
    /// use floem::views::{Container, Label};
    ///
    /// let count = RwSignal::new(0);
    ///
    /// Container::derived(move || {
    ///     Label::derived(move || format!("Count: {}", count.get()))
    /// });
    /// ```
    pub fn derived<CF, IV>(content: CF) -> Self
    where
        CF: Fn() -> IV + 'static,
        IV: IntoView + 'static,
    {
        let id = ViewId::new();
        let content_fn = Box::new(Scope::current().enter_child(move |_| content().into_view()));

        let (child, child_scope) = UpdaterEffect::new(
            move || content_fn(()),
            move |(new_view, new_scope)| {
                let old_child = id.children();
                id.set_children([new_view]);
                id.update_state((old_child, new_scope));
            },
        );

        id.set_children([child]);
        Container { id, child_scope }
    }

    /// Creates a new container with a specific ViewId wrapping the given child view.
    ///
    /// This is useful when you need to control the ViewId for lazy view creation.
    pub fn with_id(id: ViewId, child: impl IntoView) -> Self {
        id.set_children([child.into_view()]);
        Container {
            id,
            child_scope: Scope::current(),
        }
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

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(val) = state.downcast::<(Vec<ViewId>, Scope)>() {
            let old_child_scope = self.child_scope;
            let (old_children, child_scope) = *val;
            self.child_scope = child_scope;
            for child in old_children {
                cx.window_state.remove_view(child);
            }
            old_child_scope.dispose();
            self.id.request_all();
        }
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
