use std::{
    cell::RefCell,
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
    marker::PhantomData,
    ops::{Range, RangeInclusive},
    rc::Rc,
};

use floem_reactive::{
    Effect, ReadSignal, RwSignal, Scope, SignalGet, SignalTrack, SignalUpdate, SignalWith,
};
use peniko::kurbo::Rect;
use smallvec::SmallVec;
use taffy::{Dimension, FlexDirection, tree::NodeId};
use understory_virtual_list::{
    ExtentModel, FixedExtentModel, PrefixSumExtentModel, compute_visible_strip,
};

use crate::{
    event::listener::{EventListenerTrait, UpdatePhaseBoxTreeCommit},
    prop_extractor,
    style::{FlexDirectionProp, recalc::StyleReason},
    view::{FinalizeFn, IntoView, LayoutNodeCx, View, ViewId},
};

use super::{Diff, DiffOpAdd, FxIndexSet, HashRun, apply_diff, diff};

pub type VirtViewFn<T> = Box<dyn Fn(T) -> (Box<dyn View>, Scope)>;

prop_extractor! {
    pub VirtualExtractor {
        pub direction: FlexDirectionProp,
    }
}

enum VirtualItemSize<T> {
    Fixed(Rc<dyn Fn() -> f64>),
    Fn(Rc<dyn Fn(&T) -> f64>),
    /// Measures the first rendered item and uses that size for all items.
    Assume(Option<f64>),
}

impl<T> Clone for VirtualItemSize<T> {
    fn clone(&self) -> Self {
        match self {
            VirtualItemSize::Fixed(rc) => VirtualItemSize::Fixed(rc.clone()),
            VirtualItemSize::Fn(rc) => VirtualItemSize::Fn(rc.clone()),
            VirtualItemSize::Assume(x) => VirtualItemSize::Assume(*x),
        }
    }
}

/// Cached extent model — persisted across effect runs to avoid rebuilding.
enum CachedExtentModel {
    Fixed(FixedExtentModel<f64>),
    PrefixSum(PrefixSumExtentModel<f64>),
}

/// A trait that can be implemented on a type so that the type can be used in a [`virtual_stack`] or [`virtual_list`](super::virtual_list()).
pub trait VirtualVector<T> {
    fn total_len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.total_len() == 0
    }

    fn slice(&self, range: Range<usize>) -> impl Iterator<Item = T>;

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

/// Shared layout data for the taffy measure function.
#[derive(Clone)]
struct ContentSize {
    size: f64,
    direction: FlexDirection,
}

/// A virtual stack that is like a [`dyn_stack`](super::dyn_stack()) but also lazily loads items for performance. See [`virtual_stack`].
pub struct VirtualStack<T>
where
    T: 'static,
{
    id: ViewId,
    style: VirtualExtractor,
    pub(crate) direction: RwSignal<FlexDirection>,
    item_size: RwSignal<VirtualItemSize<T>>,
    children: Vec<Option<(ViewId, Scope)>>,
    /// Index of the first visible child in the full dataset.
    first_child_idx: usize,
    selected_idx: HashSet<usize>,
    view_fn: VirtViewFn<T>,
    before_size: f64,
    after_size: f64,
    content_size: Rc<RefCell<ContentSize>>,
    space_nodes: Option<(NodeId, NodeId)>,
    scroll_offset: RwSignal<f64>,
    viewport_size: RwSignal<f64>,
}

impl<T: Clone> VirtualStack<T> {
    pub fn new<DF, I>(data_fn: DF) -> VirtualStack<T>
    where
        DF: Fn() -> I + 'static,
        I: VirtualVector<T>,
        T: Hash + Eq + IntoView + 'static,
    {
        Self::full(
            data_fn,
            |item| {
                let mut hasher = DefaultHasher::new();
                item.hash(&mut hasher);
                hasher.finish()
            },
            |item| item.into_view(),
        )
    }

