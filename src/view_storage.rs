use std::{cell::RefCell, rc::Rc};

use slotmap::{new_key_type, SecondaryMap, SlotMap};

use crate::{context::ViewState, view::View};

thread_local! {
    pub(crate) static VIEW_STORAGE: RefCell<ViewStorage> = Default::default();
}

#[derive(Default)]
pub struct ViewStorage {
    taffy: taffy::TaffyTree,
    view_ids: SlotMap<ViewId, ()>,
    views: SecondaryMap<ViewId, Rc<RefCell<Box<dyn View>>>>,
    states: SecondaryMap<ViewId, Rc<RefCell<ViewState>>>,
}

new_key_type! {
   pub struct ViewId;
}

impl ViewId {
    pub fn new() -> ViewId {
        VIEW_STORAGE.with_borrow_mut(|s| s.view_ids.insert(()))
    }

    pub fn state(&self) -> Rc<RefCell<ViewState>> {
        VIEW_STORAGE.with_borrow_mut(|s| {
            s.states
                .entry(*self)
                .unwrap()
                .or_insert_with(|| Rc::new(RefCell::new(ViewState::new(&mut s.taffy))))
                .clone()
        })
    }
}
