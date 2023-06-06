use glazier::kurbo::Rect;
use leptos_reactive::create_effect;

use crate::{
    app_handle::AppContext,
    event::Event,
    id::Id,
    style::CursorStyle,
    view::{ChangeFlags, View},
};

pub struct Link<V: View> {
    id: Id,
    child: V,
    link: String,
    down: bool,
}

pub fn link<V: View>(
    view_fn: impl FnOnce() -> V,
    link_str: impl Fn() -> String + 'static,
) -> Link<V> {
    let cx = AppContext::get_current();
    let id = cx.new_id();
    create_effect(cx.scope, move |_| {
        let new_str = link_str();
        id.update_state(new_str, false);
    });
    let mut child_cx = cx;
    child_cx.id = id;
    AppContext::save();
    AppContext::set_current(child_cx);
    let child = view_fn();
    AppContext::restore();

    Link {
        id,
        child,
        link: String::new(),
        down: false,
    }
}

impl<V: View> View for Link<V> {
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
        "Container".into()
    }

    fn update(
        &mut self,
        _cx: &mut crate::context::UpdateCx,
        state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        if let Ok(state) = state.downcast::<String>() {
            self.link = *state;
        }
        ChangeFlags::empty()
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::NodeId {
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
        match event {
            Event::PointerDown(ref pointer_event) => {
                if pointer_event.button.is_left() {
                    self.down = true;
                    return true;
                }
            }
            Event::PointerUp(_) => {
                if self.down {
                    self.down = false;
                    // How to handle error?
                    let _ = open::that(self.link.clone());
                    return true;
                }
            }
            Event::PointerMove(_) => {
                if !matches!(cx.app_state.cursor, Some(CursorStyle::Pointer)) {
                    cx.app_state.cursor = Some(CursorStyle::Pointer);
                }
            }
            _ => {}
        }
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
