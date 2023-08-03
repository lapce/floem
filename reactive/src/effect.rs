use std::{any::Any, cell::RefCell, collections::HashSet, marker::PhantomData, rc::Rc};

use crate::{
    id::Id,
    runtime::RUNTIME,
    scope::{with_scope, Scope},
};

pub(crate) trait EffectTrait {
    fn id(&self) -> Id;
    fn run(&self) -> bool;
    fn add_observer(&self, id: Id);
    fn clear_observers(&self) -> Option<HashSet<Id>>;
}

struct Effect<T, F>
where
    T: 'static,
    F: Fn(Option<T>) -> T,
{
    id: Id,
    f: F,
    value: RefCell<Box<dyn Any>>,
    ty: PhantomData<T>,
    observers: RefCell<Option<HashSet<Id>>>,
}

impl<T, F> Drop for Effect<T, F>
where
    T: 'static,
    F: Fn(Option<T>) -> T,
{
    fn drop(&mut self) {
        self.id.dispose();
    }
}

/// Create an Effect that runs the given function whenever the Signals that subscribed
/// to it in the function.
///
/// The given function will be run immdietly once, and tracks all the signals that
/// subscribed in that run. And when these Signals update, it will rerun the function.
/// And the effect re-tracks the signals in each run, so that it will only be re-run
/// by the Signals that actually ran in the last effect run.
pub fn create_effect<T>(f: impl Fn(Option<T>) -> T + 'static)
where
    T: Any + 'static,
{
    let id = Id::next();
    let effect = Rc::new(Effect {
        id,
        f,
        value: RefCell::new(Box::new(None::<T>)),
        ty: PhantomData,
        observers: RefCell::new(None),
    });
    id.set_scope();

    run_effect(effect);
}

/// Signals that's wrapped this untrack will not subscribe to any effect
pub fn untrack<T>(f: impl FnOnce() -> T) -> T {
    let prev_effect = RUNTIME.with(|runtime| runtime.current_effect.borrow_mut().take());
    let result = f();
    RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = prev_effect;
    });
    result
}

pub(crate) fn run_effect(effect: Rc<dyn EffectTrait>) {
    let effect_id = effect.id();
    effect_id.dispose();

    observer_clean_up(&effect);

    RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = Some(effect.clone());
    });

    let effect_scope = Scope(effect_id);
    with_scope(effect_scope, move || {
        effect_scope.track();
        effect.run();
    });

    RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = None;
    });
}

/// Do a observer clean up at the beginning of each effect run. It clears the effect
/// from all the Signals that this effect subscribes to, and clears all the signals
/// that's stored in this effect, so that the next effect run can re-track signals.
pub(crate) fn observer_clean_up(effect: &Rc<dyn EffectTrait>) {
    let effect_id = effect.id();
    let observers = effect.clear_observers();
    if let Some(observers) = observers {
        for observer in observers {
            if let Some(signal) = observer.signal() {
                signal.subscribers.borrow_mut().remove(&effect_id);
            }
        }
    }
}

impl<T, F> EffectTrait for Effect<T, F>
where
    T: 'static,
    F: Fn(Option<T>) -> T,
{
    fn id(&self) -> Id {
        self.id
    }

    fn run(&self) -> bool {
        let curr_value = {
            // downcast value
            let mut value = self.value.borrow_mut();
            let value = value
                .downcast_mut::<Option<T>>()
                .expect("to downcast effect value");
            value.take()
        };

        // run the effect
        let new_value = (self.f)(curr_value);

        // set new value
        let mut value = self.value.borrow_mut();
        let value = value
            .downcast_mut::<Option<T>>()
            .expect("to downcast effect value");
        *value = Some(new_value);

        true
    }

    fn add_observer(&self, id: Id) {
        let mut observers = self.observers.borrow_mut();
        if let Some(observers) = observers.as_mut() {
            observers.insert(id);
        } else {
            *observers = Some(HashSet::from_iter([id]));
        }
    }

    fn clear_observers(&self) -> Option<HashSet<Id>> {
        self.observers.borrow_mut().take()
    }
}
