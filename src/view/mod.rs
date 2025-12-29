//! # View and Widget Traits
//! Views are self-contained components that can be composed together to create complex UIs.
//! Views are the main building blocks of Floem.
//!
//! Views are structs that implement the [`View`] and [`Widget`] traits. Many of these structs will also contain a child field that also implements [`View`]. In this way, views can be composed together easily to create complex UIs. This is the most common way to build UIs in Floem. For more information on how to compose views check out the [views](crate::views) module.
//!
//! Creating a struct and manually implementing the [`View`] and [`Widget`] traits is typically only needed for building new widgets and for special cases. The rest of this module documentation is for help when manually implementing [`View`] and [`Widget`] on your own types.
//!
//!
//! ## The View and Widget Traits
//! The [`View`] trait is the trait that Floem uses to build  and display elements, and it builds on the [`Widget`] trait. The [`Widget`] trait contains the methods for implementing updates, styling, layout, events, and painting.
//! Eventually, the goal is for Floem to integrate the [`Widget`] trait with other rust UI libraries so that the widget layer can be shared among all compatible UI libraries.
//!
//! ## State management
//!
//! For all reactive state that your type contains, either in the form of signals or derived signals, you need to process the changes within an effect.
//! The most common pattern is to [`get`](floem_reactive::ReadSignal::get) the data in an effect and pass it in to `id.update_state()` and then handle that data in the `update` method of the View trait.
//!
//! For example a minimal slider might look like the following. First, we define the struct with the [`ViewData`] that contains the [`Id`].
//! Then, we use a function to construct the slider. As part of this function we create an effect that will be re-run every time the signals in the  `percent` closure change.
//! In the effect we send the change to the associated [`Id`]. This change can then be handled in the [`Widget::update`] method.
//! ```rust
//! use floem::ViewId;
//! use floem::reactive::*;
//!
//! struct Slider {
//!     id: ViewId,
//! }
//! pub fn slider(percent: impl Fn() -> f32 + 'static) -> Slider {
//!    let id = ViewId::new();
//!
//!    // If the following effect is not created, and `percent` is accessed directly,
//!    // `percent` will only be accessed a single time and will not be reactive.
//!    // Therefore the following `create_effect` is necessary for reactivity.
//!    create_effect(move |_| {
//!        let percent = percent();
//!        id.update_state(percent);
//!    });
//!    Slider {
//!        id,
//!    }
//! }
//! ```
//!

mod id;
mod into_iter;
pub(crate) mod stacking;
pub(crate) mod state;
mod storage;
pub mod tuple;

pub use id::ViewId;
pub use into_iter::*;
pub use state::*;
pub(crate) use storage::*;
pub use tuple::*;

use floem_reactive::{Effect, ReadSignal, RwSignal, Scope, SignalGet, UpdaterEffect};
use peniko::kurbo::*;
use smallvec::SmallVec;
use std::any::Any;
use std::hash::Hash;
use std::rc::Rc;
use taffy::tree::NodeId;

use crate::{
    Renderer,
    context::{ComputeLayoutCx, EventCx, LayoutCx, PaintCx, StyleCx, UpdateCx},
    event::{Event, EventPropagation},
    style::{LayoutProps, Style, StyleClassRef},
    unit::PxPct,
    views::{DynamicView, dyn_stack::FxIndexSet, dyn_stack::HashRun, dyn_stack::diff, dyn_view},
    window::state::WindowState,
};
use state::ViewStyleProps;

/// type erased [`View`]
///
/// Views in Floem are strongly typed. [`AnyView`] allows you to escape the strong typing by converting any type implementing [`View`] into the [`AnyView`] type.
///
/// ## Bad Example
///```compile_fail
/// use floem::views::*;
/// use floem::widgets::*;
/// use floem::reactive::{RwSignal, SignalGet};
///
/// let check = true;
///
/// container(if check == true {
///     checkbox(|| true)
/// } else {
///     label(|| "no check".to_string())
/// });
/// ```
/// The above example will fail to compile because `container` is expecting a single type implementing `View` so the if and
/// the else must return the same type. However the branches return different types. The solution to this is to use the [`IntoView::into_any`] method
/// to escape the strongly typed requirement.
///
/// ```
/// use floem::reactive::{RwSignal, SignalGet};
/// use floem::views::*;
/// use floem::{IntoView, View};
///
/// let check = true;
///
/// container(if check == true {
///     checkbox(|| true).into_any()
/// } else {
///     label(|| "no check".to_string()).into_any()
/// });
/// ```
pub type AnyView = Box<dyn View>;

/// Converts a value into a [`View`].
///
/// This trait can be implemented on types which can be built into another type that implements the `View` trait.
///
/// For example, `&str` implements `IntoView` by building a `text` view and can therefore be used directly in a View tuple.
/// ```rust
/// # use floem::reactive::*;
/// # use floem::views::*;
/// # use floem::IntoView;
/// fn app_view() -> impl IntoView {
///     v_stack(("Item One", "Item Two"))
/// }
/// ```
/// Check out the [other types](#foreign-impls) that `IntoView` is implemented for.
pub trait IntoView: Sized {
    /// The final View type this converts to.
    type V: View + 'static;

    /// Intermediate type that has a [`ViewId`] before full view construction.
    ///
    /// For [`View`] types, this is `Self` (already has ViewId).
    /// For primitives, this is [`LazyView<Self>`] (creates ViewId, defers view construction).
    /// For tuples/vecs, this is the converted view type (eager conversion).
    type Intermediate: HasViewId + IntoView<V = Self::V>;

