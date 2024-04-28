use crate::{
    action::{drag_window, toggle_window_maximized},
    event::{Event, EventListener},
    id::Id,
    pointer::PointerButton,
    view::{View, ViewBuilder, ViewData},
};

use super::Decorators;

/// A view that will move the window when the mouse is dragged. See [`drag_window_area`].
pub struct DragWindowArea {
    data: ViewData,
    child: Box<dyn View>,
}

/// A view that will move the window when the mouse is dragged.
///
/// This can be useful when the window has the title bar turned off and you want to be able to still drag the window.
pub fn drag_window_area<V: View + 'static>(child: V) -> DragWindowArea {
    let id = Id::next();
    DragWindowArea {
        data: ViewData::new(id),
        child: Box::new(child),
    }
    .on_event_stop(EventListener::PointerDown, |e| {
        if let Event::PointerDown(input_event) = e {
            if input_event.button == PointerButton::Primary {
                drag_window();
            }
        }
    })
    .on_double_click_stop(|_| toggle_window_maximized())
}

impl ViewBuilder for DragWindowArea {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn build(self) -> Box<dyn View> {
        Box::new(self)
    }
}

impl View for DragWindowArea {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Drag Window Area".into()
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn View) -> bool) {
        for_each(&self.child);
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn View) -> bool) {
        for_each(&mut self.child);
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn View) -> bool,
    ) {
        for_each(&mut self.child);
    }
}
