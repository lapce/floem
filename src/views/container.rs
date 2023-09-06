use kurbo::Rect;

use crate::{
    id::Id,
    view::{ChangeFlags, View},
};

/// A simple wrapper around another View. See [`container`]
pub struct Container<V: View> {
    id: Id,
    child: V,
}

/// A simple wrapper around another View
///
/// A [`Container`] is useful for wrapping another [View](crate::view::View). This is often useful for allowing another
/// set of styles completely separate from the View that is being wrapped.
pub fn container<V: View>(child: V) -> Container<V> {
    Container {
        id: Id::next(),
        child,
    }
}

impl<V: View> View for Container<V> {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&self, id: Id) -> Option<&dyn View> {
        if self.child.id() == id {
            Some(&self.child)
        } else {
            None
        }
    }

    fn child_mut(&mut self, id: Id) -> Option<&mut dyn View> {
        if self.child.id() == id {
            Some(&mut self.child)
        } else {
            None
        }
    }

    fn children(&self) -> Vec<&dyn View> {
        vec![&self.child]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        vec![&mut self.child]
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Container".into()
    }

    fn update(
        &mut self,
        _cx: &mut crate::context::UpdateCx,
        _state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        ChangeFlags::empty()
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| vec![self.child.layout_main(cx)])
    }

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) -> Option<Rect> {
        Some(self.child.compute_layout_main(cx))
    }

    fn event(
        &mut self,
        cx: &mut crate::context::EventCx,
        id_path: Option<&[Id]>,
        event: crate::event::Event,
    ) -> bool {
        if cx.should_send(self.child.id(), &event) {
            self.child.event_main(cx, id_path, event)
        } else {
            false
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        self.child.paint_main(cx);
    }
}
