use crate::{
    app_handle::ViewContext,
    id::Id,
    view::{ChangeFlags, View},
};

pub struct Empty {
    id: Id,
}

pub fn empty() -> Empty {
    let cx = ViewContext::get_current();
    let id = cx.new_id();
    Empty { id }
}

impl View for Empty {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&mut self, _id: Id) -> Option<&mut dyn View> {
        None
    }

    fn children(&mut self) -> Vec<&mut dyn View> {
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