    /// Converts to the intermediate form which has a [`ViewId`].
    ///
    /// This is used by [`Decorators`](crate::views::Decorators) to get a [`ViewId`]
    /// for applying styles before the final view is constructed.
    fn into_intermediate(self) -> Self::Intermediate;

    /// Converts the value into a [`View`].
    fn into_view(self) -> Self::V {
        self.into_intermediate().into_view()
    }

    /// Converts the value into a [`AnyView`].
    fn into_any(self) -> AnyView {
        Box::new(self.into_view())
    }
}

/// A trait for types that have an associated [`ViewId`].
///
/// This is automatically implemented for all types that implement [`View`],
/// and can be manually implemented for intermediate types like [`Pending`].
pub trait HasViewId {
    /// Returns the [`ViewId`] associated with this value.
    fn view_id(&self) -> ViewId;
}

/// Blanket implementation of [`HasViewId`] for all [`View`] types.
impl<V: View> HasViewId for V {
    fn view_id(&self) -> ViewId {
        self.id()
    }
}

/// A trait for views that can accept children.
///
/// This provides a builder-pattern API for adding children to views,
/// similar to GPUI's `ParentElement` trait. Both methods append to
/// existing children rather than replacing them.
///
/// Views opt-in to this trait by implementing it. Not all views should
/// have children (e.g., `Label`, `TextInput`), so there is no blanket
/// implementation.
///
/// ## Example
/// ```rust,ignore
/// Stack::empty()
///     .child(text("Header"))
///     .children((0..5).map(|i| text(format!("Item {i}"))))
///     .child(text("Footer"))
/// ```
pub trait ParentView: HasViewId + Sized {
    /// Adds a single child to this view.
    fn child(self, child: impl IntoView) -> Self {
        self.view_id().add_child(child.into_any());
        self
    }

    /// Adds multiple children to this view.
    ///
    /// Accepts arrays, tuples, vectors, and iterators of views.
    fn children(self, children: impl IntoViewIter) -> Self {
        // Eagerly collect to ensure view construction (which may access VIEW_STORAGE)
        // completes before we add children to VIEW_STORAGE
        let views: Vec<AnyView> = children.into_view_iter().collect();
        self.view_id().append_children(views);
        self
    }

    /// Adds reactive children that update when signals change.
    ///
    /// The children function is called initially and re-called whenever
    /// its reactive dependencies change, replacing all children.
    ///
    /// ## Example
    /// ```rust,ignore
    /// use floem::prelude::*;
    /// use floem::views::Stem;
    ///
    /// let items = RwSignal::new(vec!["a", "b", "c"]);
    ///
    /// Stem::new().derived_children(move || {
    ///     items.get().into_iter().map(|item| text(item))
    /// });
    /// ```
    fn derived_children<CF, C>(self, children_fn: CF) -> Self
    where
        CF: Fn() -> C + 'static,
        C: IntoViewIter + 'static,
    {
        let id = self.view_id();
        let children_fn = Box::new(
            Scope::current()
                .enter_child(move |_| children_fn().into_view_iter().collect::<Vec<_>>()),
        );

        let (initial_children, initial_scope) = UpdaterEffect::new(
            move || children_fn(()),
            move |(new_children, new_scope): (Vec<AnyView>, Scope)| {
                // Dispose old scope and remove old children
                if let Some(old_scope) = id.take_children_scope() {
                    old_scope.dispose();
                }
                // Set new children and store new scope
                id.set_children_vec(new_children);
                id.set_children_scope(new_scope);
                id.request_all();
            },
        );

        // Set initial children and scope
        id.set_children_vec(initial_children);
        id.set_children_scope(initial_scope);
        self
    }

    /// Adds a single reactive child that updates when signals change.
    ///
    /// Similar to `dyn_container`, but as a method on `ParentView`.
    /// When the state changes, the old child is replaced with a new one.
    ///
    /// ## Example
    /// ```rust,ignore
    /// use floem::prelude::*;
    /// use floem::views::Stem;
    ///
    /// #[derive(Clone)]
    /// enum ViewType { One, Two }
    ///
    /// let view_type = RwSignal::new(ViewType::One);
    ///
    /// Stem::new().derived_child(
    ///     move || view_type.get(),
    ///     |value| match value {
    ///         ViewType::One => text("One"),
    ///         ViewType::Two => text("Two"),
    ///     }
    /// );
    /// ```
    fn derived_child<S, SF, CF, V>(self, state_fn: SF, child_fn: CF) -> Self
    where
        SF: Fn() -> S + 'static,
        CF: Fn(S) -> V + 'static,
        V: IntoView + 'static,
        S: 'static,
    {
        let id = self.view_id();

        // Wrap child_fn to create scoped views, using Rc to share between effect and initial setup
        let child_fn: Rc<dyn Fn(S) -> (AnyView, Scope)> =
            Rc::new(Scope::current().enter_child(move |state: S| child_fn(state).into_any()));

        let child_fn_for_effect = child_fn.clone();

        // Create effect and get initial state
        let initial_state = UpdaterEffect::new(state_fn, move |new_state: S| {
            // Dispose old scope
            if let Some(old_scope) = id.take_children_scope() {
                old_scope.dispose();
            }

            // Get old child for removal
            let old_children = id.children();

            // Create new child
            let (new_child, new_scope) = child_fn_for_effect(new_state);
            let new_child_id = new_child.id();
            new_child_id.set_parent(id);
            new_child_id.set_view(new_child);
            id.set_children_ids(vec![new_child_id]);
            id.set_children_scope(new_scope);

            // Request removal of old children
            id.request_remove_views(old_children);
            id.request_all();
        });

        // Create initial child from initial state
        let (initial_child, initial_scope) = child_fn(initial_state);
        let child_id = initial_child.id();
        child_id.set_parent(id);
        child_id.set_view(initial_child);
        id.set_children_ids(vec![child_id]);
        id.set_children_scope(initial_scope);
        self
    }

