use std::{cell::RefCell, rc::Rc};

use slotmap::{SecondaryMap, SlotMap};

use crate::{id::ViewId, view::AnyView, view_state::ViewState, IntoView};

thread_local! {
    pub(crate) static VIEW_STORAGE: RefCell<ViewStorage> = Default::default();
}

pub(crate) struct ViewStorage {
    pub(crate) taffy: Rc<RefCell<taffy::TaffyTree>>,
    pub(crate) view_ids: SlotMap<ViewId, ()>,
    pub(crate) views: SecondaryMap<ViewId, Rc<RefCell<AnyView>>>,
    pub(crate) children: SecondaryMap<ViewId, Vec<ViewId>>,
    // the parent of a View
    pub(crate) parent: SecondaryMap<ViewId, Option<ViewId>>,
    /// Cache the root ViewId for a view
    pub(crate) root: SecondaryMap<ViewId, Option<ViewId>>,
    pub(crate) states: SecondaryMap<ViewId, Rc<RefCell<ViewState>>>,
    pub(crate) stale_view_state: Rc<RefCell<ViewState>>,
    pub(crate) stale_view: Rc<RefCell<AnyView>>,
}

impl Default for ViewStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewStorage {
    pub fn new() -> Self {
        let mut taffy = taffy::TaffyTree::default();
        taffy.disable_rounding();
        let state_view_state = ViewState::new(&mut taffy);

        Self {
            taffy: Rc::new(RefCell::new(taffy)),
            view_ids: Default::default(),
            views: Default::default(),
            children: Default::default(),
            parent: Default::default(),
            root: Default::default(),
            states: Default::default(),
            stale_view_state: Rc::new(RefCell::new(state_view_state)),
            stale_view: Rc::new(RefCell::new(
                crate::views::Empty {
                    id: ViewId::default(),
                }
                .into_any(),
            )),
        }
    }

    pub(crate) fn root_view_id(&self, id: ViewId) -> Option<ViewId> {
        let mut parent = self.parent.get(id).cloned().flatten();
        while let Some(parent_id) = parent {
            match self.parent.get(parent_id).cloned().flatten() {
                Some(id) => {
                    parent = Some(id);
                }
                None => {
                    return parent;
                }
            }
        }
        parent
    }
}
