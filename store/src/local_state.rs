//! LocalState - a simple reactive value.
//!
//! `LocalState<T>` is a simpler alternative to `Store` + `Binding` when you just need
//! a single reactive value without complex nested state. It's Rc-based, so it:
//! - Automatically cleans up when all references are dropped
//! - Avoids the scope lifetime issues of arena-allocated signals
//! - Is Clone (not Copy, due to Rc)
//!
//! Use `LocalState` for simple values, use `Store` for complex nested state.

use std::{
    cell::RefCell,
    collections::HashSet,
    rc::Rc,
};

use floem_reactive::{ReactiveId, Runtime};

/// Inner storage for LocalState.
struct LocalStateInner<T> {
    data: RefCell<T>,
    subscribers: RefCell<HashSet<ReactiveId>>,
}

/// A simple reactive value.
///
/// `LocalState<T>` wraps a single value with reactive tracking. When the value
/// changes, all subscribed effects are notified and re-run.
///
/// # Example
///
/// ```rust
/// use floem_store::LocalState;
///
/// let count = LocalState::new(0);
///
/// // Read the value
/// assert_eq!(count.get(), 0);
///
/// // Update the value
/// count.set(42);
/// assert_eq!(count.get(), 42);
///
/// // Update with a closure
/// count.update(|c| *c += 1);
/// assert_eq!(count.get(), 43);
/// ```
///
/// # Reactivity
///
/// When read inside an effect, `LocalState` automatically subscribes to changes:
///
/// ```rust,ignore
/// use floem_reactive::Effect;
/// use floem_store::LocalState;
///
/// let name = LocalState::new("Alice".to_string());
///
/// // This effect re-runs whenever name changes
/// Effect::new(move |_| {
///     println!("Name is: {}", name.get());
/// });
///
/// name.set("Bob".to_string()); // Triggers effect re-run
/// ```
pub struct LocalState<T: 'static> {
    inner: Rc<LocalStateInner<T>>,
}

impl<T: 'static> Clone for LocalState<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: 'static> LocalState<T> {
    /// Create a new LocalState with the given initial value.
    pub fn new(value: T) -> Self {
        Self {
            inner: Rc::new(LocalStateInner {
                data: RefCell::new(value),
                subscribers: RefCell::new(HashSet::new()),
            }),
        }
    }

    /// Get the current value (cloned).
    ///
    /// This subscribes the current effect to changes on this value.
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
        self.inner.data.borrow().clone()
    }

    /// Access the value by reference.
    ///
    /// This subscribes the current effect to changes on this value.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.subscribe_current_effect();
        self.with_untracked(f)
    }

    /// Access the value by reference without subscribing.
    pub fn with_untracked<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.inner.data.borrow())
    }

    /// Set a new value.
    ///
    /// This notifies all subscribers.
    pub fn set(&self, value: T) {
        self.update(|v| *v = value);
    }

    /// Update the value with a function.
    ///
    /// This notifies all subscribers.
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        {
            let mut data = self.inner.data.borrow_mut();
            f(&mut *data);
        }
        self.notify_subscribers();
    }

    /// Try to update the value, returning the result of the function.
    pub fn try_update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let result = {
            let mut data = self.inner.data.borrow_mut();
            f(&mut *data)
        };
        self.notify_subscribers();
        result
    }

    /// Subscribe the current running effect to this value's changes.
    fn subscribe_current_effect(&self) {
        if let Some(effect_id) = Runtime::current_effect_id() {
            self.inner.subscribers.borrow_mut().insert(effect_id);
        }
    }

    /// Notify all subscribers that the value has changed.
    fn notify_subscribers(&self) {
        // Collect effect IDs first, then drop the borrow before notifying.
        // This avoids borrow conflicts when effects re-run and try to subscribe.
        let effect_ids: Vec<_> = self.inner.subscribers.borrow().iter().copied().collect();

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
            for dead_id in dead_effects {
                subscribers.remove(&dead_id);
            }
        }
    }
}

