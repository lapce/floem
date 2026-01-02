//! Binding handles for accessing locations in a Store.

use std::{collections::HashSet, marker::PhantomData, rc::Rc};

use floem_reactive::Runtime;

use std::hash::Hash;

use crate::{
    lens::{ComposedLens, IndexLens, KeyLens, Lens},
    path::PathId,
    store::{StoreId, StoreInner},
};

/// A handle pointing to a specific location in a Store.
///
/// Bindings are created using `#[derive(Lenses)]` which generates accessor methods
/// on the store and binding wrappers. They implement the standard reactive traits
/// so they can be used interchangeably with signals in generic code.
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
/// let store = StateStore::new(State::default());
///
/// // Access fields via generated methods
/// let count = store.count();
/// let name = store.name();
///
/// count.set(42);
/// name.set("Hello".into());
/// ```
pub struct Binding<Root: 'static, T: 'static, L: Lens<Root, T>> {
    pub(crate) store_id: StoreId,
    pub(crate) inner: Rc<StoreInner<Root>>,
    pub(crate) path_id: PathId,
    pub(crate) lens: L,
    pub(crate) _phantom: PhantomData<fn() -> T>,
}

// Binding is Clone (not Copy because it contains Rc<StoreInner>).
impl<Root: 'static, T: 'static, L: Lens<Root, T>> Clone for Binding<Root, T, L> {
    fn clone(&self) -> Self {
        Self {
            store_id: self.store_id,
            inner: self.inner.clone(),
            path_id: self.path_id,
            lens: self.lens,
            _phantom: PhantomData,
        }
    }
}

// Note: Binding cannot be Copy because it contains Rc<StoreInner>.
// This is a key design decision - we trade Copy for avoiding the scope lifetime issue.
// Users pass Binding by clone (cheap due to Rc) or by reference.

impl<Root: 'static, T: 'static, L: Lens<Root, T>> Binding<Root, T, L> {
    /// Derive a child binding using a lens type.
    ///
    /// This is used internally by the `#[derive(Lenses)]` macro to compose lenses.
    /// Users should prefer the generated accessor methods instead.
    #[doc(hidden)]
    pub fn binding_with_lens<U, L2>(
        &self,
        lens: L2,
    ) -> Binding<Root, U, ComposedLens<L, L2, T>>
    where
        U: 'static,
        L2: Lens<T, U>,
    {
        let new_lens = ComposedLens::new(self.lens, lens);
        Binding {
            store_id: self.store_id,
            inner: self.inner.clone(),
            path_id: PathId::from_hash(new_lens.path_hash()),
            lens: new_lens,
            _phantom: PhantomData,
        }
    }

    /// Get the current value (cloned).
    ///
    /// This subscribes the current effect to changes on this field.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.subscribe_current_effect();
        self.get_untracked()
    }

    /// Get the current value without subscribing to changes.
    pub fn get_untracked(&self) -> T
    where
        T: Clone,
    {
        self.lens.get(&self.inner.data.borrow()).clone()
    }

    /// Access the value by reference.
    ///
    /// This subscribes the current effect to changes on this field.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.subscribe_current_effect();
        self.with_untracked(f)
    }

    /// Access the value by reference without subscribing.
    pub fn with_untracked<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(self.lens.get(&self.inner.data.borrow()))
    }

    /// Set a new value.
    ///
    /// This notifies all subscribers of this field.
    pub fn set(&self, value: T) {
        self.update(|v| *v = value);
    }

    /// Update the value with a function.
    ///
    /// This notifies all subscribers of this field.
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        {
            let mut data = self.inner.data.borrow_mut();
            f(self.lens.get_mut(&mut *data));
        }
        self.notify_subscribers();
    }

    /// Try to update the value, returning the result of the function.
    pub fn try_update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let result = {
            let mut data = self.inner.data.borrow_mut();
            f(self.lens.get_mut(&mut *data))
        };
        self.notify_subscribers();
        result
    }

    /// Subscribe the current running effect to this field's changes.
    pub(crate) fn subscribe_current_effect(&self) {
        if let Some(effect_id) = Runtime::current_effect_id() {
            self.inner
                .subscribers
                .borrow_mut()
                .entry(self.path_id)
                .or_insert_with(HashSet::new)
                .insert(effect_id);
        }
    }

    /// Notify all subscribers that this field has changed.
    fn notify_subscribers(&self) {
        // Collect effect IDs first, then drop the borrow before notifying.
        // This avoids borrow conflicts when effects re-run and try to subscribe.
        let effect_ids: Vec<_> = {
            let subscribers = self.inner.subscribers.borrow();
            subscribers
                .get(&self.path_id)
                .map(|effects| effects.iter().copied().collect())
                .unwrap_or_default()
        };

        // Track dead effects for cleanup
        let mut dead_effects = Vec::new();

        for effect_id in effect_ids {
            if Runtime::effect_exists(effect_id) {
                Runtime::update_from_id(effect_id);
            } else {
                dead_effects.push(effect_id);
            }
        }

        // Clean up dead effect subscriptions
        if !dead_effects.is_empty() {
            let mut subscribers = self.inner.subscribers.borrow_mut();
            if let Some(effects) = subscribers.get_mut(&self.path_id) {
                for dead_id in dead_effects {
                    effects.remove(&dead_id);
                }
            }
        }
    }

    /// Get the path ID for this field (useful for debugging).
    pub fn path_id(&self) -> PathId {
        self.path_id
    }
}

