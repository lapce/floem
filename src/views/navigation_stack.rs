#![deny(missing_docs)]
//! Navigation stack view driven by an explicit path.
//!
//! `NavigationStack` is Floem's path-based navigation primitive. The caller owns
//! the navigation path and the stack reacts to that path over time, retaining
//! views for the shared prefix and rebuilding only the changed tail.
//!
//! The API is intentionally builder-first:
//! - [`NavigationStack::new`] accepts a path source
//! - [`NavigationStackBuilder::key`] is optional when the item itself can be used
//!   as the stable identity
//! - [`NavigationStackBuilder::view`] is optional when the item itself implements
//!   [`IntoView`]
//!
//! This means the common cases read naturally:
//!
//! ```rust
//! # use floem::prelude::*;
//! # use floem::views::NavigationStack;
//! # #[derive(Clone, Hash, Eq, PartialEq)]
//! # struct Route;
//! # impl IntoView for Route {
//! #     type V = floem::AnyView;
//! #     type Intermediate = floem::view::LazyView<Route>;
//! #     fn into_intermediate(self) -> Self::Intermediate { floem::view::LazyView::new(self) }
//! # }
//! let path = RwSignal::new(vec![Route]);
//!
//! let stack = NavigationStack::new(path);
//! ```
//!
//! Or with explicit identity and destination view construction:
//!
//! ```rust
//! # use floem::prelude::*;
//! # use floem::views::NavigationStack;
//! # #[derive(Clone)]
//! # enum Route { Home, Detail(u64) }
//! let path = RwSignal::new(vec![Route::Home]);
//!
//! let stack = NavigationStack::new(path)
//!     .key(|route| match route {
//!         Route::Home => "home".to_string(),
//!         Route::Detail(id) => format!("detail:{id}"),
//!     })
//!     .view(|route| match route {
//!         Route::Home => "Home".into_any(),
//!         Route::Detail(id) => format!("Detail {id}").into_any(),
//!     });
//! ```
//!
//! ## Path semantics
//!
//! The path is ordered root to top:
//! - `[]` means no visible destination
//! - `[A]` shows `A`
//! - `[A, B, C]` shows `C`
//!
//! On updates, the stack compares the old and new path item keys:
//! - matching prefix entries are retained
//! - diverging entries and their descendants are disposed
//! - new trailing entries are created
//!
//! This preserves local view state for retained destinations, which is the main
//! reason to model navigation as a stack rather than rebuilding a single current
//! destination every update.
//!
//! ## Path sources
//!
//! [`NavigationPath`] intentionally accepts several caller shapes:
//! - plain `Vec<T>`
//! - closures returning an iterable path
//! - `ReadSignal<P>`
//! - `RwSignal<P>`
//!
//! Signal-backed paths currently materialize owned path items on each read,
//! because the navigation view update must own the items it compares and may
//! need to construct new child views from them. Internally the module uses a
//! `SmallVec` for the common short-path case.
//!
//! ## Manual refresh
//!
//! Most callers should rely on reactive path sources. For non-reactive sources,
//! or when a caller wants to force a resync, send
//! [`RefreshNavigationStackPath`] to the stack's [`ViewId`].
//!
// TODO: Tighten naming and lift NavigationPath into a reusable collection-source
// abstraction shared by NavigationStack, Tab, and VirtualStack.

use std::{any::Any, hash::Hash, marker::PhantomData, rc::Rc};

use floem_reactive::{Effect, ReadSignal, RwSignal, Scope, SignalWith};
use smallvec::SmallVec;

use crate::{
    context::{StyleCx, UpdateCx},
    style::recalc::StyleReason,
    view::{HasViewId, IntoView, View, ViewId},
};

type ViewFn<T> = Rc<dyn Fn(T) -> (Box<dyn View>, Scope)>;
type PathItems<T> = SmallVec<[T; 8]>;
type PathReader<T> = Rc<dyn Fn() -> PathItems<T>>;
type KeyFn<T> = Rc<dyn Fn(&T) -> Box<dyn ErasedNavKey>>;

