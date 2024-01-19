use std::{hash::Hash, marker::PhantomData, ops::Range};

use floem_reactive::{as_child_of_current_scope, create_effect, create_signal, Scope, WriteSignal};
use kurbo::{Rect, Size};
use smallvec::SmallVec;
use taffy::{
    prelude::Node,
    style::{Dimension, FlexDirection, LengthPercentage},
};

use crate::{
    context::ComputeLayoutCx,
    id::Id,
    unit::PxPct,
    view::{self, View, ViewData},
};

use super::{apply_diff, diff, Diff, DiffOpAdd, FxIndexSet, HashRun};

#[derive(Clone, Copy)]
pub enum VirtualDirection {
    Vertical,
    Horizontal,
}

pub enum VirtualItemSize<T> {
    Fn(Box<dyn Fn(&T) -> f64>),
    Fixed(Box<dyn Fn() -> f64>),
}

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

pub struct VirtualStack<V: View, T>
where
    T: 'static,
{
    data: ViewData,
    direction: VirtualDirection,
    children: Vec<Option<(V, Scope)>>,
    viewport: Rect,
    set_viewport: WriteSignal<Rect>,
    view_fn: Box<dyn Fn(T) -> (V, Scope)>,
    phatom: PhantomData<T>,
    before_size: f64,
    content_size: f64,
    offset_node: Option<Node>,
    content_node: Option<Node>,
}

struct VirtualStackState<T> {
    diff: Diff<T>,
    before_size: f64,
    content_size: f64,
}

pub fn virtual_stack<T, IF, I, KF, K, VF, V>(
    direction: VirtualDirection,
    item_size: VirtualItemSize<T>,
    each_fn: IF,
    key_fn: KF,
    view_fn: VF,
) -> VirtualStack<V, T>
where
    T: 'static,
    IF: Fn() -> I + 'static,
    I: VirtualVector<T>,
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
        content_size: 0.0,
        offset_node: None,
        content_node: None,
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
            cx.request_all(self.id());
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id(), true, |cx| {
            let nodes = self
                .children
                .iter_mut()
                .filter_map(|child| Some(cx.layout_view(&mut child.as_mut()?.0)))
                .collect::<Vec<_>>();
            let content_size = match self.direction {
                VirtualDirection::Vertical => taffy::prelude::Size {
                    width: Dimension::Percent(1.0),
                    height: Dimension::Points(self.content_size as f32),
                },
                VirtualDirection::Horizontal => taffy::prelude::Size {
                    width: Dimension::Points(self.content_size as f32),
                    height: Dimension::Percent(1.0),
                },
            };
            if self.offset_node.is_none() {
                self.offset_node = Some(
                    cx.app_state_mut()
                        .taffy
                        .new_leaf(taffy::style::Style::DEFAULT)
                        .unwrap(),
                );
            }
            if self.content_node.is_none() {
                self.content_node = Some(
                    cx.app_state_mut()
                        .taffy
                        .new_leaf(taffy::style::Style::DEFAULT)
                        .unwrap(),
                );
            }
            let offset_node = self.offset_node.unwrap();
            let content_node = self.content_node.unwrap();
            let _ = cx.app_state_mut().taffy.set_style(
                offset_node,
                taffy::style::Style {
                    position: taffy::style::Position::Absolute,
                    padding: match self.direction {
                        VirtualDirection::Vertical => taffy::prelude::Rect {
                            left: LengthPercentage::Points(0.0),
                            top: LengthPercentage::Points(self.before_size as f32),
                            right: LengthPercentage::Points(0.0),
                            bottom: LengthPercentage::Points(0.0),
                        },
                        VirtualDirection::Horizontal => taffy::prelude::Rect {
                            left: LengthPercentage::Points(self.before_size as f32),
                            top: LengthPercentage::Points(0.0),
                            right: LengthPercentage::Points(0.0),
                            bottom: LengthPercentage::Points(0.0),
                        },
                    },
                    flex_direction: match self.direction {
                        VirtualDirection::Vertical => FlexDirection::Column,
                        VirtualDirection::Horizontal => FlexDirection::Row,
                    },
                    size: taffy::prelude::Size {
                        width: Dimension::Percent(1.0),
                        height: Dimension::Percent(1.0),
                    },
                    ..Default::default()
                },
            );
            let _ = cx.app_state_mut().taffy.set_style(
                content_node,
                taffy::style::Style {
                    min_size: content_size,
                    size: content_size,
                    ..Default::default()
                },
            );
            let _ = cx.app_state_mut().taffy.set_children(offset_node, &nodes);
            let _ = cx
                .app_state_mut()
                .taffy
                .set_children(content_node, &[offset_node]);
            vec![content_node]
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

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.save();
        let layout = cx.get_layout(self.id()).unwrap();
        let style = cx.get_builtin_style(self.id());
        let padding_left = match style.padding_left() {
            PxPct::Px(padding) => padding,
            PxPct::Pct(pct) => pct * layout.size.width as f64,
        };
        let padding_top = match style.padding_top() {
            PxPct::Px(padding) => padding,
            PxPct::Pct(pct) => pct * layout.size.width as f64,
        };
        cx.offset((padding_left, padding_top));
        self.for_each_child_mut(&mut |child| {
            cx.paint_view(child);
            false
        });

        cx.restore();
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