// Vec-specific methods
impl<Root: 'static, T: 'static, L: Lens<Root, Vec<T>>> Binding<Root, Vec<T>, L> {
    /// Get a binding for the element at the given index.
    pub fn index(&self, index: usize) -> Binding<Root, T, ComposedLens<L, IndexLens, Vec<T>>> {
        let new_lens = ComposedLens::new(self.lens, IndexLens::new(index));
        Binding {
            store_id: self.store_id,
            inner: self.inner.clone(),
            // Note: All IndexLens bindings share the same PathId because IndexLens has
            // the same TypeId regardless of index value. This is a current limitation.
            path_id: PathId::from_hash(new_lens.path_hash()),
            lens: new_lens,
            _phantom: PhantomData,
        }
    }

    /// Get the length of the Vec.
    pub fn len(&self) -> usize {
        self.with_untracked(|v| v.len())
    }

    /// Check if the Vec is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Push an element to the Vec.
    pub fn push(&self, value: T) {
        self.update(|v| v.push(value));
    }

    /// Pop an element from the Vec.
    pub fn pop(&self) -> Option<T> {
        self.try_update(|v| v.pop())
    }

    /// Clear the Vec.
    pub fn clear(&self) {
        self.update(|v| v.clear());
    }

    /// Iterate over indices, returning Bindings for each element.
    ///
    /// Note: The returned iterator captures the current length. If the Vec
    /// is modified during iteration, behavior may be unexpected.
    pub fn iter_bindings(
        &self,
    ) -> impl Iterator<Item = Binding<Root, T, ComposedLens<L, IndexLens, Vec<T>>>> {
        let len = self.len();
        let store_id = self.store_id;
        let inner = self.inner.clone();
        let lens = self.lens;

        (0..len).map(move |i| {
            let new_lens = ComposedLens::new(lens, IndexLens::new(i));
            Binding {
                store_id,
                inner: inner.clone(),
                // Note: All IndexLens bindings share the same PathId
                path_id: PathId::from_hash(new_lens.path_hash()),
                lens: new_lens,
                _phantom: PhantomData,
            }
        })
    }
}

// HashMap-specific methods
impl<Root: 'static, K, V, L> Binding<Root, std::collections::HashMap<K, V>, L>
where
    K: Hash + Eq + Clone + 'static,
    V: 'static,
    L: Lens<Root, std::collections::HashMap<K, V>>,
{
    /// Get a binding for the value at the given key.
    ///
    /// # Panics
    ///
    /// Panics if the key is not present when accessing the binding.
    pub fn key(&self, key: K) -> Binding<Root, V, ComposedLens<L, KeyLens<K>, std::collections::HashMap<K, V>>>
    where
        K: Copy,
    {
        let new_lens = ComposedLens::new(self.lens, KeyLens::new(key));
        Binding {
            store_id: self.store_id,
            inner: self.inner.clone(),
            // Note: All KeyLens bindings share the same PathId because KeyLens<K> has
            // the same TypeId regardless of key value. This is a current limitation.
            path_id: PathId::from_hash(new_lens.path_hash()),
            lens: new_lens,
            _phantom: PhantomData,
        }
    }

    /// Get the number of entries in the HashMap.
    pub fn len(&self) -> usize {
        self.with_untracked(|m| m.len())
    }

    /// Check if the HashMap is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if the HashMap contains the given key.
    pub fn contains_key(&self, key: &K) -> bool {
        self.with_untracked(|m| m.contains_key(key))
    }

    /// Insert a key-value pair into the HashMap.
    ///
    /// Returns the old value if the key was already present.
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        self.try_update(|m| m.insert(key, value))
    }

    /// Remove a key from the HashMap.
    ///
    /// Returns the value if the key was present.
    pub fn remove(&self, key: &K) -> Option<V> {
        self.try_update(|m| m.remove(key))
    }

    /// Clear the HashMap.
    pub fn clear(&self) {
        self.update(|m| m.clear());
    }

    /// Get a cloned value for the given key, if present.
    pub fn get_value(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        self.with_untracked(|m| m.get(key).cloned())
    }

    /// Iterate over keys, returning Bindings for each value.
    ///
    /// Note: The returned iterator captures the current keys. If the HashMap
    /// is modified during iteration, behavior may be unexpected.
    pub fn iter_bindings(
        &self,
    ) -> impl Iterator<Item = (K, Binding<Root, V, ComposedLens<L, KeyLens<K>, std::collections::HashMap<K, V>>>)>
    where
        K: Copy,
    {
        let keys: Vec<K> = self.with_untracked(|m| m.keys().copied().collect());
        let store_id = self.store_id;
        let inner = self.inner.clone();
        let lens = self.lens;

        keys.into_iter().map(move |k| {
            let new_lens = ComposedLens::new(lens, KeyLens::new(k));
            (
                k,
                Binding {
                    store_id,
                    inner: inner.clone(),
                    // Note: All KeyLens bindings share the same PathId
                    path_id: PathId::from_hash(new_lens.path_hash()),
                    lens: new_lens,
                    _phantom: PhantomData,
                },
            )
        })
    }
}

