use std::{hash::Hash, marker::PhantomData, ops::Range};

use floem_reactive::{
    as_child_of_current_scope, create_effect, create_signal, Scope, SignalGet, SignalUpdate,
    WriteSignal,
};
use peniko::kurbo::Rect;
use smallvec::SmallVec;
use taffy::{style::Dimension, tree::NodeId};

use crate::{
    context::ComputeLayoutCx,
    id::ViewId,
    style::Style,
    view::{self, IntoView, View},
};

use super::{apply_diff, diff, Diff, DiffOpAdd, FxIndexSet, HashRun};

type ViewFn<T> = Box<dyn Fn(T) -> (Box<dyn View>, Scope)>;

#[derive(Clone, Copy)]
pub enum VirtualDirection {
    Vertical,
    Horizontal,
}

pub enum VirtualItemSize<T> {
    Fn(Box<dyn Fn(&T) -> f64>),
    Fixed(Box<dyn Fn() -> f64>),
}

/// A trait that can be implemented on a type so that the type can be used in a [`virtual_stack`] or [`virtual_list`](super::virtual_list()).
pub trait VirtualVector<T> {
    fn total_len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.total_len() == 0
    }

    fn slice(&mut self, range: Range<usize>) -> impl Iterator<Item = T>;

    fn enumerate(self) -> Enumerate<Self, T>
    where
        Self: Sized,
    {
        Enumerate {
            inner: self,
            phantom: PhantomData,
        }
    }
}

/// A virtual stack that is like a [`dyn_stack`](super::dyn_stack()) but also lazily loads items for performance. See [`virtual_stack`].
pub struct VirtualStack<T>
where
    T: 'static,
{
    id: ViewId,
    direction: VirtualDirection,
    children: Vec<Option<(ViewId, Scope)>>,
    viewport: Rect,
    set_viewport: WriteSignal<Rect>,
    view_fn: ViewFn<T>,
    phatom: PhantomData<T>,
    before_size: f64,
    content_size: f64,
    before_node: Option<NodeId>,
}

struct VirtualStackState<T> {
    diff: Diff<T>,
    before_size: f64,
    content_size: f64,
}

