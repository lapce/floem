use leptos_reactive::create_effect;
use taffy::style::{FlexDirection, Style};

use crate::{
    app::{AppContext, UpdateMessage},
    id::Id,
    view::{ChangeFlags, View},
    view_tuple::ViewTuple,
};

enum StackDirection {
    Horizontal,
    Vertical,
}

pub struct Stack<VT> {
    id: Id,
    children: VT,
    direction: StackDirection,
}

fn stack<VT: ViewTuple + 'static>(
    cx: AppContext,
    children: impl Fn(AppContext) -> VT + 'static + Copy,
    direction: StackDirection,
) -> Stack<VT> {
    let id = cx.id.new();
    let children_cx = cx.with_id(id);
    let children = children(children_cx);
    Stack {
        id,
        children,
        direction,
    }
}

pub fn hstack<VT: ViewTuple + 'static>(
    cx: AppContext,
    children: impl Fn(AppContext) -> VT + 'static + Copy,
) -> Stack<VT> {
    stack(cx, children, StackDirection::Horizontal)
}

pub fn vstack<VT: ViewTuple + 'static>(
    cx: AppContext,
    children: impl Fn(AppContext) -> VT + 'static + Copy,
) -> Stack<VT> {
    stack(cx, children, StackDirection::Vertical)
}

impl<VT: ViewTuple + 'static> View for Stack<VT> {
    type State = VT;

    fn id(&self) -> Id {
        self.id
    }

    fn update(&mut self, id_path: &[crate::id::Id], state: Box<dyn std::any::Any>) -> ChangeFlags {
        let id = id_path[0];
        let id_path = &id_path[1..];
        if id == self.id {
            if id_path.is_empty() {
                if let Ok(state) = state.downcast() {
                    self.children = *state;
                    ChangeFlags::LAYOUT
                } else {
                    ChangeFlags::empty()
                }
            } else {
                self.children.update(id_path, state)
            }
        } else {
            ChangeFlags::empty()
        }
    }

    fn event(&mut self, event: crate::event::Event) {
        self.children.event(event);
    }

    fn build_layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        let nodes = self.children.build_layout(cx);

        let direction = match self.direction {
            StackDirection::Horizontal => FlexDirection::Row,
            StackDirection::Vertical => FlexDirection::Column,
        };
        let node = cx
            .layout_state
            .taffy
            .new_with_children(
                Style {
                    size: taffy::prelude::Size {
                        width: taffy::style::Dimension::Auto,
                        height: taffy::style::Dimension::Auto,
                    },
                    flex_direction: direction,
                    ..Default::default()
                },
                &nodes,
            )
            .unwrap();
        let layout = cx.layout_state.layouts.entry(self.id()).or_default();
        layout.node = node;
        node
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) {
        let layout = cx.layout_state.layouts.entry(self.id()).or_default();
        layout.layout = *cx.layout_state.taffy.layout(layout.node).unwrap();
        self.children.layout(cx);
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.save();
        cx.transform(self.id());
        self.children.paint(cx);
        cx.restore();
    }
}
