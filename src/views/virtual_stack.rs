use std::{
    any::Any,
    cell::RefCell,
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
    marker::PhantomData,
    ops::{Range, RangeInclusive},
    rc::Rc,
};

use crate::{
    context::LayoutCx,
    id::ViewId,
    prop_extractor,
    style::FlexDirectionProp,
    view::{IntoView, View},
    view_storage::{MeasureFunction, NodeContext},
};
use floem_reactive::{Effect, ReadSignal, RwSignal, Scope, SignalGet, SignalUpdate, SignalWith};
use peniko::kurbo::Rect;
use smallvec::SmallVec;
use taffy::{Dimension, FlexDirection, tree::NodeId};

use super::{Diff, DiffOpAdd, FxIndexSet, HashRun, apply_diff, diff};

pub type VirtViewFn<T> = Box<dyn Fn(T) -> (Box<dyn View>, Scope)>;

prop_extractor! {
    pub VirtualExtractor {
        pub direction: FlexDirectionProp,
    }
}

enum VirtualItemSize<T> {
    Fixed(Box<dyn Fn() -> f64>),
    Fn {
        size_fn: Box<dyn Fn(&T) -> f64>,
        cache: RefCell<LayoutSizeCache>,
    },
    FirstLayout(RefCell<Option<f64>>),
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

/// Shared layout data for virtual stack that can be accessed by both
/// the taffy measure function and the view itself.
#[derive(Clone)]
pub struct VirtualStackLayoutData {
    /// The total content size in the main axis (includes all virtual items)
    pub content_size: f64,
    /// Current flex direction
    pub direction: FlexDirection,
}

impl VirtualStackLayoutData {
    pub fn new() -> Self {
        Self {
            content_size: 0.0,
            direction: FlexDirection::Column,
        }
    }

    /// Create the taffy measure function for this virtual stack.
    /// Before children are set, this tells taffy our desired main axis size.
    /// Once children exist, taffy will use their layout instead.
    pub fn create_taffy_layout_fn(layout_data: Rc<RefCell<Self>>) -> Box<MeasureFunction> {
        Box::new(
            move |known_dimensions, _available_space, _node_id, _style, _measure_ctx| {
                use taffy::*;

                let data = layout_data.borrow();

                let is_vertical = matches!(
                    data.direction,
                    FlexDirection::Column | FlexDirection::ColumnReverse
                );

                let main_axis_size = data.content_size as f32;

                if is_vertical {
                    Size {
                        width: known_dimensions.width.unwrap_or(0.0),
                        height: known_dimensions.height.unwrap_or(main_axis_size),
                    }
                } else {
                    Size {
                        width: known_dimensions.width.unwrap_or(main_axis_size),
                        height: known_dimensions.height.unwrap_or(0.0),
                    }
                }
            },
        )
    }
}

impl Default for VirtualStackLayoutData {
    fn default() -> Self {
        Self::new()
    }
}

/// A virtual stack that is like a [`dyn_stack`](super::dyn_stack()) but also lazily loads items for performance. See [`virtual_stack`].
pub struct VirtualStack<T>
where
    T: 'static,
{
    id: ViewId,
    style: VirtualExtractor,
    item_size: RwSignal<VirtualItemSize<T>>,
    pub(crate) layout_data: Rc<RefCell<VirtualStackLayoutData>>,
    children: Vec<Option<(ViewId, Scope)>>,
    /// the index out of all of the items that is the first in the virtualized set. This is used to map an index to a [`ViewId`].
    first_child_idx: usize,
    selected_idx: HashSet<usize>,
    view_fn: VirtViewFn<T>,
    before_size: f64,
    after_size: f64,
    space_nodes: Option<(NodeId, NodeId)>,
    scroll_offset: RwSignal<f64>,
}
impl<T: std::clone::Clone> VirtualStack<T> {
    // For types that implement all constraints
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

    // For types that are hashable but need custom view
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

    // For types that implement IntoView but need custom keys
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
        self.item_size.set(VirtualItemSize::Fixed(Box::new(size)));
        self
    }

