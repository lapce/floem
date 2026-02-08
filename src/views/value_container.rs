use std::any::Any;

use floem_reactive::{Effect, RwSignal, SignalGet, SignalUpdate, UpdaterEffect};

use crate::{
    context::UpdateCx,
    view::ViewId,
    view::{IntoView, View},
};

/// A wrapper around another View that has value updates. See [`value_container`]
pub struct ValueContainer<T> {
    id: ViewId,
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

    let inbound_signal = RwSignal::new(initial_value.clone());
    Effect::new(move |_| {
        let checked = producer();
        inbound_signal.set(checked);
    });

    let outbound_signal = RwSignal::new(initial_value.clone());
    Effect::new(move |_| {
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
pub fn value_container<T: 'static, V: IntoView + 'static>(
    child: V,
    value_update: impl Fn() -> T + 'static,
) -> ValueContainer<T> {
    let id = ViewId::new();
    let child = child.into_view();
    id.set_children([child]);
    UpdaterEffect::new(value_update, move |new_value| id.update_state(new_value));
    ValueContainer {
        id,
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
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, _cx: &mut UpdateCx, state: Box<dyn Any>) {
        if let Ok(state) = state.downcast::<T>()
            && let Some(on_update) = self.on_update.as_ref()
        {
            on_update(*state);
        }
    }
}
