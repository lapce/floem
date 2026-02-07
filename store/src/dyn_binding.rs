//! Type-erased binding for simpler function signatures.
//!
//! `DynBinding<T>` wraps a `Binding<Root, T, L>` and erases the `Root` and `L`
//! type parameters, making it easier to pass bindings as function parameters.
//!
//! # Example
//!
//! ```rust,ignore
//! use floem_store::{DynBinding, SignalGet, SignalUpdate};
//!
//! // Instead of: fn foo<Root, L: Lens<Root, i32>>(binding: Binding<Root, i32, L>)
//! // You can write:
//! fn foo(binding: DynBinding<i32>) {
//!     let value = binding.get();
//!     binding.set(value + 1);
//! }
//!
//! // Convert a binding to DynBinding
//! let count = store.count();
//! foo(count.into_dyn());
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use floem_reactive::{ReactiveId, SignalGet, SignalTrack, SignalUpdate, SignalWith};

use crate::{binding::Binding, lens::Lens, path::PathId};

/// Trait for type-erased binding operations.
///
/// Uses `&mut dyn FnMut` for update operations to avoid 'static requirements.
trait DynBindingOps<T: 'static> {
    fn get(&self) -> T
    where
        T: Clone;
    fn get_untracked(&self) -> T
    where
        T: Clone;
    fn set(&self, value: T);
    fn update_with(&self, f: &mut dyn FnMut(&mut T));
    fn subscribe(&self);
    fn path_id(&self) -> PathId;
}

impl<Root: 'static, T: 'static, L: Lens<Root, T>> DynBindingOps<T> for Binding<Root, T, L> {
    fn get(&self) -> T
    where
        T: Clone,
    {
        Binding::get(self)
    }

    fn get_untracked(&self) -> T
    where
        T: Clone,
    {
        Binding::get_untracked(self)
    }

    fn set(&self, value: T) {
        Binding::set(self, value);
    }

    fn update_with(&self, f: &mut dyn FnMut(&mut T)) {
        Binding::update(self, |v| f(v));
    }

    fn subscribe(&self) {
        Binding::subscribe_current_effect(self);
    }

    fn path_id(&self) -> PathId {
        Binding::path_id(self)
    }
}

/// A type-erased binding that hides the `Root` and `Lens` type parameters.
///
/// This is useful when you want to pass bindings to functions without
/// exposing the full generic type. The trade-off is a small runtime cost
/// from dynamic dispatch.
///
/// # Example
///
/// ```rust,ignore
/// // A function that accepts any binding to an i32
/// fn increment(counter: &DynBinding<i32>) {
///     counter.update(|c| *c += 1);
/// }
///
/// let store = AppStateStore::new(AppState::default());
/// increment(&store.count().into_dyn());
/// ```
pub struct DynBinding<T: 'static> {
    inner: Rc<RefCell<Box<dyn DynBindingOps<T>>>>,
}

impl<T: 'static> Clone for DynBinding<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: 'static> DynBinding<T> {
    /// Create a new DynBinding from any Binding.
    pub fn new<Root: 'static, L: Lens<Root, T>>(binding: Binding<Root, T, L>) -> Self {
        Self {
            inner: Rc::new(RefCell::new(Box::new(binding))),
        }
    }

    /// Get the current value (cloned).
    ///
    /// This subscribes the current effect to changes on this field.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        let inner = self.inner.borrow();
        inner.subscribe();
        inner.get()
    }

    /// Get the current value without subscribing to changes.
    pub fn get_untracked(&self) -> T
    where
        T: Clone,
    {
        self.inner.borrow().get_untracked()
    }

    /// Set a new value.
    ///
    /// This notifies all subscribers of this field.
    pub fn set(&self, value: T) {
        self.inner.borrow().set(value);
    }

    /// Update the value with a function.
    ///
    /// This notifies all subscribers of this field.
    pub fn update(&self, mut f: impl FnMut(&mut T)) {
        self.inner.borrow().update_with(&mut f);
    }

    /// Try to update the value, returning the result of the function.
    pub fn try_update<R>(&self, mut f: impl FnMut(&mut T) -> R) -> R {
        let mut result = None;
        self.inner.borrow().update_with(&mut |v| {
            result = Some(f(v));
        });
        result.expect("update_with should have been called")
    }

    /// Get the path ID for this field (useful for debugging).
    pub fn path_id(&self) -> PathId {
        self.inner.borrow().path_id()
    }

    /// Subscribe the current running effect to this field's changes.
    pub fn track(&self) {
        self.inner.borrow().subscribe();
    }
}

