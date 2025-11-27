use std::{
    any::Any,
    cell::{Ref, RefCell},
    collections::HashSet,
    fmt,
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Mutex, MutexGuard},
};

use crate::{
    effect::run_effect,
    id::Id,
    read::{SignalRead, SignalTrack, SignalWith},
    runtime::{Runtime, RUNTIME},
    storage::{Storage, SyncStorage, UnsyncStorage},
    sync_runtime::{SyncSignal, SYNC_RUNTIME},
    write::SignalWrite,
    SignalGet, SignalUpdate,
};

pub type SyncRwSignal<T> = RwSignal<T, SyncStorage>;
pub type SyncReadSignal<T> = ReadSignal<T, SyncStorage>;
pub type SyncWriteSignal<T> = WriteSignal<T, SyncStorage>;

impl<T: Any + 'static> Storage<T> for UnsyncStorage {
    fn create(value: T) -> Id {
        Signal::new(value)
    }

    fn get(id: Id) -> Option<Self::Signal> {
        id.signal()
    }

    type Signal = Signal;
}

impl<T: Any + Send + Sync + 'static> Storage<T> for SyncStorage {
    fn create(value: T) -> Id {
        Signal::new_sync(value)
    }

    fn get(id: Id) -> Option<Self::Signal> {
        id.signal()
            .or_else(|| SYNC_RUNTIME.get_signal(&id).map(|s| s.into()))
    }

    type Signal = Signal;
}

/// A read write Signal which can act as both a Getter and a Setter
pub struct RwSignal<T, S = UnsyncStorage> {
    pub(crate) id: Id,
    pub(crate) ty: PhantomData<T>,
    pub(crate) st: PhantomData<S>,
}

impl<T, S> RwSignal<T, S> {
    pub fn id(&self) -> Id {
        self.id
    }
}

impl<T, S> Copy for RwSignal<T, S> {}

impl<T, S> Clone for RwSignal<T, S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, S> Eq for RwSignal<T, S> {}

impl<T, S> PartialEq for RwSignal<T, S> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T, S> fmt::Debug for RwSignal<T, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("RwSignal");
        s.field("id", &self.id);
        s.field("ty", &self.ty);
        s.finish()
    }
}

impl<T: Default + 'static> Default for RwSignal<T> {
    fn default() -> Self {
        RwSignal::new(T::default())
    }
}

impl<T, S> RwSignal<T, S> {
    /// Create a Getter of this Signal
    pub fn read_only(&self) -> ReadSignal<T, S> {
        ReadSignal {
            id: self.id,
            ty: PhantomData,
            st: PhantomData,
        }
    }

    /// Create a Setter of this Signal
    pub fn write_only(&self) -> WriteSignal<T, S> {
        WriteSignal {
            id: self.id,
            ty: PhantomData,
            st: PhantomData,
        }
    }
}

impl<T: Send + Sync + 'static> RwSignal<T, SyncStorage> {
    /// Creates a sync signal. When called off the UI thread, the signal is left
    /// unscoped, so callers must ensure it is disposed manually.
    pub fn new_sync(value: T) -> Self {
        let id = Signal::new_sync(value);
        if Runtime::is_ui_thread() {
            id.set_scope();
        }
        RwSignal {
            id,
            ty: PhantomData,
            st: PhantomData,
        }
    }
    /// Creates a sync signal with separate read/write handles. Off-UI calls
    /// leave the signal unscoped; the caller is responsible for disposal.
    pub fn new_sync_split(value: T) -> (ReadSignal<T, SyncStorage>, WriteSignal<T, SyncStorage>) {
        let sig = Self::new_sync(value);
        (sig.read_only(), sig.write_only())
    }
}

