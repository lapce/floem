//! Implementation of floem_reactive traits for Binding.
//!
//! This allows Bindings to be used interchangeably with Signals in generic code.

use floem_reactive::{ReactiveId, SignalGet, SignalTrack, SignalUpdate, SignalWith};

use crate::{binding::Binding, lens::Lens};

// Binding doesn't use the reactive runtime's Id system for storage.
// The id() method on traits shouldn't be called for Binding in practice,
// but we need to provide an implementation.
fn binding_id_unsupported() -> ReactiveId {
    panic!(
        "Binding does not use ReactiveId. \
         Use Binding's native methods instead of id()-based operations."
    )
}

impl<Root: 'static, T: Clone + 'static, L: Lens<Root, T>> SignalGet<T> for Binding<Root, T, L> {
    fn id(&self) -> ReactiveId {
        binding_id_unsupported()
    }

    fn get(&self) -> T
    where
        T: 'static,
    {
        Binding::get(self)
    }

    fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        Binding::get_untracked(self)
    }

    fn try_get(&self) -> Option<T>
    where
        T: 'static,
    {
        Some(Binding::get(self))
    }

    fn try_get_untracked(&self) -> Option<T>
    where
        T: 'static,
    {
        Some(Binding::get_untracked(self))
    }
}

impl<Root: 'static, T: 'static, L: Lens<Root, T>> SignalWith<T> for Binding<Root, T, L> {
    fn id(&self) -> ReactiveId {
        binding_id_unsupported()
    }

    fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        Binding::with(self, f)
    }

    fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        Binding::with_untracked(self, f)
    }

    fn try_with<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        Binding::with(self, |v| f(Some(v)))
    }

    fn try_with_untracked<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        Binding::with_untracked(self, |v| f(Some(v)))
    }
}

impl<Root: 'static, T: 'static, L: Lens<Root, T>> SignalUpdate<T> for Binding<Root, T, L> {
    fn id(&self) -> ReactiveId {
        binding_id_unsupported()
    }

    fn set(&self, new_value: T)
    where
        T: 'static,
    {
        Binding::set(self, new_value);
    }

    fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        Binding::update(self, f);
    }

    fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static,
    {
        Some(Binding::try_update(self, f))
    }
}

impl<Root: 'static, T: 'static, L: Lens<Root, T>> SignalTrack<T> for Binding<Root, T, L> {
    fn id(&self) -> ReactiveId {
        binding_id_unsupported()
    }

    fn track(&self) {
        self.subscribe_current_effect();
    }

    fn try_track(&self) {
        self.subscribe_current_effect();
    }
}
