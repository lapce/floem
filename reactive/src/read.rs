use std::{
    cell::{Ref, RefCell},
    rc::Rc,
};

use crate::id::Id;

#[derive(Clone)]
pub struct ReadSignalValue<T> {
    pub(crate) value: Rc<RefCell<T>>,
}

impl<T> ReadSignalValue<T> {
    /// Borrows the current value stored in the Signal
    pub fn borrow(&self) -> Ref<'_, T> {
        self.value.borrow()
    }
}

pub trait SignalGet<T: Clone> {
    /// get the Signal Id
    fn id(&self) -> Id;

    /// Clones and returns the current value stored in the Signal, but it doesn't subscribe
    /// to the current running effect.
    fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        self.try_get_untracked().unwrap()
    }

    /// Clones and returns the current value stored in the Signal, and subscribes
    /// to the current running effect to this Signal.
    fn get(&self) -> T
    where
        T: 'static,
    {
        self.try_get().unwrap()
    }

    /// Try to clone and return the current value stored in the Signal, and returns None
    /// if it's already disposed. It subscribes to the current running effect.
    fn try_get(&self) -> Option<T>
    where
        T: 'static,
    {
        self.id().signal().map(|signal| signal.get())
    }

    /// Try to clone and return the current value stored in the Signal, and returns None
    /// if it's already disposed. It doesn't subscribe to the current running effect.
    fn try_get_untracked(&self) -> Option<T>
    where
        T: 'static,
    {
        self.id().signal().map(|signal| signal.get_untracked())
    }
}

pub trait SignalWith<T> {
    /// get the Signal Id
    fn id(&self) -> Id;

    /// Only subscribes to the current running effect to this Signal.
    fn track(&self) {
        self.id().signal().unwrap().subscribe();
    }

    /// If the signal isn't disposed,
    // subscribes to the current running effect to this Signal.
    fn try_track(&self) {
        if let Some(signal) = self.id().signal() {
            signal.subscribe();
        }
    }

    /// Applies a closure to the current value stored in the Signal, and subscribes
    /// to the current running effect to this Memo.
    fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.id().signal().unwrap().with(f)
    }

    /// Applies a closure to the current value stored in the Signal, but it doesn't subscribe
    /// to the current running effect.
    fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.id().signal().unwrap().with_untracked(f)
    }

    /// If the signal isn't disposed, applies a closure to the current value stored in the Signal.
    /// It subscribes to the current running effect.
    fn try_with<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        if let Some(signal) = self.id().signal() {
            signal.with(|v| f(Some(v)))
        } else {
            f(None)
        }
    }

    /// If the signal isn't disposed, applies a closure to the current value stored in the Signal,
    /// but it doesn't subscribe to the current running effect.
    fn try_with_untracked<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        if let Some(signal) = self.id().signal() {
            signal.with_untracked(|v| f(Some(v)))
        } else {
            f(None)
        }
    }
}

pub trait SignalRead<T> {
    /// get the Signal Id
    fn id(&self) -> Id;

    /// Only subscribes to the current running effect to this Signal.
    fn track(&self) {
        self.id().signal().unwrap().subscribe();
    }

    /// If the signal isn't disposed,
    // subscribes to the current running effect to this Signal.
    fn try_track(&self) {
        if let Some(signal) = self.id().signal() {
            signal.subscribe();
        }
    }

    /// Reads the data stored in the Signal to a RefCell, so that you can `borrow()`
    /// and access the data.
    /// It subscribes to the current running effect.
    fn read(&self) -> ReadSignalValue<T>
    where
        T: 'static,
    {
        self.try_read().unwrap()
    }

    /// Reads the data stored in the Signal to a RefCell, so that you can `borrow()`
    /// and access the data.
    /// It doesn't subscribe to the current running effect.
    fn read_untracked(&self) -> ReadSignalValue<T>
    where
        T: 'static,
    {
        self.try_read_untracked().unwrap()
    }

    /// If the signal isn't disposed,
    /// reads the data stored in the Signal to a RefCell, so that you can `borrow()`
    /// and access the data.
    /// It subscribes to the current running effect.
    fn try_read(&self) -> Option<ReadSignalValue<T>>
    where
        T: 'static,
    {
        if let Some(signal) = self.id().signal() {
            signal.subscribe();
            Some(ReadSignalValue {
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

    /// If the signal isn't disposed,
    /// reads the data stored in the Signal to a RefCell, so that you can `borrow()`
    /// and access the data.
    /// It doesn't subscribe to the current running effect.
    fn try_read_untracked(&self) -> Option<ReadSignalValue<T>>
    where
        T: 'static,
    {
        if let Some(signal) = self.id().signal() {
            Some(ReadSignalValue {
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
