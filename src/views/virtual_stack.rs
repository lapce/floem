use std::{hash::Hash, marker::PhantomData, ops::Range};

use floem_reactive::{as_child_of_current_scope, create_effect, create_signal, Scope, WriteSignal};
use kurbo::{Rect, Size};
use smallvec::SmallVec;
use taffy::{prelude::Node, style::Dimension};

use crate::{
    context::ComputeLayoutCx,
    id::Id,
    view::{self, View, ViewData},
};

use super::{apply_diff, diff, Diff, DiffOpAdd, FxIndexSet, HashRun};

#[derive(Clone, Copy)]
pub enum VirtualStackDirection {
    Vertical,
    Horizontal,
}

pub enum VirtualStackItemSize<T> {
    Fn(Box<dyn Fn(&T) -> f64>),
    Fixed(Box<dyn Fn() -> f64>),
}

pub trait VirtualStackVector<T> {
    type ItemIterator: Iterator<Item = T>;

    fn total_len(&self) -> usize;

    fn total_size(&self) -> Option<f64> {
        None
    }

    fn is_empty(&self) -> bool {
        self.total_len() == 0
    }

    fn slice(&mut self, range: Range<usize>) -> Self::ItemIterator;
}

pub struct VirtualStack<V: View, T>
where
    T: 'static,
{
    data: ViewData,
    direction: VirtualStackDirection,
    children: Vec<Option<(V, Scope)>>,
    viewport: Rect,
    set_viewport: WriteSignal<Rect>,
    view_fn: Box<dyn Fn(T) -> (V, Scope)>,
    phatom: PhantomData<T>,
    before_size: f64,
    after_size: f64,
    before_node: Option<Node>,
    after_node: Option<Node>,
}

struct VirtualStackState<T> {
    diff: Diff<T>,
    before_size: f64,
    after_size: f64,
}

pub fn virtual_stack<T, IF, I, KF, K, VF, V>(
    direction: VirtualStackDirection,
    item_size: VirtualStackItemSize<T>,
    each_fn: IF,
    key_fn: KF,
    view_fn: VF,
) -> VirtualStack<V, T>
where
    T: 'static,
    IF: Fn() -> I + 'static,
    I: VirtualStackVector<T>,
    KF: Fn(&T) -> K + 'static,
    K: Eq + Hash + 'static,
    VF: Fn(T) -> V + 'static,
    V: View + 'static,
{
    let id = Id::next();

    let (viewport, set_viewport) = create_signal(Rect::ZERO);

    create_effect(move |prev| {
        let mut items_vector = each_fn();
        let viewport = viewport.get();
        let min = match direction {
            VirtualStackDirection::Vertical => viewport.y0,
            VirtualStackDirection::Horizontal => viewport.x0,
        };
        let max = match direction {
            VirtualStackDirection::Vertical => viewport.height() + viewport.y0,
            VirtualStackDirection::Horizontal => viewport.width() + viewport.x0,
        };
        let mut items = Vec::new();

        let mut before_size = 0.0;
        let mut after_size = 0.0;
        match &item_size {
            VirtualStackItemSize::Fixed(item_size) => {
                let item_size = item_size();
                let total_len = items_vector.total_len();
                let start = if item_size > 0.0 {
                    (min / item_size).floor() as usize
                } else {
                    0
                };
                let end = if item_size > 0.0 {
                    ((max / item_size).ceil() as usize).min(total_len)
                } else {
                    usize::MAX
                };
                before_size = item_size * (start.min(total_len)) as f64;

                for item in items_vector.slice(start..end) {
                    items.push(item);
                }

                after_size = item_size
                    * (total_len.saturating_sub(start).saturating_sub(items.len())) as f64;
            }
            VirtualStackItemSize::Fn(size_fn) => {
                let mut main_axis = 0.0;
                let total_len = items_vector.total_len();
                let total_size = items_vector.total_size();
                for item in items_vector.slice(0..total_len) {
                    let item_size = size_fn(&item);
                    if main_axis + item_size < min {
                        main_axis += item_size;
                        before_size += item_size;
                        continue;
                    }

                    if main_axis <= max {
                        main_axis += item_size;
                        items.push(item);
                    } else {
                        if let Some(total_size) = total_size {
                            after_size = (total_size - main_axis).max(0.0);
                            break;
                        }
                        after_size += item_size;
                    }
                }
            }
        };

        let hashed_items = items.iter().map(&key_fn).collect::<FxIndexSet<_>>();
        let (prev_before_size, prev_after_size, diff) =
            if let Some((prev_before_size, prev_after_size, HashRun(prev_hash_run))) = prev {
                let mut diff = diff(&prev_hash_run, &hashed_items);
                let mut items = items
                    .into_iter()
                    .map(|i| Some(i))
                    .collect::<SmallVec<[Option<_>; 128]>>();
                for added in &mut diff.added {
                    added.view = Some(items[added.at].take().unwrap());
                }
                (prev_before_size, prev_after_size, diff)
            } else {
                let mut diff = Diff::default();
                for (i, item) in items.into_iter().enumerate() {
                    diff.added.push(DiffOpAdd {
                        at: i,
                        view: Some(item),
                    });
                }
                (0.0, 0.0, diff)
            };

        if !diff.is_empty() || prev_before_size != before_size || prev_after_size != after_size {
            id.update_state(
                VirtualStackState {
                    diff,
                    before_size,
                    after_size,
                },
                false,
            );
        }
        (before_size, after_size, HashRun(hashed_items))
    });

    let view_fn = Box::new(as_child_of_current_scope(view_fn));

    VirtualStack {
        data: ViewData::new(id),
        direction,
        children: Vec::new(),
        viewport: Rect::ZERO,
        set_viewport,
        view_fn,
        phatom: PhantomData,
        before_size: 0.0,
        after_size: 0.0,
        before_node: None,
        after_node: None,
    }
}

