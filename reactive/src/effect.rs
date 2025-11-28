use std::{any::Any, cell::RefCell, collections::HashSet, marker::PhantomData, mem, rc::Rc};

use crate::{
    id::Id,
    runtime::{Runtime, RUNTIME},
    scope::Scope,
};

pub(crate) trait EffectTrait: Any {
    fn id(&self) -> Id;
    fn run(&self) -> bool;
    fn add_observer(&self, id: Id);
    fn clear_observers(&self) -> HashSet<Id>;
    fn as_any(&self) -> &dyn Any;
}

/// Handle for a running effect. Prefer `Effect::new` over `create_effect`.
pub struct Effect<T, F>
where
    T: 'static,
    F: Fn(Option<T>) -> T,
{
    id: Id,
    f: F,
    value: RefCell<Option<T>>,
    observers: RefCell<HashSet<Id>>,
    ts: PhantomData<()>,
}

impl<T, F> Drop for Effect<T, F>
where
    T: 'static,
    F: Fn(Option<T>) -> T,
{
    fn drop(&mut self) {
        if RUNTIME
            .try_with(|runtime| runtime.remove_effect(self.id))
            .is_ok()
        {
            self.id.dispose();
        }
    }
}

/// Create an Effect that runs the given function whenever the subscribed Signals in that
/// function are updated.
///
/// The given function will be run immediately once and will track all signals that are
/// subscribed in that run. On each subsequent run the list is cleared and then
/// reconstructed based on the Signals that are subscribed during that run.
#[deprecated(
    since = "0.2.0",
    note = "Use Effect::new instead; this will be removed in a future release"
)]
pub fn create_effect<T>(f: impl Fn(Option<T>) -> T + 'static)
where
    T: Any + 'static,
{
    Effect::new(f);
}

impl<T, F> Effect<T, F>
where
    T: Any + 'static,
    F: Fn(Option<T>) -> T + 'static,
{
    #[allow(clippy::new_ret_no_self)]
    pub fn new(f: F) {
        Runtime::assert_ui_thread();
        let id = Id::next();
        let effect: Rc<dyn EffectTrait> = Rc::new(Self {
            id,
            f,
            value: RefCell::new(None),
            observers: RefCell::new(HashSet::default()),
            ts: PhantomData,
        });
        id.set_scope();
        RUNTIME.with(|runtime| runtime.register_effect(&effect));

        run_initial_effect(effect);
    }
}

/// Internal updater effect handle. Prefer the associated constructors over the free functions.
pub struct UpdaterEffect<T, I, C, U> {
    id: Id,
    compute: C,
    on_change: U,
    value: RefCell<Option<T>>,
    observers: RefCell<HashSet<Id>>,
    _phantom: PhantomData<I>,
}

impl<T, I, C, U> Drop for UpdaterEffect<T, I, C, U> {
    fn drop(&mut self) {
        if RUNTIME
            .try_with(|runtime| runtime.remove_effect(self.id))
            .is_ok()
        {
            self.id.dispose();
        }
    }
}

/// Create an effect updater that runs `on_change` when any signals that subscribe during the
/// run of `compute` are updated. `compute` is immediately run only once, and its value is returned
/// from the call to `create_updater`.
impl UpdaterEffect<(), (), (), ()> {
    /// Create an effect updater that runs `on_change` when any signals that subscribe during the
    /// run of `compute` are updated. `compute` is immediately run only once, and its value is returned
    /// from the call to `create_updater`.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<R>(compute: impl Fn() -> R + 'static, on_change: impl Fn(R) + 'static) -> R
    where
        R: 'static,
    {
        UpdaterEffect::new_stateful(move |_| (compute(), ()), move |r, _| on_change(r))
    }

    /// Create an effect updater that runs `on_change` when any signals within `compute` subscribe to
    /// changes. `compute` is immediately run and its return value is returned.
    #[allow(clippy::new_ret_no_self)]
    pub fn new_stateful<T, R>(
        compute: impl Fn(Option<T>) -> (R, T) + 'static,
        on_change: impl Fn(R, T) -> T + 'static,
    ) -> R
    where
        T: Any + 'static,
        R: 'static,
    {
        Runtime::assert_ui_thread();
        let id = Id::next();
        let effect = Rc::new(UpdaterEffect {
            id,
            compute,
            on_change,
            value: RefCell::new(None),
            observers: RefCell::new(HashSet::default()),
            _phantom: PhantomData,
        });
        id.set_scope();
        let effect_dyn: Rc<dyn EffectTrait> = effect.clone();
        RUNTIME.with(|runtime| runtime.register_effect(&effect_dyn));

        run_initial_updater_effect(effect)
    }
}

#[deprecated(
    since = "0.2.0",
    note = "Use UpdaterEffect::new instead; this will be removed in a future release"
)]
pub fn create_updater<R>(compute: impl Fn() -> R + 'static, on_change: impl Fn(R) + 'static) -> R
where
    R: 'static,
{
    UpdaterEffect::new(compute, on_change)
}