/// Marker type for navigation stacks that use the item itself as the key.
pub struct AutoNavigationKey;

/// Marker type for navigation stacks that use an explicit key function.
pub struct CustomNavigationKey<T> {
    key_fn: KeyFn<T>,
}

/// Marker type for navigation stacks that use the item itself as the destination view.
pub struct AutoNavigationView;

/// Marker type for navigation stacks that use an explicit destination view function.
pub struct CustomNavigationView<T> {
    view_fn: ViewFn<T>,
}

/// Type-erased equality key used to compare retained path prefixes across updates.
///
/// `NavigationStack` is generic over the path item `T`, but not over the user's
/// chosen key type `K`. The builder therefore erases `K` behind this trait and
/// only retains the equality operation needed for prefix reuse.
trait ErasedNavKey: Any {
    fn as_any(&self) -> &dyn Any;
    fn equals(&self, other: &dyn ErasedNavKey) -> bool;
}

impl<K> ErasedNavKey for K
where
    K: Eq + 'static,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn equals(&self, other: &dyn ErasedNavKey) -> bool {
        other.as_any().downcast_ref::<K>() == Some(self)
    }
}

/// Retained navigation entry for one path segment.
///
/// The stack keeps the child `ViewId` and `Scope` alive while that segment
/// remains in the shared path prefix, and drops both once the segment falls
/// out of the stack.
struct NavChild {
    key: Box<dyn ErasedNavKey>,
    id: ViewId,
    scope: Scope,
}

enum NavigationStackState<T> {
    SetPath(PathItems<T>),
}

/// Message that can be sent to a [`NavigationStack`] [`ViewId`] to force the stack
/// to re-read its current path source.
pub struct RefreshNavigationStackPath;

/// A path source that can produce the current navigation path on demand.
///
/// This supports plain vectors, closures, and reactive signals whose values can
/// be converted into iterators of path items.
pub trait NavigationPath<T>: 'static {
    /// Convert this source into an owned path reader.
    fn into_path_reader(self) -> PathReader<T>;
}

impl<T> NavigationPath<T> for Vec<T>
where
    T: Clone + 'static,
{
    fn into_path_reader(self) -> PathReader<T> {
        Rc::new(move || self.iter().cloned().collect())
    }
}

impl<T, F, I> NavigationPath<T> for F
where
    F: Fn() -> I + 'static,
    I: IntoIterator<Item = T>,
{
    fn into_path_reader(self) -> PathReader<T> {
        Rc::new(move || self().into_iter().collect::<PathItems<_>>())
    }
}

impl<T, P, S> NavigationPath<T> for ReadSignal<P, S>
where
    ReadSignal<P, S>: SignalWith<P>,
    P: 'static,
    for<'a> &'a P: IntoIterator<Item = &'a T>,
    S: 'static,
    T: Clone + 'static,
{
    fn into_path_reader(self) -> PathReader<T> {
        Rc::new(move || self.with(|path| path.into_iter().cloned().collect::<PathItems<_>>()))
    }
}

impl<T, P, S> NavigationPath<T> for RwSignal<P, S>
where
    RwSignal<P, S>: SignalWith<P>,
    P: 'static,
    for<'a> &'a P: IntoIterator<Item = &'a T>,
    S: 'static,
    T: Clone + 'static,
{
    fn into_path_reader(self) -> PathReader<T> {
        Rc::new(move || self.with(|path| path.into_iter().cloned().collect::<PathItems<_>>()))
    }
}

/// Builder for a [`NavigationStack`].
///
/// This builder supports two optional pieces of configuration:
/// - [`Self::key`] supplies stable identity when the path item itself should not
///   be used as the key
/// - [`Self::view`] supplies destination view construction when the path item
///   itself does not implement [`IntoView`]
pub struct NavigationStackBuilder<T, K = AutoNavigationKey, V = AutoNavigationView>
where
    T: 'static,
{
    id: ViewId,
    path_reader: PathReader<T>,
    key: K,
    view: V,
}

