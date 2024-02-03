use crate::{
    id::Id,
    view::{AnyView, IntoAnyView, IntoView, View, ViewData},
};

/// A simple wrapper around another View. See [`container`]
pub struct Container {
    data: ViewData,
    child: AnyView,
}

/// A simple wrapper around another View
///
/// A [`Container`] is useful for wrapping another [View](crate::view::View). This is often useful for allowing another
/// set of styles completely separate from the View that is being wrapped.
pub fn container<V: IntoView + 'static>(child: V) -> Container {
    Container {
        data: ViewData::new(Id::next()),
        child: child.into_view().any(),
    }
}

impl View for Container {
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

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Container".into()
    }
}