    /// Adds keyed reactive children that efficiently update when signals change.
    ///
    /// Unlike `derived_children` which recreates all children on every update,
    /// `keyed_children` uses keys to identify items and only creates/removes
    /// children that actually changed. Views for unchanged items are reused.
    ///
    /// ## Arguments
    /// - `items_fn`: A function that returns an iterator of items
    /// - `key_fn`: A function that extracts a unique key from each item
    /// - `view_fn`: A function that creates a view from an item
    ///
    /// ## Example
    /// ```rust,ignore
    /// use floem::prelude::*;
    /// use floem::views::Stem;
    ///
    /// let items = RwSignal::new(vec!["a", "b", "c"]);
    ///
    /// Stem::new().keyed_children(
    ///     move || items.get(),
    ///     |item| *item,  // key by the item itself
    ///     |item| text(item),
    /// );
    /// ```
    fn keyed_children<IF, I, T, K, KF, VF, V>(self, items_fn: IF, key_fn: KF, view_fn: VF) -> Self
    where
        IF: Fn() -> I + 'static,
        I: IntoIterator<Item = T>,
        KF: Fn(&T) -> K + 'static,
        K: Eq + Hash + 'static,
        VF: Fn(T) -> V + 'static,
        V: IntoView + 'static,
        T: 'static,
    {
        let id = self.view_id();

        // Wrap view_fn to create scoped views - each child gets its own scope
        let view_fn = Scope::current().enter_child(move |item: T| view_fn(item).into_any());

        // Initialize keyed children state
        id.set_keyed_children(Vec::new());

        Effect::new(move |prev_hash_run: Option<HashRun<FxIndexSet<K>>>| {
            let items: SmallVec<[T; 128]> = items_fn().into_iter().collect();
            let new_keys: FxIndexSet<K> = items.iter().map(&key_fn).collect();

            // Take current children state
            let mut children: Vec<Option<(ViewId, Scope)>> = id
                .take_keyed_children()
                .unwrap_or_default()
                .into_iter()
                .map(Some)
                .collect();

            if let Some(HashRun(prev_keys)) = prev_hash_run {
                // Compute diff between old and new keys
                let mut diff_result = diff::<K, T>(&prev_keys, &new_keys);

                // Prepare items for added entries
                let mut items: SmallVec<[Option<T>; 128]> = items.into_iter().map(Some).collect();
                for added in &mut diff_result.added {
                    added.view = items[added.at].take();
                }

                // Apply diff operations (returns views to remove)
                let views_to_remove = apply_keyed_diff(id, &mut children, diff_result, &view_fn);

                // Request removal via message (processed during update phase)
                id.request_remove_views(views_to_remove);
            } else {
                // First run - create all children
                for item in items {
                    let (view, scope) = view_fn(item);
                    let child_id = view.id();
                    child_id.set_parent(id);
                    child_id.set_view(view);
                    children.push(Some((child_id, scope)));
                }
            }

            // Update the actual children list
            let children_ids: Vec<ViewId> = children
                .iter()
                .filter_map(|c| Some(c.as_ref()?.0))
                .collect();
            id.set_children_ids(children_ids);

            // Store updated children state (convert back from Option)
            let children_vec: Vec<(ViewId, Scope)> = children.into_iter().flatten().collect();
            id.set_keyed_children(children_vec);

            id.request_all();
            HashRun(new_keys)
        });

        self
    }
}

/// Apply keyed diff operations to children.
///
/// Returns a list of ViewIds to remove (removal is deferred via message).
fn apply_keyed_diff<T, VF>(
    parent_id: ViewId,
    children: &mut Vec<Option<(ViewId, Scope)>>,
    diff: crate::views::dyn_stack::Diff<T>,
    view_fn: &VF,
) -> Vec<ViewId>
where
    VF: Fn(T) -> (AnyView, Scope),
{
    use crate::views::dyn_stack::{DiffOpAdd, DiffOpMove, DiffOpRemove};

    let mut views_to_remove = Vec::new();

    // Resize children if needed
    if diff.added.len() > diff.removed.len() {
        let target_size = children.len() + diff.added.len() - diff.removed.len();
        children.resize_with(target_size, || None);
    }

    // Items to move (deferred to avoid overwriting)
    let mut items_to_move = Vec::with_capacity(diff.moved.len());

    // 1. Clear all if requested
    if diff.clear {
        for i in 0..children.len() {
            if let Some((view_id, scope)) = children[i].take() {
                views_to_remove.push(view_id);
                scope.dispose();
            }
        }
    }

    // 2. Remove items (collect for deferred removal)
    for DiffOpRemove { at } in diff.removed {
        if let Some((view_id, scope)) = children[at].take() {
            views_to_remove.push(view_id);
            scope.dispose();
        }
    }

    // 3. Collect items to move
    for DiffOpMove { from, to } in diff.moved {
        if let Some(item) = children[from].take() {
            items_to_move.push((to, item));
        }
    }

    // 4. Add new items
    for DiffOpAdd { at, view } in diff.added {
        if let Some(item) = view {
            let (view, scope) = view_fn(item);
            let child_id = view.id();
            child_id.set_parent(parent_id);
            child_id.set_view(view);
            children[at] = Some((child_id, scope));
        }
    }

    // 5. Apply moves
    for (to, item) in items_to_move {
        children[to] = Some(item);
    }

    // 6. Remove holes
    children.retain(|c| c.is_some());

    views_to_remove
}

