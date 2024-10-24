#[cfg(not(test))]
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    any::{Any, TypeId},
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use smallvec::SmallVec;

use crate::{
    effect::{run_effect, EffectTrait},
    id::Id,
    signal::Signal,
};

#[cfg(not(test))]
static CREATED: AtomicBool = AtomicBool::new(false);
thread_local! {
    pub(crate) static RUNTIME: Runtime = {
        #[cfg(not(test))]
        if CREATED.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            panic!("RUNTIME must only be created once. You are probably using signals from multiple threads. All signals need to be accessed exclusively from the main thread.");
        }
        Runtime::new()
    };
}

/// The internal reactive Runtime which stores all the reactive system states in a
/// thread local
pub(crate) struct Runtime {
    pub(crate) current_effect: RefCell<Option<Rc<dyn EffectTrait>>>,
    pub(crate) current_scope: RefCell<Id>,
    pub(crate) children: RefCell<HashMap<Id, HashSet<Id>>>,
    pub(crate) signals: RefCell<HashMap<Id, Signal>>,
    pub(crate) contexts: RefCell<HashMap<TypeId, Box<dyn Any>>>,
    pub(crate) batching: Cell<bool>,
    pub(crate) pending_effects: RefCell<SmallVec<[Rc<dyn EffectTrait>; 10]>>,
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
            contexts: Default::default(),
            batching: Cell::new(false),
            pending_effects: RefCell::new(SmallVec::new()),
        }
    }

    pub(crate) fn add_pending_effect(&self, effect: Rc<dyn EffectTrait>) {
        let has_effect = self
            .pending_effects
            .borrow()
            .iter()
            .any(|e| e.id() == effect.id());
        if !has_effect {
            self.pending_effects.borrow_mut().push(effect);
        }
    }

    pub(crate) fn run_pending_effects(&self) {
        let pending_effects = self.pending_effects.take();
        for effect in pending_effects {
            run_effect(effect);
        }
    }
}
