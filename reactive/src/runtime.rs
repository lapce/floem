use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use crate::{effect::EffectTrait, id::Id, signal::Signal};

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
        }
    }
}
