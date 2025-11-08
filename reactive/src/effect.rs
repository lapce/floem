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
    #[allow(dead_code)]
    fn hot_fn_ptr(&self) -> u64;
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

/// Create an Effect that runs the given function whenever the subscribed Signals in that
/// function are updated.
///
/// The given function will be run immediately once and will track all signals that are
/// subscribed in that run. On each subsequent run the list is cleared and then
/// reconstructed based on the Signals that are subscribed during that run.
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

/// Create an effect updater that runs `on_change` when any signals that subscribe during the
/// run of `compute` are updated. `compute` is immediately run only once, and its value is returned
/// from the call to `create_updater`.
pub fn create_updater<R>(compute: impl Fn() -> R + 'static, on_change: impl Fn(R) + 'static) -> R
where
    R: 'static,
{
    create_stateful_updater(move |_| (compute(), ()), move |r, _| on_change(r))
}

/// Create an effect updater that runs `on_change` when any signals within `compute` subscribe to
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

/// Signals that are wrapped with `untrack` will not subscribe to any effect.
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

    fn hot_fn_ptr(&self) -> u64 {
        std::ptr::addr_of!(self.f) as *const () as u64
        // HotFn::current(&self.f).ptr_address().0
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
    fn hot_fn_ptr(&self) -> u64 {
        std::ptr::addr_of!(self.compute) as *const () as u64
        // HotFn::current(&self.compute).ptr_address().0
    }
}

pub struct SignalTracker {
    id: Id,
    on_change: Rc<dyn Fn()>,
}

impl Drop for SignalTracker {
    fn drop(&mut self) {
        self.id.dispose();
    }
}

/// Creates a [SignalTracker] that subscribes to any changes in signals used within `on_change`.
pub fn create_tracker(on_change: impl Fn() + 'static) -> SignalTracker {
    let id = Id::next();

    SignalTracker {
        id,
        on_change: Rc::new(on_change),
    }
}

impl SignalTracker {
    /// Updates the tracking function used for [SignalTracker].
    pub fn track<T: 'static>(&self, f: impl FnOnce() -> T) -> T {
        // Clear any previous tracking by disposing the old effect
        self.id.dispose();

        let prev_effect = RUNTIME.with(|runtime| runtime.current_effect.borrow_mut().take());

        let tracking_effect = Rc::new(TrackingEffect {
            id: self.id,
            observers: RefCell::new(HashSet::default()),
            on_change: self.on_change.clone(),
        });

        RUNTIME.with(|runtime| {
            *runtime.current_effect.borrow_mut() = Some(tracking_effect.clone());
        });

        let effect_scope = Scope(self.id, PhantomData);
        let result = with_scope(effect_scope, || {
            effect_scope.track();
            f()
        });

        RUNTIME.with(|runtime| {
            *runtime.current_effect.borrow_mut() = prev_effect;
        });

        result
    }
}

struct TrackingEffect {
    id: Id,
    observers: RefCell<HashSet<Id>>,
    on_change: Rc<dyn Fn()>,
}

impl EffectTrait for TrackingEffect {
    fn id(&self) -> Id {
        self.id
    }

    fn run(&self) -> bool {
        (self.on_change)();
        true
    }

    fn add_observer(&self, id: Id) {
        self.observers.borrow_mut().insert(id);
    }

    fn clear_observers(&self) -> HashSet<Id> {
        mem::take(&mut *self.observers.borrow_mut())
    }
    fn hot_fn_ptr(&self) -> u64 {
        let rc_ptr = Rc::as_ptr(&self.on_change);
        rc_ptr as *const () as u64
        // HotFn::current(&*self.on_change).ptr_address().0
    }
}
#[cfg(feature = "hotpatch")]
pub use hotpatch::*;

#[cfg(feature = "hotpatch")]
mod hotpatch {
    use std::{cell::RefCell, collections::HashSet, marker::PhantomData, mem, rc::Rc};

    use dioxus_devtools::subsecond::{HotFn, HotFunction};

    use super::*;

    struct HotUpdaterEffect<R, C, U, M, T>
    where
        C: dioxus_devtools::subsecond::HotFunction<T, M, Return = R>,
        U: Fn(R),
    {
        id: Id,
        compute: RefCell<HotFn<T, M, C>>,
        on_change: U,
        observers: RefCell<HashSet<Id>>,
        phantom: PhantomData<M>,
    }
    impl<R, C, U, M, T> Drop for HotUpdaterEffect<R, C, U, M, T>
    where
        C: dioxus_devtools::subsecond::HotFunction<T, M, Return = R>,
        U: Fn(R),
    {
        fn drop(&mut self) {
            self.id.dispose();
        }
    }
    /// Create an effect updater that runs `on_change` when any signals that subscribe during the
    /// run of `compute` are updated. `compute` is immediately run only once, and its value is returned
    /// from the call to `create_updater`.
    pub fn create_hot_updater<T, R, C, M: 'static>(
        compute: HotFn<T, M, C>,
        on_change: impl Fn(R) + 'static,
    ) -> R
    where
        R: 'static,
        C: dioxus_devtools::subsecond::HotFunction<T, M, Return = R> + 'static,
        T: std::default::Default + 'static, // C: HotFunction<(), M, Return = R> + 'static,
    {
        let id = Id::next();
        let effect = Rc::new(HotUpdaterEffect {
            id,
            compute: RefCell::new(compute),
            on_change,
            observers: RefCell::new(HashSet::default()),
            phantom: PhantomData,
        });
        crate::runtime::register_effect(effect.clone());
        id.set_scope();
        run_initial_hot_updater_effect(effect)
    }
    impl<R, C, U, M, T: std::default::Default> EffectTrait for HotUpdaterEffect<R, C, U, M, T>
    where
        R: 'static,
        C: HotFunction<T, M, Return = R>,
        U: Fn(R),
    {
        fn id(&self) -> Id {
            self.id
        }

        fn run(&self) -> bool {
            let compute_fn = &self.compute;
            let result = compute_fn.borrow_mut().call(T::default());
            (self.on_change)(result);
            true
        }

        fn add_observer(&self, id: Id) {
            self.observers.borrow_mut().insert(id);
        }

        fn clear_observers(&self) -> HashSet<Id> {
            mem::take(&mut *self.observers.borrow_mut())
        }

        fn hot_fn_ptr(&self) -> u64 {
            let compute_fn = &self.compute;
            compute_fn.borrow().ptr_address().0
        }
    }
    fn run_initial_hot_updater_effect<R, C, U, M: 'static, T: std::default::Default + 'static>(
        effect: Rc<HotUpdaterEffect<R, C, U, M, T>>,
    ) -> R
    where
        R: 'static,
        C: HotFunction<T, M, Return = R> + 'static,
        U: Fn(R) + 'static,
    {
        let effect_id = effect.id();
        let result = RUNTIME.with(|runtime| {
            *runtime.current_effect.borrow_mut() = Some(effect.clone());
            let effect_scope = Scope(effect_id, PhantomData);
            let result = with_scope(effect_scope, || {
                effect_scope.track();
                let compute_fn = &effect.compute;
                compute_fn.borrow_mut().call(T::default())
            });
            *runtime.current_effect.borrow_mut() = None;
            result
        });
        result
    }
}