    pub fn with_view<DF, I, V>(data_fn: DF, view_fn: impl Fn(T) -> V + 'static) -> VirtualStack<T>
    where
        DF: Fn() -> I + 'static,
        I: VirtualVector<T>,
        T: Hash + Eq + 'static,
        V: IntoView,
    {
        Self::full(
            data_fn,
            |item| {
                let mut hasher = DefaultHasher::new();
                item.hash(&mut hasher);
                hasher.finish()
            },
            move |item| view_fn(item).into_view(),
        )
    }

    pub fn with_key<DF, I, K>(data_fn: DF, key_fn: impl Fn(&T) -> K + 'static) -> VirtualStack<T>
    where
        DF: Fn() -> I + 'static,
        I: VirtualVector<T>,
        T: IntoView + 'static,
        K: Hash + Eq + 'static,
    {
        Self::full(data_fn, key_fn, |item| item.into_view())
    }

    pub fn full<DF, I, KF, K, VF, V>(data_fn: DF, key_fn: KF, view_fn: VF) -> VirtualStack<T>
    where
        DF: Fn() -> I + 'static,
        I: VirtualVector<T>,
        KF: Fn(&T) -> K + 'static,
        K: Eq + Hash + 'static,
        VF: Fn(T) -> V + 'static,
        V: IntoView + 'static,
        T: 'static,
    {
        virtual_stack(data_fn, key_fn, view_fn)
    }
}

impl<T> VirtualStack<T> {
    pub fn item_size_fixed(self, size: impl Fn() -> f64 + 'static) -> Self {
        self.item_size.set(VirtualItemSize::Fixed(Rc::new(size)));
        self
    }

    pub fn item_size_fn(self, size: impl Fn(&T) -> f64 + 'static) -> Self {
        self.item_size.set(VirtualItemSize::Fn(Rc::new(size)));
        self
    }

    pub fn first_layout(self) -> Self {
        self.item_size.set(VirtualItemSize::Assume(None));
        self
    }

    fn ensure_space_nodes(&mut self) -> (NodeId, NodeId) {
        if self.space_nodes.is_none() {
            let before = self
                .id
                .taffy()
                .borrow_mut()
                .new_leaf(taffy::Style::DEFAULT)
                .unwrap();
            let after = self
                .id
                .taffy()
                .borrow_mut()
                .new_leaf(taffy::Style::DEFAULT)
                .unwrap();
            self.space_nodes = Some((before, after));
        }
        let (before_node, after_node) = self.space_nodes.unwrap();
        let direction = self.content_size.borrow().direction;
        let _ = self.id.taffy().borrow_mut().set_style(
            before_node,
            taffy::style::Style {
                size: match direction {
                    FlexDirection::Column | FlexDirection::ColumnReverse => taffy::prelude::Size {
                        width: Dimension::auto(),
                        height: Dimension::length(self.before_size as f32),
                    },
                    FlexDirection::Row | FlexDirection::RowReverse => taffy::prelude::Size {
                        width: Dimension::length(self.before_size as f32),
                        height: Dimension::auto(),
                    },
                },
                ..Default::default()
            },
        );
        let _ = self.id.taffy().borrow_mut().set_style(
            after_node,
            taffy::style::Style {
                size: match direction {
                    FlexDirection::Column | FlexDirection::ColumnReverse => taffy::prelude::Size {
                        width: Dimension::auto(),
                        height: Dimension::length(self.after_size as f32),
                    },
                    FlexDirection::Row | FlexDirection::RowReverse => taffy::prelude::Size {
                        width: Dimension::length(self.after_size as f32),
                        height: Dimension::auto(),
                    },
                },
                ..Default::default()
            },
        );
        (before_node, after_node)
    }
}

pub(crate) struct VirtualStackState<T> {
    diff: Diff<T>,
    first_idx: usize,
    before_size: f64,
    after_size: f64,
    content_size: f64,
}

/// A View that lazily loads items as they appear in a scroll view.
///
/// ## Example
/// ```
/// use floem::prelude::*;
///
/// VirtualStack::new(move || 1..=1000000)
///     .style(|s| {
///         s.flex_col().class(LabelClass, |s| {
///             s.padding_vert(2.5).width_full().justify_center()
///         })
///     })
///     .scroll()
///     .style(|s| s.size(200., 500.).border(1.0))
/// ```
pub fn virtual_stack<T, IF, I, KF, K, VF, V>(
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
    id.register_listener(UpdatePhaseBoxTreeCommit::listener_key());

    let item_size: RwSignal<VirtualItemSize<T>> = RwSignal::new(VirtualItemSize::Assume(None));
    let direction = RwSignal::new(FlexDirection::Column);
    let scroll_offset = RwSignal::new(0.0_f64);
    let viewport_size = RwSignal::new(0.0_f64);

    let content_size = Rc::new(RefCell::new(ContentSize {
        size: 0.0,
        direction: FlexDirection::Column,
    }));
    let content_size_for_measure = content_size.clone();

    // Set the taffy measure function so the node reports full content size
    // before children are attached (initial frame).
    let taffy_node = id.taffy_node();
    id.taffy()
        .borrow_mut()
        .set_node_context(
            taffy_node,
            Some(LayoutNodeCx::Custom {
                measure: Box::new(
                    move |known_dimensions, _available_space, _node_id, _style, _cx| {
                        let data = content_size_for_measure.borrow();
                        let main = data.size as f32;
                        match data.direction {
                            FlexDirection::Column | FlexDirection::ColumnReverse => taffy::Size {
                                width: known_dimensions.width.unwrap_or(0.0),
                                height: known_dimensions.height.unwrap_or(main),
                            },
                            FlexDirection::Row | FlexDirection::RowReverse => taffy::Size {
                                width: known_dimensions.width.unwrap_or(main),
                                height: known_dimensions.height.unwrap_or(0.0),
                            },
                        }
                    },
                ),
                finalize: None::<Box<FinalizeFn>>,
            }),
        )
        .unwrap();

    let cached_model: Rc<RefCell<Option<CachedExtentModel>>> = Rc::new(RefCell::new(None));

    Effect::new(move |prev: Option<(f64, f64, HashRun<FxIndexSet<K>>)>| {
        let items_vector = each_fn();
        let total_len = items_vector.total_len();
        let scroll = scroll_offset.get();
        let viewport = viewport_size.get();
        direction.track();

        let mut items = Vec::new();
        let mut before_size = 0.0_f64;
        let mut after_size = 0.0_f64;
        let mut content_sz = 0.0_f64;
        let mut start = 0usize;

        let mut cached = cached_model.borrow_mut();

        item_size.with(|s| match s {
            VirtualItemSize::Fixed(size_fn) => {
                let extent = size_fn();
                // Reuse or rebuild Fixed model.
                let model = match cached.as_mut() {
                    Some(CachedExtentModel::Fixed(m)) => {
                        m.set_len(total_len);
                        m.set_extent(extent);
                        m
                    }
                    _ => {
                        *cached = Some(CachedExtentModel::Fixed(FixedExtentModel::new(
                            total_len, extent,
                        )));
                        match cached.as_mut().unwrap() {
                            CachedExtentModel::Fixed(m) => m,
                            _ => unreachable!(),
                        }
                    }
                };
                let strip = compute_visible_strip(model, scroll, viewport, 0.0, 0.0);
                start = strip.start;
                before_size = strip.before_extent;
                after_size = strip.after_extent;
                content_sz = strip.content_extent;
                for item in items_vector.slice(strip.start..strip.end) {
                    items.push(item);
                }
            }
            VirtualItemSize::Fn(size_fn) => {
                // Reuse or rebuild PrefixSum model, only rebuilding when len changes.
                let model = match cached.as_mut() {
                    Some(CachedExtentModel::PrefixSum(m)) if m.len() == total_len => m,
                    _ => {
                        let mut m = PrefixSumExtentModel::<f64>::new();
                        let all: Vec<T> = items_vector.slice(0..total_len).collect();
                        m.rebuild(all, &|item: &T| size_fn(item));
                        *cached = Some(CachedExtentModel::PrefixSum(m));
                        match cached.as_mut().unwrap() {
                            CachedExtentModel::PrefixSum(m) => m,
                            _ => unreachable!(),
                        }
                    }
                };
                let strip = compute_visible_strip(model, scroll, viewport, 0.0, 0.0);
                start = strip.start;
                before_size = strip.before_extent;
                after_size = strip.after_extent;
                content_sz = strip.content_extent;
                for item in items_vector.slice(strip.start..strip.end) {
                    items.push(item);
                }
            }
            VirtualItemSize::Assume(assumed) => {
                let extent = assumed.unwrap_or(10.0);
                let model = match cached.as_mut() {
                    Some(CachedExtentModel::Fixed(m)) => {
                        m.set_len(total_len);
                        m.set_extent(extent);
                        m
                    }
                    _ => {
                        *cached = Some(CachedExtentModel::Fixed(FixedExtentModel::new(
                            total_len, extent,
                        )));
                        match cached.as_mut().unwrap() {
                            CachedExtentModel::Fixed(m) => m,
                            _ => unreachable!(),
                        }
                    }
                };
                let strip = compute_visible_strip(model, scroll, viewport, 0.0, 0.0);
                start = strip.start;
                before_size = strip.before_extent;
                after_size = strip.after_extent;
                content_sz = strip.content_extent;
                if assumed.is_none() {
                    if total_len > 0
                        && let Some(item) = items_vector.slice(0..1).next()
                    {
                        items.push(item);
                    }
                } else {
                    for item in items_vector.slice(strip.start..strip.end) {
                        items.push(item);
                    }
                }
            }
        });

        let hashed_items = items.iter().map(&key_fn).collect::<FxIndexSet<_>>();
        let (prev_before, prev_content, diff) =
            if let Some((prev_before, prev_content, HashRun(prev_hash))) = prev {
                let mut diff = diff(&prev_hash, &hashed_items);
                let mut items = items
                    .into_iter()
                    .map(Some)
                    .collect::<SmallVec<[Option<_>; 128]>>();
                for added in &mut diff.added {
                    added.view = Some(items[added.at].take().unwrap());
                }
                (prev_before, prev_content, diff)
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

        if !diff.is_empty() || prev_before != before_size || prev_content != content_sz {
            id.update_state(VirtualStackState {
                diff,
                first_idx: start,
                before_size,
                after_size,
                content_size: content_sz,
            });
        }
        (before_size, content_sz, HashRun(hashed_items))
    });

    let view_fn = Box::new(Scope::current().enter_child(move |e| view_fn(e).into_any()));

    VirtualStack {
        id,
        style: Default::default(),
        direction,
        item_size,
        children: Vec::new(),
        selected_idx: HashSet::with_capacity(1),
        first_child_idx: 0,
        view_fn,
        before_size: 0.0,
        after_size: 0.0,
        content_size,
        space_nodes: None,
        scroll_offset,
        viewport_size,
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
        match state.downcast::<VirtualStackState<T>>() {
            Ok(state) => {
                if self.before_size == state.before_size
                    && self.content_size.borrow().size == state.content_size
                    && state.diff.is_empty()
                {
                    return;
                }
                self.before_size = state.before_size;
                self.after_size = state.after_size;
                self.content_size.borrow_mut().size = state.content_size;
                self.first_child_idx = state.first_idx;
                apply_diff(
                    self.id(),
                    cx.window_state,
                    state.diff,
                    &mut self.children,
                    &self.view_fn,
                );
                let (before, after) = self.ensure_space_nodes();
                let taffy = self.id.taffy();
                let mut taffy = taffy.borrow_mut();
                let this_node = self.id.taffy_node();
                taffy.insert_child_at_index(this_node, 0, before).unwrap();
                taffy.add_child(this_node, after).unwrap();
                self.id.request_style(StyleReason::style_pass());
                self.id.request_layout();
            }
            Err(state) => {
                // check if we got a selection change
                if let Ok(idx) = state.downcast::<usize>() {
                    self.id.request_style(StyleReason::style_pass());
                    self.scroll_to_idx(*idx);
                    self.selected_idx.clear();
                    self.selected_idx.insert(*idx);
                }
            }
        }
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.style.read(cx) {
            cx.window_state.request_paint(self.id);
            let dir = self.style.direction();
            self.direction.set(dir);
            self.content_size.borrow_mut().direction = dir;
        }
        for (child_id_index, child) in self.id.children().into_iter().enumerate() {
            if self
                .selected_idx
                .contains(&(child_id_index + self.first_child_idx))
            {
                child.parent_set_selected();
            } else {
                child.parent_clear_selected();
            }
        }
    }

    fn event(&mut self, cx: &mut crate::context::EventCx) -> crate::event::EventPropagation {
        if UpdatePhaseBoxTreeCommit::extract(&cx.event).is_some() {
            // Read scroll offset and viewport size from parent scroll view.
            let translation = self.id.get_scroll_cx();
            let dir = self.direction.get_untracked();
            let new_scroll = match dir {
                FlexDirection::Row | FlexDirection::RowReverse => translation.x,
                FlexDirection::Column | FlexDirection::ColumnReverse => translation.y,
            }
            .max(0.0);

            let parent_rect = self
                .id
                .parent()
                .map(|id| id.get_content_rect_local())
                .unwrap_or_default();
            let new_viewport = match dir {
                FlexDirection::Row | FlexDirection::RowReverse => parent_rect.width(),
                FlexDirection::Column | FlexDirection::ColumnReverse => parent_rect.height(),
            };

            if new_scroll != self.scroll_offset.get_untracked() {
                self.scroll_offset.set(new_scroll);
            }
            if new_viewport != self.viewport_size.get_untracked() {
                self.viewport_size.set(new_viewport);
            }

            // For Assume(None): measure the first rendered child.
            let is_unassumed = self
                .item_size
                .with_untracked(|s| matches!(s, VirtualItemSize::Assume(None)));
            if is_unassumed && let Some(Some((first_child, _))) = self.children.first() {
                let dir = self.direction.get_untracked();
                let rect = first_child.get_layout_rect_local();
                let size = match dir {
                    FlexDirection::Row | FlexDirection::RowReverse => rect.width(),
                    FlexDirection::Column | FlexDirection::ColumnReverse => rect.height(),
                };
                if size > 0.0 {
                    self.item_size.set(VirtualItemSize::Assume(Some(size)));
                }
            }
        }
        crate::event::EventPropagation::Continue
    }
}

impl<T> VirtualStack<T> {
    pub fn scroll_to_idx(&self, index: usize) {
        let (offset, size) = self.calculate_offset(index);
        let rect = match self.direction.get_untracked() {
            FlexDirection::Column | FlexDirection::ColumnReverse => {
                Rect::from_origin_size((0.0, offset), (0.0, size))
            }
            FlexDirection::Row | FlexDirection::RowReverse => {
                Rect::from_origin_size((offset, 0.0), (size, 0.0))
            }
        };
        self.id.scroll_to(Some(rect));
    }

    fn calculate_offset(&self, index: usize) -> (f64, f64) {
        self.item_size.with(|size| match size {
            VirtualItemSize::Fixed(size_fn) => {
                let s = size_fn();
                (s * index as f64, s)
            }
            VirtualItemSize::Fn(_) => (0.0, 0.0),
            VirtualItemSize::Assume(Some(s)) => (s * index as f64, *s),
            VirtualItemSize::Assume(None) => (0.0, 0.0),
        })
    }
}

// --- VirtualVector impls ---

impl<T: Clone> VirtualVector<T> for imbl::Vector<T> {
    fn total_len(&self) -> usize {
        self.len()
    }

    fn slice(&self, range: Range<usize>) -> impl Iterator<Item = T> {
        imbl::Vector::slice(&mut self.clone(), range).into_iter()
    }
}

impl<T> VirtualVector<T> for Range<T>
where
    T: Copy + std::ops::Sub<Output = T> + std::ops::Add<Output = T> + PartialOrd + From<usize>,
    usize: From<T>,
    Range<T>: Iterator<Item = T>,
{
    fn total_len(&self) -> usize {
        (self.end - self.start).into()
    }

    fn slice(&self, range: Range<usize>) -> impl Iterator<Item = T> {
        let start = self.start + T::from(range.start);
        let end = self.start + T::from(range.end);
        start..end
    }
}

impl<T> VirtualVector<T> for RangeInclusive<T>
where
    T: Copy + std::ops::Sub<Output = T> + std::ops::Add<Output = T> + PartialOrd + From<usize>,
    usize: From<T>,
    Range<T>: Iterator<Item = T>,
{
    fn total_len(&self) -> usize {
        let diff = *self.end() - *self.start();
        Into::<usize>::into(diff) + 1
    }

    fn slice(&self, range: Range<usize>) -> impl Iterator<Item = T> {
        let start = *self.start() + T::from(range.start);
        let end = *self.start() + T::from(range.end);
        start..end
    }
}

impl<T> VirtualVector<T> for RwSignal<Vec<T>>
where
    T: Clone + 'static,
{
    fn total_len(&self) -> usize {
        self.with(|v| v.len())
    }

    #[allow(clippy::unnecessary_to_owned)]
    fn slice(&self, range: Range<usize>) -> impl Iterator<Item = T> {
        self.with(|v| v[range].to_vec().into_iter())
    }
}

impl<T> VirtualVector<T> for ReadSignal<Vec<T>>
where
    T: Clone + 'static,
{
    fn total_len(&self) -> usize {
        self.with(|v| v.len())
    }

    #[allow(clippy::unnecessary_to_owned)]
    fn slice(&self, range: Range<usize>) -> impl Iterator<Item = T> {
        self.with(|v| v[range].to_vec().into_iter())
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

    fn slice(&self, range: Range<usize>) -> impl Iterator<Item = (usize, T)> {
        let start = range.start;
        self.inner
            .slice(range)
            .enumerate()
            .map(move |(i, e)| (i + start, e))
    }
}