impl<V: View + 'static, T> View for VirtualStack<V, T> {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn View) -> bool) {
        for child in self.children.iter().filter_map(|child| child.as_ref()) {
            if for_each(&child.0) {
                break;
            }
        }
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn View) -> bool) {
        for child in self.children.iter_mut().filter_map(|child| child.as_mut()) {
            if for_each(&mut child.0) {
                break;
            }
        }
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn View) -> bool,
    ) {
        for child in self
            .children
            .iter_mut()
            .rev()
            .filter_map(|child| child.as_mut())
        {
            if for_each(&mut child.0) {
                break;
            }
        }
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "VirtualStack".into()
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<VirtualStackState<T>>() {
            if self.before_size == state.before_size
                && self.after_size == state.after_size
                && state.diff.is_empty()
            {
                return;
            }
            self.before_size = state.before_size;
            self.after_size = state.after_size;
            apply_diff(
                self.id(),
                cx.app_state,
                state.diff,
                &mut self.children,
                &self.view_fn,
            );
            cx.request_all(self.id());
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id(), true, |cx| {
            let mut nodes = self
                .children
                .iter_mut()
                .filter_map(|child| Some(cx.layout_view(&mut child.as_mut()?.0)))
                .collect::<Vec<_>>();
            let before_size = match self.direction {
                VirtualStackDirection::Vertical => taffy::prelude::Size {
                    width: Dimension::Percent(1.0),
                    height: Dimension::Points(self.before_size as f32),
                },
                VirtualStackDirection::Horizontal => taffy::prelude::Size {
                    width: Dimension::Points(self.before_size as f32),
                    height: Dimension::Percent(1.0),
                },
            };
            let after_size = match self.direction {
                VirtualStackDirection::Vertical => taffy::prelude::Size {
                    width: Dimension::Percent(1.0),
                    height: Dimension::Points(self.after_size as f32),
                },
                VirtualStackDirection::Horizontal => taffy::prelude::Size {
                    width: Dimension::Points(self.after_size as f32),
                    height: Dimension::Percent(1.0),
                },
            };
            if self.before_node.is_none() {
                self.before_node = Some(
                    cx.app_state_mut()
                        .taffy
                        .new_leaf(taffy::style::Style::DEFAULT)
                        .unwrap(),
                );
            }
            if self.after_node.is_none() {
                self.after_node = Some(
                    cx.app_state_mut()
                        .taffy
                        .new_leaf(taffy::style::Style::DEFAULT)
                        .unwrap(),
                );
            }
            let before_node = self.before_node.unwrap();
            let after_node = self.after_node.unwrap();
            let _ = cx.app_state_mut().taffy.set_style(
                before_node,
                taffy::style::Style {
                    min_size: before_size,
                    size: before_size,
                    ..Default::default()
                },
            );
            let _ = cx.app_state_mut().taffy.set_style(
                after_node,
                taffy::style::Style {
                    min_size: after_size,
                    size: after_size,
                    ..Default::default()
                },
            );
            nodes.insert(0, before_node);
            nodes.push(after_node);
            nodes
        })
    }

    fn compute_layout(&mut self, cx: &mut ComputeLayoutCx<'_>) -> Option<Rect> {
        let viewport = cx.current_viewport();
        if self.viewport != viewport {
            let layout = cx.app_state().get_layout(self.id()).unwrap();
            let _size = Size::new(layout.size.width as f64, layout.size.height as f64);

            self.viewport = viewport;
            self.set_viewport.set(viewport);
        }

        view::default_compute_layout(self, cx)
    }
}

impl<T: Clone> VirtualStackVector<T> for im::Vector<T> {
    type ItemIterator = im::vector::ConsumingIter<T>;

    fn total_len(&self) -> usize {
        self.len()
    }

    fn slice(&mut self, range: Range<usize>) -> Self::ItemIterator {
        self.slice(range).into_iter()
    }
}