impl<T: 'static> RwSignal<T, UnsyncStorage> {
    pub fn new(value: T) -> Self {
        Runtime::assert_ui_thread();
        let id = <UnsyncStorage as Storage<T>>::create(value);
        id.set_scope();
        RwSignal {
            id,
            ty: PhantomData,
            st: PhantomData,
        }
    }
    pub fn new_split(value: T) -> (ReadSignal<T, UnsyncStorage>, WriteSignal<T, UnsyncStorage>) {
        let sig = Self::new(value);
        (sig.read_only(), sig.write_only())
    }
}

/// Creates a new RwSignal which can act both as a setter and a getter.
///
/// Accessing the signal value in an Effect will make the Effect subscribe
/// to the value change of the Signal. And whenever the signal value changes,
/// it will trigger an effect run.
#[deprecated(
    since = "0.2.0",
    note = "Use RwSignal::new for sync signals or RwSignal::new_local for local ones"
)]
pub fn create_rw_signal<T>(value: T) -> RwSignal<T>
where
    T: Any + 'static,
{
    RwSignal::new(value)
}

/// A getter only Signal
pub struct ReadSignal<T, S = UnsyncStorage> {
    pub(crate) id: Id,
    pub(crate) ty: PhantomData<T>,
    pub(crate) st: PhantomData<S>,
}

impl<T, S> ReadSignal<T, S> {
    pub fn id(&self) -> Id {
        self.id
    }
}

impl<T, S> Copy for ReadSignal<T, S> {}

impl<T, S> Clone for ReadSignal<T, S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, S> Eq for ReadSignal<T, S> {}

impl<T, S> PartialEq for ReadSignal<T, S> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// A setter only Signal
pub struct WriteSignal<T, S = UnsyncStorage> {
    pub(crate) id: Id,
    pub(crate) ty: PhantomData<T>,
    pub(crate) st: PhantomData<S>,
}

impl<T, S> WriteSignal<T, S> {
    pub fn id(&self) -> Id {
        self.id
    }
}

impl<T, S> Copy for WriteSignal<T, S> {}

impl<T, S> Clone for WriteSignal<T, S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, S> Eq for WriteSignal<T, S> {}

impl<T, S> PartialEq for WriteSignal<T, S> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// Creates a new setter and getter Signal.
///
/// Accessing the signal value in an Effect will make the Effect subscribe
/// to the value change of the Signal. And whenever the signal value changes,
/// it will trigger an effect run.
#[deprecated(
    since = "0.2.0",
    note = "Use RwSignal::new for sync signals or RwSignal::new_local for local ones"
)]
pub fn create_signal<T>(value: T) -> (ReadSignal<T, UnsyncStorage>, WriteSignal<T, UnsyncStorage>)
where
    T: Any + 'static,
{
    let id = Signal::new(value);
    (
        ReadSignal {
            id,
            ty: PhantomData,
            st: PhantomData,
        },
        WriteSignal {
            id,
            ty: PhantomData,
            st: PhantomData,
        },
    )
}

/// The internal Signal where the value is stored, and effects are stored.
#[derive(Clone)]
pub(crate) struct Signal {
    pub(crate) id: Id,
    pub(crate) value: SignalValue,
    pub(crate) subscribers: Arc<Mutex<HashSet<Id>>>,
}

#[derive(Clone)]
pub(crate) enum SignalValue {
    Sync(Arc<dyn Any + Send + Sync>),
    Local(Rc<dyn Any>),
}

