use glazier::kurbo::{Point, Rect};

use crate::{
    app_handle::AppContext,
    event::Event,
    id::Id,
    view::{ChangeFlags, View},
};

pub struct WindowDragArea<V: View> {
    id: Id,
    prev_pos: Option<Point>,
    child: V,
}

pub fn window_drag_area<V: View>(child: impl FnOnce() -> V) -> WindowDragArea<V> {
    let cx = AppContext::get_current();
    let id = cx.new_id();
    let mut child_cx = cx;
    child_cx.id = id;
    AppContext::save();
    AppContext::set_current(child_cx);
    let child = child();
    AppContext::restore();

    WindowDragArea {
        id,
        child,
        prev_pos: None,
    }
}

impl<V: View> View for WindowDragArea<V> {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&mut self, id: Id) -> Option<&mut dyn View> {
        if self.child.id() == id {
            Some(&mut self.child)
        } else {
            None
        }
    }

    fn children(&mut self) -> Vec<&mut dyn View> {
        vec![&mut self.child]
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
        event: Event,
    ) -> bool {
        if !self.child.event_main(cx, id_path, event.clone()) {
            match &event {
                Event::PointerDown(mouse_event) => {
                    if mouse_event.button.is_left() {
                        self.prev_pos = Some(mouse_event.pos);
                        cx.update_active(self.id);
                        self.id.set_handle_titlebar(true);
                    }
                    true
                }
                Event::PointerUp(mouse_event) => {
                    if mouse_event.button.is_left() {
                        self.id.set_handle_titlebar(false);
                        self.prev_pos = None;
                    }
                    true
                }
                Event::PointerMove(mouse_event) => {
                    if let Some(prev_pos) = self.prev_pos {
                        let position_diff = mouse_event.pos - prev_pos;
                        self.id.set_window_delta(position_diff);
                        return true;
                    }
                    false
                }
                _ => false,
            }
        } else {
            false
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        self.child.paint_main(cx);
    }
}
