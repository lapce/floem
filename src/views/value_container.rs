use std::any::Any;

use floem_reactive::{create_effect, create_rw_signal, create_updater, RwSignal};

use crate::{
    context::UpdateCx,
    id::Id,
    view::{View, ViewData, Widget},
};

/// A wrapper around another View that has value updates. See [`value_container`]
pub struct ValueContainer<T> {
    data: ViewData,
    child: Box<dyn Widget>,
    on_update: Option<Box<dyn Fn(T)>>,
}

/// A convenience function that creates two signals for use in a [`value_container`]
/// - The outbound signal enables a widget's internal input event handlers
///   to publish state changes via `ValueContainer::on_update`.
/// - The inbound signal propagates value changes in the producer function
///   into a widget's internals.
pub fn create_value_container_signals<T>(
    producer: impl Fn() -> T + 'static,
) -> (RwSignal<T>, RwSignal<T>)
where
    T: Clone + 'static,
{
    let initial_value = producer();

    let inbound_signal = create_rw_signal(initial_value.clone());
    create_effect(move |_| {
        let checked = producer();
        inbound_signal.set(checked);
    });

    let outbound_signal = create_rw_signal(initial_value.clone());
    create_effect(move |_| {
        let checked = outbound_signal.get();
        inbound_signal.set(checked);
    });

    (inbound_signal, outbound_signal)
}

/// A wrapper around another View that has value updates.
///
/// A [`ValueContainer`] is useful for wrapping another [View](crate::view::View).
/// This is to provide the `on_update` method which can notify when the view's
/// internal value was get changed
pub fn value_container<T: 'static, V: View + 'static>(
    child: V,
    value_update: impl Fn() -> T + 'static,
) -> ValueContainer<T> {
    let id = Id::next();
    create_updater(value_update, move |new_value| id.update_state(new_value));
    ValueContainer {
        data: ViewData::new(id),
        child: child.build(),
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

    fn build(self) -> Box<dyn Widget> {
        Box::new(self)
    }
}

impl<T: 'static> Widget for ValueContainer<T> {
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

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "ValueContainer".into()
    }
}