// IndexMap-specific methods - O(1) key access with insertion order preservation
impl<Root: 'static, K, V, L> Binding<Root, indexmap::IndexMap<K, V>, L>
where
    K: Hash + Eq + Clone + 'static,
    V: 'static,
    L: Lens<Root, indexmap::IndexMap<K, V>>,
{
    /// Get a binding for the value at the given key (O(1) lookup).
    ///
    /// # Panics
    ///
    /// Panics if the key is not present when accessing the binding.
    pub fn key(&self, key: K) -> Binding<Root, V, ComposedLens<L, KeyLens<K>, indexmap::IndexMap<K, V>>>
    where
        K: Copy,
    {
        let new_lens = ComposedLens::new(self.lens, KeyLens::new(key));
        Binding {
            store_id: self.store_id,
            inner: self.inner.clone(),
            path_id: PathId::from_hash(new_lens.path_hash()),
            lens: new_lens,
            _phantom: PhantomData,
        }
    }

    /// Get the number of entries in the IndexMap.
    pub fn len(&self) -> usize {
        self.with_untracked(|m| m.len())
    }

    /// Check if the IndexMap is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if the IndexMap contains the given key.
    pub fn contains_key(&self, key: &K) -> bool {
        self.with_untracked(|m| m.contains_key(key))
    }

    /// Insert a key-value pair into the IndexMap.
    ///
    /// If the key already exists, the value is updated but position is preserved.
    /// Returns the old value if the key was already present.
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        self.try_update(|m| m.insert(key, value))
    }

    /// Remove a key from the IndexMap.
    ///
    /// Uses shift_remove to preserve insertion order of remaining elements.
    /// Returns the value if the key was present.
    pub fn remove(&self, key: &K) -> Option<V> {
        self.try_update(|m| m.shift_remove(key))
    }

    /// Clear the IndexMap.
    pub fn clear(&self) {
        self.update(|m| m.clear());
    }

    /// Get a cloned value for the given key, if present (O(1) lookup).
    pub fn get_value(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        self.with_untracked(|m| m.get(key).cloned())
    }

    /// Iterate over keys in insertion order, returning Bindings for each value.
    ///
    /// Note: The returned iterator captures the current keys. If the IndexMap
    /// is modified during iteration, behavior may be unexpected.
    pub fn iter_bindings(
        &self,
    ) -> impl Iterator<Item = (K, Binding<Root, V, ComposedLens<L, KeyLens<K>, indexmap::IndexMap<K, V>>>)>
    where
        K: Copy,
    {
        let keys: Vec<K> = self.with_untracked(|m| m.keys().copied().collect());
        let store_id = self.store_id;
        let inner = self.inner.clone();
        let lens = self.lens;

        keys.into_iter().map(move |k| {
            let new_lens = ComposedLens::new(lens, KeyLens::new(k));
            (
                k,
                Binding {
                    store_id,
                    inner: inner.clone(),
                    path_id: PathId::from_hash(new_lens.path_hash()),
                    lens: new_lens,
                    _phantom: PhantomData,
                },
            )
        })
    }
}
