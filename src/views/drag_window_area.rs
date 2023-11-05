use crate::{
    action::{drag_window, toggle_window_maximized},
    event::EventListener,
    id::Id,
    view::View,
};

use super::Decorators;

pub struct DragWindowArea {
    id: Id,
    child: Box<dyn View>,
}

pub fn drag_window_area<V: View + 'static>(child: V) -> DragWindowArea {
    let id = Id::next();
    DragWindowArea {
        id,
        child: Box::new(child),
    }
    .on_event(EventListener::PointerDown, |_| {
        drag_window();
        true
    })
    .on_double_click(|_| {
        toggle_window_maximized();
        true
    })
}

impl View for DragWindowArea {
    fn id(&self) -> Id {
        self.id
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn View) -> bool) {
        for_each(&self.child);
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn View) -> bool) {
        for_each(&mut self.child);
    }
}
