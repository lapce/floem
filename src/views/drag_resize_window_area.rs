use kurbo::Rect;
use winit::window::ResizeDirection;

use crate::{
    action::drag_resize_window,
    event::{Event, EventListener},
    id::Id,
    style::CursorStyle,
    view::View,
};

use super::Decorators;

pub struct DragResizeWindowArea {
    id: Id,
    child: Box<dyn View>,
}

pub fn drag_resize_window_area<V: View + 'static>(
    direction: ResizeDirection,
    child: V,
) -> DragResizeWindowArea {
    let id = Id::next();
    DragResizeWindowArea {
        id,
        child: Box::new(child),
    }
    .on_event(EventListener::PointerDown, move |_| {
        drag_resize_window(direction);
        true
    })
    .base_style(move |s| {
        let cursor = match direction {
            ResizeDirection::East => CursorStyle::ColResize,
            ResizeDirection::West => CursorStyle::ColResize,
            ResizeDirection::North => CursorStyle::RowResize,
            ResizeDirection::South => CursorStyle::RowResize,
            ResizeDirection::NorthEast => CursorStyle::NeswResize,
            ResizeDirection::SouthWest => CursorStyle::NeswResize,
            ResizeDirection::SouthEast => CursorStyle::NwseResize,
            ResizeDirection::NorthWest => CursorStyle::NwseResize,
        };
        s.cursor(cursor)
    })
}

impl View for DragResizeWindowArea {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&self, id: Id) -> Option<&dyn View> {
        if self.child.id() == id {
            Some(&self.child)
        } else {
            None
        }
    }

    fn child_mut(&mut self, id: Id) -> Option<&mut dyn View> {
        if self.child.id() == id {
            Some(&mut self.child)
        } else {
            None
        }
    }

    fn children(&self) -> Vec<&dyn View> {
        vec![&self.child]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        vec![&mut self.child]
    }

    fn update(
        &mut self,
        _cx: &mut crate::context::UpdateCx,
        _state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        crate::view::ChangeFlags::empty()
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| vec![cx.layout_view(&mut self.child)])
    }

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) -> Option<Rect> {
        Some(cx.compute_view_layout(&mut self.child))
    }

    fn event(
        &mut self,
        cx: &mut crate::context::EventCx,
        id_path: Option<&[Id]>,
        event: Event,
    ) -> bool {
        cx.view_event(&mut self.child, id_path, event)
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.paint_view(&mut self.child);
    }
}
