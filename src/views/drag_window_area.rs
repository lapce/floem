use ui_events::pointer::{PointerButton, PointerButtonEvent, PointerEvent};

use crate::{
    action::{drag_window, toggle_window_maximized},
    event::{Event, EventListener},
    view::ViewId,
    view::{IntoView, View},
};

use super::Decorators;

/// A view that will move the window when the mouse is dragged. See [`drag_window_area`].
pub struct DragWindowArea {
    id: ViewId,
}

/// A view that will move the window when the mouse is dragged.
///
/// This can be used to allow dragging the window when the title bar is disabled.
pub fn drag_window_area<V: IntoView + 'static>(child: V) -> DragWindowArea {
    let id = ViewId::new();
    id.set_children([child]);
    DragWindowArea { id }
        .on_event_stop(EventListener::PointerDown, |e| {
            if let Event::Pointer(PointerEvent::Down(PointerButtonEvent { button, .. })) = e
                && button.is_some_and(|b| b == PointerButton::Primary)
            {
                drag_window();
            }
        })
        .on_double_click_stop(|_| toggle_window_maximized())
}
impl View for DragWindowArea {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Drag Window Area".into()
    }
}
