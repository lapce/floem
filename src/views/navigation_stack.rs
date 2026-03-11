#![deny(missing_docs)]
// TODO: Tighten naming and lift NavigationPath into a reusable collection-source
// abstraction shared by NavigationStack, Tab, and VirtualStack.

use std::{any::Any, hash::Hash, marker::PhantomData, rc::Rc};

use floem_reactive::{Effect, ReadSignal, RwSignal, Scope, SignalGet};

use crate::{
    context::{StyleCx, UpdateCx},
    style::recalc::StyleReason,
    view::{IntoView, View, ViewId},
};

type ViewFn<T> = Rc<dyn Fn(T) -> (Box<dyn View>, Scope)>;
type PathReader<T> = Rc<dyn Fn() -> Vec<T>>;
type KeyFn<T> = Rc<dyn Fn(&T) -> Box<dyn NavigationKey>>;

trait NavigationKey: Any {
    fn as_any(&self) -> &dyn Any;
    fn equals(&self, other: &dyn NavigationKey) -> bool;
}

impl<K> NavigationKey for K
where
    K: Eq + 'static,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn equals(&self, other: &dyn NavigationKey) -> bool {
        other.as_any().downcast_ref::<K>() == Some(self)
    }
}

struct NavChild {
    key: Box<dyn NavigationKey>,
    id: ViewId,
    scope: Scope,
}

struct SetPath<T>(Vec<T>);

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
        let items = Rc::new(self);
        Rc::new(move || items.iter().cloned().collect())
    }
}

impl<T, F, I> NavigationPath<T> for F
where
    F: Fn() -> I + 'static,
    I: IntoIterator<Item = T>,
{
    fn into_path_reader(self) -> PathReader<T> {
        Rc::new(move || self().into_iter().collect())
    }
}

impl<T, P, S> NavigationPath<T> for ReadSignal<P, S>
where
    ReadSignal<P, S>: SignalGet<P>,
    P: IntoIterator<Item = T> + Clone + 'static,
    S: 'static,
    T: 'static,
{
    fn into_path_reader(self) -> PathReader<T> {
        Rc::new(move || self.get().into_iter().collect())
    }
}

impl<T, P, S> NavigationPath<T> for RwSignal<P, S>
where
    RwSignal<P, S>: SignalGet<P>,
    P: IntoIterator<Item = T> + Clone + 'static,
    S: 'static,
    T: 'static,
{
    fn into_path_reader(self) -> PathReader<T> {
        Rc::new(move || self.get().into_iter().collect())
    }
}

/// Builder for a [`NavigationStack`] that allows a custom key function to be supplied.
pub struct NavigationStackBuilder<T>
where
    T: 'static,
{
    path_reader: PathReader<T>,
    view_fn: ViewFn<T>,
    key_fn: Option<KeyFn<T>>,
}

impl<T> NavigationStackBuilder<T>
where
    T: 'static,
{
    fn new<P, VF, V>(path: P, view_fn: VF) -> Self
    where
        P: NavigationPath<T>,
        VF: Fn(T) -> V + 'static,
        V: IntoView + 'static,
    {
        Self {
            path_reader: path.into_path_reader(),
            view_fn: Rc::new(Scope::current().enter_child(move |item| view_fn(item).into_any())),
            key_fn: None,
        }
    }

    /// Provide a stable key function for navigation path items.
    pub fn key<KF, K>(mut self, key_fn: KF) -> Self
    where
        KF: Fn(&T) -> K + 'static,
        K: Eq + Hash + 'static,
    {
        self.key_fn = Some(Rc::new(move |item| Box::new(key_fn(item))));
        self
    }

    /// Build the final [`NavigationStack`].
    pub fn build(self) -> NavigationStack<T> {
        NavigationStack::from_parts(
            self.path_reader,
            self.key_fn
                .expect("NavigationStack::builder(...).build() requires .key(...) unless T implements Eq + Hash + Clone and NavigationStack::new(...) is used"),
            self.view_fn,
        )
    }
}

impl<T> NavigationStackBuilder<T>
where
    T: Eq + Hash + Clone + 'static,
{
    fn build_auto(self) -> NavigationStack<T> {
        NavigationStack::from_parts(
            self.path_reader,
            Rc::new(|item: &T| Box::new(item.clone())),
            self.view_fn,
        )
    }
}

/// A stack of views driven by an explicit navigation path.
///
/// The path is interpreted from root to top. Shared prefixes are retained across
/// updates; any segment after the first changed key is recreated.
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
    /// Create a navigation stack builder from a path source and a destination view builder.
    pub fn new<P, VF, V>(path: P, view_fn: VF) -> NavigationStackBuilder<T>
    where
        P: NavigationPath<T>,
        VF: Fn(T) -> V + 'static,
        V: IntoView + 'static,
    {
        NavigationStackBuilder::new(path, view_fn)
    }

    /// Create a navigation stack that uses the path item itself as the stable key.
    ///
    /// This requires `T` to implement `Eq`, `Hash`, and `Clone`.
    pub fn auto<P, VF, V>(path: P, view_fn: VF) -> Self
    where
        P: NavigationPath<T>,
        VF: Fn(T) -> V + 'static,
        V: IntoView + 'static,
        T: Eq + Hash + Clone + 'static,
    {
        Self::new(path, view_fn).build_auto()
    }

    fn from_parts(path_reader: PathReader<T>, key_fn: KeyFn<T>, view_fn: ViewFn<T>) -> Self {
        let id = ViewId::new();
        let effect_path_reader = path_reader.clone();

        Effect::new(move |_| {
            id.update_state(SetPath(effect_path_reader()));
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

    fn sync_path(&mut self, cx: &mut UpdateCx, path: Vec<T>) {
        let mut keyed_items = path
            .into_iter()
            .map(|item| ((self.key_fn)(&item), item))
            .collect::<Vec<_>>();

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
        self.id.request_all();
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
        match state.downcast::<SetPath<T>>() {
            Ok(state) => {
                self.sync_path(cx, state.0);
            }
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
