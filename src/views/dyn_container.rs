use std::marker::PhantomData;

use floem_reactive::create_effect;
use glazier::kurbo::Rect;

use crate::{
    app_handle::ViewContext,
    id::Id,
    view::{ChangeFlags, View},
};

pub struct DynamicContainer<CF: Fn(T) -> Box<dyn View> + 'static, T: 'static> {
    id: Id,
    child: Box<dyn View>,
    child_fn: CF,
    phantom: PhantomData<T>,
    cx: ViewContext,
}

pub fn dyn_container<CF: Fn(T) -> Box<dyn View> + 'static, T: 'static>(
    update_view: impl Fn() -> T + 'static,
    child: CF,
) -> DynamicContainer<CF, T> {
    let cx = ViewContext::get_current();
    let id = cx.new_id();

    let mut child_cx = cx;
    child_cx.id = id;

    create_effect(move |_| {
        id.update_state(update_view(), false);
    });

    DynamicContainer {
        id,
        child: Box::new(crate::views::empty()),
        child_fn: child,
        phantom: PhantomData,
        cx: child_cx,
    }
}

impl<CF: Fn(T) -> Box<dyn View> + 'static + Copy, T: 'static> View for DynamicContainer<CF, T> {
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
        cx: &mut crate::context::UpdateCx,
        state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        if let Ok(val) = state.downcast::<T>() {
            ViewContext::save();
            ViewContext::set_current(self.cx);

            let child_fn = self.child_fn;
            self.child = child_fn(*val);

            ViewContext::restore();
            cx.request_layout(self.id());
            ChangeFlags::LAYOUT
        } else {
            ChangeFlags::empty()
        }
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
