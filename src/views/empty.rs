use crate::{
    id::Id,
    view::{ChangeFlags, View},
};

pub struct Empty {
    id: Id,
}

pub fn empty() -> Empty {
    Empty { id: Id::next() }
}

impl View for Empty {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&self, _id: Id) -> Option<&dyn View> {
        None
    }

    fn child_mut(&mut self, _id: Id) -> Option<&mut dyn View> {
        None
    }

    fn children(&self) -> Vec<&dyn View> {
        Vec::new()
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        Vec::new()
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Empty".into()
    }

    fn update(
        &mut self,
        _cx: &mut crate::context::UpdateCx,
        _state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        ChangeFlags::empty()
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, false, |_| Vec::new())
    }

    fn event(
        &mut self,
        _cx: &mut crate::context::EventCx,
        _id_path: Option<&[Id]>,
        _event: crate::event::Event,
    ) -> bool {
        false
    }

    fn paint(&mut self, _cx: &mut crate::context::PaintCx) {}
}
