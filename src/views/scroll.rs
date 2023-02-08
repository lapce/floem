use glazier::kurbo::{Point, Size};
use taffy::{style::Position, Taffy};

use crate::{
    app::AppContext,
    event::Event,
    id::Id,
    view::{ChangeFlags, View},
};

pub struct Scroll<V: View> {
    id: Id,
    child: V,
    child_origin: Point,
}

pub fn scroll<V: View>(cx: AppContext, child: impl Fn(AppContext) -> V) -> Scroll<V> {
    let id = cx.new_id();

    let mut child_cx = cx;
    child_cx.id = id;
    let child = child(child_cx);

    Scroll {
        id,
        child,
        child_origin: Point::ZERO,
    }
}

impl<V: View> View for Scroll<V> {
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
        cx: &mut crate::context::UpdateCx,
        state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        ChangeFlags::empty()
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| {
            let child_node = self.child.layout(cx);
            let vritual_node = cx
                .layout_state
                .taffy
                .new_with_children(
                    taffy::prelude::Style {
                        position: Position::Absolute,
                        ..Default::default()
                    },
                    &[child_node],
                )
                .unwrap();
            vec![vritual_node]
        })
    }

    fn event(
        &mut self,
        cx: &mut crate::context::EventCx,
        id_path: Option<&[Id]>,
        event: crate::event::Event,
    ) -> bool {
        if self.child.event_main(cx, id_path, event.clone()) {
            return true;
        }
        if let Event::MouseWheel(mouse_event) = event {
            let child_size = cx
                .app_state
                .view_states
                .get(&self.id)
                .and_then(|view| view.children_nodes.as_ref())
                .and_then(|nodes| nodes.get(0))
                .and_then(|node| cx.app_state.taffy.layout(*node).ok())
                .map(|layout| Size::new(layout.size.width as f64, layout.size.height as f64))
                .unwrap_or_default();
            let size = cx
                .app_state
                .get_layout(self.id)
                .map(|layout| Size::new(layout.size.width as f64, layout.size.height as f64))
                .unwrap_or_default();

            self.child_origin -= mouse_event.wheel_delta;
            if size.width >= child_size.width {
                self.child_origin.x = 0.0;
            } else if self.child_origin.x < size.width - child_size.width {
                self.child_origin.x = size.width - child_size.width;
            } else if self.child_origin.x > 0.0 {
                self.child_origin.x = 0.0;
            }

            if size.height >= child_size.height {
                self.child_origin.y = 0.0;
            } else if self.child_origin.y < size.height - child_size.height {
                self.child_origin.y = size.height - child_size.height;
            } else if self.child_origin.y > 0.0 {
                self.child_origin.y = 0.0;
            }

            cx.request_layout(self.id);
        }

        true
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.save();
        cx.offset((self.child_origin.x, self.child_origin.y));
        self.child.paint_main(cx);
        cx.restore();
    }
}