#[allow(dead_code)]
pub enum SignalBorrow<'a, T> {
    Sync(MutexGuard<'a, T>),
    Local(Ref<'a, T>),
}

impl Signal {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<T>(value: T) -> Id
    where
        T: Any + 'static,
    {
        Runtime::assert_ui_thread();
        let id = Id::next();
        let value = RefCell::new(value);
        let signal = Signal {
            id,
            subscribers: Arc::new(Mutex::new(HashSet::new())),
            value: SignalValue::Local(Rc::new(value)),
        };
        id.add_signal(signal);
        id
    }

    pub fn new_sync<T>(value: T) -> Id
    where
        T: Any + Send + Sync + 'static,
    {
        let id = Id::next();
        let value = Arc::new(Mutex::new(value));
        let subscribers = Arc::new(Mutex::new(HashSet::new()));
        // Sync signals live in the global sync runtime; we don't store them in the TLS runtime.
        SYNC_RUNTIME.insert_signal(
            id,
            SyncSignal {
                id,
                value,
                subscribers,
            },
        );
        id
    }

    #[deprecated(
        since = "0.2.0",
        note = "Use Signal::new_sync for sync signals or Signal::new for local ones"
    )]
    #[allow(dead_code)]
    pub fn create<T>(value: T) -> Id
    where
        T: Any + Send + Sync + 'static,
    {
        Self::new_sync(value)
    }

    #[allow(dead_code)]
    pub fn borrow<T: 'static>(&self) -> SignalBorrow<'_, T> {
        match &self.value {
            SignalValue::Sync(v) => {
                let v = v
                    .as_ref()
                    .downcast_ref::<Mutex<T>>()
                    .expect("to downcast signal type");
                SignalBorrow::Sync(v.lock().unwrap())
            }
            SignalValue::Local(v) => {
                let v = v
                    .as_ref()
                    .downcast_ref::<RefCell<T>>()
                    .expect("to downcast signal type");
                SignalBorrow::Local(v.borrow())
            }
        }
    }

    pub(crate) fn get_untracked<T: Clone + 'static>(&self) -> T {
        match &self.value {
            SignalValue::Sync(v) => {
                let v = v
                    .as_ref()
                    .downcast_ref::<Mutex<T>>()
                    .expect("to downcast signal type");
                v.lock().unwrap().clone()
            }
            SignalValue::Local(v) => {
                let v = v
                    .as_ref()
                    .downcast_ref::<RefCell<T>>()
                    .expect("to downcast signal type");
                v.borrow().clone()
            }
        }
    }

    pub(crate) fn get<T: Clone + 'static>(&self) -> T {
        self.subscribe();
        self.get_untracked()
    }

    pub(crate) fn with_untracked<O, T: 'static>(&self, f: impl FnOnce(&T) -> O) -> O {
        match &self.value {
            SignalValue::Sync(v) => {
                let v = v
                    .as_ref()
                    .downcast_ref::<Mutex<T>>()
                    .expect("to downcast signal type");
                f(&v.lock().unwrap())
            }
            SignalValue::Local(v) => {
                let v = v
                    .as_ref()
                    .downcast_ref::<RefCell<T>>()
                    .expect("to downcast signal type");
                f(&v.borrow())
            }
        }
    }

    pub(crate) fn with<O, T: 'static>(&self, f: impl FnOnce(&T) -> O) -> O {
        self.subscribe();
        self.with_untracked(f)
    }

    pub(crate) fn update_value_sync<U, T: Send + Sync + 'static>(
        &self,
        f: impl FnOnce(&mut T) -> U,
    ) -> U {
        let value = self.as_sync::<T>();
        let mut guard = value.lock().unwrap();
        let result = f(&mut *guard);
        drop(guard);
        self.run_effects();
        result
    }

    pub(crate) fn update_value_local<U, T: 'static>(&self, f: impl FnOnce(&mut T) -> U) -> U {
        let value = self.as_local::<T>();
        let mut guard = value.borrow_mut();
        let result = f(&mut *guard);
        drop(guard);
        self.run_effects();
        result
    }

    pub(crate) fn subscriber_ids(&self) -> HashSet<Id> {
        self.subscribers.lock().unwrap().iter().copied().collect()
    }

    pub(crate) fn run_effects(&self) {
        let ids = self.subscriber_ids();
        if !Runtime::is_ui_thread() {
            SYNC_RUNTIME.enqueue_effects(ids);
            return;
        }
        if RUNTIME.with(|r| r.batching.get()) {
            RUNTIME.with(|r| {
                for id in &ids {
                    r.add_pending_effect(*id);
                }
            });
            return;
        }

        for id in ids {
            if let Some(effect) = RUNTIME.with(|r| r.get_effect(id)) {
                run_effect(effect);
            }
        }
    }

    pub(crate) fn subscribe(&self) {
        RUNTIME.with(|runtime| {
            if let Some(effect) = runtime.current_effect.borrow().as_ref() {
                self.subscribers.lock().unwrap().insert(effect.id());
                effect.add_observer(self.id);
            }
        });
    }

    fn as_sync<T: Send + Sync + 'static>(&self) -> Arc<Mutex<T>> {
        match &self.value {
            SignalValue::Sync(v) => v
                .clone()
                .downcast::<Mutex<T>>()
                .expect("to downcast signal type"),
            SignalValue::Local(_) => unreachable!("expected sync signal storage"),
        }
    }

    fn as_local<T: 'static>(&self) -> Rc<RefCell<T>> {
        match &self.value {
            SignalValue::Local(v) => v
                .clone()
                .downcast::<RefCell<T>>()
                .expect("to downcast signal type"),
            SignalValue::Sync(_) => unreachable!("expected local signal storage"),
        }
    }
}

