use std::hash::Hash;

use glazier::kurbo::Rect;
use indexmap::IndexMap;
use leptos_reactive::create_effect;
use smallvec::SmallVec;

use crate::{
    app::AppContext,
    id::Id,
    view::{ChangeFlags, View},
};

use super::{apply_diff, diff, Diff, DiffOpAdd, FxIndexSet, HashRun};

#[derive(Clone, Copy)]
pub enum VirtualListDirection {
    Vertical,
    Horizontal,
}

pub struct VirtualList<V: View> {
    id: Id,
    direction: VirtualListDirection,
    children: IndexMap<Id, Option<V>>,
}

struct VirtualListState<V> {
    diff: Diff<V>,
    view_port: Rect,
}

pub fn virtual_list<T, IF, I, VPF, SF, KF, K, VF, V>(
    cx: AppContext,
    direction: VirtualListDirection,
    each_fn: IF,
    key_fn: KF,
    view_fn: VF,
    view_port_fn: VPF,
    size_fn: SF,
    fixed_size: bool,
) -> VirtualList<V>
where
    T: 'static,
    IF: Fn() -> I + 'static,
    I: IntoIterator<Item = T>,
    VPF: Fn() -> Rect + 'static,
    SF: Fn(&T) -> f64 + 'static,
    KF: Fn(&T) -> K + 'static,
    K: Eq + Hash + 'static,
    VF: Fn(AppContext, T) -> V + 'static,
    V: View + 'static,
{
    let id = cx.new_id();

    let mut child_cx = cx;
    child_cx.id = id;
    create_effect(cx.scope, move |prev_hash_run| {
        let items_iter = each_fn();
        let view_port = view_port_fn();
        let min = match direction {
            VirtualListDirection::Vertical => view_port.y0,
            VirtualListDirection::Horizontal => view_port.x0,
        };
        let max = match direction {
            VirtualListDirection::Vertical => view_port.y1,
            VirtualListDirection::Horizontal => view_port.x1,
        };
        let mut main_axis = 0.0;
        let mut items = Vec::new();

        let mut items_iter = items_iter.into_iter().peekable();
        if fixed_size {
            let mut item_size = 0.0;
            if let Some(item) = items_iter.peek() {
                item_size = size_fn(item);
            }
            let start = if item_size > 0.0 {
                (min / item_size).floor() as usize
            } else {
                0
            };
            let end = if item_size > 0.0 {
                (max / item_size).ceil() as usize
            } else {
                usize::MAX
            };
            items = items_iter
                .skip(start.saturating_sub(1))
                .take(end - start)
                .collect::<Vec<_>>();
        } else {
            for item in items_iter {
                let item_size = size_fn(&item);
                if main_axis < min {
                    main_axis += item_size;
                    continue;
                }

                items.push(item);

                if main_axis > max {
                    break;
                }
            }
        }

        let hashed_items = items.iter().map(&key_fn).collect::<FxIndexSet<_>>();
        let diff = if let Some(HashRun(prev_hash_run)) = prev_hash_run {
            let mut diff = diff(&prev_hash_run, &hashed_items);
            let mut items = items
                .into_iter()
                .map(|i| Some(i))
                .collect::<SmallVec<[Option<_>; 128]>>();
            for added in &mut diff.added {
                added.view = Some(view_fn(child_cx, items[added.at].take().unwrap()));
            }
            diff
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
        AppContext::update_state(id, VirtualListState { diff, view_port });
        HashRun(hashed_items)
    });

    VirtualList {
        id,
        direction,
        children: IndexMap::new(),
    }
}

impl<V: View + 'static> View for VirtualList<V> {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&mut self, id: Id) -> Option<&mut dyn View> {
        match self.children.get_mut(&id) {
            Some(view) => view.as_mut().map(|view| view as &mut dyn View),
            None => None,
        }
    }

    fn update(
        &mut self,
        cx: &mut crate::context::UpdateCx,
        state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        if let Ok(diff) = state.downcast() {
            apply_diff(*diff, &mut self.children);
            cx.request_layout(self.id());
            cx.reset_children_layout(self.id);
            ChangeFlags::LAYOUT
        } else {
            ChangeFlags::empty()
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

    fn event(
        &mut self,
        cx: &mut crate::context::EventCx,
        id_path: Option<&[Id]>,
        event: crate::event::Event,
    ) -> bool {
        for (_, child) in self.children.iter_mut() {
            if let Some(child) = child.as_mut() {
                let id = child.id();
                if cx.should_send(id, &event) {
                    let event = cx.offset_event(id, event.clone());
                    if child.event_main(cx, id_path, cx.offset_event(id, event)) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        for (_, child) in self.children.iter_mut() {
            if let Some(child) = child.as_mut() {
                child.paint_main(cx);
            }
        }
    }
}
