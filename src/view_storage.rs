use std::{cell::RefCell, collections::HashMap, rc::Rc};

use slotmap::{SecondaryMap, SlotMap};
use taffy::NodeId;
use understory_box_tree::{NodeId as UnderNode, Tree as UnderTree};

use crate::{IntoView, id::ViewId, view::AnyView, view_state::ViewState};

thread_local! {
    pub(crate) static VIEW_STORAGE: RefCell<ViewStorage> = Default::default();
}

/// The available_space is the size of the "containing block" (roughly, the parent node) (closely related to the parent_size, but sometimes the available_space is set to MinContent or MaxContent as part of a multi-pass layout algorithm even if the size of parent is actually known. It's best to think of available_space as a constraint that asks: "If you had this much space, then what size would you be?".
///
/// The known_dimensions are the size of the node itself. And indeed, if the known_dimension in a particular axis is set then you can generally (always I think) ignore the available_space in that axis. The purpose of known_dimensions is to allow the algorithm to ask "If your size in this dimension is exactly X, then what would you size in the other dimension be".
///
/// TL;DR: known_dimensions takes precedence over available_space.
///
/// P.S. known_dimensions is a hard constraint. If known_dimensions is set in an axis, then the output size in that axis will be ignored entirely and only the output size in the other axis used.
///
/// P.P.S. It may well be possible to combine them into a single enum at somepoint. In which case known_dimensions: Some(_) would become something like an AvailableSpace::Exact(_) variant.
///
/// taken from https://github.com/DioxusLabs/taffy/discussions/716#discussioncomment-10846100
pub type MeasureFunction = dyn FnMut(
    taffy::Size<Option<f32>>,
    taffy::Size<taffy::AvailableSpace>,
    taffy::NodeId,
    &taffy::Style,
) -> taffy::Size<f32>;

#[derive(Default)]
#[non_exhaustive]
pub enum NodeContext {
    #[default]
    None,
    Custom(Box<MeasureFunction>),
}

pub(crate) struct ViewStorage {
    pub(crate) taffy: Rc<RefCell<taffy::TaffyTree<NodeContext>>>,
    pub(crate) view_ids: SlotMap<ViewId, ()>,
    pub(crate) views: SecondaryMap<ViewId, Rc<RefCell<AnyView>>>,
    pub(crate) children: SecondaryMap<ViewId, Vec<ViewId>>,
    // the parent of a View
    pub(crate) parent: SecondaryMap<ViewId, Option<ViewId>>,
    /// Cache the root [`ViewId`] for a view
    pub(crate) root: SecondaryMap<ViewId, Option<ViewId>>,
    pub(crate) states: SecondaryMap<ViewId, Rc<RefCell<ViewState>>>,
    pub(crate) node_to_view: HashMap<NodeId, ViewId>,
    pub(crate) box_node_to_view: RefCell<HashMap<understory_box_tree::NodeId, ViewId>>,
    pub(crate) box_tree: Rc<RefCell<UnderTree>>,
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
        let mut taffy = taffy::TaffyTree::<NodeContext>::new();
        let mut box_tree = UnderTree::new();
        taffy.disable_rounding();
        let state_view_state = ViewState::new(&mut taffy, &mut box_tree);

        Self {
            taffy: Rc::new(RefCell::new(taffy)),
            view_ids: Default::default(),
            views: Default::default(),
            children: Default::default(),
            parent: Default::default(),
            root: Default::default(),
            states: Default::default(),
            node_to_view: Default::default(),
            box_node_to_view: Default::default(),
            box_tree: Rc::new(RefCell::new(box_tree)),
            stale_view_state: Rc::new(RefCell::new(state_view_state)),
            stale_view: Rc::new(RefCell::new(
                crate::views::Empty {
                    id: ViewId::default(),
                }
                .into_any(),
            )),
        }
    }

    pub(crate) fn state(&mut self, id: ViewId) -> Rc<RefCell<ViewState>> {
        if !self.view_ids.contains_key(id) {
            // if view_ids doesn't have this view id, that means it's been cleaned up,
            // so we shouldn't create a new ViewState for this Id.
            self.stale_view_state.clone()
        } else {
            self.states
                .entry(id)
                .unwrap()
                .or_insert_with(|| {
                    let state = Rc::new(RefCell::new(ViewState::new(
                        &mut self.taffy.borrow_mut(),
                        &mut self.box_tree.borrow_mut(),
                    )));
                    // Add to reverse mapping
                    self.node_to_view.insert(state.borrow().node, id);
                    self.box_node_to_view
                        .borrow_mut()
                        .insert(state.borrow().box_node, id);
                    state
                })
                .clone()
        }
    }

    /// Returns the deepest view ID encountered traversing parents.  It does *not* guarantee
    /// that it is a real window root; any caller should perform the same test
    /// of `window_tracking::is_known_root()` that `ViewId.root()` does before
    /// assuming the returned value is really a window root.
    pub(crate) fn root_view_id(&self, id: ViewId) -> Option<ViewId> {
        if let Some(p) = self.parent.get(id).unwrap_or(&None) {
            self.root_view_id(*p)
        } else {
            Some(id)
        }
    }
}