/// A wrapper type for lazy view construction.
///
/// `LazyView<T>` wraps a value that will eventually be converted into a [`View`],
/// but creates its [`ViewId`] eagerly. This allows decorators to be applied
/// before the actual view is constructed.
///
/// ## Example
/// ```rust
/// use floem::views::*;
/// use floem::{IntoView, LazyView};
///
/// // The ViewId is created when LazyView is constructed,
/// // but the Label is only created when into_view() is called
/// let lazy = LazyView::new("Hello");
/// let view = lazy.style(|s| s.padding(10.0)).into_view();
/// ```
pub struct LazyView<T> {
    /// The ViewId created eagerly for this lazy view.
    pub id: ViewId,
    /// The content that will be converted into a view.
    pub content: T,
}

impl<T> LazyView<T> {
    /// Creates a new `LazyView` wrapper with an eagerly-created [`ViewId`].
    pub fn new(content: T) -> Self {
        Self {
            id: ViewId::new(),
            content,
        }
    }

    /// Creates a new `LazyView` wrapper with an existing [`ViewId`].
    pub fn with_id(id: ViewId, content: T) -> Self {
        Self { id, content }
    }
}

impl<T> HasViewId for LazyView<T> {
    fn view_id(&self) -> ViewId {
        self.id
    }
}

impl<IV: IntoView + 'static> IntoView for Box<dyn Fn() -> IV> {
    type V = DynamicView;
    type Intermediate = DynamicView;

    fn into_intermediate(self) -> Self::Intermediate {
        dyn_view(self)
    }
}

impl<T: IntoView + Clone + 'static> IntoView for RwSignal<T> {
    type V = DynamicView;
    type Intermediate = DynamicView;

    fn into_intermediate(self) -> Self::Intermediate {
        dyn_view(move || self.get())
    }
}

impl<T: IntoView + Clone + 'static> IntoView for ReadSignal<T> {
    type V = DynamicView;
    type Intermediate = DynamicView;

    fn into_intermediate(self) -> Self::Intermediate {
        dyn_view(move || self.get())
    }
}

impl<VW: View + 'static> IntoView for VW {
    type V = VW;
    type Intermediate = Self;

    fn into_intermediate(self) -> Self::Intermediate {
        self
    }

    fn into_view(self) -> Self::V {
        self
    }
}

impl IntoView for i32 {
    type V = crate::views::Label;
    type Intermediate = LazyView<i32>;

    fn into_intermediate(self) -> Self::Intermediate {
        LazyView::new(self)
    }
}

impl IntoView for usize {
    type V = crate::views::Label;
    type Intermediate = LazyView<usize>;

    fn into_intermediate(self) -> Self::Intermediate {
        LazyView::new(self)
    }
}

impl IntoView for &str {
    type V = crate::views::Label;
    type Intermediate = LazyView<String>;

    fn into_intermediate(self) -> Self::Intermediate {
        LazyView::new(self.to_string())
    }
}

impl IntoView for String {
    type V = crate::views::Label;
    type Intermediate = LazyView<String>;

    fn into_intermediate(self) -> Self::Intermediate {
        LazyView::new(self)
    }
}

impl IntoView for () {
    type V = crate::views::Empty;
    type Intermediate = LazyView<()>;

    fn into_intermediate(self) -> Self::Intermediate {
        LazyView::new(self)
    }
}

impl<IV: IntoView + 'static> IntoView for Vec<IV> {
    type V = crate::views::Stack;
    type Intermediate = LazyView<Vec<IV>>;

    fn into_intermediate(self) -> Self::Intermediate {
        LazyView::new(self)
    }
}

impl<IV: IntoView + 'static> IntoView for LazyView<Vec<IV>> {
    type V = crate::views::Stack;
    type Intermediate = Self;

    fn into_intermediate(self) -> Self::Intermediate {
        self
    }

    fn into_view(self) -> Self::V {
        crate::views::from_iter_with_id(self.id, self.content, None)
    }
}

// IntoView implementations for LazyView<T> types
// These use the pre-created ViewId from LazyView
// LazyView is its own Intermediate since it already has a ViewId

impl IntoView for LazyView<i32> {
    type V = crate::views::Label;
    type Intermediate = Self;

    fn into_intermediate(self) -> Self::Intermediate {
        self
    }

    fn into_view(self) -> Self::V {
        crate::views::Label::with_id(self.id, self.content)
    }
}

impl IntoView for LazyView<usize> {
    type V = crate::views::Label;
    type Intermediate = Self;

    fn into_intermediate(self) -> Self::Intermediate {
        self
    }

    fn into_view(self) -> Self::V {
        crate::views::Label::with_id(self.id, self.content)
    }
}

impl IntoView for LazyView<&str> {
    type V = crate::views::Label;
    type Intermediate = Self;

    fn into_intermediate(self) -> Self::Intermediate {
        self
    }

    fn into_view(self) -> Self::V {
        crate::views::Label::with_id(self.id, self.content)
    }
}

impl IntoView for LazyView<String> {
    type V = crate::views::Label;
    type Intermediate = Self;

