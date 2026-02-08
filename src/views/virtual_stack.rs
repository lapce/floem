use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
    marker::PhantomData,
    ops::{Range, RangeInclusive},
    rc::Rc,
};

use floem_reactive::{
    Effect, ReadSignal, RwSignal, Scope, SignalGet, SignalTrack, SignalUpdate, SignalWith,
    WriteSignal,
};
use peniko::kurbo::{Rect, Size};
use smallvec::SmallVec;
use taffy::{FlexDirection, style::Dimension, tree::NodeId};

use crate::{
    context::ComputeLayoutCx,
    prop_extractor,
    style::{FlexDirectionProp, Style},
    view::ViewId,
    view::{self, IntoView, View},
};

use super::{Diff, DiffOpAdd, FxIndexSet, HashRun, apply_diff, diff};

pub type VirtViewFn<T> = Box<dyn Fn(T) -> (Box<dyn View>, Scope)>;

prop_extractor! {
    pub VirtualExtractor {
        pub direction: FlexDirectionProp,
    }
}

enum VirtualItemSize<T> {
    Fn(Rc<dyn Fn(&T) -> f64>),
    Fixed(Rc<dyn Fn() -> f64>),
    /// This will try to calculate the size of the items using the computed layout.
    Assume(Option<f64>),
}
impl<T> Clone for VirtualItemSize<T> {
    fn clone(&self) -> Self {
        match self {
            VirtualItemSize::Fn(rc) => VirtualItemSize::Fn(rc.clone()),
            VirtualItemSize::Fixed(rc) => VirtualItemSize::Fixed(rc.clone()),
            VirtualItemSize::Assume(x) => VirtualItemSize::Assume(*x),
        }
    }
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
    first_content_id: Option<ViewId>,
    style: VirtualExtractor,
    pub(crate) direction: RwSignal<FlexDirection>,
    item_size: RwSignal<VirtualItemSize<T>>,
    children: Vec<Option<(ViewId, Scope)>>,
    /// the index out of all of the items that is the first in the virtualized set. This is used to map an index to a [`ViewId`].
    first_child_idx: usize,
    selected_idx: HashSet<usize>,
    viewport: Rect,
    set_viewport: WriteSignal<Rect>,
    view_fn: VirtViewFn<T>,
    before_size: f64,
    content_size: f64,
    before_node: Option<NodeId>,
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
        self.item_size.set(VirtualItemSize::Fixed(Rc::new(size)));
        self
    }

    pub fn item_size_fn(self, size: impl Fn(&T) -> f64 + 'static) -> Self {
        self.item_size.set(VirtualItemSize::Fn(Rc::new(size)));
        self
    }
}

pub(crate) struct VirtualStackState<T> {
    diff: Diff<T>,
    first_idx: usize,
    before_size: f64,
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

    let (viewport, set_viewport) = RwSignal::new_split(Rect::ZERO);

    let item_size = RwSignal::new(VirtualItemSize::Assume(None));

    let direction = RwSignal::new(FlexDirection::Row);
    Effect::new(move |_| {
        direction.track();
        id.request_style();
    });

