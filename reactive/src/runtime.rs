use std::{
    any::{Any, TypeId},
    cell::{Cell, RefCell},
    cmp::Reverse,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        OnceLock,
    },
    thread::{self, ThreadId},
};

use smallvec::SmallVec;

use crate::{
    effect::{run_effect, EffectPriority, EffectTrait},
    id::Id,
    signal::SignalState,
    sync_runtime::SYNC_RUNTIME,
};

thread_local! {
pub(crate) static RUNTIME: Runtime = Runtime::new();
}

static UI_THREAD_ID: OnceLock<ThreadId> = OnceLock::new();
#[cfg(debug_assertions)]
static UI_THREAD_SET_LOCATION: OnceLock<&'static std::panic::Location<'static>> = OnceLock::new();
static ENFORCE_UI_THREAD: AtomicBool = AtomicBool::new(false);

/// The internal reactive Runtime which stores all the reactive system states in a
/// thread local.
pub struct Runtime {
    pub(crate) current_effect: RefCell<Option<Rc<dyn EffectTrait>>>,
    pub(crate) current_scope: RefCell<Id>,
    pub(crate) children: RefCell<HashMap<Id, HashSet<Id>>>,
    pub(crate) signals: RefCell<HashMap<Id, SignalState>>,
    pub(crate) effects: RefCell<HashMap<Id, Rc<dyn EffectTrait>>>,
    pub(crate) contexts: RefCell<HashMap<TypeId, Box<dyn Any>>>,
    pub(crate) batching: Cell<bool>,
    pub(crate) pending_effects: RefCell<SmallVec<[Id; 10]>>,
    pub(crate) pending_effects_set: RefCell<HashSet<Id>>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub(crate) fn new() -> Self {
        Self {
            current_effect: RefCell::new(None),
            current_scope: RefCell::new(Id::next()),
            children: RefCell::new(HashMap::new()),
            signals: Default::default(),
            effects: Default::default(),
            contexts: Default::default(),
            batching: Cell::new(false),
            pending_effects: RefCell::new(SmallVec::new()),
            pending_effects_set: RefCell::new(HashSet::new()),
        }
    }

    /// Call this once on the UI thread during initialization.
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn init_on_ui_thread() {
        let current = thread::current().id();
        match UI_THREAD_ID.set(current) {
            Ok(_) => {}
            Err(_) => {
                assert_eq!(
                    UI_THREAD_ID.get(),
                    Some(&current),
                    "UI thread id already set to a different thread"
                );
            }
        }
        #[cfg(debug_assertions)]
        {
            let caller = std::panic::Location::caller();
            let _ = UI_THREAD_SET_LOCATION.set(caller);
        }
        ENFORCE_UI_THREAD.store(true, Ordering::Relaxed);
    }

    #[cfg_attr(debug_assertions, track_caller)]
    pub fn assert_ui_thread() {
        if !ENFORCE_UI_THREAD.load(Ordering::Relaxed) {
            return;
        }

        let current = thread::current().id();
        match UI_THREAD_ID.get() {
            Some(ui_id) => {
                if *ui_id != current {
                    #[cfg(debug_assertions)]
                    {
                        let caller = std::panic::Location::caller();
                        let set_at = UI_THREAD_SET_LOCATION.get();
                        let set_info = set_at
                            .map(|loc| format!(" (set at {}:{})", loc.file(), loc.line()))
                            .unwrap_or_default();
                        panic!(
                            "Unsync runtime access from non-UI thread\n  expected UI thread: {:?}{}\n  current thread: {:?}\n  caller: {}:{}",
                            ui_id,
                            set_info,
                            current,
                            caller.file(),
                            caller.line(),
                        );
                    }
                    #[cfg(not(debug_assertions))]
                    {
                        assert_eq!(
                            *ui_id, current,
                            "Unsync runtime access from non-UI thread: expected {:?}, got {:?}",
                            ui_id, current
                        );
                    }
                }
            }
            None => {
                // Once enforcement is on, first access defines the UI thread.
                let _ = UI_THREAD_ID.set(current);
            }
        }
    }

    pub fn is_ui_thread() -> bool {
        if !ENFORCE_UI_THREAD.load(Ordering::Relaxed) {
            true
        } else {
            UI_THREAD_ID
                .get()
                .map(|id| *id == thread::current().id())
                .unwrap_or(false)
        }
    }

    pub(crate) fn register_effect(&self, effect: &Rc<dyn EffectTrait>) {
        self.effects
            .borrow_mut()
            .insert(effect.id(), effect.clone());
    }

    pub(crate) fn remove_effect(&self, id: Id) {
        self.effects.borrow_mut().remove(&id);
    }

    pub(crate) fn get_effect(&self, id: Id) -> Option<Rc<dyn EffectTrait>> {
        self.effects.borrow().get(&id).cloned()
    }

    pub(crate) fn add_pending_effect(&self, effect_id: Id) {
        let mut set = self.pending_effects_set.borrow_mut();
        if set.insert(effect_id) {
            self.pending_effects.borrow_mut().push(effect_id);
        }
    }

    pub(crate) fn run_pending_effects(&self) {
        loop {
            let mut pending_effects = self.pending_effects.take();
            if pending_effects.is_empty() {
                break;
            }
            pending_effects.sort_by_key(|id| {
                let priority = self
                    .get_effect(*id)
                    .map(|effect| effect.priority())
                    .unwrap_or(EffectPriority::Normal);
                (Reverse(priority), *id)
            });
            for effect_id in pending_effects {
                self.pending_effects_set.borrow_mut().remove(&effect_id);
                if let Some(effect) = self.get_effect(effect_id) {
                    run_effect(effect);
                }
            }
        }
    }

    /// Drain any queued work from the sync runtime and run pending UI effects.
    pub fn drain_pending_work() {
        Runtime::assert_ui_thread();
        let pending_effects = SYNC_RUNTIME.take_pending_effects();
        let pending_disposals = SYNC_RUNTIME.take_pending_disposals();
        RUNTIME.with(|runtime| {
            for id in pending_effects {
                runtime.add_pending_effect(id);
            }
            for id in pending_disposals {
                id.dispose();
            }
            runtime.run_pending_effects();
        });
    }

    /// Returns true if there is queued work for this runtime or the sync runtime.
    pub fn has_pending_work() -> bool {
        RUNTIME.with(|runtime| !runtime.pending_effects.borrow().is_empty())
            || SYNC_RUNTIME.has_pending_effects()
            || SYNC_RUNTIME.has_pending_disposals()
    }

    /// Set a waker that will be called when a sync signal is updated off the UI thread.
    /// The waker should nudge the UI event loop (e.g., by sending a proxy event).
    pub fn set_sync_effect_waker(waker: impl Fn() + Send + Sync + 'static) {
        SYNC_RUNTIME.set_waker(waker);
    }
}
