use std::{ops::Deref, rc::Rc, sync::Arc};

use parking_lot::{Mutex, MutexGuard};

use crate::{
    id::Id,
    runtime::Runtime,
    signal::{SignalValue, TrackedRef, TrackedRefCell},
};

pub struct SyncReadRef<'a, T> {
    _handle: Arc<Mutex<T>>,
    pub(crate) guard: MutexGuard<'a, T>,
}

pub struct LocalReadRef<'a, T> {
    _handle: Rc<TrackedRefCell<T>>,
    pub(crate) guard: TrackedRef<'a, T>,
}

pub enum ReadRef<'a, T> {
    Sync(SyncReadRef<'a, T>),
    Local(LocalReadRef<'a, T>),
}

impl<'a, T> Deref for SyncReadRef<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a, T> Deref for LocalReadRef<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a, T> Deref for ReadRef<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        match self {
            ReadRef::Sync(v) => &*v.guard,
            ReadRef::Local(v) => &*v.guard,
        }
    }
}

impl<'a, T> SyncReadRef<'a, T> {
    pub(crate) fn new(handle: Arc<Mutex<T>>) -> Self {
        let guard = handle.lock();
        // The Arc keeps the data alive for the lifetime of the guard.
        let guard = unsafe { std::mem::transmute::<MutexGuard<'_, T>, MutexGuard<'a, T>>(guard) };
        Self {
            _handle: handle,
            guard,
        }
    }
}

impl<'a, T> LocalReadRef<'a, T> {
    pub(crate) fn new(handle: Rc<TrackedRefCell<T>>) -> Self {
        let guard = handle.borrow();
        // The Rc keeps the data alive for the lifetime of the guard.
        let guard = unsafe { std::mem::transmute::<TrackedRef<'_, T>, TrackedRef<'a, T>>(guard) };
        Self {
            _handle: handle,
            guard,
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
        self.id().signal().map(|signal| {
            if matches!(signal.value, SignalValue::Local(_)) {
                Runtime::assert_ui_thread();
            }
            signal.get()
        })
    }

    fn try_get_untracked(&self) -> Option<T>
    where
        T: 'static,
    {
        self.id().signal().map(|signal| {
            if matches!(signal.value, SignalValue::Local(_)) {
                Runtime::assert_ui_thread();
            }
            signal.get_untracked()
        })
    }
}

pub trait SignalTrack<T> {
    fn id(&self) -> Id;
    /// Only subscribes to the current running effect to this Signal.
    ///
    fn track(&self) {
        let signal = self.id().signal().unwrap();
        if matches!(signal.value, SignalValue::Local(_)) {
            Runtime::assert_ui_thread();
        }
        signal.subscribe();
    }

    /// If the signal isn't disposed,
    // subscribes to the current running effect to this Signal.
    fn try_track(&self) {
        if let Some(signal) = self.id().signal() {
            if matches!(signal.value, SignalValue::Local(_)) {
                Runtime::assert_ui_thread();
            }
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
        let signal = self.id().signal().unwrap();
        if matches!(signal.value, SignalValue::Local(_)) {
            Runtime::assert_ui_thread();
        }
        signal.with(f)
    }

    fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        let signal = self.id().signal().unwrap();
        if matches!(signal.value, SignalValue::Local(_)) {
            Runtime::assert_ui_thread();
        }
        signal.with_untracked(f)
    }

    fn try_with<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        if let Some(signal) = self.id().signal() {
            if matches!(signal.value, SignalValue::Local(_)) {
                Runtime::assert_ui_thread();
            }
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
            if matches!(signal.value, SignalValue::Local(_)) {
                Runtime::assert_ui_thread();
            }
            signal.with_untracked(|v| f(Some(v)))
        } else {
            f(None)
        }
    }
}

pub trait SignalRead<T> {
    /// get the Signal Id
    fn id(&self) -> Id;

    /// Reads the data stored in the Signal, subscribing the current running effect.
    fn read(&self) -> ReadRef<'_, T>
    where
        T: 'static,
    {
        self.try_read().unwrap()
    }

    /// Reads the data stored in the Signal without subscribing.
    fn read_untracked(&self) -> ReadRef<'_, T>
    where
        T: 'static,
    {
        self.try_read_untracked().unwrap()
    }

    /// If the signal isn't disposed,
    /// reads the data stored in the Signal and subscribes to the current running effect.
    fn try_read(&self) -> Option<ReadRef<'_, T>>
    where
        T: 'static;

    /// If the signal isn't disposed,
    /// reads the data stored in the Signal without subscribing.
    fn try_read_untracked(&self) -> Option<ReadRef<'_, T>>
    where
        T: 'static;
}
