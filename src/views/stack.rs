use kurbo::Rect;
use taffy::style::FlexDirection;

use crate::{
    context::{EventCx, UpdateCx},
    id::Id,
    style::Style,
    view::{ChangeFlags, View},
    view_tuple::ViewTuple,
};

pub struct Stack<VT> {
    id: Id,
    children: VT,
    direction: Option<FlexDirection>,
}

pub fn stack<VT: ViewTuple + 'static>(children: VT) -> Stack<VT> {
    let id = Id::next();
    Stack {
        id,
        children,
        direction: None,
    }
}

/// A stack which defaults to `FlexDirection::Row`.
pub fn h_stack<VT: ViewTuple + 'static>(children: VT) -> Stack<VT> {
    let id = Id::next();
    Stack {
        id,
        children,
        direction: Some(FlexDirection::Row),
    }
}

/// A stack which defaults to `FlexDirection::Column`.
pub fn v_stack<VT: ViewTuple + 'static>(children: VT) -> Stack<VT> {
    let id = Id::next();
    Stack {
        id,
        children,
        direction: Some(FlexDirection::Column),
    }
}

impl<VT: ViewTuple + 'static> View for Stack<VT> {
    fn id(&self) -> Id {
        self.id
    }

    fn view_style(&self) -> Option<crate::style::Style> {
        self.direction
            .map(|direction| Style::new().flex_direction(direction))
    }

    fn child(&self, id: Id) -> Option<&dyn View> {
        self.children.child(id)
    }

    fn child_mut(&mut self, id: Id) -> Option<&mut dyn View> {
        self.children.child_mut(id)
    }

    fn children(&self) -> Vec<&dyn View> {
        self.children.children()
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        self.children.children_mut()
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
        self.children.foreach_mut(&mut |view| {
            view.style_main(cx);
            false
        });
    }

    fn event(
        &mut self,
        cx: &mut EventCx,
        id_path: Option<&[Id]>,
        event: crate::event::Event,
    ) -> bool {
        let mut handled = false;
        self.children.foreach_rev(&mut |view| {
            let id = view.id();
            if cx.should_send(id, &event) {
                handled = view.event_main(cx, id_path, event.clone());
                if handled {
                    return true;
                }
            }
            false
        });
        handled
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| {
            let mut nodes = Vec::new();
            self.children.foreach_mut(&mut |view| {
                let node = view.layout_main(cx);
                nodes.push(node);
                false
            });
            nodes
        })
    }

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) -> Option<Rect> {
        let mut layout_rect = Rect::ZERO;
        self.children.foreach_mut(&mut |view| {
            layout_rect = layout_rect.union(view.compute_layout_main(cx));
            false
        });
        Some(layout_rect)
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        self.children.foreach_mut(&mut |view| {
            view.paint_main(cx);
            false
        });
    }
}
