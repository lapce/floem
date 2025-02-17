use std::{any::Any, cell::RefCell, collections::HashSet, marker::PhantomData, mem, rc::Rc};

use crate::{
    id::Id,
    runtime::RUNTIME,
    scope::{with_scope, Scope},
    signal::NotThreadSafe,
};

pub(crate) trait EffectTrait {
    fn id(&self) -> Id;
    fn run(&self) -> bool;
    fn add_observer(&self, id: Id);
    fn clear_observers(&self) -> HashSet<Id>;
}

struct Effect<T, F>
where
    T: 'static,
    F: Fn(Option<T>) -> T,
{
    id: Id,
    f: F,
    value: RefCell<Option<T>>,
    observers: RefCell<HashSet<Id>>,
    ts: PhantomData<NotThreadSafe>,
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
/// The given function will be run immediately once, and tracks all the signals that
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
        value: RefCell::new(None),
        observers: RefCell::new(HashSet::default()),
        ts: PhantomData,
    });
    id.set_scope();

    run_initial_effect(effect);
}

struct UpdaterEffect<T, I, C, U>
where
    C: Fn(Option<T>) -> (I, T),
    U: Fn(I, T) -> T,
{
    id: Id,
    compute: C,
    on_change: U,
    value: RefCell<Option<T>>,
    observers: RefCell<HashSet<Id>>,
}

impl<T, I, C, U> Drop for UpdaterEffect<T, I, C, U>
where
    C: Fn(Option<T>) -> (I, T),
    U: Fn(I, T) -> T,
{
    fn drop(&mut self) {
        self.id.dispose();
    }
}

/// Create an effect updater that runs `on_change` when any signals `compute` subscribes to
/// changes. `compute` is immediately run and its return value is returned from `create_updater`.
pub fn create_updater<R>(compute: impl Fn() -> R + 'static, on_change: impl Fn(R) + 'static) -> R
where
    R: 'static,
{
    create_stateful_updater(move |_| (compute(), ()), move |r, _| on_change(r))
}

/// Create an effect updater that runs `on_change` when any signals `compute` subscribes to
/// changes. `compute` is immediately run and its return value is returned from `create_updater`.
pub fn create_stateful_updater<T, R>(
    compute: impl Fn(Option<T>) -> (R, T) + 'static,
    on_change: impl Fn(R, T) -> T + 'static,
) -> R
where
    T: Any + 'static,
    R: 'static,
{
    let id = Id::next();
    let effect = Rc::new(UpdaterEffect {
        id,
        compute,
        on_change,
        value: RefCell::new(None),
        observers: RefCell::new(HashSet::default()),
    });
    id.set_scope();

    run_initial_updater_effect(effect)
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

pub fn batch<T>(f: impl FnOnce() -> T) -> T {
    let already_batching = RUNTIME.with(|runtime| {
        let batching = runtime.batching.get();
        if !batching {
            runtime.batching.set(true);
        }

        batching
    });

    let result = f();
    if !already_batching {
        RUNTIME.with(|runtime| {
            runtime.batching.set(false);
            runtime.run_pending_effects();
        });
    }

    result
}

pub(crate) fn run_initial_effect(effect: Rc<dyn EffectTrait>) {
    let effect_id = effect.id();

    RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = Some(effect.clone());

        let effect_scope = Scope(effect_id, PhantomData);
        with_scope(effect_scope, || {
            effect_scope.track();
            effect.run();
        });

        *runtime.current_effect.borrow_mut() = None;
    });
}

pub(crate) fn run_effect(effect: Rc<dyn EffectTrait>) {
    let effect_id = effect.id();
    effect_id.dispose();

    observer_clean_up(&effect);

    RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = Some(effect.clone());

        let effect_scope = Scope(effect_id, PhantomData);
        with_scope(effect_scope, move || {
            effect_scope.track();
            effect.run();
        });

        *runtime.current_effect.borrow_mut() = None;
    });
}

fn run_initial_updater_effect<T, I, C, U>(effect: Rc<UpdaterEffect<T, I, C, U>>) -> I
where
    T: 'static,
    I: 'static,
    C: Fn(Option<T>) -> (I, T) + 'static,
    U: Fn(I, T) -> T + 'static,
{
    let effect_id = effect.id();

    let result = RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = Some(effect.clone());

        let effect_scope = Scope(effect_id, PhantomData);
        let (result, new_value) = with_scope(effect_scope, || {
            effect_scope.track();
            (effect.compute)(None)
        });

        // set new value
        *effect.value.borrow_mut() = Some(new_value);

        *runtime.current_effect.borrow_mut() = None;

        result
    });

    result
}

/// Do a observer clean up at the beginning of each effect run. It clears the effect
/// from all the Signals that this effect subscribes to, and clears all the signals
/// that's stored in this effect, so that the next effect run can re-track signals.
pub(crate) fn observer_clean_up(effect: &Rc<dyn EffectTrait>) {
    let effect_id = effect.id();
    let observers = effect.clear_observers();
    for observer in observers {
        if let Some(signal) = observer.signal() {
            signal.subscribers.borrow_mut().remove(&effect_id);
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
        let curr_value = self.value.borrow_mut().take();

        // run the effect
        let new_value = (self.f)(curr_value);

        *self.value.borrow_mut() = Some(new_value);

        true
    }

    fn add_observer(&self, id: Id) {
        self.observers.borrow_mut().insert(id);
    }

    fn clear_observers(&self) -> HashSet<Id> {
        mem::take(&mut *self.observers.borrow_mut())
    }
}

impl<T, I, C, U> EffectTrait for UpdaterEffect<T, I, C, U>
where
    T: 'static,
    C: Fn(Option<T>) -> (I, T),
    U: Fn(I, T) -> T,
{
    fn id(&self) -> Id {
        self.id
    }

    fn run(&self) -> bool {
        let curr_value = self.value.borrow_mut().take();

        // run the effect
        let (i, t) = (self.compute)(curr_value);
        let new_value = (self.on_change)(i, t);

        *self.value.borrow_mut() = Some(new_value);
        true
    }

    fn add_observer(&self, id: Id) {
        self.observers.borrow_mut().insert(id);
    }

    fn clear_observers(&self) -> HashSet<Id> {
        mem::take(&mut *self.observers.borrow_mut())
    }
}