// Sync storage trait impls (requires Send + Sync)
impl<T: Clone + Send + Sync> SignalGet<T> for RwSignal<T, SyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        self.id()
            .signal()
            .map(|signal| signal.as_sync::<T>().lock().unwrap().clone())
            .unwrap()
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
            signal.subscribe();
            signal.as_sync::<T>().lock().unwrap().clone()
        })
    }

    fn try_get_untracked(&self) -> Option<T>
    where
        T: 'static,
    {
        self.id()
            .signal()
            .map(|signal| signal.as_sync::<T>().lock().unwrap().clone())
    }
}

impl<T: Send + Sync> SignalWith<T> for RwSignal<T, SyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        let signal = self.id().signal().unwrap();
        signal.subscribe();
        let handle = signal.as_sync::<T>();
        let guard = handle.lock().unwrap();
        f(&*guard)
    }

    fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        let signal = self.id().signal().unwrap();
        let handle = signal.as_sync::<T>();
        let guard = handle.lock().unwrap();
        f(&*guard)
    }

    fn try_with<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        if let Some(signal) = self.id().signal() {
            signal.subscribe();
            let handle = signal.as_sync::<T>();
            let guard = handle.lock().unwrap();
            f(Some(&*guard))
        } else {
            f(None)
        }
    }

    fn try_with_untracked<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        if let Some(signal) = self.id().signal() {
            let handle = signal.as_sync::<T>();
            let guard = handle.lock().unwrap();
            f(Some(&*guard))
        } else {
            f(None)
        }
    }
}

impl<T> SignalTrack<T> for RwSignal<T, SyncStorage> {
    fn id(&self) -> Id {
        self.id
    }
}

impl<T: Send + Sync> SignalRead<T> for RwSignal<T, SyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn read(&self) -> crate::read::ReadSignalValue<T>
    where
        T: 'static,
    {
        self.try_read().unwrap()
    }

    fn read_untracked(&self) -> crate::read::ReadSignalValue<T>
    where
        T: 'static,
    {
        self.try_read_untracked().unwrap()
    }

    fn try_read(&self) -> Option<crate::read::ReadSignalValue<T>>
    where
        T: 'static,
    {
        self.id().signal().map(|signal| {
            signal.subscribe();
            crate::read::ReadSignalValue {
                value: crate::read::ValueHandle::Sync(signal.as_sync::<T>()),
            }
        })
    }

    fn try_read_untracked(&self) -> Option<crate::read::ReadSignalValue<T>>
    where
        T: 'static,
    {
        self.id()
            .signal()
            .map(|signal| crate::read::ReadSignalValue {
                value: crate::read::ValueHandle::Sync(signal.as_sync::<T>()),
            })
    }
}