impl<T> NavigationStackBuilder<T, AutoNavigationKey, AutoNavigationView>
where
    T: 'static,
{
    fn new<P>(path: P) -> Self
    where
        P: NavigationPath<T>,
    {
        Self {
            id: ViewId::new(),
            path_reader: path.into_path_reader(),
            key: AutoNavigationKey,
            view: AutoNavigationView,
        }
    }
}

impl<T, K, V> NavigationStackBuilder<T, K, V>
where
    T: 'static,
{
    /// Provide a stable key function for navigation path items.
    ///
    /// Use this when `T` does not implement `Eq + Hash`, or when the path item
    /// contains additional data that should not participate in destination
    /// identity.
    pub fn key<KF, Key>(self, key_fn: KF) -> NavigationStackBuilder<T, CustomNavigationKey<T>, V>
    where
        KF: Fn(&T) -> Key + 'static,
        Key: Eq + Hash + 'static,
    {
        NavigationStackBuilder {
            id: self.id,
            path_reader: self.path_reader,
            key: CustomNavigationKey {
                key_fn: Rc::new(move |item| Box::new(key_fn(item))),
            },
            view: self.view,
        }
    }

    /// Provide an explicit destination view function for navigation path items.
    ///
    /// Use this when `T` is route data rather than a view in its own right.
    pub fn view<VF, IV>(self, view_fn: VF) -> NavigationStackBuilder<T, K, CustomNavigationView<T>>
    where
        VF: Fn(T) -> IV + 'static,
        IV: IntoView + 'static,
    {
        NavigationStackBuilder {
            id: self.id,
            path_reader: self.path_reader,
            key: self.key,
            view: CustomNavigationView {
                view_fn: Rc::new(
                    Scope::current().enter_child(move |item| view_fn(item).into_any()),
                ),
            },
        }
    }
}

impl<T, K, V> HasViewId for NavigationStackBuilder<T, K, V>
where
    T: 'static,
{
    fn view_id(&self) -> ViewId {
        self.id
    }
}

impl<T> IntoView for NavigationStackBuilder<T, AutoNavigationKey, AutoNavigationView>
where
    T: Eq + Hash + Clone + IntoView + 'static,
{
    type V = NavigationStack<T>;
    type Intermediate = Self;

    fn into_intermediate(self) -> Self::Intermediate {
        self
    }

    fn into_view(self) -> Self::V {
        NavigationStack::from_parts(
            self.id,
            self.path_reader,
            Rc::new(|item: &T| Box::new(item.clone())),
            Rc::new(Scope::current().enter_child(move |item: T| item.into_any())),
        )
    }
}

impl<T> IntoView for NavigationStackBuilder<T, AutoNavigationKey, CustomNavigationView<T>>
where
    T: Eq + Hash + Clone + 'static,
{
    type V = NavigationStack<T>;
    type Intermediate = Self;

    fn into_intermediate(self) -> Self::Intermediate {
        self
    }

    fn into_view(self) -> Self::V {
        NavigationStack::from_parts(
            self.id,
            self.path_reader,
            Rc::new(|item: &T| Box::new(item.clone())),
            self.view.view_fn,
        )
    }
}

impl<T> IntoView for NavigationStackBuilder<T, CustomNavigationKey<T>, AutoNavigationView>
where
    T: IntoView + 'static,
{
    type V = NavigationStack<T>;
    type Intermediate = Self;

    fn into_intermediate(self) -> Self::Intermediate {
        self
    }

    fn into_view(self) -> Self::V {
        NavigationStack::from_parts(
            self.id,
            self.path_reader,
            self.key.key_fn,
            Rc::new(Scope::current().enter_child(move |item: T| item.into_any())),
        )
    }
}