impl<T: Default + 'static> Default for LocalState<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

// Trait implementations for interoperability with floem_reactive

impl<T: Clone + 'static> floem_reactive::SignalGet<T> for LocalState<T> {
    fn id(&self) -> ReactiveId {
        panic!(
            "LocalState does not use ReactiveId. \
             Use LocalState's native methods instead of id()-based operations."
        )
    }

    fn get(&self) -> T {
        LocalState::get(self)
    }

    fn get_untracked(&self) -> T {
        LocalState::get_untracked(self)
    }

    fn try_get(&self) -> Option<T> {
        Some(LocalState::get(self))
    }

    fn try_get_untracked(&self) -> Option<T> {
        Some(LocalState::get_untracked(self))
    }
}

impl<T: 'static> floem_reactive::SignalWith<T> for LocalState<T> {
    fn id(&self) -> ReactiveId {
        panic!(
            "LocalState does not use ReactiveId. \
             Use LocalState's native methods instead of id()-based operations."
        )
    }

    fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O {
        LocalState::with(self, f)
    }

    fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O {
        LocalState::with_untracked(self, f)
    }

    fn try_with<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O {
        LocalState::with(self, |v| f(Some(v)))
    }

    fn try_with_untracked<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O {
        LocalState::with_untracked(self, |v| f(Some(v)))
    }
}

impl<T: 'static> floem_reactive::SignalUpdate<T> for LocalState<T> {
    fn id(&self) -> ReactiveId {
        panic!(
            "LocalState does not use ReactiveId. \
             Use LocalState's native methods instead of id()-based operations."
        )
    }

    fn set(&self, new_value: T) {
        LocalState::set(self, new_value);
    }

    fn update(&self, f: impl FnOnce(&mut T)) {
        LocalState::update(self, f);
    }

    fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O> {
        Some(LocalState::try_update(self, f))
    }
}

impl<T: 'static> floem_reactive::SignalTrack<T> for LocalState<T> {
    fn id(&self) -> ReactiveId {
        panic!(
            "LocalState does not use ReactiveId. \
             Use LocalState's native methods instead of id()-based operations."
        )
    }

    fn track(&self) {
        self.subscribe_current_effect();
    }

    fn try_track(&self) {
        self.subscribe_current_effect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_state_basic_get_set() {
        let state = LocalState::new(10);
        assert_eq!(state.get(), 10);

        state.set(20);
        assert_eq!(state.get(), 20);
    }

    #[test]
    fn local_state_update() {
        let state = LocalState::new(5);
        state.update(|v| *v *= 2);
        assert_eq!(state.get(), 10);
    }

    #[test]
    fn local_state_try_update() {
        let state = LocalState::new(vec![1, 2, 3]);
        let popped = state.try_update(|v| v.pop());
        assert_eq!(popped, Some(3));
        assert_eq!(state.get(), vec![1, 2]);
    }

    #[test]
    fn local_state_with() {
        let state = LocalState::new("hello".to_string());
        let len = state.with(|s| s.len());
        assert_eq!(len, 5);
    }

    #[test]
    fn local_state_clone() {
        let state1 = LocalState::new(42);
        let state2 = state1.clone();

        state1.set(100);
        // Both point to the same inner state
        assert_eq!(state2.get(), 100);
    }

    #[test]
    fn local_state_default() {
        let state: LocalState<i32> = LocalState::default();
        assert_eq!(state.get(), 0);

        let state: LocalState<String> = LocalState::default();
        assert_eq!(state.get(), "");
    }

    #[test]
    fn local_state_with_complex_type() {
        #[derive(Clone, Default, PartialEq, Debug)]
        struct User {
            name: String,
            age: i32,
        }

        let user = LocalState::new(User {
            name: "Alice".into(),
            age: 30,
        });

        assert_eq!(user.get().name, "Alice");

        user.update(|u| u.age += 1);
        assert_eq!(user.get().age, 31);
    }
}