    pub fn item_size_fn(self, size_fn: impl Fn(&T) -> f64 + 'static) -> Self {
        self.item_size.set(VirtualItemSize::Fn {
            size_fn: Box::new(size_fn),
            cache: RefCell::new(LayoutSizeCache::new()),
        });
        self
    }

    pub fn first_layout(self) -> Self {
        self.item_size
            .set(VirtualItemSize::FirstLayout(RefCell::new(None)));
        self
    }

    fn ensure_space_nodes(&mut self) -> (NodeId, NodeId) {
        let (before_node, after_node) = match self.space_nodes {
            Some(nodes) => nodes,
            None => {
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
                (before, after)
            }
        };
        let layout_data = self.layout_data.borrow();
        let _ = self.id.taffy().borrow_mut().set_style(
            before_node,
            taffy::style::Style {
                size: match layout_data.direction {
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
                size: match layout_data.direction {
                    FlexDirection::Column | FlexDirection::ColumnReverse => taffy::prelude::Size {
                        width: Dimension::auto(),
                        height: Dimension::length(self.after_size as f32),
                    },
                    FlexDirection::Row | FlexDirection::RowReverse => taffy::prelude::Size {
                        width: Dimension::length((self.after_size) as f32),
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

/// A View that is like a [`dyn_stack`](super::dyn_stack()) but also lazily loads the items as they appear in a [scroll view](super::scroll())
///
/// This virtualization/lazy loading is done for performance and allows for lists of millions of items to be used with very high performance.
///
/// By default, this view tries to calculate and assume the size of the items in the list by calculating the size of the first item that is loaded.
/// If all of your items are not of a consistent size in the relevant axis (ie a consistent width when flex_row or a consistent height when in flex_col) you will need to specify the size of the items using [`item_size_fixed`](VirtualStack::item_size_fixed) or [`item_size_fn`](VirtualStack::item_size_fn).
///
/// ## Example
/// ```
/// use floem::prelude::*;
///
/// VirtualStack::new(move || 1..=1000000)
///     .style(|s| {
///         s.flex_col().class(LabelClass, |s| {
///             // give each of the numbers some vertical padding and make them take up the full width of the stack
///             s.padding_vert(2.5).width_full().justify_center()
///         })
///     })
///     .scroll()
///     .style(|s| s.size(200., 500.).border(1.0))
///     .container()
///     .style(|s| {
///         s.size_full()
///             .items_center()
///             .justify_center()
/// });
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
    id.needs_post_layout();

    let item_size = RwSignal::new(VirtualItemSize::FirstLayout(RefCell::new(None)));

    let scroll_offset = RwSignal::new(1.);

    let layout_data = Rc::new(RefCell::new(VirtualStackLayoutData::default()));
    let layout_data_ = layout_data.clone();

    Effect::new(move |prev| {
        let mut items_vector = each_fn();
        let viewport = id.world_bounds().unwrap_or_default();
        let world_transform = id.world_transform().unwrap_or_default();
        let viewport = world_transform.inverse().transform_rect_bbox(viewport);
        let scroll_offset = scroll_offset.get();
        let direction = layout_data_.borrow().direction;
        let min = scroll_offset;
        let max = match direction {
            FlexDirection::Column | FlexDirection::ColumnReverse => {
                viewport.height() + scroll_offset
            }
            FlexDirection::Row | FlexDirection::RowReverse => viewport.width() + scroll_offset,
        };
        let prev_scroll = prev.as_ref().map(|(_, _, _, s)| *s);
        let scroll_changed = prev_scroll != Some(scroll_offset);

        let mut items = Vec::new();

        let mut before_size = 0.;
        let mut after_size = 0.;
        let mut content_size = 0.;
        let mut start = 0;
        item_size.with_untracked(|s| match s {
            VirtualItemSize::Fixed(item_size) => {
                let item_size = item_size();
                let total_len = items_vector.total_len();
                start = if item_size > 0.0 {
                    (min / item_size).floor() as usize
                } else {
                    0
                };
                let end = if item_size > 0.0 {
                    ((max / item_size).ceil() as usize).min(total_len)
                } else {
                    // TODO: Log an error
                    (start + 1).min(total_len)
                };
                before_size = item_size * (start.min(total_len)) as f64;

                for item in items_vector.slice(start..end) {
                    items.push(item);
                }

                content_size = item_size * total_len as f64;

                let after_count = total_len.saturating_sub(end);
                after_size = item_size * after_count as f64;
            }
            VirtualItemSize::Fn { size_fn, cache } => {
                let total_len = items_vector.total_len();

                let mut cache = cache.borrow_mut();

                // Rebuild sizes from current data
                if !scroll_changed || cache.cached_len != total_len {
                    cache.rebuild(items_vector.slice(0..total_len), size_fn.as_ref());
                }

                start = cache.find_index_at_position(min, total_len);
                before_size = cache.start_at(start);

                let mut idx = start;
                while idx < total_len && cache.start_at(idx) < max {
                    idx += 1;
                }
                let end = idx;

                for item in items_vector.slice(start..end) {
                    items.push(item);
                }

                content_size = cache.total_size(total_len);
                let end_start = if end < total_len {
                    cache.start_at(end)
                } else {
                    content_size
                };
                after_size = content_size - end_start;
            }
            VirtualItemSize::FirstLayout(size) => {
                let item_size = size.borrow().unwrap_or(10.);
                let total_len = items_vector.total_len();
                start = if item_size > 0.0 {
                    (min / item_size).floor() as usize
                } else {
                    0
                };
                let end = if item_size > 0.0 {
                    ((max / item_size).ceil() as usize).min(total_len)
                } else {
                    // TODO: Log an error
                    (start + 1).min(total_len)
                };
                before_size = item_size * (start.min(total_len)) as f64;

                for item in items_vector.slice(start..end) {
                    items.push(item);
                }

                content_size = item_size * total_len as f64;

                let after_count = total_len.saturating_sub(end);
                after_size = item_size * after_count as f64;
            }
        });

        let hashed_items = items.iter().map(&key_fn).collect::<FxIndexSet<_>>();
        let (prev_before_size, prev_content_size, diff) =
            if let Some((prev_before_size, prev_content_size, HashRun(prev_hash_run), _)) = prev {
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
                first_idx: start,
                before_size,
                after_size,
                content_size,
            });
        }
        (
            before_size,
            content_size,
            HashRun(hashed_items),
            scroll_offset,
        )
    });

    let view_fn = Box::new(Scope::current().enter_child(move |e| view_fn(e).into_any()));

    let taffy_id = id.taffy_node();

    id.taffy()
        .borrow_mut()
        .set_node_context(
            taffy_id,
            Some(NodeContext::Custom {
                measure: Box::new(VirtualStackLayoutData::create_taffy_layout_fn(
                    layout_data.clone(),
                )),
                finalize: None,
            }),
        )
        .unwrap();

    VirtualStack {
        id,
        style: Default::default(),
        item_size,
        layout_data,
        children: Vec::new(),
        selected_idx: HashSet::with_capacity(1),
        first_child_idx: 0,
        view_fn,
        before_size: 0.0,
        after_size: 0.0,
        scroll_offset,
        space_nodes: None,
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
        if state.is::<VirtualStackState<T>>() {
            if let Ok(state) = state.downcast::<VirtualStackState<T>>() {
                if self.before_size == state.before_size
                    && self.layout_data.borrow().content_size == state.content_size
                    && state.diff.is_empty()
                {
                    return;
                }
                self.before_size = state.before_size;
                self.after_size = state.after_size;
                self.layout_data.borrow_mut().content_size = state.content_size;
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
                let this_taffy = self.id.taffy_node();
                taffy.insert_child_at_index(this_taffy, 0, before).unwrap();
                taffy.add_child(this_taffy, after).unwrap();
                self.id.request_all();
            }
        } else if state.is::<usize>() {
            if let Ok(idx) = state.downcast::<usize>() {
                self.id.request_style_recursive();
                self.scroll_to_idx(*idx);
                self.selected_idx.clear();
                self.selected_idx.insert(*idx);
            }
        }
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.style.read(cx) {
            cx.window_state.request_paint(self.id);
            self.layout_data.borrow_mut().direction = self.style.direction();
        }
        for (child_id_index, child) in self.id.children().into_iter().enumerate() {
            if self
                .selected_idx
                .contains(&(child_id_index + self.first_child_idx))
            {
                cx.save();
                cx.selected();
                cx.style_view(child);
                cx.restore();
            } else {
                cx.style_view(child);
            }
        }
    }

    fn post_layout(&mut self, _lcx: &mut crate::context::LayoutCx) {
        let scroll_ctx = self.id.state().borrow().scroll_ctx.clone();
        let direction = self.layout_data.borrow().direction;
        let new_offset = match direction {
            FlexDirection::Row | FlexDirection::RowReverse => scroll_ctx.offset.x,
            FlexDirection::Column | FlexDirection::ColumnReverse => scroll_ctx.offset.y,
        };
        self.item_size.with_untracked(|i| {
            if let VirtualItemSize::FirstLayout(size) = i {
                let size_opt = *size.borrow();
                if size_opt.is_none() {
                    if let Some(Some((first, _))) = self.children.first() {
                        let lcx = &mut LayoutCx::new(*first);
                        let layout_is_some = lcx.layout().is_some();
                        if layout_is_some {
                            let first_size = lcx.layout_rect_local().size();
                            let first_size = match direction {
                                FlexDirection::Row | FlexDirection::RowReverse => first_size.width,
                                FlexDirection::Column | FlexDirection::ColumnReverse => {
                                    first_size.height
                                }
                            };
                            *size.borrow_mut() = Some(first_size);
                            self.id.request_layout();
                        }
                    }
                }
            }
        });
        if new_offset != self.scroll_offset.get_untracked() {
            self.scroll_offset.set(new_offset);
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        cx.paint_children(self.id());
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl<T> VirtualStack<T> {
    /// Scrolls to bring the item at the given index into view
    pub fn scroll_to_idx(&self, index: usize) {
        let (offset, size) = self.calculate_offset(index);

        // Create a rectangle at the calculated offset
        let rect = match self.layout_data.borrow().direction {
            FlexDirection::Column | FlexDirection::ColumnReverse => {
                Rect::from_origin_size((0.0, offset), (0.0, size))
            }
            FlexDirection::Row | FlexDirection::RowReverse => {
                Rect::from_origin_size((offset, 0.0), (size, 0.0))
            }
        };

        self.id.scroll_to(Some(rect));
    }

    /// Calculates the offset position for an item at the given index
    fn calculate_offset(&self, index: usize) -> (f64, f64) {
        let mut result = (0.0, 0.0);
        self.item_size.with(|size| {
            result = match size {
                VirtualItemSize::Fixed(item_size) => {
                    let size = item_size();
                    (size * index as f64, size)
                }
                VirtualItemSize::Fn { cache, .. } => {
                    let mut cache = cache.borrow_mut();
                    let offset = cache.start_at(index);
                    let size = cache.size_at(index);
                    (offset, size)
                }
                VirtualItemSize::FirstLayout(first_size) => {
                    let size = first_size.borrow().unwrap_or(10.);
                    (size * index as f64, size)
                }
            };
        });
        result
    }
}

impl<T: Clone> VirtualVector<T> for imbl::Vector<T> {
    fn total_len(&self) -> usize {
        self.len()
    }

    fn slice(&mut self, range: Range<usize>) -> impl Iterator<Item = T> {
        self.slice(range).into_iter()
    }
}

impl<T> VirtualVector<T> for Range<T>
where
    T: Copy + std::ops::Sub<Output = T> + std::ops::Add<Output = T> + PartialOrd + From<usize>,
    usize: From<T>,
    std::ops::Range<T>: Iterator<Item = T>,
{
    fn total_len(&self) -> usize {
        // Convert the difference between end and start to usize
        (self.end - self.start).into()
    }

    fn slice(&mut self, range: Range<usize>) -> impl Iterator<Item = T> {
        let start = self.start + T::from(range.start);
        let end = self.start + T::from(range.end);

        // Create a new range for the slice
        start..end
    }
}
impl<T> VirtualVector<T> for RangeInclusive<T>
where
    T: Copy + std::ops::Sub<Output = T> + std::ops::Add<Output = T> + PartialOrd + From<usize>,
    usize: From<T>,
    std::ops::Range<T>: Iterator<Item = T>,
{
    fn total_len(&self) -> usize {
        // For inclusive range, we need to add 1 to include the end value
        let diff = *self.end() - *self.start();
        Into::<usize>::into(diff) + 1
    }

    fn slice(&mut self, range: Range<usize>) -> impl Iterator<Item = T> {
        let start = *self.start() + T::from(range.start);
        let end = *self.start() + T::from(range.end);
        // Create a new range for the slice (non-inclusive since that's what the Range parameter specifies)
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

    // false positive on the clippy
    #[allow(clippy::unnecessary_to_owned)]
    fn slice(&mut self, range: Range<usize>) -> impl Iterator<Item = T> {
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

    // false positive on the clippy
    #[allow(clippy::unnecessary_to_owned)]
    fn slice(&mut self, range: Range<usize>) -> impl Iterator<Item = T> {
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

    fn slice(&mut self, range: Range<usize>) -> impl Iterator<Item = (usize, T)> {
        let start = range.start;
        self.inner
            .slice(range)
            .enumerate()
            .map(move |(i, e)| (i + start, e))
    }
}

#[derive(Clone)]
struct LayoutSizeCache {
    sizes: Vec<f64>,
    cumulative_starts: Vec<f64>,
    dirty_from: Option<usize>,
    cached_len: usize,
}

impl LayoutSizeCache {
    pub fn new() -> Self {
        Self {
            sizes: Vec::new(),
            cumulative_starts: Vec::new(),
            cached_len: 0,
            dirty_from: Some(0),
        }
    }

    fn rebuild<T>(&mut self, items: impl Iterator<Item = T>, size_fn: &dyn Fn(&T) -> f64) {
        self.sizes.clear();
        self.sizes.extend(items.map(|item| size_fn(&item)));
        self.cached_len = self.sizes.len();
        self.dirty_from = Some(0);
    }

    fn ensure_cumulative_through(&mut self, through: usize) {
        if through >= self.sizes.len() {
            return;
        }

        let dirty_from = match self.dirty_from {
            Some(d) if d <= through => d,
            _ => return,
        };

        if self.cumulative_starts.len() < self.sizes.len() {
            self.cumulative_starts.resize(self.sizes.len(), 0.0);
        }

        let mut pos = if dirty_from == 0 {
            0.0
        } else {
            self.cumulative_starts[dirty_from - 1] + self.sizes[dirty_from - 1]
        };

        for i in dirty_from..=through {
            self.cumulative_starts[i] = pos;
            pos += self.sizes[i];
        }

        if through >= self.sizes.len().saturating_sub(1) {
            self.dirty_from = None;
        } else {
            self.dirty_from = Some(through + 1);
        }
    }

    fn start_at(&mut self, index: usize) -> f64 {
        self.ensure_cumulative_through(index);
        self.cumulative_starts.get(index).copied().unwrap_or(0.0)
    }

    fn size_at(&self, index: usize) -> f64 {
        self.sizes.get(index).copied().unwrap_or(0.0)
    }

    fn find_index_at_position(&mut self, target: f64, total_len: usize) -> usize {
        if total_len == 0 {
            return 0;
        }

        self.ensure_cumulative_through(total_len.saturating_sub(1));

        match self.cumulative_starts[..total_len].binary_search_by(|pos| {
            pos.partial_cmp(&target)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }

    fn total_size(&mut self, total_len: usize) -> f64 {
        if total_len == 0 {
            return 0.0;
        }
        self.ensure_cumulative_through(total_len - 1);
        self.start_at(total_len - 1) + self.sizes[total_len - 1]
    }
}

impl Default for LayoutSizeCache {
    fn default() -> Self {
        Self::new()
    }
}