impl<T> IntoView for NavigationStackBuilder<T, CustomNavigationKey<T>, CustomNavigationView<T>>
where
    T: 'static,
{
    type V = NavigationStack<T>;
    type Intermediate = Self;

    fn into_intermediate(self) -> Self::Intermediate {
        self
    }

    fn into_view(self) -> Self::V {
        NavigationStack::from_parts(
            self.id,
            self.path_reader,
            self.key.key_fn,
            self.view.view_fn,
        )
    }
}

/// A navigation view that retains a stack of destination views for an explicit path.
///
/// `NavigationStack` is designed for hierarchical navigation where the caller
/// owns the full path and updates it over time. The path is interpreted from
/// root to top:
/// - `[A]` shows `A`
/// - `[A, B]` shows `B`
/// - `[A, B, C]` shows `C`
///
/// On each update, the stack compares the old and new path using stable keys:
/// - the shared prefix is retained
/// - any diverging tail is removed and disposed
/// - any new tail is created
///
/// This means destination views in the shared prefix keep their local view
/// state while deeper navigation changes happen above them.
///
/// `NavigationStack` is intentionally built through [`NavigationStack::new`],
/// which returns a builder. That builder lets callers choose whether to:
/// - use the path item itself as the stable key, or provide [`NavigationStackBuilder::key`]
/// - use the path item itself as the destination view, or provide [`NavigationStackBuilder::view`]
///
/// Typical usage when the route type implements both [`IntoView`] and
/// `Eq + Hash + Clone`:
///
/// ```rust
/// # use floem::prelude::*;
/// # use floem::views::NavigationStack;
/// # #[derive(Clone, Eq, PartialEq, Hash)]
/// # struct Route;
/// # impl IntoView for Route {
/// #     type V = floem::AnyView;
/// #     type Intermediate = floem::view::LazyView<Route>;
/// #     fn into_intermediate(self) -> Self::Intermediate {
/// #         floem::view::LazyView::new(self)
/// #     }
/// # }
/// let path = RwSignal::new(vec![Route]);
///
/// let stack = NavigationStack::new(path);
/// ```
///
/// Typical usage when route data needs explicit keying and destination
/// construction:
///
/// ```rust
/// # use floem::prelude::*;
/// # use floem::views::NavigationStack;
/// # #[derive(Clone)]
/// # enum Route { Home, Detail(u64) }
/// let path = RwSignal::new(vec![Route::Home]);
///
/// let stack = NavigationStack::new(path)
///     .key(|route| match route {
///         Route::Home => "home".to_string(),
///         Route::Detail(id) => format!("detail:{id}"),
///     })
///     .view(|route| match route {
///         Route::Home => "Home",
///         Route::Detail(id) => format!("Detail {id}"),
///     });
/// ```
pub struct NavigationStack<T>
where
    T: 'static,
{
    id: ViewId,
    path_reader: PathReader<T>,
    key_fn: KeyFn<T>,
    view_fn: ViewFn<T>,
    children: Vec<NavChild>,
    phantom: PhantomData<T>,
}

