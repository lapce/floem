use std::{
    cell::{Ref, RefCell},
    ops::Deref,
    rc::Rc,
    sync::{Arc, Mutex, MutexGuard},
};

use crate::{id::Id, signal::SignalValue};

#[derive(Clone)]
pub struct ReadSignalValue<T> {
    pub(crate) value: ValueHandle<T>,
}

impl<T> ReadSignalValue<T> {
    /// Borrows the current value stored in the Signal
    pub fn borrow(&self) -> ReadBorrow<'_, T> {
        match &self.value {
            ValueHandle::Sync(v) => ReadBorrow::Sync(v.lock().unwrap()),
            ValueHandle::Local(v) => ReadBorrow::Local(v.borrow()),
        }
    }
}

#[derive(Clone)]
pub enum ValueHandle<T> {
    Sync(Arc<Mutex<T>>),
    Local(Rc<RefCell<T>>),
}

pub enum ReadBorrow<'a, T> {
    Sync(MutexGuard<'a, T>),
    Local(Ref<'a, T>),
}

impl<'a, T> Deref for ReadBorrow<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        match self {
            ReadBorrow::Sync(v) => v,
            ReadBorrow::Local(v) => v,
        }
    }
}

pub trait SignalGet<T: Clone> {
    /// get the Signal Id
    fn id(&self) -> Id;

    fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        self.try_get_untracked().unwrap()
    }

    fn get(&self) -> T
    where
        T: 'static,
    {
        self.try_get().unwrap()
    }

    fn try_get(&self) -> Option<T>
    where
        T: 'static,
    {
        self.id().signal().map(|signal| signal.get())
    }

    fn try_get_untracked(&self) -> Option<T>
    where
        T: 'static,
    {
        self.id().signal().map(|signal| signal.get_untracked())
    }
}

pub trait SignalTrack<T> {
    fn id(&self) -> Id;
    /// Only subscribes to the current running effect to this Signal.
    ///
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
}

pub trait SignalWith<T> {
    /// get the Signal Id
    fn id(&self) -> Id;

    fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.id().signal().unwrap().with(f)
    }

    fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.id().signal().unwrap().with_untracked(f)
    }

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
        self.id().signal().map(|signal| {
            signal.subscribe();
            match signal.value.clone() {
                SignalValue::Local(value) => ReadSignalValue {
                    value: ValueHandle::Local(
                        value
                            .downcast::<RefCell<T>>()
                            .expect("to downcast signal type"),
                    ),
                },
                SignalValue::Sync(_) => {
                    unreachable!("sync SignalRead should use the SyncStorage impls")
                }
            }
        })
    }

    /// If the signal isn't disposed,
    /// reads the data stored in the Signal to a RefCell, so that you can `borrow()`
    /// and access the data.
    /// It doesn't subscribe to the current running effect.
    fn try_read_untracked(&self) -> Option<ReadSignalValue<T>>
    where
        T: 'static,
    {
        self.id().signal().map(|signal| match signal.value.clone() {
            SignalValue::Local(value) => ReadSignalValue {
                value: ValueHandle::Local(
                    value
                        .downcast::<RefCell<T>>()
                        .expect("to downcast signal type"),
                ),
            },
            SignalValue::Sync(_) => {
                unreachable!("sync SignalRead should use the SyncStorage impls")
            }
        })
    }
}