impl<T: Send + Sync> SignalUpdate<T> for RwSignal<T, SyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn set(&self, new_value: T)
    where
        T: 'static,
    {
        if let Some(signal) = self.id().signal() {
            signal.update_value_sync(|v| *v = new_value);
        }
    }

    fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        if let Some(signal) = self.id().signal() {
            signal.update_value_sync(f);
        }
    }

    fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static,
    {
        self.id().signal().map(|signal| signal.update_value_sync(f))
    }
}

impl<T: Send + Sync> SignalWrite<T> for RwSignal<T, SyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn write(&self) -> crate::write::WriteSignalValue<T>
    where
        T: 'static,
    {
        self.try_write().unwrap()
    }

    fn try_write(&self) -> Option<crate::write::WriteSignalValue<T>>
    where
        T: 'static,
    {
        self.id()
            .signal()
            .map(|signal| crate::write::WriteSignalValue {
                id: signal.id,
                value: crate::write::ValueHandle::Sync(signal.as_sync::<T>()),
            })
    }
}

impl<T: Clone + Send + Sync> SignalGet<T> for ReadSignal<T, SyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

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
            signal.subscribe();
            signal.as_sync::<T>().lock().unwrap().clone()
        })
    }

    fn try_get_untracked(&self) -> Option<T>
    where
        T: 'static,
    {
        self.id()
            .signal()
            .map(|signal| signal.as_sync::<T>().lock().unwrap().clone())
    }
}

impl<T: Send + Sync> SignalWith<T> for ReadSignal<T, SyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        let signal = self.id().signal().unwrap();
        signal.subscribe();
        let handle = signal.as_sync::<T>();
        let guard = handle.lock().unwrap();
        f(&*guard)
    }

    fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        let signal = self.id().signal().unwrap();
        let handle = signal.as_sync::<T>();
        let guard = handle.lock().unwrap();
        f(&*guard)
    }

    fn try_with<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        if let Some(signal) = self.id().signal() {
            signal.subscribe();
            let handle = signal.as_sync::<T>();
            let guard = handle.lock().unwrap();
            f(Some(&*guard))
        } else {
            f(None)
        }
    }

    fn try_with_untracked<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        if let Some(signal) = self.id().signal() {
            let handle = signal.as_sync::<T>();
            let guard = handle.lock().unwrap();
            f(Some(&*guard))
        } else {
            f(None)
        }
    }
}

impl<T: Send + Sync> SignalRead<T> for ReadSignal<T, SyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn read(&self) -> crate::read::ReadSignalValue<T>
    where
        T: 'static,
    {
        self.try_read().unwrap()
    }

    fn read_untracked(&self) -> crate::read::ReadSignalValue<T>
    where
        T: 'static,
    {
        self.try_read_untracked().unwrap()
    }

    fn try_read(&self) -> Option<crate::read::ReadSignalValue<T>>
    where
        T: 'static,
    {
        self.id().signal().map(|signal| {
            signal.subscribe();
            crate::read::ReadSignalValue {
                value: crate::read::ValueHandle::Sync(signal.as_sync::<T>()),
            }
        })
    }

    fn try_read_untracked(&self) -> Option<crate::read::ReadSignalValue<T>>
    where
        T: 'static,
    {
        self.id()
            .signal()
            .map(|signal| crate::read::ReadSignalValue {
                value: crate::read::ValueHandle::Sync(signal.as_sync::<T>()),
            })
    }
}

impl<T: Send + Sync> SignalUpdate<T> for WriteSignal<T, SyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn set(&self, new_value: T)
    where
        T: 'static,
    {
        if let Some(signal) = self.id().signal() {
            signal.update_value_sync(|v| *v = new_value);
        }
    }

    fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        if let Some(signal) = self.id().signal() {
            signal.update_value_sync(f);
        }
    }

    fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static,
    {
        self.id().signal().map(|signal| signal.update_value_sync(f))
    }
}

