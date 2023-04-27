use glazier::kurbo::Size;

use crate::{
    app_handle::AppContext,
    id::Id,
    view::{ChangeFlags, View},
};

pub struct Clip<V: View> {
    id: Id,
    child: V,
}

pub fn clip<V: View>(cx: AppContext, child: impl FnOnce(AppContext) -> V) -> Clip<V> {
    let id = cx.new_id();
    let mut child_cx = cx;
    child_cx.id = id;
    let child = child(child_cx);
    Clip { id, child }
}

impl<V: View> View for Clip<V> {
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
        event: crate::event::Event,
    ) -> bool {
        self.child.event_main(cx, id_path, event)
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.save();
        let size = cx
            .get_layout(self.id)
            .map(|layout| Size::new(layout.size.width as f64, layout.size.height as f64))
            .unwrap_or_default();
        cx.clip(&size.to_rect());
        self.child.paint_main(cx);
        cx.restore();
    }
}