    fn into_intermediate(self) -> Self::Intermediate {
        self
    }

    fn into_view(self) -> Self::V {
        crate::views::Label::with_id(self.id, self.content)
    }
}

impl IntoView for LazyView<()> {
    type V = crate::views::Empty;
    type Intermediate = Self;

    fn into_intermediate(self) -> Self::Intermediate {
        self
    }

    fn into_view(self) -> Self::V {
        crate::views::Empty::with_id(self.id)
    }
}

/// Default implementation of `View::layout()` which can be used by
/// view implementations that need the default behavior and also need
/// to implement that method to do additional work.
pub fn recursively_layout_view(id: ViewId, cx: &mut LayoutCx) -> NodeId {
    cx.layout_node(id, true, |cx| {
        let mut nodes = Vec::new();
        for child in id.children() {
            let view = child.view();
            let mut view = view.borrow_mut();
            nodes.push(view.layout(cx));
        }
        nodes
    })
}

/// The View trait contains the methods for implementing updates, styling, layout, events, and painting.
///
/// The [`id`](View::id) method must be implemented.
/// The other methods may be implemented as necessary to implement the functionality of the View.
/// ## State Management in a Custom View
///
/// For all reactive state that your type contains, either in the form of signals or derived signals, you need to process the changes within an effect.
/// The most common pattern is to [`get`](floem_reactive::SignalGet::get) the data in an effect and pass it in to `id.update_state()` and then handle that data in the `update` method of the `View` trait.
///
/// For example a minimal slider might look like the following. First, we define the struct that contains the [`ViewId`](crate::ViewId).
/// Then, we use a function to construct the slider. As part of this function we create an effect that will be re-run every time the signals in the  `percent` closure change.
/// In the effect we send the change to the associated [`ViewId`](crate::ViewId). This change can then be handled in the [`View::update`](crate::View::update) method.
/// ```rust
/// # use floem::{*, views::*, reactive::*};
///
/// struct Slider {
///     id: ViewId,
///     percent: f32,
/// }
/// pub fn slider(percent: impl Fn() -> f32 + 'static) -> Slider {
///     let id = ViewId::new();
///
///     // If the following effect is not created, and `percent` is accessed directly,
///     // `percent` will only be accessed a single time and will not be reactive.
///     // Therefore the following `create_effect` is necessary for reactivity.
///     create_effect(move |_| {
///         let percent = percent();
///         id.update_state(percent);
///     });
///     Slider { id, percent: 0.0 }
/// }
/// impl View for Slider {
///     fn id(&self) -> ViewId {
///         self.id
///     }
///
///     fn update(&mut self, cx: &mut floem::context::UpdateCx, state: Box<dyn std::any::Any>) {
///         if let Ok(percent) = state.downcast::<f32>() {
///             self.percent = *percent;
///             self.id.request_layout();
///         }
///     }
/// }
/// ```
pub trait View {
    fn id(&self) -> ViewId;

    fn view_style(&self) -> Option<Style> {
        None
    }

    fn view_class(&self) -> Option<StyleClassRef> {
        None
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        core::any::type_name::<Self>().into()
    }

    /// Use this method to react to changes in view-related state.
    /// You will usually send state to this hook manually using the `View`'s `Id` handle
    ///
    /// ```ignore
    /// self.id.update_state(SomeState)
    /// ```
    ///
    /// You are in charge of downcasting the state to the expected type.
    ///
    /// If the update needs other passes to run you're expected to call
    /// `_cx.window_state_mut().request_changes`.
    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) {
        // these are here to just ignore these arguments in the default case
        let _ = cx;
        let _ = state;
    }

    /// Use this method to style the view's children.
    ///
    /// If the style changes needs other passes to run you're expected to call
    /// `cx.window_state.style_dirty.insert(view_id)`.
    fn style_pass(&mut self, cx: &mut StyleCx<'_>) {
        let _ = cx;
    }

    /// Use this method to layout the view's children.
    /// Usually you'll do this by calling [`LayoutCx::layout_node`].
    ///
    /// If the layout changes needs other passes to run you're expected to call
    /// `cx.window_state_mut().request_changes`.
    fn layout(&mut self, cx: &mut LayoutCx) -> NodeId {
        recursively_layout_view(self.id(), cx)
    }

    /// Responsible for computing the layout of the view's children.
    ///
    /// If the layout changes needs other passes to run you're expected to call
    /// `cx.window_state_mut().request_changes`.
    fn compute_layout(&mut self, cx: &mut ComputeLayoutCx) -> Option<Rect> {
        default_compute_layout(self.id(), cx)
    }

    fn event_before_children(&mut self, cx: &mut EventCx, event: &Event) -> EventPropagation {
        // these are here to just ignore these arguments in the default case
        let _ = cx;
        let _ = event;

        EventPropagation::Continue
    }

    fn event_after_children(&mut self, cx: &mut EventCx, event: &Event) -> EventPropagation {
        // these are here to just ignore these arguments in the default case
        let _ = cx;
        let _ = event;

        EventPropagation::Continue
    }

    /// `View`-specific implementation. Will be called in [`PaintCx::paint_view`](crate::context::PaintCx::paint_view).
    /// Usually you'll call `paint_view` for every child view. But you might also draw text, adjust the offset, clip
    /// or draw text.
    fn paint(&mut self, cx: &mut PaintCx) {
        cx.paint_children(self.id());
    }

    /// Scrolls the view and all direct and indirect children to bring the `target` view to be
    /// visible. Returns true if this view contains or is the target.
    fn scroll_to(&mut self, cx: &mut WindowState, target: ViewId, rect: Option<Rect>) -> bool {
        if self.id() == target {
            return true;
        }
        let mut found = false;

        for child in self.id().children() {
            found |= child.view().borrow_mut().scroll_to(cx, target, rect);
        }
        found
    }
}

