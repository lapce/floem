use std::{
    any::Any,
    cell::RefCell,
    collections::HashSet,
    fmt,
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Mutex},
};

use crate::{
    create_effect, create_updater,
    id::Id,
    memo::{create_memo, Memo},
    runtime::{Runtime, RUNTIME},
    signal::{ReadSignal, RwSignal, Signal, SignalValue, WriteSignal},
    storage::{SyncStorage, UnsyncStorage},
    trigger::{create_trigger, Trigger},
};

/// You can manually control Signal's lifetime by using Scope.
///
/// Every Signal has a Scope created explicitly or implicitly,
/// and when you Dispose the Scope, it will clean up all the Signals
/// that belong to the Scope and all the child Scopes
#[derive(Clone, Copy)]
pub struct Scope(pub(crate) Id, pub(crate) PhantomData<()>);

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
        Self(Id::next(), PhantomData)
    }

    /// The current Scope in the Runtime. Any Signal/Effect/Memo created with
    /// implicitly Scope will be under this Scope
    pub fn current() -> Scope {
        RUNTIME.with(|runtime| Scope(*runtime.current_scope.borrow(), PhantomData))
    }

    /// Create a child Scope of this Scope
    pub fn create_child(&self) -> Scope {
        let child = Id::next();
        RUNTIME.with(|runtime| {
            let mut children = runtime.children.borrow_mut();
            let children = children.entry(self.0).or_default();
            children.insert(child);
        });
        Scope(child, PhantomData)
    }

    /// Create a new Signal under this Scope
    pub fn create_signal<T>(self, value: T) -> (ReadSignal<T>, WriteSignal<T>)
    where
        T: Any + 'static,
    {
        self.enter(|| RwSignal::new_split(value))
    }

    /// Create a RwSignal under this Scope (local/unsync by default)
    pub fn create_rw_signal<T>(self, value: T) -> RwSignal<T>
    where
        T: Any + 'static,
    {
        self.enter(|| RwSignal::new(value))
    }

    /// Create a sync Signal under this Scope
    pub fn create_sync_signal<T>(
        self,
        value: T,
    ) -> (ReadSignal<T, SyncStorage>, WriteSignal<T, SyncStorage>)
    where
        T: Any + Send + Sync + 'static,
    {
        self.enter(|| RwSignal::<T, SyncStorage>::new_sync_split(value))
    }

    /// Create a sync RwSignal under this Scope
    pub fn create_sync_rw_signal<T>(self, value: T) -> RwSignal<T, SyncStorage>
    where
        T: Any + Send + Sync + 'static,
    {
        self.enter(|| RwSignal::<T, SyncStorage>::new_sync(value))
    }

    /// Create a local (unsync) Signal under this Scope
    pub fn create_local_signal<T>(
        self,
        value: T,
    ) -> (ReadSignal<T, UnsyncStorage>, WriteSignal<T, UnsyncStorage>)
    where
        T: Any + 'static,
    {
        self.enter(|| RwSignal::new_split(value))
    }

    /// Create a local (unsync) RwSignal under this Scope
    pub fn create_local_rw_signal<T>(self, value: T) -> RwSignal<T, UnsyncStorage>
    where
        T: Any + 'static,
    {
        self.enter(|| RwSignal::new(value))
    }

    /// Create a Memo under this Scope
    pub fn create_memo<T>(self, f: impl Fn(Option<&T>) -> T + 'static) -> Memo<T>
    where
        T: PartialEq + 'static,
    {
        self.enter(|| create_memo(f))
    }

    /// Create a Trigger under this Scope
    pub fn create_trigger(self) -> Trigger {
        self.enter(create_trigger)
    }

    /// Create effect under this Scope
    pub fn create_effect<T>(self, f: impl Fn(Option<T>) -> T + 'static)
    where
        T: Any + 'static,
    {
        self.enter(|| create_effect(f))
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
        self.enter(|| create_updater(compute, on_change))
    }

    /// Runs the given closure within this scope.
    pub fn enter<T>(&self, f: impl FnOnce() -> T) -> T
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        let prev_scope = RUNTIME.with(|runtime| {
            let mut current_scope = runtime.current_scope.borrow_mut();
            let prev_scope = *current_scope;
            *current_scope = self.0;
            prev_scope
        });

        let result = f();

        RUNTIME.with(|runtime| {
            *runtime.current_scope.borrow_mut() = prev_scope;
        });

        result
    }

    /// Wraps a closure so it runs under a new child scope of this scope.
    pub fn enter_child<T, U>(&self, f: impl Fn(T) -> U + 'static) -> impl Fn(T) -> (U, Scope)
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        let parent = *self;
        move |t| {
            let scope = parent.create_child();
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

    /// This is normally used in create_effect, and it will bind the effect's lifetime
    /// to this scope
    pub fn track(&self) {
        Runtime::assert_ui_thread();
        let signal = if let Some(signal) = self.0.signal() {
            signal
        } else {
            let signal = Signal {
                id: self.0,
                subscribers: Arc::new(Mutex::new(HashSet::new())),
                value: SignalValue::Local(Rc::new(RefCell::new(()))),
            };
            self.0.add_signal(signal.clone());
            signal
        };
        signal.subscribe();
    }

    /// Dispose this Scope, and it will cleanup all the Signals and child Scope
    /// of this Scope.
    pub fn dispose(&self) {
        self.0.dispose();
    }
}

#[deprecated(
    since = "0.2.0",
    note = "Use Scope::enter instead; this will be removed in a future release"
)]
/// Runs the given code with the given Scope
pub fn with_scope<T>(scope: Scope, f: impl FnOnce() -> T) -> T
where
    T: 'static,
{
    scope.enter(f)
}

/// Wrap the closure so that whenever the closure runs, it will be under a child Scope
/// of the current Scope
#[deprecated(
    since = "0.2.0",
    note = "Use Scope::current().enter_child instead; this will be removed in a future release"
)]
pub fn as_child_of_current_scope<T, U>(f: impl Fn(T) -> U + 'static) -> impl Fn(T) -> (U, Scope)
where
    T: 'static,
{
    Scope::current().enter_child(f)
}
