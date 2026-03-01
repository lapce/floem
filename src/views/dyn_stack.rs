use std::{
    hash::{BuildHasherDefault, Hash},
    marker::PhantomData,
};

use floem_reactive::{Effect, Scope};
use rustc_hash::FxHasher;
use smallvec::SmallVec;

use crate::{
    context::UpdateCx,
    view::ViewId,
    view::{IntoView, View},
    window::state::WindowState,
};

pub(crate) type FxIndexSet<T> = indexmap::IndexSet<T, BuildHasherDefault<FxHasher>>;

type ViewFn<T> = Box<dyn Fn(T) -> (Box<dyn View>, Scope)>;

#[derive(educe::Educe)]
#[educe(Debug)]
pub(crate) struct HashRun<T>(#[educe(Debug(ignore))] pub(crate) T);

pub struct DynStack<T>
where
    T: 'static,
{
    id: ViewId,
    children: Vec<Option<(ViewId, Scope)>>,
    view_fn: ViewFn<T>,
    phantom: PhantomData<T>,
}

/// A stack whose items can be reactively updated.
///
/// This is useful when you have a list of views that change over time.
///
/// The [`dyn_stack`] takes a function that returns an iterator of items.
/// If the function contains a signal, such as an `RwSignal<Vec<u32>>`, when that signal is updated the views will also update.
/// The [`dyn_stack`] internally keeps track of changes to the items and ensures that, if an item hash did not change, the associated view is not reloaded.
///
/// The [`dyn_stack`] tracks the uniqueness of items by letting you provide a `key function`.
/// This key function gives you a reference to an item from the list and lets you return a value that can be hashed.
/// That value is what tells the [`dyn_stack`] how the item is unique from the others.
/// Often times, the item in the list, such as u32 in this case, already implements hash and you can simply return the same value.
///
/// ## Example
/// ```
/// use floem::reactive::*;
/// use floem::views::*;
///
/// let items = create_rw_signal(vec![1,2,3,4]);
///
/// dyn_stack(
///    move || items.get(),
///    move |item| *item,
///    move |item| label(move || item),
/// );
/// ```
/// This will only work if all of the items in the list are unique.
/// If all of the items are not unique, you can choose some other value to hash. It is common to use an [`AtomicU32`](std::sync::atomic::AtomicU32) to accomplish this.
///
/// ## Example
/// ```
///
/// use std::sync::atomic::{AtomicU32, Ordering};
/// use floem::reactive::*;
/// use floem::views::*;
///
/// let items = create_rw_signal(vec![1,1,2,2,3,3,4,4]);
/// let unique_atomic = AtomicU32::new(0);
///
/// dyn_stack(
///    move || items.get(),
///    move |_item| unique_atomic.fetch_add(1, Ordering::Relaxed),
///    move |item| label(move || item),
/// );
/// ```
///
pub fn dyn_stack<IF, I, T, KF, K, VF, V>(each_fn: IF, key_fn: KF, view_fn: VF) -> DynStack<T>
where
    IF: Fn() -> I + 'static,
    I: IntoIterator<Item = T>,
    KF: Fn(&T) -> K + 'static,
    K: Eq + Hash + 'static,
    VF: Fn(T) -> V + 'static,
    V: IntoView + 'static,
    T: 'static,
{
    let id = ViewId::new();
    Effect::new(move |prev_hash_run| {
        let items = each_fn();
        let items = items.into_iter().collect::<SmallVec<[_; 128]>>();
        let hashed_items = items.iter().map(&key_fn).collect::<FxIndexSet<_>>();
        let diff = if let Some(HashRun(prev_hash_run)) = prev_hash_run {
            let mut cmds = diff(&prev_hash_run, &hashed_items);
            let mut items = items
                .into_iter()
                .map(|i| Some(i))
                .collect::<SmallVec<[Option<_>; 128]>>();
            for added in &mut cmds.added {
                added.view = Some(items[added.at].take().unwrap());
            }
            cmds
        } else {
            let mut diff = Diff::default();
            for (i, item) in each_fn().into_iter().enumerate() {
                diff.added.push(DiffOpAdd {
                    at: i,
                    view: Some(item),
                });
            }
            diff
        };
        id.update_state(diff);
        HashRun(hashed_items)
    });
    let view_fn = Box::new(Scope::current().enter_child(move |e| view_fn(e).into_any()));
    DynStack {
        id,
        children: Vec::new(),
        view_fn,
        phantom: PhantomData,
    }
}

impl<T> View for DynStack<T> {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "DynStack".into()
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(diff) = state.downcast() {
            apply_diff(
                self.id(),
                cx.window_state,
                *diff,
                &mut self.children,
                &self.view_fn,
            );
            self.id.request_all();
        }
    }
}

#[derive(Debug)]
pub struct Diff<V> {
    pub(crate) removed: SmallVec<[DiffOpRemove; 8]>,
    pub(crate) moved: SmallVec<[DiffOpMove; 8]>,
    pub(crate) added: SmallVec<[DiffOpAdd<V>; 8]>,
    pub(crate) clear: bool,
}

impl<V> Default for Diff<V> {
    fn default() -> Self {
        Self {
            removed: Default::default(),
            moved: Default::default(),
            added: Default::default(),
            clear: false,
        }
    }
}

