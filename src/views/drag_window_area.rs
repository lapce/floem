use kurbo::Rect;

use crate::{
    action::{drag_window, toggle_window_maximized},
    event::{Event, EventListener},
    id::Id,
    view::View,
};

use super::Decorators;

pub struct DragWindowArea<V: View> {
    id: Id,
    child: V,
}

pub fn drag_window_area<V: View>(child: V) -> DragWindowArea<V> {
    let id = Id::next();
    DragWindowArea { id, child }
        .on_event(EventListener::PointerDown, |_| {
            drag_window();
            true
        })
        .on_double_click(|_| {
            toggle_window_maximized();
            true
        })
}

impl<V: View> View for DragWindowArea<V> {
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

    fn update(
        &mut self,
        _cx: &mut crate::context::UpdateCx,
        _state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        crate::view::ChangeFlags::empty()
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| vec![cx.layout_view(&mut self.child)])
    }

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) -> Option<Rect> {
        Some(cx.compute_view_layout(&mut self.child))
    }

    fn event(
        &mut self,
        cx: &mut crate::context::EventCx,
        id_path: Option<&[Id]>,
        event: Event,
    ) -> bool {
        cx.view_event(&mut self.child, id_path, event)
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        self.child.paint_main(cx);
    }
}
