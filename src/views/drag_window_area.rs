use crate::{
    action::{drag_window, toggle_window_maximized},
    event::{Event, EventListener},
    id::ViewId,
    pointer::PointerButton,
    view::{IntoView, View},
};

use super::Decorators;

/// A view that will move the window when the mouse is dragged. See [`drag_window_area`].
pub struct DragWindowArea {
    id: ViewId,
}

/// A view that will move the window when the mouse is dragged.
///
/// This can be useful when the window has the title bar turned off and you want to be able to still drag the window.
pub fn drag_window_area<V: IntoView + 'static>(child: V) -> DragWindowArea {
    let id = ViewId::new();
    id.set_children(vec![child.into_view()]);
    DragWindowArea { id }
        .on_event_stop(EventListener::PointerDown, |e| {
            if let Event::PointerDown(input_event) = e {
                if input_event.button == PointerButton::Primary {
                    drag_window();
                }
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
