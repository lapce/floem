use std::{any::Any, cell::RefCell, collections::HashMap, marker::PhantomData, rc::Rc};

use crate::{
    id::Id,
    runtime::RUNTIME,
    scope::{with_scope, Scope},
    signal::Signal,
};

pub(crate) trait EffectTrait {
    fn id(&self) -> Id;
    fn run(&self) -> bool;
    fn add_observer(&self, signal: Signal);
    fn current_observers(&self) -> HashMap<Id, Signal>;
    fn clear_observers(&self);
}

struct Effect<T, F>
where
    T: 'static,
    F: Fn(Option<T>) -> T,
{
    id: Id,
    f: F,
    value: Rc<RefCell<dyn Any>>,
    ty: PhantomData<T>,
    observers: Rc<RefCell<HashMap<Id, Signal>>>,
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

pub fn create_effect<T>(f: impl Fn(Option<T>) -> T + 'static)
where
    T: Any + 'static,
{
    let id = Id::next();
    let effect = Rc::new(Effect {
        id,
        f,
        value: Rc::new(RefCell::new(None::<T>)),
        ty: PhantomData,
        observers: Rc::new(RefCell::new(HashMap::new())),
    });
    id.set_scope();

    run_effect(effect);
}

pub fn untrack<T>(f: impl FnOnce() -> T) -> T {
    let prev_effect = RUNTIME.with(|runtime| runtime.current_effect.borrow_mut().take());
    let result = f();
    RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = prev_effect;
    });
    result
}

pub(crate) fn run_effect(effect: Rc<dyn EffectTrait>) {
    effect.id().dispose();

    observer_clean_up(&effect);

    RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = Some(effect.clone());
    });

    with_scope(Scope(effect.id()), move || {
        effect.run();
    });

    RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = None;
    });
}

pub(crate) fn observer_clean_up(effect: &Rc<dyn EffectTrait>) {
    for (_, observer) in effect.current_observers().iter() {
        observer.subscribers.borrow_mut().remove(&effect.id());
    }
    effect.clear_observers();
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
        let value = self.value.clone();

        let curr_value = {
            // downcast value
            let mut value = value.borrow_mut();
            let value = value
                .downcast_mut::<Option<T>>()
                .expect("to downcast effect value");
            value.take()
        };

        // run the effect
        let new_value = (self.f)(curr_value);

        // set new value
        let mut value = value.borrow_mut();
        let value = value
            .downcast_mut::<Option<T>>()
            .expect("to downcast effect value");
        *value = Some(new_value);

        true
    }

    fn add_observer(&self, signal: Signal) {
        self.observers.borrow_mut().insert(signal.id, signal);
    }

    fn current_observers(&self) -> HashMap<Id, Signal> {
        self.observers.borrow().clone()
    }

    fn clear_observers(&self) {
        self.observers.borrow_mut().clear();
    }
}
