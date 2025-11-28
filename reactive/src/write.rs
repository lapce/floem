use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::Arc,
};

use parking_lot::{Mutex, MutexGuard};

use crate::{id::Id, signal::TrackedRefCell};

pub struct SyncWriteRef<'a, T> {
    id: Id,
    _handle: Arc<Mutex<T>>,
    pub(crate) guard: Option<MutexGuard<'a, T>>,
}

pub struct LocalWriteRef<'a, T> {
    id: Id,
    _handle: Rc<TrackedRefCell<T>>,
    pub(crate) guard: Option<crate::signal::TrackedRefMut<'a, T>>,
}

pub enum WriteRef<'a, T> {
    Sync(SyncWriteRef<'a, T>),
    Local(LocalWriteRef<'a, T>),
}

impl<'a, T> SyncWriteRef<'a, T> {
    pub(crate) fn new(id: Id, handle: Arc<Mutex<T>>) -> Self {
        let guard = handle.lock();
        let guard = unsafe { std::mem::transmute::<MutexGuard<'_, T>, MutexGuard<'a, T>>(guard) };
        Self {
            id,
            _handle: handle,
            guard: Some(guard),
        }
    }
}

impl<'a, T> Drop for SyncWriteRef<'a, T> {
    fn drop(&mut self) {
        if let Some(guard) = self.guard.take() {
            drop(guard);
        }
        if let Some(signal) = self.id.signal() {
            signal.run_effects();
        }
    }
}

impl<'a, T> LocalWriteRef<'a, T> {
    pub(crate) fn new(id: Id, handle: Rc<TrackedRefCell<T>>) -> Self {
        let guard = handle.borrow_mut();
        let guard = unsafe {
            std::mem::transmute::<
                crate::signal::TrackedRefMut<'_, T>,
                crate::signal::TrackedRefMut<'a, T>,
            >(guard)
        };
        Self {
            id,
            _handle: handle,
            guard: Some(guard),
        }
    }
}

impl<'a, T> Drop for LocalWriteRef<'a, T> {
    fn drop(&mut self) {
        if let Some(guard) = self.guard.take() {
            drop(guard);
        }
        if let Some(signal) = self.id.signal() {
            signal.run_effects();
        }
    }
}

impl<'a, T> Deref for WriteRef<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        match self {
            WriteRef::Sync(v) => v.guard.as_ref().expect("guard present"),
            WriteRef::Local(v) => v.guard.as_ref().expect("guard present"),
        }
    }
}

impl<'a, T> DerefMut for WriteRef<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            WriteRef::Sync(v) => &mut *v.guard.as_mut().expect("guard present"),
            WriteRef::Local(v) => &mut *v.guard.as_mut().expect("guard present"),
        }
    }
}

pub trait SignalUpdate<T> {
    /// get the Signal Id
    fn id(&self) -> Id;

    /// Sets the new_value to the Signal and triggers effect run
    fn set(&self, new_value: T)
    where
        T: 'static,
    {
        let _ = self.try_update(|v| *v = new_value);
    }

    /// Update the stored value with the given function and triggers effect run
    fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        let _ = self.try_update(f);
    }

    /// Update the stored value with the given function, triggers effect run,
    /// and returns the value returned by the function
    fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static;
}

pub trait SignalWrite<T> {
    /// get the Signal Id
    fn id(&self) -> Id;
    /// Mutably borrows the signal value, triggering subscribers when dropped.
    fn write(&self) -> WriteRef<'_, T>
    where
        T: 'static,
    {
        self.try_write().unwrap()
    }

    /// If the Signal isn't disposed, mutably borrows the signal value.
    fn try_write(&self) -> Option<WriteRef<'_, T>>
    where
        T: 'static;
}
