use taffy::style::Dimension;

use crate::{app::AppContext, id::Id, view::View};

pub struct Width<V: View> {
    id: Id,
    dimension: Dimension,
    child: V,
}

pub fn width<V: View>(
    cx: AppContext,
    dimension: Dimension,
    child: impl Fn(AppContext) -> V,
) -> Width<V> {
    let id = cx.new_id();
    let mut child_cx = cx;
    child_cx.id = id;
    Width {
        id,
        dimension,
        child: child(child_cx),
    }
}

impl<V: View> View for Width<V> {
    type State = ();

    fn id(&self) -> crate::id::Id {
        self.id
    }

    fn update(
        &mut self,
        id_path: &[crate::id::Id],
        state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        self.child.update(&id_path[1..], state)
    }

    fn build_layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        let child_node = self.child.build_layout(cx);
        let node = cx
            .layout_state
            .taffy
            .new_with_children(
                taffy::style::Style {
                    size: taffy::prelude::Size {
                        width: self.dimension,
                        height: taffy::style::Dimension::Percent(1.0),
                    },
                    ..Default::default()
                },
                &[child_node],
            )
            .unwrap();
        let layout = cx.layout_state.layouts.entry(self.id()).or_default();
        layout.node = node;
        node
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) {
        let layout = cx.layout_state.layouts.entry(self.id()).or_default();
        layout.layout = *cx.layout_state.taffy.layout(layout.node).unwrap();
        self.child.layout(cx);
    }

    fn event(&mut self, event: crate::event::Event) {
        self.child.event(event);
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.save();
        cx.transform(self.id());
        self.child.paint(cx);
        cx.restore();
    }
}
