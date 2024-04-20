use floem_winit::window::ResizeDirection;

use crate::{
    action::drag_resize_window,
    event::EventListener,
    id::Id,
    style::CursorStyle,
    view::{View, ViewData, Widget},
};

use super::Decorators;

/// A view that will resize the window when the mouse is dragged. See [`drag_resize_window_area`].
///
/// ## Platform-specific
///
/// - **macOS:** Not supported.
/// - **iOS / Android / Web / Orbital:** Not supported.
pub struct DragResizeWindowArea {
    data: ViewData,
    child: Box<dyn Widget>,
}

/// A view that will resize the window when the mouse is dragged.
///
/// ## Platform-specific
///
/// - **macOS:** Not supported.
/// - **iOS / Android / Web / Orbital:** Not supported.
pub fn drag_resize_window_area<V: Widget + 'static>(
    direction: ResizeDirection,
    child: V,
) -> DragResizeWindowArea {
    let id = Id::next();
    DragResizeWindowArea {
        data: ViewData::new(id),
        child: Box::new(child),
    }
    .on_event_stop(EventListener::PointerDown, move |_| {
        drag_resize_window(direction)
    })
    .style(move |s| {
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
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn build(self) -> Box<dyn Widget> {
        Box::new(self)
    }
}

impl Widget for DragResizeWindowArea {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Drag-Resize Window Area".into()
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn Widget) -> bool) {
        for_each(&self.child);
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn Widget) -> bool) {
        for_each(&mut self.child);
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn Widget) -> bool,
    ) {
        for_each(&mut self.child);
    }
}
