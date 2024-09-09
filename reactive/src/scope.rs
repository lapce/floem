use std::{any::Any, cell::RefCell, collections::HashMap, fmt, rc::Rc};

use crate::{
    create_effect, create_updater,
    id::Id,
    memo::{create_memo, Memo},
    runtime::RUNTIME,
    signal::{create_rw_signal, create_signal, ReadSignal, RwSignal, Signal, WriteSignal},
    trigger::{create_trigger, Trigger},
};

/// You can manually control Signal's lifetime by using Scope.
/// Every Signal has a Scope created explicitly or implicitly,
/// and when you Dispose the Scope, it will clean up all the Signals
/// that belong to the Scope and all the child Scopes
#[derive(Clone, Copy)]
pub struct Scope(pub(crate) Id);

impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("Scope");
        s.field("id", &self.0);
        s.finish()
    }
}

impl Scope {
    /// Create a new Scope that isn't a child or parent of any scope
    pub fn new() -> Self {
        Self(Id::next())
    }

    /// The current Scope in the Runtime. Any Signal/Effect/Memo created with
    /// implicitly Scope will be under this Scope
    pub fn current() -> Scope {
        RUNTIME.with(|runtime| Scope(*runtime.current_scope.borrow()))
    }

    /// Create a child Scope of this Scope
    pub fn create_child(&self) -> Scope {
        let child = Id::next();
        RUNTIME.with(|runtime| {
            let mut children = runtime.children.borrow_mut();
            let children = children.entry(self.0).or_default();
            children.insert(child);
        });
        Scope(child)
    }

    /// Create a new Signal under this Scope
    pub fn create_signal<T>(self, value: T) -> (ReadSignal<T>, WriteSignal<T>)
    where
        T: Any + 'static,
    {
        with_scope(self, || create_signal(value))
    }

    /// Create a RwSignal under this Scope
    pub fn create_rw_signal<T>(self, value: T) -> RwSignal<T>
    where
        T: Any + 'static,
    {
        with_scope(self, || create_rw_signal(value))
    }

    /// Create a Memo under this Scope
    pub fn create_memo<T>(self, f: impl Fn(Option<&T>) -> T + 'static) -> Memo<T>
    where
        T: PartialEq + 'static,
    {
        with_scope(self, || create_memo(f))
    }

    /// Create a Trigger under this Scope
    pub fn create_trigger(self) -> Trigger {
        with_scope(self, create_trigger)
    }

    /// Create effect under this Scope
    pub fn create_effect<T>(self, f: impl Fn(Option<T>) -> T + 'static)
    where
        T: Any + 'static,
    {
        with_scope(self, || create_effect(f))
    }

    /// Create updater under this Scope
    pub fn create_updater<R>(
        self,
        compute: impl Fn() -> R + 'static,
        on_change: impl Fn(R) + 'static,
    ) -> R
    where
        R: 'static,
    {
        with_scope(self, || create_updater(compute, on_change))
    }

    /// This is normally used in create_effect, and it will bind the effect's lifetime
    /// to this scope
    pub fn track(&self) {
        let tracker = if let Some(signal) = self.0.signal() {
            signal
        } else {
            let signal = Signal {
                id: self.0,
                subscribers: Rc::new(RefCell::new(HashMap::new())),
                value: Rc::new(RefCell::new(())),
            };
            self.0.add_signal(signal.clone());
            signal
        };
        tracker.subscribe();
    }

    /// Dispose this Scope, and it will cleanup all the Signals and child Scope
    /// of this Scope.
    pub fn dispose(&self) {
        self.0.dispose();
    }
}

/// Runs the given code with the given Scope
pub fn with_scope<T>(scope: Scope, f: impl FnOnce() -> T) -> T
where
    T: 'static,
{
    let prev_scope = RUNTIME.with(|runtime| {
        let mut current_scope = runtime.current_scope.borrow_mut();
        let prev_scope = *current_scope;
        *current_scope = scope.0;
        prev_scope
    });

    let result = f();

    RUNTIME.with(|runtime| {
        *runtime.current_scope.borrow_mut() = prev_scope;
    });

    result
}

/// Wrap the closure so that whenever the closure runs, it will be under a child Scope
/// of the current Scope
pub fn as_child_of_current_scope<T, U>(f: impl Fn(T) -> U + 'static) -> impl Fn(T) -> (U, Scope)
where
    T: 'static,
{
    let current_scope = Scope::current();
    move |t| {
        let scope = current_scope.create_child();
        let prev_scope = RUNTIME.with(|runtime| {
            let mut current_scope = runtime.current_scope.borrow_mut();
            let prev_scope = *current_scope;
            *current_scope = scope.0;
            prev_scope
        });

        let result = f(t);

        RUNTIME.with(|runtime| {
            *runtime.current_scope.borrow_mut() = prev_scope;
        });

        (result, scope)
    }
}