    Effect::new(move |prev| {
        let mut items_vector = each_fn();
        let viewport = viewport.get();
        let min = match direction.get() {
            FlexDirection::Column | FlexDirection::ColumnReverse => viewport.y0,
            FlexDirection::Row | FlexDirection::RowReverse => viewport.x0,
        };
        let max = match direction.get() {
            FlexDirection::Column | FlexDirection::ColumnReverse => viewport.height() + viewport.y0,
            FlexDirection::Row | FlexDirection::RowReverse => viewport.width() + viewport.x0,
        };
        let mut items = Vec::new();

        let mut before_size = 0.0;
        let mut content_size = 0.0;
        let mut start = 0;
        item_size.with(|s| match s {
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
            }
            VirtualItemSize::Fn(size_fn) => {
                let mut main_axis = 0.0;
                let total_len = items_vector.total_len();
                for (idx, item) in items_vector.slice(0..total_len).enumerate() {
                    let item_size = size_fn(&item);
                    content_size += item_size;
                    if main_axis + item_size < min {
                        main_axis += item_size;
                        before_size += item_size;
                        start = idx;
                        continue;
                    }

                    if main_axis <= max {
                        main_axis += item_size;
                        items.push(item);
                    }
                }
            }
            VirtualItemSize::Assume(None) => {
                // For the initial run with Assume(None), we need to render at least one item
                let total_len = items_vector.total_len();
                if total_len > 0 {
                    // Add just the first item so we can measure it
                    items.push(items_vector.slice(0..1).next().unwrap());

                    // Set minimal sizes for the first render
                    before_size = 0.0;
                    content_size = total_len as f64 * 10.0; // Temporary content size to ensure rendering
                }
            }
            VirtualItemSize::Assume(Some(item_size)) => {
                // Once we have the assumed size, behave like Fixed size
                let total_len = items_vector.total_len();
                start = if *item_size > 0.0 {
                    (min / item_size).floor() as usize
                } else {
                    0
                };
                let end = if *item_size > 0.0 {
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
            }
        });

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
                first_idx: start,
                before_size,
                content_size,
            });
        }
        (before_size, content_size, HashRun(hashed_items))
    });

    let view_fn = Box::new(Scope::current().enter_child(move |e| view_fn(e).into_any()));

    VirtualStack {
        id,
        first_content_id: None,
        style: Default::default(),
        direction,
        item_size,
        children: Vec::new(),
        selected_idx: HashSet::with_capacity(1),
        first_child_idx: 0,
        viewport: Rect::ZERO,
        set_viewport,
        view_fn,
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
        if state.is::<VirtualStackState<T>>() {
            if let Ok(state) = state.downcast::<VirtualStackState<T>>() {
                if self.before_size == state.before_size
                    && self.content_size == state.content_size
                    && state.diff.is_empty()
                {
                    return;
                }
                self.before_size = state.before_size;
                self.content_size = state.content_size;
                self.first_child_idx = state.first_idx;
                apply_diff(
                    self.id(),
                    cx.window_state,
                    state.diff,
                    &mut self.children,
                    &self.view_fn,
                );
                self.id.request_all();
            }
        } else if state.is::<usize>()
            && let Ok(idx) = state.downcast::<usize>()
        {
            self.id.request_style_recursive();
            self.scroll_to_idx(*idx);
            self.selected_idx.clear();
            self.selected_idx.insert(*idx);
        }
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.style.read(cx) {
            cx.window_state.request_paint(self.id);
            self.direction.set(self.style.direction());
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

    fn view_style(&self) -> Option<crate::style::Style> {
        let style = match self.direction.get_untracked() {
            // using min width and height because we strongly assume that these are respected
            FlexDirection::Column | FlexDirection::ColumnReverse => {
                Style::new().min_height(self.content_size)
            }
            FlexDirection::Row | FlexDirection::RowReverse => {
                Style::new().min_width(self.content_size)
            }
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
                    size: match self.direction.get_untracked() {
                        FlexDirection::Column | FlexDirection::ColumnReverse => {
                            taffy::prelude::Size {
                                width: Dimension::auto(),
                                height: Dimension::length(self.before_size as f32),
                            }
                        }
                        FlexDirection::Row | FlexDirection::RowReverse => taffy::prelude::Size {
                            width: Dimension::length(self.before_size as f32),
                            height: Dimension::auto(),
                        },
                    },
                    ..Default::default()
                },
            );
            self.first_content_id = self.id.children().first().copied();
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

        let layout = view::default_compute_layout(self.id, cx);

        let new_size = self.item_size.with(|s| match s {
            VirtualItemSize::Assume(None) => {
                if let Some(first_content) = self.first_content_id {
                    let taffy_layout = first_content.get_layout()?;
                    let size = taffy_layout.size;
                    if size.width == 0. || size.height == 0. {
                        return None;
                    }
                    let rect = Size::new(size.width as f64, size.height as f64).to_rect();
                    let relevant_size = match self.direction.get_untracked() {
                        FlexDirection::Column | FlexDirection::ColumnReverse => rect.height(),
                        FlexDirection::Row | FlexDirection::RowReverse => rect.width(),
                    };
                    Some(relevant_size)
                } else {
                    None
                }
            }
            _ => None,
        });
        if let Some(new_size) = new_size {
            self.item_size.set(VirtualItemSize::Assume(Some(new_size)));
        }

        layout
    }
}

impl<T> VirtualStack<T> {
    /// Scrolls to bring the item at the given index into view
    pub fn scroll_to_idx(&self, index: usize) {
        let (offset, size) = self.calculate_offset(index);

        // Create a rectangle at the calculated offset
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

    /// Calculates the offset position for an item at the given index
    fn calculate_offset(&self, index: usize) -> (f64, f64) {
        self.item_size.with(|size| match size {
            // For fixed size items, we can calculate the offset directly
            VirtualItemSize::Fixed(item_size) => {
                let size = item_size();
                (size * index as f64, size)
            }

            // For items with a size function, we would need to sum up sizes
            VirtualItemSize::Fn(_size_fn) => {
                // TODO? This method just doesn't work for variable item size.
                // this will make it so that if arrow keys are used on a virtual list
                // with item size fn, it won't scroll.
                (0., 0.)
            }

            // For assumed size items, use the assumed size if available
            VirtualItemSize::Assume(Some(size)) => (size * index as f64, *size),

            // If we don't have size information yet, default to 0
            VirtualItemSize::Assume(None) => (0.0, 0.),
        })
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
