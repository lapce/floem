use crate::{app::AppContext, id::Id, view::View};

pub struct Container<V: View> {
    id: Id,
    child: V,
}

pub fn container<V: View>(cx: AppContext, child: impl Fn(AppContext) -> V) -> Container<V> {
    let id = cx.new_id();
    let mut child_cx = cx;
    child_cx.id = id;
    let child = child(child_cx);
    Container { id, child }
}

impl<V: View> View for Container<V> {
    fn id(&self) -> Id {
        self.id
    }

    fn update(
        &mut self,
        cx: &mut crate::context::UpdateCx,
        id_path: &[Id],
        state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        let id_path = &id_path[1..];
        self.child.update(cx, id_path, state)
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| vec![self.child.layout(cx)])
    }

    fn event(&mut self, cx: &mut crate::context::EventCx, event: crate::event::Event) {
        self.child.event_main(cx, event);
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        self.child.paint_main(cx);
    }
}
