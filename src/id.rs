use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    num::NonZeroU64,
};

thread_local! {
    pub(crate) static IDPATHS: RefCell<HashMap<Id,IdPath>> = Default::default();
    pub(crate) static IDPATHS_CHILDREN: RefCell<HashMap<Id, HashSet<Id>>> = Default::default();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Hash)]
/// A stable identifier for an element.
pub struct Id(NonZeroU64);

#[derive(Clone, Default)]
pub struct IdPath(pub(crate) Vec<Id>);

impl Id {
    /// Allocate a new, unique `Id`.
    pub fn next() -> Id {
        use glazier::Counter;
        static WIDGET_ID_COUNTER: Counter = Counter::new();
        Id(WIDGET_ID_COUNTER.next_nonzero())
    }

    pub fn new(&self) -> Id {
        let mut id_path =
            IDPATHS.with(|id_paths| id_paths.borrow().get(self).cloned().unwrap_or_default());
        let new_id = Self::next();
        id_path.0.push(new_id);
        IDPATHS.with(|id_paths| {
            id_paths.borrow_mut().insert(new_id, id_path);
        });
        IDPATHS_CHILDREN.with(|children| {
            let mut children = children.borrow_mut();
            let children = children.entry(*self).or_default();
            children.insert(new_id);
        });
        new_id
    }

    pub fn parent(&self) -> Option<Id> {
        IDPATHS.with(|id_paths| {
            id_paths.borrow().get(self).and_then(|id_path| {
                let id_path = &id_path.0;
                let len = id_path.len();
                if len >= 2 {
                    Some(id_path[len - 2])
                } else {
                    None
                }
            })
        })
    }

    pub fn all_chilren(&self) -> Vec<Id> {
        let mut children = Vec::new();
        let mut parents = Vec::new();
        parents.push(*self);

        IDPATHS_CHILDREN.with(|idpaths_children| {
            let idpaths_children = idpaths_children.borrow();
            while !parents.is_empty() {
                let parent = parents.pop().unwrap();
                if let Some(c) = idpaths_children.get(&parent) {
                    for child in c {
                        children.push(*child);
                        parents.push(*child);
                    }
                }
            }
        });
        children
    }

    pub fn remove_idpath(&self) {
        let id_path = IDPATHS.with(|id_paths| id_paths.borrow_mut().remove(self));
        if let Some(id_path) = id_path.as_ref() {
            if let Some(parent) = id_path.0.get(id_path.0.len().saturating_sub(2)) {
                IDPATHS_CHILDREN.with(|idpaths_children| {
                    if let Some(children) = idpaths_children.borrow_mut().get_mut(parent) {
                        children.remove(self);
                    }
                });
            }
        }
        IDPATHS_CHILDREN.with(|idpaths_children| {
            idpaths_children.borrow_mut().remove(self);
        });
    }

    #[allow(unused)]
    pub fn to_raw(self) -> u64 {
        self.0.into()
    }

    pub fn to_nonzero_raw(self) -> NonZeroU64 {
        self.0
    }
}
