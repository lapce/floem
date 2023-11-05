use kurbo::Rect;
use taffy::style::FlexDirection;

use crate::{
    context::{EventCx, UpdateCx},
    id::Id,
    style::Style,
    view::{ChangeFlags, View},
    view_tuple::ViewTuple,
};

pub struct Stack {
    id: Id,
    children: Vec<Box<dyn View>>,
    direction: Option<FlexDirection>,
}

pub fn stack<VT: ViewTuple + 'static>(children: VT) -> Stack {
    let id = Id::next();
    Stack {
        id,
        children: children.into_views(),
        direction: None,
    }
}

/// A stack which defaults to `FlexDirection::Row`.
pub fn h_stack<VT: ViewTuple + 'static>(children: VT) -> Stack {
    let id = Id::next();
    Stack {
        id,
        children: children.into_views(),
        direction: Some(FlexDirection::Row),
    }
}

/// A stack which defaults to `FlexDirection::Column`.
pub fn v_stack<VT: ViewTuple + 'static>(children: VT) -> Stack {
    let id = Id::next();
    Stack {
        id,
        children: children.into_views(),
        direction: Some(FlexDirection::Column),
    }
}

impl View for Stack {
    fn id(&self) -> Id {
        self.id
    }

    fn view_style(&self) -> Option<crate::style::Style> {
        self.direction
            .map(|direction| Style::new().flex_direction(direction))
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
        match self.direction {
            Some(FlexDirection::Column) => "Vertical Stack".into(),
            Some(FlexDirection::Row) => "Horizontal Stack".into(),
            _ => "Stack".into(),
        }
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn std::any::Any>) -> ChangeFlags {
        if let Ok(state) = state.downcast() {
            self.children = *state;
            cx.request_all(self.id);
            ChangeFlags::all()
        } else {
            ChangeFlags::empty()
        }
    }

    fn style(&mut self, cx: &mut crate::context::StyleCx) {
        for child in &mut self.children {
            cx.style_view(child);
        }
    }

    fn event(
        &mut self,
        cx: &mut EventCx,
        id_path: Option<&[Id]>,
        event: crate::event::Event,
    ) -> bool {
        for child in self.children.iter_mut() {
            if cx.view_event(child, id_path, event.clone()) {
                return true;
            }
        }
        false
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| {
            let nodes = self
                .children
                .iter_mut()
                .map(|child| cx.layout_view(child))
                .collect::<Vec<_>>();
            nodes
        })
    }

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) -> Option<Rect> {
        let mut layout_rect = Rect::ZERO;
        for child in &mut self.children {
            layout_rect = layout_rect.union(cx.compute_view_layout(child));
        }
        Some(layout_rect)
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        for child in self.children.iter_mut() {
            cx.paint_view(child);
        }
    }
}