impl<T: Send + Sync> SignalWrite<T> for WriteSignal<T, SyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn write(&self) -> crate::write::WriteSignalValue<T>
    where
        T: 'static,
    {
        self.try_write().unwrap()
    }

    fn try_write(&self) -> Option<crate::write::WriteSignalValue<T>>
    where
        T: 'static,
    {
        self.id()
            .signal()
            .map(|signal| crate::write::WriteSignalValue {
                id: signal.id,
                value: crate::write::ValueHandle::Sync(signal.as_sync::<T>()),
            })
    }
}

// Unsync storage trait impls (no Send + Sync required)
impl<T: Clone> SignalGet<T> for RwSignal<T, UnsyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.id()
            .signal()
            .map(|signal| signal.as_local::<T>().borrow().clone())
            .unwrap()
    }

    fn get(&self) -> T
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.try_get().unwrap()
    }

    fn try_get(&self) -> Option<T>
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.id().signal().map(|signal| {
            signal.subscribe();
            signal.as_local::<T>().borrow().clone()
        })
    }

    fn try_get_untracked(&self) -> Option<T>
    where
        T: 'static,
    {
        self.id()
            .signal()
            .map(|signal| signal.as_local::<T>().borrow().clone())
    }
}

impl<T> SignalWith<T> for RwSignal<T, UnsyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        let signal = self.id().signal().unwrap();
        signal.subscribe();
        let handle = signal.as_local::<T>();
        let guard = handle.borrow();
        f(&*guard)
    }

    fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        let signal = self.id().signal().unwrap();
        let handle = signal.as_local::<T>();
        let guard = handle.borrow();
        f(&*guard)
    }

    fn try_with<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        if let Some(signal) = self.id().signal() {
            signal.subscribe();
            let handle = signal.as_local::<T>();
            let guard = handle.borrow();
            f(Some(&*guard))
        } else {
            f(None)
        }
    }

    fn try_with_untracked<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        if let Some(signal) = self.id().signal() {
            let handle = signal.as_local::<T>();
            let guard = handle.borrow();
            f(Some(&*guard))
        } else {
            f(None)
        }
    }
}

impl<T> SignalTrack<T> for RwSignal<T, UnsyncStorage> {
    fn id(&self) -> Id {
        self.id
    }
}

impl<T> SignalRead<T> for RwSignal<T, UnsyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn read(&self) -> crate::read::ReadSignalValue<T>
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.try_read().unwrap()
    }

    fn read_untracked(&self) -> crate::read::ReadSignalValue<T>
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.try_read_untracked().unwrap()
    }

    fn try_read(&self) -> Option<crate::read::ReadSignalValue<T>>
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.id().signal().map(|signal| {
            signal.subscribe();
            crate::read::ReadSignalValue {
                value: crate::read::ValueHandle::Local(signal.as_local::<T>()),
            }
        })
    }

    fn try_read_untracked(&self) -> Option<crate::read::ReadSignalValue<T>>
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.id()
            .signal()
            .map(|signal| crate::read::ReadSignalValue {
                value: crate::read::ValueHandle::Local(signal.as_local::<T>()),
            })
    }
}

impl<T> SignalUpdate<T> for RwSignal<T, UnsyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn set(&self, new_value: T)
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        if let Some(signal) = self.id().signal() {
            signal.update_value_local(|v| *v = new_value);
        }
    }

    fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        if let Some(signal) = self.id().signal() {
            signal.update_value_local(f);
        }
    }

    fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.id()
            .signal()
            .map(|signal| signal.update_value_local(f))
    }
}

