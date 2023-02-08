use crate::{
    app::AppContext,
    context::{EventCx, UpdateCx},
    id::Id,
    view::{ChangeFlags, View},
    view_tuple::ViewTuple,
};

pub struct Stack<VT> {
    id: Id,
    children: VT,
}

pub fn stack<VT: ViewTuple + 'static>(
    cx: AppContext,
    children: impl Fn(AppContext) -> VT,
) -> Stack<VT> {
    let id = cx.id.new();

    let mut children_cx = cx;
    children_cx.id = id;
    let children = children(children_cx);

    Stack { id, children }
}

impl<VT: ViewTuple + 'static> View for Stack<VT> {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&mut self, id: Id) -> Option<&mut dyn View> {
        self.children.child(id)
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn std::any::Any>) -> ChangeFlags {
        if let Ok(state) = state.downcast() {
            self.children = *state;
            cx.reset_children_layout(self.id);
            cx.request_layout(self.id);
            ChangeFlags::LAYOUT
        } else {
            ChangeFlags::empty()
        }
    }

    fn event(
        &mut self,
        cx: &mut EventCx,
        id_path: Option<&[Id]>,
        event: crate::event::Event,
    ) -> bool {
        let mut handled = false;
        self.children.foreach(&mut |view| {
            let id = view.id();
            if cx.should_send(id, &event) {
                let event = cx.offset_event(id, event.clone());
                handled = view.event_main(cx, id_path, event);
                return true;
            }
            false
        });
        handled
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| {
            let mut nodes = Vec::new();
            self.children.foreach(&mut |view| {
                let node = view.layout(cx);
                nodes.push(node);
                false
            });
            nodes
        })
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        self.children.foreach(&mut |view| {
            view.paint_main(cx);
            false
        });
    }
}