/// A View that is like a [`dyn_stack`](super::dyn_stack()) but also lazily loads the items as they appear in a [scroll view](super::scroll())
///
/// This virtualization/lazy loading is done for performance and allows for lists of millions of items to be used with very high performance.
///
/// ## Example
/// ```
/// use floem::{reactive::*, views::*, unit::UnitExt};
///
/// let long_list: im::Vector<i32> = (0..1000000).collect();
/// let (long_list, _set_long_list) = create_signal(long_list);
///
/// container(
///     scroll(
///         virtual_list(
///             VirtualDirection::Vertical,
///             VirtualItemSize::Fixed(Box::new(|| 20.0)),
///             move || long_list.get(),
///             move |item| *item,
///             move |item| label(move || item.to_string()).style(|s| s.height(20.0)),
///         )
///         .style(|s| s.flex_col().width_full()),
///     )
///     .style(|s| s.width(100.0).height(100.pct()).border(1.0)),
/// )
/// .style(|s| {
///     s.size(100.pct(), 100.pct())
///         .padding_vert(20.0)
///         .flex_col()
///         .items_center()
/// });
/// ```
pub fn virtual_stack<T, IF, I, KF, K, VF, V>(
    direction: VirtualDirection,
    item_size: VirtualItemSize<T>,
    each_fn: IF,
    key_fn: KF,
    view_fn: VF,
) -> VirtualStack<T>
where
    T: 'static,
    IF: Fn() -> I + 'static,
    I: VirtualVector<T>,
    KF: Fn(&T) -> K + 'static,
    K: Eq + Hash + 'static,
    VF: Fn(T) -> V + 'static,
    V: IntoView + 'static,
{
    let id = ViewId::new();

    let (viewport, set_viewport) = create_signal(Rect::ZERO);

    create_effect(move |prev| {
        let mut items_vector = each_fn();
        let viewport = viewport.get();
        let min = match direction {
            VirtualDirection::Vertical => viewport.y0,
            VirtualDirection::Horizontal => viewport.x0,
        };
        let max = match direction {
            VirtualDirection::Vertical => viewport.height() + viewport.y0,
            VirtualDirection::Horizontal => viewport.width() + viewport.x0,
        };
        let mut items = Vec::new();

        let mut before_size = 0.0;
        let mut content_size = 0.0;
        match &item_size {
            VirtualItemSize::Fixed(item_size) => {
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

                content_size = item_size * total_len as f64;
            }
            VirtualItemSize::Fn(size_fn) => {
                let mut main_axis = 0.0;
                let total_len = items_vector.total_len();
                for item in items_vector.slice(0..total_len) {
                    let item_size = size_fn(&item);
                    content_size += item_size;
                    if main_axis + item_size < min {
                        main_axis += item_size;
                        before_size += item_size;
                        continue;
                    }

                    if main_axis <= max {
                        main_axis += item_size;
                        items.push(item);
                    }
                }
            }
        };

        let hashed_items = items.iter().map(&key_fn).collect::<FxIndexSet<_>>();
        let (prev_before_size, prev_content_size, diff) =
            if let Some((prev_before_size, prev_content_size, HashRun(prev_hash_run))) = prev {
                let mut diff = diff(&prev_hash_run, &hashed_items);
                let mut items = items
                    .into_iter()
                    .map(|i| Some(i))
                    .collect::<SmallVec<[Option<_>; 128]>>();
                for added in &mut diff.added {
                    added.view = Some(items[added.at].take().unwrap());
                }
                (prev_before_size, prev_content_size, diff)
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

        if !diff.is_empty() || prev_before_size != before_size || prev_content_size != content_size
        {
            id.update_state(VirtualStackState {
                diff,
                before_size,
                content_size,
            });
        }
        (before_size, content_size, HashRun(hashed_items))
    });

    let view_fn = Box::new(as_child_of_current_scope(move |e| view_fn(e).into_any()));

    VirtualStack {
        id,
        direction,
        children: Vec::new(),
        viewport: Rect::ZERO,
        set_viewport,
        view_fn,
        phatom: PhantomData,
        before_size: 0.0,
        content_size: 0.0,
        before_node: None,
    }
}

impl<T> View for VirtualStack<T> {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "VirtualStack".into()
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<VirtualStackState<T>>() {
            if self.before_size == state.before_size
                && self.content_size == state.content_size
                && state.diff.is_empty()
            {
                return;
            }
            self.before_size = state.before_size;
            self.content_size = state.content_size;
            apply_diff(
                self.id(),
                cx.app_state,
                state.diff,
                &mut self.children,
                &self.view_fn,
            );
            self.id.request_all();
        }
    }

    fn view_style(&self) -> Option<crate::style::Style> {
        let style = match self.direction {
            VirtualDirection::Vertical => Style::new().height(self.content_size),
            VirtualDirection::Horizontal => Style::new().width(self.content_size),
        };
        Some(style)
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::tree::NodeId {
        cx.layout_node(self.id(), true, |cx| {
            let mut content_nodes = self
                .id
                .children()
                .into_iter()
                .map(|id| id.view().borrow_mut().layout(cx))
                .collect::<Vec<_>>();

            if self.before_node.is_none() {
                self.before_node = Some(
                    self.id
                        .taffy()
                        .borrow_mut()
                        .new_leaf(taffy::style::Style::DEFAULT)
                        .unwrap(),
                );
            }
            let before_node = self.before_node.unwrap();
            let _ = self.id.taffy().borrow_mut().set_style(
                before_node,
                taffy::style::Style {
                    size: match self.direction {
                        VirtualDirection::Vertical => taffy::prelude::Size {
                            width: Dimension::Auto,
                            height: Dimension::Length(self.before_size as f32),
                        },
                        VirtualDirection::Horizontal => taffy::prelude::Size {
                            width: Dimension::Length(self.before_size as f32),
                            height: Dimension::Auto,
                        },
                    },
                    ..Default::default()
                },
            );
            let mut nodes = vec![before_node];
            nodes.append(&mut content_nodes);
            nodes
        })
    }

    fn compute_layout(&mut self, cx: &mut ComputeLayoutCx<'_>) -> Option<Rect> {
        let viewport = cx.current_viewport();
        if self.viewport != viewport {
            self.viewport = viewport;
            self.set_viewport.set(viewport);
        }

        view::default_compute_layout(self.id, cx)
    }
}

impl<T: Clone> VirtualVector<T> for im::Vector<T> {
    fn total_len(&self) -> usize {
        self.len()
    }

    fn slice(&mut self, range: Range<usize>) -> impl Iterator<Item = T> {
        self.slice(range).into_iter()
    }
}

pub struct Enumerate<V: VirtualVector<T>, T> {
    inner: V,
    phantom: PhantomData<T>,
}

impl<V: VirtualVector<T>, T> VirtualVector<(usize, T)> for Enumerate<V, T> {
    fn total_len(&self) -> usize {
        self.inner.total_len()
    }

    fn slice(&mut self, range: Range<usize>) -> impl Iterator<Item = (usize, T)> {
        let start = range.start;
        self.inner
            .slice(range)
            .enumerate()
            .map(move |(i, e)| (i + start, e))
    }
}
