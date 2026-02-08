use std::{cell::RefCell, rc::Rc};

use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::{SecondaryMap, SlotMap};

use super::{AnyView, state::ViewState};
use crate::{BoxTree, IntoView, view::ViewId, window::handle::set_current_view};

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
pub type MeasureFn = dyn FnMut(
    taffy::Size<Option<f32>>,
    taffy::Size<taffy::AvailableSpace>,
    taffy::NodeId,
    &taffy::Style,
    &mut MeasureCx,
) -> taffy::Size<f32>;

pub type LayoutTree = taffy::TaffyTree<LayoutNodeCx>;

#[derive(Default, Clone, Debug)]
pub struct MeasureCx {
    pub(crate) needs_finalization: FxHashSet<taffy::NodeId>,
}
impl MeasureCx {
    pub fn needs_finalization(&mut self, node_id: taffy::NodeId) {
        self.needs_finalization.insert(node_id);
    }
}

/// the sizes are the total size and then the content size
pub type FinalizeFn = dyn Fn(taffy::NodeId, &taffy::Layout);

#[non_exhaustive]
pub enum LayoutNodeCx {
    Custom {
        measure: Box<MeasureFn>,
        finalize: Option<Box<FinalizeFn>>,
    },
}
impl std::fmt::Debug for LayoutNodeCx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Custom { .. } => f
                .debug_struct("Custom")
                .field("measure", &"<measure fn>")
                .field("finalize", &"<finalize fn>")
                .finish(),
        }
    }
}

pub(crate) struct ViewStorage {
    /// a map from the root view id to a taffy tree
    pub(crate) taffy: Rc<RefCell<taffy::TaffyTree<LayoutNodeCx>>>,
    /// a map from the root view id to a box tree
    pub(crate) box_tree: FxHashMap<ViewId, Rc<RefCell<crate::BoxTree>>>,
    pub(crate) view_ids: SlotMap<ViewId, ()>,
    pub(crate) views: SecondaryMap<ViewId, Rc<RefCell<AnyView>>>,
    pub(crate) children: SecondaryMap<ViewId, Vec<ViewId>>,
    // the parent of a View
    pub(crate) parent: SecondaryMap<ViewId, Option<ViewId>>,
    /// Cache the root [`ViewId`] for a view
    pub(crate) root: SecondaryMap<ViewId, ViewId>,
    pub(crate) states: SecondaryMap<ViewId, Rc<RefCell<ViewState>>>,
    pub(crate) stale_view_state: Rc<RefCell<ViewState>>,
    pub(crate) stale_view: Rc<RefCell<AnyView>>,
    /// Views registered as overlays - maps overlay ViewId to its window root ViewId
    pub(crate) overlays: SecondaryMap<ViewId, ViewId>,
    pub(crate) taffy_to_view: FxHashMap<taffy::NodeId, ViewId>,
}

impl Default for ViewStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewStorage {
    pub fn new() -> Self {
        // a taffy tree that is used for the stale view state. this will just be dropped
        let mut taffy = taffy::TaffyTree::<LayoutNodeCx>::new();
        taffy.disable_rounding();
        let mut view_ids = SlotMap::<ViewId, ()>::default();
        let stale_id = view_ids.insert(());
        let mut root = SecondaryMap::new();
        root.insert(stale_id, stale_id);
        set_current_view(stale_id);

        // a box tree that is used for the stale view state. this will just be dropped
        let mut box_tree = BoxTree::with_backend(understory_index::backends::GridF64::new(100.));
        // let mut box_tree = BoxTree::new();

        let state_view_state = ViewState::new(stale_id, &mut taffy, &mut box_tree);

        Self {
            taffy: Rc::new(RefCell::new(taffy)),
            box_tree: FxHashMap::default(),
            view_ids,
            views: Default::default(),
            children: Default::default(),
            parent: Default::default(),
            root,
            states: Default::default(),
            stale_view_state: Rc::new(RefCell::new(state_view_state)),
            stale_view: Rc::new(RefCell::new(
                crate::views::Empty {
                    id: ViewId::default(),
                }
                .into_any(),
            )),
            overlays: Default::default(),
            taffy_to_view: FxHashMap::default(),
        }
    }

    pub(crate) fn box_tree(&mut self, view_id: ViewId) -> Rc<RefCell<BoxTree>> {
        let root = self
            .root
            .get(view_id)
            .expect("all view ids are created with a root");
        self.box_tree
            .entry(*root)
            .or_insert_with(|| {
                Rc::new(RefCell::new(BoxTree::with_backend(
                    understory_index::backends::GridF64::new(100.),
                )))
            })
            .clone()
    }

    pub(crate) fn state(&mut self, id: ViewId) -> Rc<RefCell<ViewState>> {
        if !self.view_ids.contains_key(id) {
            // if view_ids doesn't have this view id, that means it's been cleaned up,
            // so we shouldn't create a new ViewState for this Id.
            self.stale_view_state.clone()
        } else {
            let root = self
                .root
                .get(id)
                .expect("all view ids are created with a root");
            self.states
                .entry(id)
                .unwrap()
                .or_insert_with(|| {
                    let taffy = self.taffy.clone();
                    let box_tree = self.box_tree.entry(*root).or_insert_with(|| {
                        Rc::new(RefCell::new(BoxTree::with_backend(
                            understory_index::backends::GridF64::new(100.),
                        )))
                    });
                    let state = Rc::new(RefCell::new(ViewState::new(
                        id,
                        &mut taffy.borrow_mut(),
                        &mut box_tree.borrow_mut(),
                    )));
                    // Add to reverse mapping
                    self.taffy_to_view.insert(state.borrow().layout_id, id);
                    state
                })
                .clone()
        }
    }
}
