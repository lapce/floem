use std::any::Any;

use leptos_reactive::create_effect;

use crate::{
    app::{AppContext, UpdateMessage},
    context::{EventCx, UpdateCx},
    event::Event,
    id::Id,
    view::{ChangeFlags, View},
};

pub struct Button<V: View> {
    id: Id,
    child: V,
    onclick: Box<dyn Fn()>,
}

pub fn button<V: View>(
    cx: AppContext,
    child: impl FnOnce(AppContext) -> V,
    onclick: impl Fn() + 'static,
) -> Button<V> {
    let id = cx.new_id();
    let mut child_cx = cx;
    child_cx.id = id;
    let child = child(child_cx);
    Button {
        id,
        child,
        onclick: Box::new(onclick),
    }
}

impl<V: View> View for Button<V> {
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

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) -> ChangeFlags {
        ChangeFlags::empty()
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| vec![self.child.layout(cx)])
    }

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) {
        self.child.compute_layout_main(cx);
    }

    fn event(&mut self, cx: &mut EventCx, id_path: Option<&[Id]>, event: Event) -> bool {
        if self.child.event_main(cx, id_path, event.clone()) {
            return true;
        }

        match &event {
            Event::MouseDown(_) => {
                cx.update_active(self.id);
                true
            }
            Event::MouseUp(event) => {
                let rect = cx.get_size(self.id).unwrap_or_default().to_rect();
                if rect.contains(event.pos) {
                    (self.onclick)();
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
