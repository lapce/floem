use glazier::kurbo::{Point, Rect, Size};
use taffy::style::Position;

use crate::{
    app::AppContext,
    context::{AppState, LayoutCx},
    event::Event,
    id::Id,
    view::{ChangeFlags, View},
};

pub struct Scroll<V: View> {
    id: Id,
    child: V,
    child_viewport: Rect,
    onscroll: Option<Box<dyn Fn(Rect)>>,
}

pub fn scroll<V: View>(cx: AppContext, child: impl Fn(AppContext) -> V) -> Scroll<V> {
    let id = cx.new_id();

    let mut child_cx = cx;
    child_cx.id = id;
    let child = child(child_cx);

    Scroll {
        id,
        child,
        child_viewport: Rect::ZERO,
        onscroll: None,
    }
}

impl<V: View> Scroll<V> {
    pub fn onscroll(mut self, onscroll: impl Fn(Rect) + 'static) -> Self {
        self.onscroll = Some(Box::new(onscroll));
        self
    }

    fn clamp_child_viewport(
        &mut self,
        app_state: &mut AppState,
        child_viewport: Rect,
    ) -> Option<()> {
        let size = self.size(app_state)?;
        let child_size = self.child_size(app_state)?;

        let mut child_viewport = child_viewport;
        if size.width >= child_size.width {
            child_viewport.x0 = 0.0;
        } else if child_viewport.x0 < size.width - child_size.width {
            child_viewport.x0 = size.width - child_size.width;
        } else if child_viewport.x0 > 0.0 {
            child_viewport.x0 = 0.0;
        }

        if size.height >= child_size.height {
            child_viewport.y0 = 0.0;
        } else if child_viewport.y0 < size.height - child_size.height {
            child_viewport.y0 = size.height - child_size.height;
        } else if child_viewport.y0 > 0.0 {
            child_viewport.y0 = 0.0;
        }
        child_viewport = child_viewport.with_size(size);

        if child_viewport != self.child_viewport {
            app_state.set_viewport(self.child.id(), child_viewport);
            app_state.request_layout(self.id);
            self.child_viewport = child_viewport;
            if let Some(onscroll) = &self.onscroll {
                onscroll(child_viewport);
            }
        }
        Some(())
    }

    fn child_size(&self, app_state: &mut AppState) -> Option<Size> {
        app_state
            .view_states
            .get(&self.id)
            .and_then(|view| view.children_nodes.as_ref())
            .and_then(|nodes| nodes.get(0))
            .and_then(|node| app_state.taffy.layout(*node).ok())
            .map(|layout| Size::new(layout.size.width as f64, layout.size.height as f64))
    }

    fn size(&self, app_state: &mut AppState) -> Option<Size> {
        app_state
            .get_layout(self.id)
            .map(|layout| Size::new(layout.size.width as f64, layout.size.height as f64))
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

    fn compute_layout(&mut self, cx: &mut LayoutCx) {
        self.clamp_child_viewport(cx.layout_state, self.child_viewport);
        self.child.compute_layout(cx);
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
            self.clamp_child_viewport(cx.app_state, self.child_viewport - mouse_event.wheel_delta);
        }

        true
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.save();
        cx.offset((self.child_viewport.x0, self.child_viewport.y0));
        self.child.paint_main(cx);
        cx.restore();
    }
}
