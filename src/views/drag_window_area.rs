use crate::{
    action::{drag_window, toggle_window_maximized},
    event::EventListener,
    id::Id,
    view::{AnyView, View, ViewData},
};

use super::Decorators;

pub struct DragWindowArea {
    data: ViewData,
    child: AnyView,
}

pub fn drag_window_area<V: View + 'static>(child: V) -> DragWindowArea {
    let id = Id::next();
    DragWindowArea {
        data: ViewData::new(id),
        child: Box::new(child),
    }
    .on_event_stop(EventListener::PointerDown, |_| drag_window())
    .on_double_click_stop(|_| toggle_window_maximized())
}

impl View for DragWindowArea {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
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
