use glazier::kurbo::Point;

use crate::{
    app_handle::AppContext,
    event::Event,
    id::Id,
    view::{ChangeFlags, View},
};

pub struct TitleBar<V: View> {
    id: Id,
    prev_mouse_pos: Option<Point>,
    child: V,
}

pub fn title_bar<V: View>(child: impl FnOnce() -> V) -> TitleBar<V> {
    let cx = AppContext::get_current();
    let id = cx.new_id();
    let mut child_cx = cx;
    child_cx.id = id;
    AppContext::save();
    AppContext::set_current(child_cx);
    let child = child();
    AppContext::restore();

    TitleBar {
        id,
        prev_mouse_pos: None,
        child,
    }
}

impl<V: View> View for TitleBar<V> {
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

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "TitleBar".into()
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

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) {
        self.child.compute_layout_main(cx);
    }

    fn event(
        &mut self,
        cx: &mut crate::context::EventCx,
        id_path: Option<&[Id]>,
        event: Event,
    ) -> bool {
        match &event {
            Event::PointerDown(mouse_event) => {
                if mouse_event.button.is_left() {
                    self.prev_mouse_pos = Some(mouse_event.pos);
                }
            }
            Event::PointerUp(mouse_event) => {
                if mouse_event.button.is_left() {
                    self.prev_mouse_pos = None
                }
            }
            Event::PointerMove(mouse_event) => {
                if let Some(prev_pos) = self.prev_mouse_pos {
                    self.id.update_window_position(prev_pos - mouse_event.pos)
                }
            }
            _ => {}
        }
        self.child.event_main(cx, id_path, event)
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        self.child.paint_main(cx);
    }
}
