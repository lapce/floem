use std::{
    cell::{RefCell, RefMut},
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::{Arc, Mutex, MutexGuard},
};

use crate::id::Id;

#[derive(Clone)]
pub struct WriteSignalValue<T> {
    pub(crate) id: Id,
    pub(crate) value: ValueHandle<T>,
}

impl<T> Drop for WriteSignalValue<T> {
    fn drop(&mut self) {
        if let Some(signal) = self.id.signal() {
            signal.run_effects();
        }
    }
}

impl<T> WriteSignalValue<T> {
    /// Mutably borrows the current value stored in the Signal
    pub fn borrow_mut(&self) -> WriteBorrow<'_, T> {
        match &self.value {
            ValueHandle::Sync(v) => WriteBorrow::Sync(v.lock().unwrap()),
            ValueHandle::Local(v) => WriteBorrow::Local(v.borrow_mut()),
        }
    }
}

#[derive(Clone)]
pub enum ValueHandle<T> {
    Sync(Arc<Mutex<T>>),
    Local(Rc<RefCell<T>>),
}

pub enum WriteBorrow<'a, T> {
    Sync(MutexGuard<'a, T>),
    Local(RefMut<'a, T>),
}

impl<'a, T> Deref for WriteBorrow<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        match self {
            WriteBorrow::Sync(v) => v,
            WriteBorrow::Local(v) => v,
        }
    }
}

impl<'a, T> DerefMut for WriteBorrow<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            WriteBorrow::Sync(v) => &mut *v,
            WriteBorrow::Local(v) => &mut *v,
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
    /// Convert the Signal to `WriteSignalValue` where it holds a Mutex-protected
    /// reference to the signal data, so that you can `borrow_mut()` to update the data.
    ///
    /// When `WriteSignalValue` drops, it triggers effect run
    fn write(&self) -> WriteSignalValue<T>
    where
        T: 'static,
    {
        self.try_write().unwrap()
    }

    /// If the Signal isn't disposed,
    /// convert the Signal to `WriteSignalValue` where it holds a Mutex-protected
    /// reference to the signal data, so that you can `borrow_mut()` to update the data.
    ///
    /// When `WriteSignalValue` drops, it triggers effect run
    fn try_write(&self) -> Option<WriteSignalValue<T>>
    where
        T: 'static;
}
