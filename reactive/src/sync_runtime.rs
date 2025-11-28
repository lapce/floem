use std::{
    any::Any,
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use parking_lot::Mutex;

use crate::id::Id;

pub(crate) static SYNC_RUNTIME: LazyLock<SyncRuntime> = LazyLock::new(SyncRuntime::new);

/// Sync-only signal representation for the global runtime.
#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct SyncSignal {
    pub(crate) id: Id,
    pub(crate) value: Arc<dyn Any + Send + Sync>,
    pub(crate) subscribers: Arc<Mutex<HashSet<Id>>>,
}

impl From<SyncSignal> for crate::signal::Signal {
    fn from(sync: SyncSignal) -> Self {
        crate::signal::Signal {
            id: sync.id,
            value: crate::signal::SignalValue::Sync(sync.value),
            subscribers: sync.subscribers,
        }
    }
}

/// Global runtime for sync signals (shared across threads).
#[allow(dead_code)]
pub(crate) struct SyncRuntime {
    signals: RwLock<HashMap<Id, SyncSignal>>,
    pending_effects: Mutex<Vec<Id>>,
    pending_disposals: Mutex<Vec<Id>>,
    waker: OnceLock<Arc<dyn Fn() + Send + Sync>>,
}

impl SyncRuntime {
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self {
            signals: RwLock::new(HashMap::new()),
            pending_effects: Mutex::new(Vec::new()),
            pending_disposals: Mutex::new(Vec::new()),
            waker: OnceLock::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn get_signal(&self, id: &Id) -> Option<SyncSignal> {
        self.signals.read().ok()?.get(id).cloned()
    }

    #[allow(dead_code)]
    pub(crate) fn insert_signal(&self, id: Id, signal: SyncSignal) {
        if let Ok(mut signals) = self.signals.write() {
            signals.insert(id, signal);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn remove_signal(&self, id: &Id) -> Option<SyncSignal> {
        self.signals.write().ok()?.remove(id)
    }

    #[allow(dead_code)]
    pub(crate) fn read_guard(&self) -> Option<RwLockReadGuard<'_, HashMap<Id, SyncSignal>>> {
        self.signals.read().ok()
    }

    #[allow(dead_code)]
    pub(crate) fn write_guard(&self) -> Option<RwLockWriteGuard<'_, HashMap<Id, SyncSignal>>> {
        self.signals.write().ok()
    }

    pub(crate) fn enqueue_effects(&self, ids: impl IntoIterator<Item = Id>) {
        let waker = {
            let mut queue = self.pending_effects.lock();
            queue.extend(ids);
            self.waker.get().cloned()
        };

        if let Some(waker) = waker {
            waker();
        }
    }

    pub(crate) fn enqueue_disposals(&self, ids: impl IntoIterator<Item = Id>) {
        let waker = {
            let mut queue = self.pending_disposals.lock();
            queue.extend(ids);
            self.waker.get().cloned()
        };

        if let Some(waker) = waker {
            waker();
        }
    }

    pub(crate) fn take_pending_effects(&self) -> Vec<Id> {
        std::mem::take(&mut *self.pending_effects.lock())
    }

    pub(crate) fn take_pending_disposals(&self) -> Vec<Id> {
        std::mem::take(&mut *self.pending_disposals.lock())
    }

    pub(crate) fn has_pending_effects(&self) -> bool {
        !self.pending_effects.lock().is_empty()
    }

    pub(crate) fn has_pending_disposals(&self) -> bool {
        !self.pending_disposals.lock().is_empty()
    }

    pub(crate) fn set_waker(&self, waker: impl Fn() + Send + Sync + 'static) {
        let _ = self.waker.set(Arc::new(waker));
    }
}
