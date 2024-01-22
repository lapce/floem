use std::any::Any;

use floem_reactive::{create_updater, ReadSignal};

use crate::{
    context::UpdateCx,
    id::Id,
    view::{View, ViewData},
};

/// A wrapper around another View that has value updates. See [`value_container`]
pub struct ValueContainer<T> {
    data: ViewData,
    child: Box<dyn View>,
    on_update: Option<Box<dyn Fn(T)>>,
}

/// A wrapper around another View that has value updates.
///
/// A [`ValueContainer`] is useful for wrapping another [View](crate::view::View).
/// This is to provide the `on_update` method which can notify when the view's
/// internal value was get changed
pub fn value_container<T: Clone + 'static, V: View + 'static>(
    child: V,
    value: ReadSignal<T>,
) -> ValueContainer<T> {
    let id = Id::next();
    create_updater(
        move || value.get(),
        move |new_value| id.update_state(new_value),
    );
    ValueContainer {
        data: ViewData::new(id),
        child: Box::new(child),
        on_update: None,
    }
}

impl<T> ValueContainer<T> {
    pub fn on_update(mut self, action: impl Fn(T) + 'static) -> Self {
        self.on_update = Some(Box::new(action));
        self
    }
}

impl<T: 'static> View for ValueContainer<T> {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn update(&mut self, _cx: &mut UpdateCx, state: Box<dyn Any>) {
        if let Ok(state) = state.downcast::<T>() {
            if let Some(on_update) = self.on_update.as_ref() {
                on_update(*state);
            }
        }
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
        "ValueContainer".into()
    }
}
