#![deny(missing_docs)]

use peniko::kurbo::Size;

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

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.save();
        let view_state = self.id.state();
        let size = self
            .id
            .get_layout()
            .map(|layout| Size::new(layout.size.width as f64, layout.size.height as f64))
            .unwrap_or_default();

        let radii = crate::view::border_to_radii(&view_state.borrow().combined_style, size);

        if crate::view::radii_max(radii) > 0.0 {
            let rect = size.to_rect().to_rounded_rect(radii);
            cx.clip(&rect);
        } else {
            cx.clip(&size.to_rect());
        }
        cx.paint_children(self.id);
        cx.restore();
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