impl<T> SignalWrite<T> for RwSignal<T, UnsyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn write(&self) -> crate::write::WriteSignalValue<T>
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.try_write().unwrap()
    }

    fn try_write(&self) -> Option<crate::write::WriteSignalValue<T>>
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.id()
            .signal()
            .map(|signal| crate::write::WriteSignalValue {
                id: signal.id,
                value: crate::write::ValueHandle::Local(signal.as_local::<T>()),
            })
    }
}

impl<T> SignalUpdate<T> for WriteSignal<T, UnsyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn set(&self, new_value: T)
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        if let Some(signal) = self.id().signal() {
            signal.update_value_local(|v| *v = new_value);
        }
    }

    fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        if let Some(signal) = self.id().signal() {
            signal.update_value_local(f);
        }
    }

    fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.id()
            .signal()
            .map(|signal| signal.update_value_local(f))
    }
}

impl<T> SignalWrite<T> for WriteSignal<T, UnsyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn write(&self) -> crate::write::WriteSignalValue<T>
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.try_write().unwrap()
    }

    fn try_write(&self) -> Option<crate::write::WriteSignalValue<T>>
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.id()
            .signal()
            .map(|signal| crate::write::WriteSignalValue {
                id: signal.id,
                value: crate::write::ValueHandle::Local(signal.as_local::<T>()),
            })
    }
}

impl<T: Clone> SignalGet<T> for ReadSignal<T, UnsyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.try_get_untracked().unwrap()
    }

    fn get(&self) -> T
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.try_get().unwrap()
    }

    fn try_get(&self) -> Option<T>
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.id().signal().map(|signal| {
            signal.subscribe();
            signal.as_local::<T>().borrow().clone()
        })
    }

    fn try_get_untracked(&self) -> Option<T>
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        self.id()
            .signal()
            .map(|signal| signal.as_local::<T>().borrow().clone())
    }
}

impl<T> SignalWith<T> for ReadSignal<T, UnsyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        let signal = self.id().signal().unwrap();
        signal.subscribe();
        let handle = signal.as_local::<T>();
        let guard = handle.borrow();
        f(&*guard)
    }

    fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        let signal = self.id().signal().unwrap();
        let handle = signal.as_local::<T>();
        let guard = handle.borrow();
        f(&*guard)
    }

    fn try_with<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        if let Some(signal) = self.id().signal() {
            signal.subscribe();
            let handle = signal.as_local::<T>();
            let guard = handle.borrow();
            f(Some(&*guard))
        } else {
            f(None)
        }
    }

    fn try_with_untracked<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        Runtime::assert_ui_thread();
        if let Some(signal) = self.id().signal() {
            let handle = signal.as_local::<T>();
            let guard = handle.borrow();
            f(Some(&*guard))
        } else {
            f(None)
        }
    }
}

impl<T> SignalTrack<T> for ReadSignal<T, UnsyncStorage> {
    fn id(&self) -> Id {
        self.id
    }
}

impl<T> SignalRead<T> for ReadSignal<T, UnsyncStorage> {
    fn id(&self) -> Id {
        self.id
    }

    fn read(&self) -> crate::read::ReadSignalValue<T>
    where
        T: 'static,
    {
        self.try_read().unwrap()
    }

    fn read_untracked(&self) -> crate::read::ReadSignalValue<T>
    where
        T: 'static,
    {
        self.try_read_untracked().unwrap()
    }

    fn try_read(&self) -> Option<crate::read::ReadSignalValue<T>>
    where
        T: 'static,
    {
        self.id().signal().map(|signal| {
            signal.subscribe();
            crate::read::ReadSignalValue {
                value: crate::read::ValueHandle::Local(signal.as_local::<T>()),
            }
        })
    }

    fn try_read_untracked(&self) -> Option<crate::read::ReadSignalValue<T>>
    where
        T: 'static,
    {
        self.id()
            .signal()
            .map(|signal| crate::read::ReadSignalValue {
                value: crate::read::ValueHandle::Local(signal.as_local::<T>()),
            })
    }
}