impl<V> Diff<V> {
    pub fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.moved.is_empty() && self.added.is_empty() && !self.clear
    }
}

#[derive(Debug)]
pub struct DiffOpMove {
    pub(crate) from: usize,
    pub(crate) to: usize,
}

#[derive(Debug)]
pub struct DiffOpAdd<V> {
    pub(crate) at: usize,
    pub(crate) view: Option<V>,
}

#[derive(Debug)]
pub struct DiffOpRemove {
    pub(crate) at: usize,
}

/// Calculates the operations need to get from `a` to `b`.
pub(crate) fn diff<K: Eq + Hash, V>(from: &FxIndexSet<K>, to: &FxIndexSet<K>) -> Diff<V> {
    if from.is_empty() && to.is_empty() {
        return Diff::default();
    } else if to.is_empty() {
        return Diff {
            clear: true,
            ..Default::default()
        };
    }

    // Get removed items
    let mut removed = from.difference(to);

    let removed_cmds = removed
        .clone()
        .map(|k| from.get_full(k).unwrap().0)
        .map(|idx| DiffOpRemove { at: idx });

    // Get added items
    let mut added = to.difference(from);

    let added_cmds = added
        .clone()
        .map(|k| to.get_full(k).unwrap().0)
        .map(|idx| DiffOpAdd {
            at: idx,
            view: None,
        });

    // Get moved items
    let mut normalized_idx = 0;
    let mut move_cmds = SmallVec::<[_; 8]>::with_capacity(to.len());
    let mut added_idx = added.next().map(|k| to.get_full(k).unwrap().0);
    let mut removed_idx = removed.next().map(|k| from.get_full(k).unwrap().0);

    for (idx, k) in to.iter().enumerate() {
        if let Some(added_idx) = added_idx.as_mut().filter(|r_i| **r_i == idx)
            && let Some(next_added) = added.next().map(|k| to.get_full(k).unwrap().0)
        {
            *added_idx = next_added;

            normalized_idx = usize::wrapping_sub(normalized_idx, 1);
        }

        if let Some(removed_idx) = removed_idx.as_mut().filter(|r_i| **r_i == idx) {
            normalized_idx = normalized_idx.wrapping_add(1);

            if let Some(next_removed) = removed.next().map(|k| from.get_full(k).unwrap().0) {
                *removed_idx = next_removed;
            }
        }

        if let Some((from_idx, _)) = from.get_full(k)
            && (from_idx != normalized_idx || from_idx != idx)
        {
            move_cmds.push(DiffOpMove {
                from: from_idx,
                to: idx,
            });
        }

        normalized_idx = normalized_idx.wrapping_add(1);
    }

    let mut diffs = Diff {
        removed: removed_cmds.collect(),
        moved: move_cmds,
        added: added_cmds.collect(),
        clear: false,
    };

    if !from.is_empty()
        && !to.is_empty()
        && diffs.removed.len() == from.len()
        && diffs.moved.is_empty()
    {
        diffs.clear = true;
    }

    diffs
}

fn remove_index(
    window_state: &mut WindowState,
    children: &mut [Option<(ViewId, Scope)>],
    index: usize,
) -> Option<()> {
    let (view_id, scope) = std::mem::take(&mut children[index])?;
    window_state.remove_view(view_id);
    scope.dispose();
    Some(())
}

pub(crate) fn apply_diff<T, VF>(
    view_id: ViewId,
    window_state: &mut WindowState,
    mut diff: Diff<T>,
    children: &mut Vec<Option<(ViewId, Scope)>>,
    view_fn: &VF,
) where
    VF: Fn(T) -> (Box<dyn View>, Scope),
{
    // Resize children if needed
    if diff.added.len().checked_sub(diff.removed.len()).is_some() {
        let target_size =
            children.len() + (diff.added.len() as isize - diff.removed.len() as isize) as usize;

        children.resize_with(target_size, || None);
    }

    // We need to hold a list of items which will be moved, and
    // we can only perform the move after all commands have run, otherwise,
    // we risk overwriting one of the values
    let mut items_to_move = Vec::with_capacity(diff.moved.len());

    // The order of cmds needs to be:
    // 1. Clear
    // 2. Removed
    // 3. Moved
    // 4. Add
    if diff.clear {
        for i in 0..children.len() {
            remove_index(window_state, children, i);
        }
        diff.removed.clear();
    }

    for DiffOpRemove { at } in diff.removed {
        remove_index(window_state, children, at);
    }

    for DiffOpMove { from, to } in diff.moved {
        let item = children[from].take().unwrap();
        items_to_move.push((to, item));
    }

    for DiffOpAdd { at, view } in diff.added {
        let new_child = view.map(view_fn);
        children[at] = new_child.map(|(view, scope)| {
            let id = view.id();
            id.set_view(view);
            id.set_parent(view_id);
            (id, scope)
        });
    }

    for (to, each_item) in items_to_move {
        children[to] = Some(each_item);
    }

    // Now, remove the holes that might have been left from removing
    // items
    children.retain(|c| c.is_some());

    let children_ids: Vec<ViewId> = children
        .iter()
        .filter_map(|c| Some(c.as_ref()?.0))
        .collect();
    view_id.set_children_ids(children_ids);
}