impl View for Box<dyn View> {
    fn id(&self) -> ViewId {
        (**self).id()
    }

    fn view_style(&self) -> Option<Style> {
        (**self).view_style()
    }

    fn view_class(&self) -> Option<StyleClassRef> {
        (**self).view_class()
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        (**self).debug_name()
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) {
        (**self).update(cx, state)
    }

    fn style_pass(&mut self, cx: &mut StyleCx) {
        (**self).style_pass(cx)
    }

    fn layout(&mut self, cx: &mut LayoutCx) -> NodeId {
        (**self).layout(cx)
    }

    fn event_before_children(&mut self, cx: &mut EventCx, event: &Event) -> EventPropagation {
        (**self).event_before_children(cx, event)
    }

    fn event_after_children(&mut self, cx: &mut EventCx, event: &Event) -> EventPropagation {
        (**self).event_after_children(cx, event)
    }

    fn compute_layout(&mut self, cx: &mut ComputeLayoutCx) -> Option<Rect> {
        (**self).compute_layout(cx)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        (**self).paint(cx)
    }

    fn scroll_to(&mut self, cx: &mut WindowState, target: ViewId, rect: Option<Rect>) -> bool {
        (**self).scroll_to(cx, target, rect)
    }
}

/// Computes the layout of the view's children, if any.
pub fn default_compute_layout(id: ViewId, cx: &mut ComputeLayoutCx) -> Option<Rect> {
    let mut layout_rect: Option<Rect> = None;
    for child in id.children() {
        if !child.is_hidden() {
            let child_layout = cx.compute_view_layout(child);
            if let Some(child_layout) = child_layout {
                if let Some(rect) = layout_rect {
                    layout_rect = Some(rect.union(child_layout));
                } else {
                    layout_rect = Some(child_layout);
                }
            }
        }
    }
    layout_rect
}

pub(crate) fn border_radius(radius: crate::unit::PxPct, size: f64) -> f64 {
    match radius {
        crate::unit::PxPct::Px(px) => px,
        crate::unit::PxPct::Pct(pct) => size * (pct / 100.),
    }
}

