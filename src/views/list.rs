use std::hash::{BuildHasherDefault, Hash};

use indexmap::IndexMap;
use leptos_reactive::create_effect;
use rustc_hash::FxHasher;
use smallvec::SmallVec;
use taffy::style::{FlexDirection, Style};

use crate::{
    app::{AppContext, UpdateMessage},
    context::{EventCx, UpdateCx},
    id::Id,
    view::{ChangeFlags, View},
};

type FxIndexSet<T> = indexmap::IndexSet<T, BuildHasherDefault<FxHasher>>;

#[derive(educe::Educe)]
#[educe(Debug)]
struct HashRun<T>(#[educe(Debug(ignore))] T);

enum ListDirection {
    Horizontal,
    Vertical,
}

pub struct List<V: View> {
    id: Id,
    children: IndexMap<Id, Option<V>>,
}

pub fn list<IF, I, T, KF, K, VF, V>(cx: AppContext, each_fn: IF, key_fn: KF, view_fn: VF) -> List<V>
where
    IF: Fn() -> I + 'static,
    I: IntoIterator<Item = T>,
    KF: Fn(&T) -> K + 'static,
    K: Eq + Hash + 'static,
    VF: Fn(AppContext, T) -> V + 'static,
    V: View + 'static,
    T: 'static,
{
    let id = cx.new_id();

    let mut child_cx = cx;
    child_cx.id = id;
    create_effect(cx.scope, move |prev_hash_run| {
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
                added.view = Some(view_fn(child_cx, items[added.at].take().unwrap()));
            }
            cmds
        } else {
            let mut diff = Diff::default();
            for (i, item) in each_fn().into_iter().enumerate() {
                diff.added.push(DiffOpAdd {
                    at: i,
                    view: Some(view_fn(child_cx, item)),
                });
            }
            diff
        };
        AppContext::add_update(UpdateMessage::new(id, diff));
        HashRun(hashed_items)
    });
    List {
        id,
        children: IndexMap::new(),
    }
}

impl<V: View + 'static> View for List<V> {
    fn id(&self) -> Id {
        self.id
    }

    fn update(
        &mut self,
        cx: &mut UpdateCx,
        id_path: &[Id],
        state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        if id_path.last().unwrap() == &self.id() {
            if let Ok(diff) = state.downcast() {
                apply_cmds(*diff, &mut self.children);
                cx.request_layout(self.id());
                cx.reset_children_layout(self.id);
                ChangeFlags::LAYOUT
            } else {
                ChangeFlags::empty()
            }
        } else {
            let id_path = &id_path[1..];
            if let Some(Some(child)) = self.children.get_mut(id_path.first().unwrap()) {
                child.update(cx, id_path, state)
            } else {
                ChangeFlags::empty()
            }
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| {
            let nodes = self
                .children
                .iter_mut()
                .filter_map(|(_id, child)| Some(child.as_mut()?.layout(cx)))
                .collect::<Vec<_>>();
            nodes
        })
    }

    fn event(&mut self, cx: &mut EventCx, event: crate::event::Event) {
        for (_, child) in self.children.iter_mut() {
            if let Some(child) = child.as_mut() {
                let id = child.id();
                if cx.should_send(id, &event) {
                    let event = cx.offset_event(id, event.clone());
                    child.event(cx, cx.offset_event(id, event));
                    break;
                }
            }
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        for (_, child) in self.children.iter_mut() {
            if let Some(child) = child.as_mut() {
                child.paint_main(cx);
            }
        }
    }
}

#[derive(Debug)]
pub struct Diff<V> {
    removed: SmallVec<[DiffOpRemove; 8]>,
    moved: SmallVec<[DiffOpMove; 8]>,
    added: SmallVec<[DiffOpAdd<V>; 8]>,
    clear: bool,
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

#[derive(Debug)]
struct DiffOpMove {
    from: usize,
    to: usize,
}

#[derive(Debug)]
struct DiffOpAdd<V> {
    at: usize,
    view: Option<V>,
}

#[derive(Debug)]
struct DiffOpRemove {
    at: usize,
}

#[derive(Debug)]
enum DiffOpAddMode {
    Normal,
    Append,
}

impl Default for DiffOpAddMode {
    fn default() -> Self {
        Self::Normal
    }
}

/// Calculates the operations need to get from `a` to `b`.
fn diff<K: Eq + Hash, V>(from: &FxIndexSet<K>, to: &FxIndexSet<K>) -> Diff<V> {
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
        if let Some(added_idx) = added_idx.as_mut().filter(|r_i| **r_i == idx) {
            if let Some(next_added) = added.next().map(|k| to.get_full(k).unwrap().0) {
                *added_idx = next_added;

                normalized_idx = usize::wrapping_sub(normalized_idx, 1);
            }
        }

        if let Some(removed_idx) = removed_idx.as_mut().filter(|r_i| **r_i == idx) {
            normalized_idx = normalized_idx.wrapping_add(1);

            if let Some(next_removed) = removed.next().map(|k| from.get_full(k).unwrap().0) {
                *removed_idx = next_removed;
            }
        }

        if let Some((from_idx, _)) = from.get_full(k) {
            if from_idx != normalized_idx || from_idx != idx {
                move_cmds.push(DiffOpMove {
                    from: from_idx,
                    to: idx,
                });
            }
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

fn apply_cmds<V>(mut cmds: Diff<V>, children: &mut IndexMap<Id, Option<V>>)
where
    V: View,
{
    // Resize children if needed
    if cmds.added.len().checked_sub(cmds.removed.len()).is_some() {
        let target_size =
            children.len() + (cmds.added.len() as isize - cmds.removed.len() as isize) as usize;

        if target_size > children.len() {
            children.extend(
                (0..target_size - children.len())
                    .into_iter()
                    .map(|_| (Id::next(), None)),
            );
        } else if target_size < children.len() {
            children.truncate(target_size);
        }
        // children.resize_with(target_size, || None);
    }

    // We need to hold a list of items which will be moved, and
    // we can only perform the move after all commands have run, otherwise,
    // we risk overwriting one of the values
    let mut items_to_move = Vec::with_capacity(cmds.moved.len());

    // The order of cmds needs to be:
    // 1. Clear
    // 2. Removed
    // 3. Moved
    // 4. Add
    if cmds.clear {
        cmds.removed.clear();
    }

    for DiffOpRemove { at } in cmds.removed {
        let item_to_remove = std::mem::take(&mut children[at]).unwrap();
    }

    for DiffOpMove { from, to } in cmds.moved {
        let item = std::mem::take(&mut children[from]).unwrap();
        items_to_move.push((to, item));
    }

    for DiffOpAdd { at, view } in cmds.added {
        children[at] = view;
    }

    for (to, each_item) in items_to_move {
        children[to] = Some(each_item);
    }

    // Now, remove the holes that might have been left from removing
    // items
    children.retain(|_, c| c.is_some());
}
