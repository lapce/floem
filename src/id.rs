use std::{cell::RefCell, collections::HashMap, num::NonZeroU64};

thread_local! {
    pub(crate) static IDPATHS: RefCell<HashMap<Id,IdPath>> = Default::default();
    pub(crate) static IDPATHS_CHILDREN: RefCell<HashMap<Id, Vec<Id>>> = Default::default();
    pub(crate) static NEXT_SIBLING: RefCell<HashMap<Id, Option<Id>>> = Default::default();
    pub(crate) static PREVIOUS_SIBLING: RefCell<HashMap<Id, Option<Id>>> = Default::default();
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
            if let Some(previous_child) = children.last() {
                NEXT_SIBLING.with(|next_sibling| {
                    next_sibling
                        .borrow_mut()
                        .insert(*previous_child, Some(new_id))
                });
                PREVIOUS_SIBLING.with(|previous_sibling| {
                    previous_sibling
                        .borrow_mut()
                        .insert(new_id, Some(*previous_child))
                });
            }
            children.push(new_id);
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

    /// Get the id of the view after this one (but with the same parent and level of nesting)
    pub fn next_sibling(&self) -> Option<Id> {
        NEXT_SIBLING.with(|next_sibling| next_sibling.borrow().get(self).copied().flatten())
    }

    /// Get the id of the view before this one (but with the same parent and level of nesting)
    pub fn previous_sibling(&self) -> Option<Id> {
        PREVIOUS_SIBLING
            .with(|previous_sibling| previous_sibling.borrow().get(self).copied().flatten())
    }

    /// A list of all the direct children of this view (no deep nesting)
    pub fn direct_children(&self) -> Vec<Id> {
        IDPATHS_CHILDREN.with(|idpaths_children| {
            idpaths_children
                .borrow()
                .get(self)
                .cloned()
                .unwrap_or_default()
        })
    }

    /// The first child with this view as a parent. The depth increases only by 1.
    pub fn first_child(&self) -> Option<Id> {
        IDPATHS_CHILDREN.with(|idpaths_children| {
            idpaths_children
                .borrow()
                .get(self)
                .and_then(|children| children.first())
                .copied()
        })
    }

    /// The last child with this view as a parent. The depth increases only by 1.
    pub fn last_child(&self) -> Option<Id> {
        IDPATHS_CHILDREN.with(|idpaths_children| {
            idpaths_children
                .borrow()
                .get(self)
                .and_then(|children| children.last())
                .copied()
        })
    }

    /// Get the next item in the tree, either the first child or the next sibling of this view or of the first parent view
    pub fn tree_next(&self) -> Option<Id> {
        self.first_child().or_else(|| {
            let mut ancestor = *self;
            loop {
                if let Some(next_sibling) = ancestor.next_sibling() {
                    return Some(next_sibling);
                }
                ancestor = ancestor.parent()?;
            }
        })
    }

    /// Get the next item in the tree, the deepest last child of the previous sibling of this view or the parent
    pub fn tree_previous(&self) -> Option<Id> {
        self.previous_sibling()
            .map(|id| id.nested_last_child())
            .or_else(|| self.parent())
    }

    /// Repeatedly get the last child until the deepest last child is found
    pub fn nested_last_child(&self) -> Id {
        let mut last_child = *self;
        while let Some(new_last_child) = last_child.last_child() {
            last_child = new_last_child;
        }
        last_child
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
                        let index = children.iter().position(|&id| id == *self).unwrap();
                        children.remove(index);
                        let previous_child = index.checked_sub(1).map(|index| children[index]);
                        let next_child = children.get(index + 1).copied();
                        if let Some(previous_child) = previous_child {
                            NEXT_SIBLING.with(|next_sibling| {
                                next_sibling.borrow_mut().insert(previous_child, next_child)
                            });
                        }
                        if let Some(next_child) = next_child {
                            PREVIOUS_SIBLING.with(|previous_sibling| {
                                previous_sibling
                                    .borrow_mut()
                                    .insert(next_child, previous_child)
                            });
                        }
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
