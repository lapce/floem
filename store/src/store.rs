//! Central state container.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    marker::PhantomData,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use floem_reactive::{ReactiveId, Runtime};

use crate::{
    binding::Binding,
    lens::{IdentityLens, Lens},
    path::PathId,
};

/// Unique identifier for a Store instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StoreId(u64);

impl StoreId {
    fn next() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        StoreId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

/// Internal state shared between Store and all its Fields.
pub(crate) struct StoreInner<T> {
    /// The actual data.
    pub(crate) data: RefCell<T>,
    /// Subscribers for each path: path_id -> set of effect ids.
    pub(crate) subscribers: RefCell<HashMap<PathId, HashSet<ReactiveId>>>,
    /// Pending updates to apply (for batching).
    pub(crate) pending_updates: RefCell<Vec<Box<dyn FnOnce(&mut T)>>>,
    /// Paths that have been modified and need notification.
    pub(crate) dirty_paths: RefCell<HashSet<PathId>>,
}

impl<T> StoreInner<T> {
    fn new(data: T) -> Self {
        Self {
            data: RefCell::new(data),
            subscribers: RefCell::new(HashMap::new()),
            pending_updates: RefCell::new(Vec::new()),
            dirty_paths: RefCell::new(HashSet::new()),
        }
    }
}

/// A central store that owns state and provides Binding handles for access.
///
/// Unlike signals, a Store is not tied to any scope. It lives as long as
/// there are references to it (via Rc). Bindings derived from the store
/// are Clone handles that point back to this store.
///
/// Use `#[derive(Lenses)]` on your state type to generate a typed store wrapper
/// with accessor methods. For advanced use cases, you can use `Store` directly
/// with `root()` to get a binding to the entire state.
///
/// # Example
///
/// ```rust,ignore
/// use floem_store::Lenses;
///
/// #[derive(Lenses, Default)]
/// struct State {
///     count: i32,
///     name: String,
/// }
///
/// // Generated wrapper provides typed access
/// let store = StateStore::new(State::default());
/// store.count().set(42);
/// store.name().set("Hello".into());
/// ```
pub struct Store<T: 'static> {
    pub(crate) id: StoreId,
    pub(crate) inner: Rc<StoreInner<T>>,
}

impl<T: 'static> Clone for Store<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            inner: self.inner.clone(),
        }
    }
}

impl<T: 'static> Store<T> {
    /// Create a new store with the given initial value.
    pub fn new(value: T) -> Self {
        Self {
            id: StoreId::next(),
            inner: Rc::new(StoreInner::new(value)),
        }
    }

    /// Get a Binding handle for the root of the store.
    ///
    /// This provides direct access to the entire state. For typed field access,
    /// use `#[derive(Lenses)]` which generates accessor methods.
    pub fn root(&self) -> Binding<T, T, IdentityLens<T>> {
        Binding {
            store_id: self.id,
            inner: self.inner.clone(),
            path_id: PathId::root(),
            lens: IdentityLens::default(),
            _phantom: PhantomData,
        }
    }

    /// Get a Binding handle using a lens type.
    ///
    /// This is used internally by the `#[derive(Lenses)]` macro.
    /// Users should prefer the generated accessor methods instead.
    #[doc(hidden)]
    pub fn binding_with_lens<U, L>(&self, lens: L) -> Binding<T, U, L>
    where
        U: 'static,
        L: Lens<T, U>,
    {
        Binding {
            store_id: self.id,
            inner: self.inner.clone(),
            path_id: PathId::from_hash(lens.path_hash()),
            lens,
            _phantom: PhantomData,
        }
    }

    /// Read the entire store value.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.inner.data.borrow())
    }

    /// Update the entire store value.
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        f(&mut self.inner.data.borrow_mut());
        self.notify_all();
    }

    /// Apply all pending updates and notify subscribers.
    pub fn flush(&self) {
        let updates: Vec<_> = self.inner.pending_updates.borrow_mut().drain(..).collect();
        if updates.is_empty() {
            return;
        }

        let mut data = self.inner.data.borrow_mut();
        for update in updates {
            update(&mut *data);
        }
        drop(data);

        self.flush_notifications();
    }

    /// Notify all subscribers of the root path.
    fn notify_all(&self) {
        self.inner.dirty_paths.borrow_mut().insert(PathId::root());
        self.flush_notifications();
    }

    /// Flush pending notifications to subscribers.
    pub(crate) fn flush_notifications(&self) {
        let dirty: HashSet<_> = self.inner.dirty_paths.borrow_mut().drain().collect();
        let subscribers = self.inner.subscribers.borrow();

        for path_id in dirty {
            if let Some(effects) = subscribers.get(&path_id) {
                for effect_id in effects {
                    // Trigger the effect to re-run by using floem_reactive's mechanism
                    // For now, we use direct notification
                    Self::notify_effect(*effect_id);
                }
            }
        }
    }

    fn notify_effect(effect_id: ReactiveId) {
        // Access the reactive runtime to trigger the effect
        // This integrates with floem_reactive's effect system
        Runtime::update_from_id(effect_id);
    }
}

impl<T: Default + 'static> Default for Store<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}