// Add into_dyn method to Binding
impl<Root: 'static, T: 'static, L: Lens<Root, T>> Binding<Root, T, L> {
    /// Convert this binding to a type-erased `DynBinding<T>`.
    ///
    /// This erases the `Root` and `Lens` type parameters, making it easier
    /// to pass bindings as function parameters.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// fn show_value(binding: DynBinding<String>) {
    ///     println!("{}", binding.get());
    /// }
    ///
    /// let store = AppStateStore::new(AppState::default());
    /// show_value(store.name().into_dyn());
    /// ```
    pub fn into_dyn(self) -> DynBinding<T> {
        DynBinding::new(self)
    }

    /// Convert a reference to this binding to a `DynBinding<T>`.
    ///
    /// This clones the binding internally (cheap due to Rc).
    pub fn to_dyn(&self) -> DynBinding<T> {
        DynBinding::new(self.clone())
    }
}

// ============================================================================
// Reactive trait implementations for DynBinding
// ============================================================================

fn dyn_binding_id_unsupported() -> ReactiveId {
    panic!(
        "DynBinding does not use ReactiveId. \
         Use DynBinding's native methods instead of id()-based operations."
    )
}

impl<T: Clone + 'static> SignalGet<T> for DynBinding<T> {
    fn id(&self) -> ReactiveId {
        dyn_binding_id_unsupported()
    }

    fn get(&self) -> T
    where
        T: 'static,
    {
        DynBinding::get(self)
    }

    fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        DynBinding::get_untracked(self)
    }

    fn try_get(&self) -> Option<T>
    where
        T: 'static,
    {
        Some(DynBinding::get(self))
    }

    fn try_get_untracked(&self) -> Option<T>
    where
        T: 'static,
    {
        Some(DynBinding::get_untracked(self))
    }
}

impl<T: Clone + 'static> SignalWith<T> for DynBinding<T> {
    fn id(&self) -> ReactiveId {
        dyn_binding_id_unsupported()
    }

    fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        let inner = self.inner.borrow();
        inner.subscribe();
        f(&inner.get())
    }

    fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        f(&self.inner.borrow().get_untracked())
    }

    fn try_with<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        let inner = self.inner.borrow();
        inner.subscribe();
        f(Some(&inner.get()))
    }

    fn try_with_untracked<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        f(Some(&self.inner.borrow().get_untracked()))
    }
}

impl<T: 'static> SignalUpdate<T> for DynBinding<T> {
    fn id(&self) -> ReactiveId {
        dyn_binding_id_unsupported()
    }

    fn set(&self, new_value: T)
    where
        T: 'static,
    {
        DynBinding::set(self, new_value);
    }

    fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        // Wrap FnOnce in Option to call it as FnMut
        let mut f_opt = Some(f);
        self.inner.borrow().update_with(&mut |v| {
            if let Some(func) = f_opt.take() {
                func(v);
            }
        });
    }

    fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static,
    {
        let mut f_opt = Some(f);
        let mut result = None;
        self.inner.borrow().update_with(&mut |v| {
            if let Some(func) = f_opt.take() {
                result = Some(func(v));
            }
        });
        result
    }
}

impl<T: 'static> SignalTrack<T> for DynBinding<T> {
    fn id(&self) -> ReactiveId {
        dyn_binding_id_unsupported()
    }

    fn track(&self) {
        self.inner.borrow().subscribe();
    }

    fn try_track(&self) {
        self.inner.borrow().subscribe();
    }
}
