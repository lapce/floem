use kurbo::Rect;

use crate::{
    id::Id,
    view::{ChangeFlags, View},
};

/// A wrapper around any type that implements View. See [`container_box`]
pub struct ContainerBox {
    id: Id,
    child: Box<dyn View>,
}

/// A wrapper around any type that implements View.
///
/// Views in Floem are strongly typed. A [`ContainerBox`] allows you to escape the strongly typed View and contain a `Box<dyn View>`.
///
/// ## Bad Example
///```compile_fail
/// use floem::views::*;
/// use floem_reactive::*;
/// let check = true;
///
/// container(|| {
///     if check == true {
///         checkbox(create_rw_signal(true).read_only())
///     } else {
///         label(|| "no check".to_string())
///     }
/// });
/// ```
/// The above example will fail to compile because the container is strongly typed so the if and
/// the else must return the same type. The problem is that checkbox is an [Svg](crate::views::Svg)
/// and the else returns a [Label](crate::views::Label). The solution to this is to use a
/// [`ContainerBox`] to escape the strongly typed requirement.
///
/// ```
/// use floem::views::*;
/// use floem_reactive::*;
/// let check = true;
///
/// if check == true {
///     container_box(checkbox(create_rw_signal(true).read_only()))
/// } else {
///     container_box(label(|| "no check".to_string()))
/// };
/// ```
pub fn container_box(child: impl View + 'static) -> ContainerBox {
    ContainerBox {
        id: Id::next(),
        child: Box::new(child),
    }
}

impl View for ContainerBox {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&self, id: Id) -> Option<&dyn View> {
        if self.child.id() == id {
            Some(&*self.child)
        } else {
            None
        }
    }

    fn child_mut(&mut self, id: Id) -> Option<&mut dyn View> {
        if self.child.id() == id {
            Some(&mut *self.child)
        } else {
            None
        }
    }

    fn children(&self) -> Vec<&dyn View> {
        vec![&*self.child]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        vec![&mut *self.child]
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "ContainerBox".into()
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
