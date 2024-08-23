use std::{
    cell::{RefCell, RefMut},
    rc::Rc,
};

use crate::id::Id;

#[derive(Clone)]
pub struct WriteSignalValue<T> {
    pub(crate) id: Id,
    pub(crate) value: Rc<RefCell<T>>,
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
    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        self.value.borrow_mut()
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
        if let Some(signal) = self.id().signal() {
            signal.update_value(|v| *v = new_value);
        }
    }

    /// Update the stored value with the given function and triggers effect run
    fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        if let Some(signal) = self.id().signal() {
            signal.update_value(f);
        }
    }

    /// Update the stored value with the given function, triggers effect run,
    /// and returns the value returned by the function
    fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static,
    {
        self.id().signal().map(|signal| signal.update_value(f))
    }
}

pub trait SignalWrite<T> {
    /// get the Signal Id
    fn id(&self) -> Id;
    /// Convert the Signal to `WriteSignalValue` where it holds a RefCell wrapped
    /// original data of the signal, so that you can `borrow_mut()` to update the data.
    ///
    /// When `WriteSignalValue` drops, it triggers effect run
    fn write(&self) -> WriteSignalValue<T>
    where
        T: 'static,
    {
        self.try_write().unwrap()
    }

    /// If the Signal isn't disposed,
    /// convert the Signal to `WriteSignalValue` where it holds a RefCell wrapped
    /// original data of the signal, so that you can `borrow_mut()` to update the data.
    ///
    /// When `WriteSignalValue` drops, it triggers effect run
    fn try_write(&self) -> Option<WriteSignalValue<T>>
    where
        T: 'static,
    {
        if let Some(signal) = self.id().signal() {
            Some(WriteSignalValue {
                id: signal.id,
                value: signal
                    .value
                    .clone()
                    .downcast::<RefCell<T>>()
                    .expect("to downcast signal type"),
            })
        } else {
            None
        }
    }
}
