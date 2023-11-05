use crate::{id::Id, view::View};

/// A simple wrapper around another View. See [`container`]
pub struct Container {
    id: Id,
    child: Box<dyn View>,
}

/// A simple wrapper around another View
///
/// A [`Container`] is useful for wrapping another [View](crate::view::View). This is often useful for allowing another
/// set of styles completely separate from the View that is being wrapped.
pub fn container<V: View + 'static>(child: V) -> Container {
    Container {
        id: Id::next(),
        child: Box::new(child),
    }
}

impl View for Container {
    fn id(&self) -> Id {
        self.id
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn View) -> bool) {
        for_each(&self.child);
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn View) -> bool) {
        for_each(&mut self.child);
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Container".into()
    }
}