impl<T> NavigationStack<T>
where
    T: 'static,
{
    /// Start building a [`NavigationStack`] from a path source.
    ///
    /// This is the single entrypoint for the navigation stack API. It accepts a
    /// path source and returns a [`NavigationStackBuilder`], which can then be
    /// used in one of two ways:
    ///
    /// 1. Build directly when `T` already provides everything the stack needs:
    ///    - `T: IntoView` for destination construction
    ///    - `T: Eq + Hash + Clone` for stable destination identity
    /// 2. Refine the builder with:
    ///    - [`NavigationStackBuilder::key`] when identity should come from a
    ///      derived key rather than the whole item
    ///    - [`NavigationStackBuilder::view`] when the path item is route data
    ///      rather than a view
    ///
    /// The `path` argument can be any supported [`NavigationPath`], including:
    /// - a plain `Vec<T>`
    /// - a closure returning an iterable path
    /// - `ReadSignal<P>`
    /// - `RwSignal<P>`
    ///
    /// The stack reads the full path and retains destination views for the
    /// shared prefix across updates.
    ///
    /// # Example: route items are their own views
    ///
    /// ```rust
    /// # use floem::prelude::*;
    /// # use floem::views::NavigationStack;
    /// # #[derive(Clone, Eq, PartialEq, Hash)]
    /// # struct Route;
    /// # impl IntoView for Route {
    /// #     type V = floem::AnyView;
    /// #     type Intermediate = floem::view::LazyView<Route>;
    /// #     fn into_intermediate(self) -> Self::Intermediate {
    /// #         floem::view::LazyView::new(self)
    /// #     }
    /// # }
    /// let path = RwSignal::new(vec![Route]);
    ///
    /// let stack = NavigationStack::new(path);
    /// ```
    ///
    /// # Example: route items need explicit key and view functions
    ///
    /// ```rust
    /// # use floem::prelude::*;
    /// # use floem::views::NavigationStack;
    /// # #[derive(Clone)]
    /// # enum Route { Home, Detail(u64) }
    /// let path = RwSignal::new(vec![Route::Home]);
    ///
    /// let stack = NavigationStack::new(path)
    ///     .key(|route| match route {
    ///         Route::Home => "home".to_string(),
    ///         Route::Detail(id) => format!("detail:{id}"),
    ///     })
    ///     .view(|route| match route {
    ///         Route::Home => "Home",
    ///         Route::Detail(id) => format!("Detail {id}"),
    ///     });
    /// ```
    #[allow(clippy::new_ret_no_self)]
    pub fn new<P>(path: P) -> NavigationStackBuilder<T>
    where
        P: NavigationPath<T>,
    {
        NavigationStackBuilder::new(path)
    }

    fn from_parts(
        id: ViewId,
        path_reader: PathReader<T>,
        key_fn: KeyFn<T>,
        view_fn: ViewFn<T>,
    ) -> Self {
        let effect_path_reader = path_reader.clone();

        Effect::new(move |_| {
            id.update_state(NavigationStackState::SetPath(effect_path_reader()));
        });

        Self {
            id,
            path_reader,
            key_fn,
            view_fn,
            children: Vec::new(),
            phantom: PhantomData,
        }
    }

    // Reconcile the retained child stack with the current path.
    fn sync_path(&mut self, cx: &mut UpdateCx, path: PathItems<T>) {
        let mut keyed_items = path
            .into_iter()
            .map(|item| ((self.key_fn)(&item), item))
            .collect::<SmallVec<[(_, _); 8]>>();

        let shared_prefix = self
            .children
            .iter()
            .zip(keyed_items.iter())
            .take_while(|(existing, incoming)| existing.key.equals(&*incoming.0))
            .count();

        while self.children.len() > shared_prefix {
            if let Some(child) = self.children.pop() {
                cx.window_state.remove_view(child.id);
                child.scope.dispose();
            }
        }

        for (key, item) in keyed_items.drain(shared_prefix..) {
            let (view, scope) = (self.view_fn)(item);
            let child_id = view.id();
            child_id.set_view(view);
            child_id.set_parent(self.id);
            self.children.push(NavChild {
                key,
                id: child_id,
                scope,
            });
        }

        self.id
            .set_children_ids(self.children.iter().map(|child| child.id).collect());
        self.id.request_style(StyleReason::style_pass());
    }
}

impl<T> View for NavigationStack<T>
where
    T: 'static,
{
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "NavigationStack".into()
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) {
        match state.downcast::<NavigationStackState<T>>() {
            Ok(state) => match *state {
                NavigationStackState::SetPath(path) => self.sync_path(cx, path),
            },
            Err(state) => {
                if state.downcast::<RefreshNavigationStackPath>().is_ok() {
                    self.sync_path(cx, (self.path_reader)());
                }
            }
        }
    }

    fn style_pass(&mut self, _cx: &mut StyleCx<'_>) {
        let last = self.children.len().checked_sub(1);
        for (index, child) in self.id.children().into_iter().enumerate() {
            if Some(index) == last {
                child.set_visible();
            } else {
                child.set_hidden();
            }
        }
    }
}
