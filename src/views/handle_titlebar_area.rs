use kurbo::Rect;

use crate::{
    action::{set_handle_titlebar, toggle_window_maximized},
    context::ViewContext,
    event::{Event, EventListener},
    id::Id,
    view::View,
};

use super::Decorators;

pub struct HandleTitlebarArea<V: View> {
    id: Id,
    child: V,
}

pub fn handle_titlebar_area<V: View>(child: impl FnOnce() -> V) -> HandleTitlebarArea<V> {
    let (id, child) = ViewContext::new_id_with_child(child);
    HandleTitlebarArea { id, child }
        .on_event(EventListener::PointerDown, |_| {
            set_handle_titlebar(true);
            true
        })
        .on_event(EventListener::PointerUp, |_| {
            set_handle_titlebar(false);
            true
        })
        .on_double_click(|_| {
            toggle_window_maximized();
            true
        })
}

impl<V: View> View for HandleTitlebarArea<V> {
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
        cx.layout_node(self.id, true, |cx| vec![self.child.layout_main(cx)])
    }

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) -> Option<Rect> {
        Some(self.child.compute_layout_main(cx))
    }

    fn event(
        &mut self,
        cx: &mut crate::context::EventCx,
        id_path: Option<&[Id]>,
        event: Event,
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
