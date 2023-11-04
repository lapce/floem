use crate::{
    context::{EventCx, UpdateCx},
    id::Id,
    view::{ChangeFlags, View},
};
use kurbo::Rect;

pub struct StaticList<V>
where
    V: View,
{
    id: Id,
    children: Vec<V>,
}

pub fn static_list<V>(iterator: impl IntoIterator<Item = V>) -> StaticList<V>
where
    V: View,
{
    StaticList {
        id: Id::next(),
        children: iterator.into_iter().collect(),
    }
}

impl<V: View> View for StaticList<V> {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&self, id: Id) -> Option<&dyn View> {
        self.children
            .iter()
            .find(|v| v.id() == id)
            .map(|child| child as &dyn View)
    }

    fn child_mut(&mut self, id: Id) -> Option<&mut dyn View> {
        self.children
            .iter_mut()
            .find(|v| v.id() == id)
            .map(|child| child as &mut dyn View)
    }

    fn children(&self) -> Vec<&dyn View> {
        self.children
            .iter()
            .map(|child| child as &dyn View)
            .collect()
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        self.children
            .iter_mut()
            .map(|child| child as &mut dyn View)
            .collect()
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "StaticList".into()
    }

    fn update(
        &mut self,
        _cx: &mut UpdateCx,
        _state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        ChangeFlags::empty()
    }

    fn style(&mut self, cx: &mut crate::context::StyleCx) {
        for child in &mut self.children {
            child.style_main(cx);
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| {
            let nodes = self
                .children
                .iter_mut()
                .map(|child| child.layout_main(cx))
                .collect::<Vec<_>>();
            nodes
        })
    }

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) -> Option<Rect> {
        let mut layout_rect = Rect::ZERO;
        for child in &mut self.children {
            layout_rect = layout_rect.union(child.compute_layout_main(cx));
        }
        Some(layout_rect)
    }

    fn event(
        &mut self,
        cx: &mut EventCx,
        id_path: Option<&[Id]>,
        event: crate::event::Event,
    ) -> bool {
        for child in self.children.iter_mut() {
            let id = child.id();
            if cx.should_send(id, &event) && child.event_main(cx, id_path, event.clone()) {
                return true;
            }
        }
        false
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        for child in self.children.iter_mut() {
            child.paint_main(cx);
        }
    }
}