fn border_to_radii_view(style: &ViewStyleProps, size: Size) -> RoundedRectRadii {
    let border_radii = style.border_radius();
    RoundedRectRadii {
        top_left: border_radius(
            border_radii.top_left.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
        top_right: border_radius(
            border_radii.top_right.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
        bottom_left: border_radius(
            border_radii.bottom_left.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
        bottom_right: border_radius(
            border_radii.bottom_right.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
    }
}

pub(crate) fn border_to_radii(style: &Style, size: Size) -> RoundedRectRadii {
    let border_radii = style.get(crate::style::BorderRadiusProp);
    RoundedRectRadii {
        top_left: border_radius(
            border_radii.top_left.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
        top_right: border_radius(
            border_radii.top_right.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
        bottom_left: border_radius(
            border_radii.bottom_left.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
        bottom_right: border_radius(
            border_radii.bottom_right.unwrap_or(PxPct::Px(0.0)),
            size.min_side(),
        ),
    }
}

pub(crate) fn paint_bg(cx: &mut PaintCx, style: &ViewStyleProps, size: Size) {
    let radii = border_to_radii_view(style, size);
    if radii_max(radii) > 0.0 {
        let rect = size.to_rect();
        paint_box_shadow(cx, style, rect, Some(radii));
        let bg = match style.background() {
            Some(color) => color,
            None => return,
        };
        let rounded_rect = rect.to_rounded_rect(radii);
        cx.fill(&rounded_rect, &bg, 0.0);
    } else {
        paint_box_shadow(cx, style, size.to_rect(), None);
        let bg = match style.background() {
            Some(color) => color,
            None => return,
        };
        cx.fill(&size.to_rect(), &bg, 0.0);
    }
}

fn paint_box_shadow(
    cx: &mut PaintCx,
    style: &ViewStyleProps,
    rect: Rect,
    rect_radius: Option<RoundedRectRadii>,
) {
    for shadow in &style.shadow() {
        let min = rect.size().min_side();
        let left_offset = match shadow.left_offset {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => min * (pct / 100.),
        };
        let right_offset = match shadow.right_offset {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => min * (pct / 100.),
        };
        let top_offset = match shadow.top_offset {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => min * (pct / 100.),
        };
        let bottom_offset = match shadow.bottom_offset {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => min * (pct / 100.),
        };
        let spread = match shadow.spread {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => min * (pct / 100.),
        };
        let blur_radius = match shadow.blur_radius {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => min * (pct / 100.),
        };
        let inset = Insets::new(
            left_offset / 2.0,
            top_offset / 2.0,
            right_offset / 2.0,
            bottom_offset / 2.0,
        );
        let rect = rect.inflate(spread, spread).inset(inset);
        if let Some(radii) = rect_radius {
            let rounded_rect = RoundedRect::from_rect(rect, radii_add(radii, spread));
            cx.fill(&rounded_rect, shadow.color, blur_radius);
        } else {
            cx.fill(&rect, shadow.color, blur_radius);
        }
    }
}
#[cfg(feature = "vello")]
pub(crate) fn paint_outline(cx: &mut PaintCx, style: &ViewStyleProps, size: Size) {
    use crate::{
        paint::{BorderPath, BorderPathEvent},
        unit::Pct,
    };

    let outlines = [
        (style.outline().0, style.outline_color()),
        (style.outline().0, style.outline_color()),
        (style.outline().0, style.outline_color()),
        (style.outline().0, style.outline_color()),
    ];

    // Early return if no outlines
    if outlines.iter().any(|o| o.0.width == 0.0) {
        return;
    }

    let outline_color = style.outline_color();
    let Pct(outline_progress) = style.outline_progress();

    let half_width = outlines[0].0.width / 2.0;
    let rect = size.to_rect().inflate(half_width, half_width);

    let radii = radii_map(border_to_radii_view(style, size), |r| {
        (r + half_width).max(0.0)
    });

    let mut outline_path = BorderPath::new(rect, radii);

    // Only create subsegment if needed
    if outline_progress < 100. {
        outline_path.subsegment(0.0..(outline_progress.clamp(0.0, 100.) / 100.));
    }

    let mut current_path = Vec::new();
    for event in outline_path.path_elements(&outlines, 0.1) {
        match event {
            BorderPathEvent::PathElement(el) => current_path.push(el),
            BorderPathEvent::NewStroke(stroke) => {
                // Render current path with previous stroke if any
                if !current_path.is_empty() {
                    cx.stroke(&current_path.as_slice(), &outline_color, &stroke.0);
                    current_path.clear();
                }
            }
        }
    }
    assert!(current_path.is_empty());
}

#[cfg(not(feature = "vello"))]
pub(crate) fn paint_outline(cx: &mut PaintCx, style: &ViewStyleProps, size: Size) {
    let outline = &style.outline().0;
    if outline.width == 0. {
        // TODO: we should warn! when outline is < 0
        return;
    }
    let half = outline.width / 2.0;
    let rect = size.to_rect().inflate(half, half);
    let border_radii = border_to_radii_view(style, size);
    cx.stroke(
        &rect.to_rounded_rect(radii_add(border_radii, half)),
        &style.outline_color(),
        outline,
    );
}

#[cfg(not(feature = "vello"))]
pub(crate) fn paint_border(
    cx: &mut PaintCx,
    layout_style: &LayoutProps,
    style: &ViewStyleProps,
    size: Size,
) {
    let border = layout_style.border();

    let left = border.left.map(|v| v.0).unwrap_or(Stroke::new(0.));
    let top = border.top.map(|v| v.0).unwrap_or(Stroke::new(0.));
    let right = border.right.map(|v| v.0).unwrap_or(Stroke::new(0.));
    let bottom = border.bottom.map(|v| v.0).unwrap_or(Stroke::new(0.));

    if left.width == top.width
        && top.width == right.width
        && right.width == bottom.width
        && bottom.width == left.width
        && left.width > 0.0
        && style.border_color().left.is_some()
        && style.border_color().top.is_some()
        && style.border_color().right.is_some()
        && style.border_color().bottom.is_some()
        && style.border_color().left == style.border_color().top
        && style.border_color().top == style.border_color().right
        && style.border_color().right == style.border_color().bottom
    {
        let half = left.width / 2.0;
        let rect = size.to_rect().inflate(-half, -half);
        let radii = border_to_radii_view(style, size);
        if let Some(color) = style.border_color().left {
            if radii_max(radii) > 0.0 {
                let radii = radii_map(radii, |r| (r - half).max(0.0));
                cx.stroke(&rect.to_rounded_rect(radii), &color, &left);
            } else {
                cx.stroke(&rect, &color, &left);
            }
        }
    } else {
        // TODO: now with vello should we do this left.width > 0. check?
        if left.width > 0.0
            && let Some(color) = style.border_color().left
        {
            let half = left.width / 2.0;
            cx.stroke(
                &Line::new(Point::new(half, 0.0), Point::new(half, size.height)),
                &color,
                &left,
            );
        }
        if right.width > 0.0
            && let Some(color) = style.border_color().right
        {
            let half = right.width / 2.0;
            cx.stroke(
                &Line::new(
                    Point::new(size.width - half, 0.0),
                    Point::new(size.width - half, size.height),
                ),
                &color,
                &right,
            );
        }
        if top.width > 0.0
            && let Some(color) = style.border_color().top
        {
            let half = top.width / 2.0;
            cx.stroke(
                &Line::new(Point::new(0.0, half), Point::new(size.width, half)),
                &color,
                &top,
            );
        }
        if bottom.width > 0.0
            && let Some(color) = style.border_color().bottom
        {
            let half = bottom.width / 2.0;
            cx.stroke(
                &Line::new(
                    Point::new(0.0, size.height - half),
                    Point::new(size.width, size.height - half),
                ),
                &color,
                &bottom,
            );
        }
    }
}

#[cfg(feature = "vello")]
pub(crate) fn paint_border(
    cx: &mut PaintCx,
    layout_style: &LayoutProps,
    style: &ViewStyleProps,
    size: Size,
) {
    use crate::{
        paint::{BorderPath, BorderPathEvent},
        unit::Pct,
    };

    let border = layout_style.border();
    let borders = [
        (
            border.top.map(|v| v.0).unwrap_or(Stroke::new(0.)),
            style.border_color().top.unwrap_or_default(),
        ),
        (
            border.right.map(|v| v.0).unwrap_or(Stroke::new(0.)),
            style.border_color().right.unwrap_or_default(),
        ),
        (
            border.bottom.map(|v| v.0).unwrap_or(Stroke::new(0.)),
            style.border_color().bottom.unwrap_or_default(),
        ),
        (
            border.left.map(|v| v.0).unwrap_or(Stroke::new(0.)),
            style.border_color().left.unwrap_or_default(),
        ),
    ];

    // Early return if no borders
    if borders.iter().all(|b| b.0.width == 0.0) {
        return;
    }

    let Pct(border_progress) = style.border_progress();

    let half_width = borders[0].0.width / 2.0;
    let rect = size.to_rect().inflate(-half_width, -half_width);

    let radii = radii_map(border_to_radii_view(style, size), |r| {
        (r - half_width).max(0.0)
    });

    let mut border_path = BorderPath::new(rect, radii);

    // Only create subsegment if needed
    if border_progress < 100. {
        border_path.subsegment(0.0..(border_progress.clamp(0.0, 100.) / 100.));
    }

    // optimize for maximum which is 12 paths and a single move to
    let mut current_path = smallvec::SmallVec::<[_; 13]>::new();
    for event in border_path.path_elements(&borders, 0.1) {
        match event {
            BorderPathEvent::PathElement(el) => {
                if !current_path.is_empty() && matches!(el, PathEl::MoveTo(_)) {
                    // extra move to's will mess up dashed patterns
                    continue;
                }
                current_path.push(el)
            }
            BorderPathEvent::NewStroke(stroke) => {
                // Render current path with previous stroke if any
                if !current_path.is_empty() && stroke.0.width > 0. {
                    cx.stroke(&current_path.as_slice(), &stroke.1, &stroke.0);
                    current_path.clear();
                } else if stroke.0.width == 0. {
                    current_path.clear();
                }
            }
        }
    }
    assert!(current_path.is_empty());
}

/// Tab navigation finds the next or previous view with the `keyboard_navigatable` status in the tree.
pub(crate) fn view_tab_navigation(
    root_view: ViewId,
    window_state: &mut WindowState,
    backwards: bool,
) {
    let start = window_state
        .focus
        .unwrap_or(window_state.prev_focus.unwrap_or(root_view));

    let tree_iter = |id: ViewId| {
        if backwards {
            view_tree_previous(root_view, id).unwrap_or_else(|| view_nested_last_child(root_view))
        } else {
            view_tree_next(id).unwrap_or(root_view)
        }
    };

    let mut new_focus = tree_iter(start);
    while new_focus != start && !window_state.focusable.contains(&new_focus) {
        new_focus = tree_iter(new_focus);
    }

    window_state.clear_focus();
    window_state.update_focus(new_focus, true);
}

/// Get the next item in the tree, either the first child or the next sibling of this view or of the first parent view
fn view_tree_next(id: ViewId) -> Option<ViewId> {
    if let Some(child) = id.children().into_iter().next() {
        return Some(child);
    }

    let mut ancestor = id;
    loop {
        if let Some(next_sibling) = view_next_sibling(ancestor) {
            return Some(next_sibling);
        }
        ancestor = ancestor.parent()?;
    }
}

/// Get the id of the view after this one (but with the same parent and level of nesting)
fn view_next_sibling(id: ViewId) -> Option<ViewId> {
    let parent = id.parent();

    let Some(parent) = parent else {
        // We're the root, which has no sibling
        return None;
    };

    let children = parent.children();
    //TODO: Log a warning if the child isn't found. This shouldn't happen (error in floem if it does), but this shouldn't panic if that does happen
    let pos = children.iter().position(|v| v == &id)?;

    if pos + 1 < children.len() {
        Some(children[pos + 1])
    } else {
        None
    }
}

/// Get the next item in the tree, the deepest last child of the previous sibling of this view or the parent
fn view_tree_previous(root_view: ViewId, id: ViewId) -> Option<ViewId> {
    view_previous_sibling(id)
        .map(view_nested_last_child)
        .or_else(|| {
            (root_view != id).then_some(
                id.parent()
                    .unwrap_or_else(|| view_nested_last_child(root_view)),
            )
        })
}

/// Get the id of the view before this one (but with the same parent and level of nesting)
fn view_previous_sibling(id: ViewId) -> Option<ViewId> {
    let parent = id.parent();

    let Some(parent) = parent else {
        // We're the root, which has no sibling
        return None;
    };

    let children = parent.children();
    let pos = children.iter().position(|v| v == &id).unwrap();

    if pos > 0 {
        Some(children[pos - 1])
    } else {
        None
    }
}

fn view_nested_last_child(view: ViewId) -> ViewId {
    let mut last_child = view;
    while let Some(new_last_child) = last_child.children().pop() {
        last_child = new_last_child;
    }
    last_child
}

// Helper functions for futzing with RoundedRectRadii. These should probably be in kurbo.

fn radii_map(radii: RoundedRectRadii, f: impl Fn(f64) -> f64) -> RoundedRectRadii {
    RoundedRectRadii {
        top_left: f(radii.top_left),
        top_right: f(radii.top_right),
        bottom_left: f(radii.bottom_left),
        bottom_right: f(radii.bottom_right),
    }
}

pub(crate) const fn radii_max(radii: RoundedRectRadii) -> f64 {
    radii
        .top_left
        .max(radii.top_right)
        .max(radii.bottom_left)
        .max(radii.bottom_right)
}

fn radii_add(radii: RoundedRectRadii, offset: f64) -> RoundedRectRadii {
    radii_map(radii, |r| r + offset)
}