/// Create an effect updater that runs `on_change` when any signals within `compute` subscribe to
/// changes. `compute` is immediately run and its return value is returned from `create_updater`.
#[deprecated(
    since = "0.2.0",
    note = "Use UpdaterEffect::new_stateful instead; this will be removed in a future release"
)]
pub fn create_stateful_updater<T, R>(
    compute: impl Fn(Option<T>) -> (R, T) + 'static,
    on_change: impl Fn(R, T) -> T + 'static,
) -> R
where
    T: Any + 'static,
    R: 'static,
{
    UpdaterEffect::new_stateful(compute, on_change)
}

/// Signals that are wrapped with `untrack` will not subscribe to any effect.
impl Effect<(), fn(Option<()>) -> ()> {
    #[allow(clippy::new_ret_no_self)]
    pub fn untrack<T>(f: impl FnOnce() -> T) -> T {
        Runtime::assert_ui_thread();
        let prev_effect = RUNTIME.with(|runtime| runtime.current_effect.borrow_mut().take());
        let result = f();
        RUNTIME.with(|runtime| {
            *runtime.current_effect.borrow_mut() = prev_effect;
        });
        result
    }

    #[allow(clippy::new_ret_no_self)]
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
}

#[deprecated(
    since = "0.2.0",
    note = "Use Effect::untrack instead; this will be removed in a future release"
)]
pub fn untrack<T>(f: impl FnOnce() -> T) -> T {
    Effect::untrack(f)
}

#[deprecated(
    since = "0.2.0",
    note = "Use Effect::batch instead; this will be removed in a future release"
)]
pub fn batch<T>(f: impl FnOnce() -> T) -> T {
    Effect::batch(f)
}

pub(crate) fn run_initial_effect(effect: Rc<dyn EffectTrait>) {
    Runtime::assert_ui_thread();
    let effect_id = effect.id();

    RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = Some(effect.clone());

        let effect_scope = Scope(effect_id, PhantomData);
        effect_scope.enter(|| {
            effect.run();
        });

        *runtime.current_effect.borrow_mut() = None;
    });
}

pub(crate) fn run_effect(effect: Rc<dyn EffectTrait>) {
    Runtime::assert_ui_thread();
    let effect_id = effect.id();
    effect_id.dispose_children();

    observer_clean_up(&effect);

    RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = Some(effect.clone());

        let effect_scope = Scope(effect_id, PhantomData);
        effect_scope.enter(move || {
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
    Runtime::assert_ui_thread();
    let effect_id = effect.id();

    let result = RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = Some(effect.clone());

        let effect_scope = Scope(effect_id, PhantomData);
        let (result, new_value) = effect_scope.enter(|| (effect.compute)(None));

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
            signal.subscribers.lock().remove(&effect_id);
        }
    }
}

impl<T, F> EffectTrait for Effect<T, F>
where
    T: 'static,
    F: Fn(Option<T>) -> T + 'static,
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

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl<T, I, C, U> EffectTrait for UpdaterEffect<T, I, C, U>
where
    T: 'static,
    I: 'static,
    C: Fn(Option<T>) -> (I, T) + 'static,
    U: Fn(I, T) -> T + 'static,
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

    fn as_any(&self) -> &dyn Any {
        self
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

#[deprecated(
    since = "0.2.0",
    note = "Use SignalTracker::new instead; this will be removed in a future release"
)]
pub fn create_tracker(on_change: impl Fn() + 'static) -> SignalTracker {
    SignalTracker::new(on_change)
}

impl SignalTracker {
    /// Creates a [SignalTracker] that subscribes to any changes in signals used within `on_change`.
    pub fn new(on_change: impl Fn() + 'static) -> Self {
        let id = Id::next();

        SignalTracker {
            id,
            on_change: Rc::new(on_change),
        }
    }

    /// Updates the tracking function used for [SignalTracker].
    pub fn track<T: 'static>(&self, f: impl FnOnce() -> T) -> T {
        Runtime::assert_ui_thread();
        // Clear any previous tracking by disposing the old effect
        self.id.dispose();

        let prev_effect = RUNTIME.with(|runtime| runtime.current_effect.borrow_mut().take());

        let tracking_effect: Rc<dyn EffectTrait> = Rc::new(TrackingEffect {
            id: self.id,
            observers: RefCell::new(HashSet::default()),
            on_change: self.on_change.clone(),
        });

        RUNTIME.with(|runtime| {
            runtime.register_effect(&tracking_effect);
            *runtime.current_effect.borrow_mut() = Some(tracking_effect.clone());
        });

        let effect_scope = Scope(self.id, PhantomData);
        let result = effect_scope.enter(|| {
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

    fn as_any(&self) -> &dyn Any {
        self
    }
}
