#![deny(missing_docs)]

use peniko::kurbo::Size;
use std::any::Any;

use crate::{
    id::ViewId,
    view::{IntoView, View},
};

/// A wrapper around a child View to clip painting. See [`clip`].
pub struct Clip {
    id: ViewId,
}

/// A clip is a wrapper around a child View that will clip the painting of the child so that it does not show outside of the viewport of the [`Clip`].
///
/// This can be useful for limiting child painting, including for rounded borders using border radius.
pub fn clip<V: IntoView + 'static>(child: V) -> Clip {
    let child = child.into_view();
    let id = ViewId::new();
    id.set_children([child]);
    Clip { id }
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
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// A trait that adds a `clip` method to any type that implements `IntoView`.
pub trait ClipExt {
    /// Wrap the view in a clip view.
    fn clip(self) -> Clip;
}

impl<T: IntoView + 'static> ClipExt for T {
    fn clip(self) -> Clip {
        clip(self)
    }
}
