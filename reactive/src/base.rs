use std::marker::PhantomData;

use crate::{
    id::Id,
    signal::Signal,
    storage::SyncStorage,
    ReadSignal,
    RwSignal,
    SignalGet,
    SignalUpdate,
    SignalWith,
    WriteSignal,
};

/// BaseSignal gives you another way to control the lifetime of a Signal apart
/// from Scope. This unsync variant stores its value on the UI thread.
///
/// When BaseSignal is dropped, it will dispose the underlying Signal as well.
/// The signal isn't put in any Scope when a BaseSignal is created, so that the
/// lifetime of the signal can only be determined by BaseSignal rather than
/// Scope dependencies.
pub struct BaseSignal<T: 'static> {
    id: Id,
    ty: PhantomData<T>,
}

impl<T: 'static> Eq for BaseSignal<T> {}

impl<T: 'static> PartialEq for BaseSignal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: 'static> Drop for BaseSignal<T> {
    fn drop(&mut self) {
        self.id.dispose();
    }
}

#[deprecated(
    since = "0.2.0",
    note = "Use BaseSignal::new instead; this will be removed in a future release"
)]
pub fn create_base_signal<T: 'static>(value: T) -> BaseSignal<T> {
    BaseSignal::new(value)
}

impl<T: 'static> BaseSignal<T> {
    pub fn new(value: T) -> Self {
        let id = Signal::new(value);
        BaseSignal {
            id,
            ty: PhantomData,
        }
    }

    /// Create a RwSignal of this Signal
    pub fn rw(&self) -> RwSignal<T> {
        RwSignal {
            id: self.id,
            ty: PhantomData,
            st: PhantomData,
        }
    }

    /// Create a Getter of this Signal
    pub fn read_only(&self) -> ReadSignal<T> {
        ReadSignal {
            id: self.id,
            ty: PhantomData,
            st: PhantomData,
        }
    }

    /// Create a Setter of this Signal
    pub fn write_only(&self) -> WriteSignal<T> {
        WriteSignal {
            id: self.id,
            ty: PhantomData,
            st: PhantomData,
        }
    }
}

impl<T: Clone + 'static> SignalGet<T> for BaseSignal<T> {
    fn id(&self) -> Id {
        self.id
    }

    fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        self.read_only().get_untracked()
    }

    fn get(&self) -> T
    where
        T: 'static,
    {
        self.read_only().get()
    }

    fn try_get(&self) -> Option<T>
    where
        T: 'static,
    {
        Some(self.get())
    }

    fn try_get_untracked(&self) -> Option<T>
    where
        T: 'static,
    {
        Some(self.get_untracked())
    }
}

impl<T: Clone + 'static> SignalWith<T> for BaseSignal<T> {
    fn id(&self) -> Id {
        self.id
    }

    fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.read_only().with(f)
    }

    fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.read_only().with_untracked(f)
    }

    fn try_with<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        f(Some(&self.get()))
    }

    fn try_with_untracked<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        f(Some(&self.get_untracked()))
    }
}

impl<T: 'static> SignalUpdate<T> for BaseSignal<T> {
    fn id(&self) -> Id {
        self.id
    }

    fn set(&self, new_value: T)
    where
        T: 'static,
    {
        self.write_only().set(new_value)
    }

    fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        self.write_only().update(f)
    }

    fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static,
    {
        self.write_only().try_update(f)
    }
}

/// Thread-safe variant that stores the signal in the sync runtime.
pub struct SyncBaseSignal<T: Send + Sync + 'static> {
    id: Id,
    ty: PhantomData<T>,
}

impl<T: Send + Sync + 'static> Eq for SyncBaseSignal<T> {}

impl<T: Send + Sync + 'static> PartialEq for SyncBaseSignal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: Send + Sync + 'static> Drop for SyncBaseSignal<T> {
    fn drop(&mut self) {
        self.id.dispose();
    }
}

#[deprecated(
    since = "0.2.0",
    note = "Use SyncBaseSignal::new instead; this will be removed in a future release"
)]
pub fn create_sync_base_signal<T: Send + Sync + 'static>(value: T) -> SyncBaseSignal<T> {
    SyncBaseSignal::new(value)
}

impl<T: Send + Sync + 'static> SyncBaseSignal<T> {
    pub fn new(value: T) -> Self {
        let id = Signal::new_sync(value);
        SyncBaseSignal {
            id,
            ty: PhantomData,
        }
    }

    /// Create a RwSignal of this Signal
    pub fn rw(&self) -> RwSignal<T, SyncStorage> {
        RwSignal {
            id: self.id,
            ty: PhantomData,
            st: PhantomData,
        }
    }

    /// Create a Getter of this Signal
    pub fn read_only(&self) -> ReadSignal<T, SyncStorage> {
        ReadSignal {
            id: self.id,
            ty: PhantomData,
            st: PhantomData,
        }
    }

    /// Create a Setter of this Signal
    pub fn write_only(&self) -> WriteSignal<T, SyncStorage> {
        WriteSignal {
            id: self.id,
            ty: PhantomData,
            st: PhantomData,
        }
    }
}

impl<T: Clone + Send + Sync + 'static> SignalGet<T> for SyncBaseSignal<T> {
    fn id(&self) -> Id {
        self.id
    }

    fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        self.read_only().get_untracked()
    }

    fn get(&self) -> T
    where
        T: 'static,
    {
        self.read_only().get()
    }

    fn try_get(&self) -> Option<T>
    where
        T: 'static,
    {
        Some(self.get())
    }

    fn try_get_untracked(&self) -> Option<T>
    where
        T: 'static,
    {
        Some(self.get_untracked())
    }
}

impl<T: Clone + Send + Sync + 'static> SignalWith<T> for SyncBaseSignal<T> {
    fn id(&self) -> Id {
        self.id
    }

    fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.read_only().with(f)
    }

    fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.read_only().with_untracked(f)
    }

    fn try_with<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        f(Some(&self.get()))
    }

    fn try_with_untracked<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        f(Some(&self.get_untracked()))
    }
}

impl<T: Send + Sync + 'static> SignalUpdate<T> for SyncBaseSignal<T> {
    fn id(&self) -> Id {
        self.id
    }

    fn set(&self, new_value: T)
    where
        T: 'static,
    {
        self.write_only().set(new_value)
    }

    fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        self.write_only().update(f)
    }

    fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static,
    {
        self.write_only().try_update(f)
    }
}
