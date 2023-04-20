use std::any::Any;

use crate::{
    app::AppContext,
    context::{EventCx, UpdateCx},
    event::Event,
    id::Id,
    view::{ChangeFlags, View},
};

pub struct Click<V: View> {
    id: Id,
    child: V,
    on_click: Box<dyn Fn()>,
}

pub fn click<V: View>(
    cx: AppContext,
    child: impl FnOnce(AppContext) -> V,
    on_click: impl Fn() + 'static,
) -> Click<V> {
    let id = cx.new_id();
    let mut child_cx = cx;
    child_cx.id = id;
    let child = child(child_cx);
    Click {
        id,
        child,
        on_click: Box::new(on_click),
    }
}

impl<V: View> View for Click<V> {
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

    fn update(&mut self, _cx: &mut UpdateCx, _state: Box<dyn Any>) -> ChangeFlags {
        ChangeFlags::empty()
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| vec![self.child.layout_main(cx)])
    }

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) {
        self.child.compute_layout_main(cx);
    }

    fn event(&mut self, cx: &mut EventCx, id_path: Option<&[Id]>, event: Event) -> bool {
        if id_path.is_none() {
            // only send event to child if id_path is_none,
            // because if id_path is_some, this event is destined to this view
            if self.child.event_main(cx, id_path, event.clone()) {
                return true;
            }
        }

        match &event {
            Event::MouseDown(_event) => {
                cx.update_active(self.id);
                true
            }
            Event::MouseUp(event) => {
                let rect = cx.get_size(self.id).unwrap_or_default().to_rect();
                if rect.contains(event.pos) {
                    (self.on_click)();
                }
                true
            }
            _ => false,
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        self.child.paint_main(cx);
    }
}
