use std::{
    any::Any, cell::RefCell, collections::HashSet, fmt, marker::PhantomData, rc::Rc, sync::Arc,
};

use parking_lot::Mutex;

use crate::{
    create_effect, create_updater,
    id::Id,
    memo::{create_memo, Memo},
    runtime::{Runtime, RUNTIME},
    signal::{ReadSignal, RwSignal, SignalState, SignalValue, WriteSignal},
    storage::{SyncStorage, UnsyncStorage},
    trigger::{create_trigger, Trigger},
};

/// You can manually control Signal's lifetime by using Scope.
///
/// Every Signal has a Scope created explicitly or implicitly,
/// and when you Dispose the Scope, it will clean up all the Signals
/// that belong to the Scope and all the child Scopes
#[derive(Clone, Copy, PartialEq, Eq)]
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
            runtime
                .children
                .borrow_mut()
                .entry(self.0)
                .or_default()
                .insert(child);
            runtime.parents.borrow_mut().insert(child, self.0);
        });
        Scope(child, PhantomData)
    }

    /// Re-parent this scope to be a child of another scope.
    ///
    /// If this scope already has a parent, it will be removed from that parent first.
    /// This is useful when the scope hierarchy needs to be adjusted after construction
    /// to match the view hierarchy.
    ///
    /// # Example
    /// ```rust
    /// # use floem_reactive::Scope;
    /// let parent = Scope::new();
    /// let child = Scope::new(); // Initially has no parent
    ///
    /// child.set_parent(parent);
    /// // Now child is a child of parent, and will be disposed when parent is disposed
    /// ```
    pub fn set_parent(&self, new_parent: Scope) {
        RUNTIME.with(|runtime| {
            // Remove from old parent's children set (if any)
            if let Some(old_parent) = runtime.parents.borrow_mut().remove(&self.0) {
                if let Some(children) = runtime.children.borrow_mut().get_mut(&old_parent) {
                    children.remove(&self.0);
                }
            }

            // Add to new parent's children set
            runtime
                .children
                .borrow_mut()
                .entry(new_parent.0)
                .or_default()
                .insert(self.0);

            // Set new parent
            runtime.parents.borrow_mut().insert(self.0, new_parent.0);
        });
    }

    /// Returns the parent scope of this scope, if any.
    pub fn parent(&self) -> Option<Scope> {
        RUNTIME.with(|runtime| {
            runtime
                .parents
                .borrow()
                .get(&self.0)
                .map(|id| Scope(*id, PhantomData))
        })
    }

    /// Create a new Signal under this Scope
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn create_signal<T>(self, value: T) -> (ReadSignal<T>, WriteSignal<T>)
    where
        T: Any + 'static,
    {
        self.enter(|| RwSignal::new_split(value))
    }

    /// Create a RwSignal under this Scope (local/unsync by default)
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn create_rw_signal<T>(self, value: T) -> RwSignal<T>
    where
        T: Any + 'static,
    {
        self.enter(|| RwSignal::new(value))
    }

    /// Create a sync Signal under this Scope
    #[cfg_attr(debug_assertions, track_caller)]
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
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn create_sync_rw_signal<T>(self, value: T) -> RwSignal<T, SyncStorage>
    where
        T: Any + Send + Sync + 'static,
    {
        self.enter(|| RwSignal::<T, SyncStorage>::new_sync(value))
    }

    /// Create a local (unsync) Signal under this Scope
    #[cfg_attr(debug_assertions, track_caller)]
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
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn create_local_rw_signal<T>(self, value: T) -> RwSignal<T, UnsyncStorage>
    where
        T: Any + 'static,
    {
        self.enter(|| RwSignal::new(value))
    }

    /// Create a Memo under this Scope
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn create_memo<T>(self, f: impl Fn(Option<&T>) -> T + 'static) -> Memo<T>
    where
        T: PartialEq + 'static,
    {
        self.enter(|| create_memo(f))
    }

    /// Create a Trigger under this Scope
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn create_trigger(self) -> Trigger {
        self.enter(create_trigger)
    }

    /// Create effect under this Scope
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn create_effect<T>(self, f: impl Fn(Option<T>) -> T + 'static)
    where
        T: Any + 'static,
    {
        self.enter(|| create_effect(f))
    }

    /// Create updater under this Scope
    #[cfg_attr(debug_assertions, track_caller)]
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

    /// Store a context value in this scope.
    ///
    /// The stored context value can be retrieved by this scope and any of its
    /// descendants using [`Scope::get_context`] or [`Context::get`](crate::Context::get).
    /// Child scopes can provide their own values of the same type, which will
    /// shadow the parent's value for that subtree.
    ///
    /// Context values are automatically cleaned up when the scope is disposed.
    ///
    /// # Example
    /// ```rust
    /// # use floem_reactive::Scope;
    /// let scope = Scope::new();
    /// scope.provide_context(42i32);
    /// scope.enter(|| {
    ///     assert_eq!(scope.get_context::<i32>(), Some(42));
    /// });
    /// ```
    pub fn provide_context<T>(&self, value: T)
    where
        T: Clone + 'static,
    {
        self.enter(|| crate::context::Context::provide(value))
    }

    /// Try to retrieve a stored context value from this scope or its ancestors.
    ///
    /// Context lookup walks up the scope tree from this scope to find the
    /// nearest ancestor that provides a value of the requested type.
    ///
    /// Note: This method must be called while the scope is entered (i.e., inside
    /// [`Scope::enter`]) for the lookup to work correctly.
    ///
    /// # Example
    /// ```rust
    /// # use floem_reactive::Scope;
    /// let parent = Scope::new();
    /// parent.provide_context(42i32);
    ///
    /// let child = parent.create_child();
    /// child.enter(|| {
    ///     // Child sees parent's context
    ///     assert_eq!(child.get_context::<i32>(), Some(42));
    /// });
    /// ```
    pub fn get_context<T>(&self) -> Option<T>
    where
        T: Clone + 'static,
    {
        self.enter(crate::context::Context::get::<T>)
    }

    /// Runs the given closure within this scope.
    #[cfg_attr(debug_assertions, track_caller)]
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
    #[cfg_attr(debug_assertions, track_caller)]
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
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn track(&self) {
        Runtime::assert_ui_thread();
        let signal = if let Some(signal) = self.0.signal() {
            signal
        } else {
            let signal = SignalState {
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
