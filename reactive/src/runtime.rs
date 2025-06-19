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

thread_local! {
    pub(crate) static RUNTIME: Runtime = Runtime::new();
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
    pub(crate) hot_patched_effects: RefCell<HashMap<Id, u64>>,
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
            hot_patched_effects: RefCell::new(HashMap::new()),
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

pub(crate) fn register_effect<T: EffectTrait + 'static>(effect: Rc<T>) {
    RUNTIME.with(|runtime| {
        let Ok(mut hot_patched_effects) = runtime.hot_patched_effects.try_borrow_mut() else {
            return;
        };
        let effect_id = effect.id();
        let current_ptr = effect.hot_fn_ptr();
        hot_patched_effects.insert(effect_id, current_ptr);
    });
}

pub fn hotpatch() {
    RUNTIME.with(|runtime| {
        let Ok(mut hot_patched_effects) = runtime.hot_patched_effects.try_borrow_mut() else {
            return;
        };
        println!("Starting hotpatch");
        let mut effects = HashMap::new();
        for signal in runtime.signals.borrow().values() {
            for effect in signal.subscribers.borrow().values() {
                if Some(effect.id()) != runtime.current_effect.borrow().as_ref().map(|v| v.id()) {
                    effects.insert(effect.id(), effect.clone());
                }
            }
        }
        for effect in effects.values() {
            let effect_id = effect.id();
            let new_ptr = effect.hot_fn_ptr();
            if let Some(current_ptr) = hot_patched_effects.get(&effect_id) {
                if new_ptr != *current_ptr {
                    dbg!("different pointer");
                    hot_patched_effects.insert(effect_id, new_ptr);
                    effect.run();
                } else {
                }
            } else {
                hot_patched_effects.insert(effect_id, new_ptr);
            }
        }
        println!("Finished hotpatch");
    });
}
