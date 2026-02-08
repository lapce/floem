#![deny(missing_docs)]

use crate::{
    view::ViewId,
    view::{IntoView, View},
};

/// A wrapper around a child View that clips painting so the child does not show outside of the viewport.
///
/// This can be useful for limiting child painting, including for rounded borders using border radius.
pub struct Clip {
    id: ViewId,
}

impl Clip {
    /// Creates a new clip view wrapping the given child.
    ///
    /// ## Example
    /// ```rust
    /// use floem::views::{Clip, Label};
    ///
    /// let clipped = Clip::new(Label::new("Clipped content"));
    /// ```
    pub fn new(child: impl IntoView) -> Self {
        let child = child.into_view();
        let id = ViewId::new();
        id.set_children([child]);
        Clip { id }
    }
}

/// A clip is a wrapper around a child View that will clip the painting of the child so that it does not show outside of the viewport of the [`Clip`].
///
/// This can be useful for limiting child painting, including for rounded borders using border radius.
#[deprecated(since = "0.2.0", note = "Use Clip::new() instead")]
pub fn clip<V: IntoView + 'static>(child: V) -> Clip {
    Clip::new(child)
}

impl View for Clip {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Clip".into()
    }

    fn paint(&mut self, _cx: &mut crate::context::PaintCx) {
        // Clipping is now handled by the box tree and applied automatically
        // during traversal. The clip view's main purpose is to set clip style
        // which affects layout/box tree generation.
        // No explicit painting needed - children are painted by traversal.
    }
}

/// A trait that adds a `clip` method to any type that implements `IntoView`.
pub trait ClipExt {
    /// Wrap the view in a clip view.
    fn clip(self) -> Clip;
}

impl<T: IntoView + 'static> ClipExt for T {
    fn clip(self) -> Clip {
        Clip::new(self)
    }
}
