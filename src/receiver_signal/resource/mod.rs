//! Reactive resources that automatically fetch data when dependencies change.

mod builder;

use super::common::{EventLoopExecutor, NoInitial};
use builder::ResourceBuilder;
use floem_reactive::ReadSignal;
use std::hash::Hash;

// Resource-specific type-state markers
#[doc(hidden)]
pub struct DefaultHashKeyFn;
#[doc(hidden)]
pub struct CustomKeyFn<F>(pub F);
#[doc(hidden)]
pub struct WithMemo;
#[doc(hidden)]
pub struct NoMemoization;

/// A reactive resource that automatically fetches data when its dependencies change.
///
/// `Resource` provides a way to reactively fetch data from async operations and automatically
/// refetch when the input dependencies change. It manages loading states and ensures that only the
/// most recent result is kept if multiple async operations complete rapidly.
///
/// # Concurrency Behavior
///
/// If multiple async operations complete while the main thread is busy processing other work,
/// only the most recent result is kept.
///
/// # Examples
///
/// ```rust,ignore
/// // Simple case: event loop executor, hash-based memoization, returns Option<T>
/// let resource = Resource::new(|| user_id.get(), |id| fetch_user(id));
///
/// // With initial value
/// let resource = Resource::with_initial(
///     || user_id.get(),
///     |id| fetch_user(id),
///     User::default()
/// );
///
/// // Full customization
/// let resource = Resource::custom(|| user_id.get(), |id| fetch_user(id))
///     .executor(my_executor)
///     .key_fn(|id| *id)
///     .initial(User::default())
///     .build();
/// ```
pub struct Resource<T> {
    /// The signal containing the fetched data.
    pub(super) data: ReadSignal<T>,
    /// The signal indicating whether an async fetch operation is currently in progress.
    pub(super) finished: ReadSignal<bool>,
    /// A trigger for manually rerunning the `fetcher` with the last used `source`.
    pub(super) refetch_trigger: floem_reactive::RwSignal<u64>,
}

impl<T> Copy for Resource<T> {}

impl<T> Clone for Resource<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Resource<T> {
    /// Creates a new reactive resource with sensible defaults.
    ///
    /// - **Executor**: Main event loop (future does not need to be `Send`)
    /// - **Memoization**: Hash-based (only refetches when dependency hash changes)
    /// - **Initial value**: None (returns `Resource<Option<U>>`)
    ///
    /// # Parameters
    ///
    /// * `source` - A function that returns the current dependency value(s).
    /// * `fetcher` - An async function that takes the dependency value and returns the fetched data.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let user_resource = Resource::new(
    ///     || user_id.get(),
    ///     |id| async move { fetch_user(id).await }
    /// );
    /// ```
    pub fn new<Fut, Dep>(
        source: impl Fn() -> Dep + 'static,
        fetcher: impl Fn(Dep) -> Fut + 'static,
    ) -> Resource<Option<T>>
    where
        Fut: std::future::Future<Output = T> + 'static,
        Dep: Hash + 'static,
    {
        ResourceBuilder::new(source, fetcher).build()
    }

    /// Creates a new reactive resource with an initial value.
    ///
    /// Uses the same defaults as `new()` but allows you to specify an initial value
    /// so the resource is never `None`.
    ///
    /// - **Executor**: Main event loop (future does not need to be `Send`)
    /// - **Memoization**: Hash-based (only refetches when dependency hash changes)
    ///
    /// # Parameters
    ///
    /// * `source` - A function that returns the current dependency value(s).
    /// * `fetcher` - An async function that takes the dependency value and returns the fetched data.
    /// * `initial` - The initial value for the resource before any fetch completes.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let user_resource = Resource::with_initial(
    ///     || user_id.get(),
    ///     |id| async move { fetch_user(id).await },
    ///     User::default()
    /// );
    /// ```
    pub fn with_initial<Fut, Dep>(
        source: impl Fn() -> Dep + 'static,
        fetcher: impl Fn(Dep) -> Fut + 'static,
        initial: T,
    ) -> Self
    where
        T: 'static,
        Fut: std::future::Future<Output = T> + 'static,
        Dep: Hash + 'static,
    {
        ResourceBuilder::new(source, fetcher)
            .initial(initial)
            .build()
    }

    /// Creates a resource builder for full customization.
    ///
    /// Use this when you need to customize:
    /// - The executor (tokio, custom, etc.)
    /// - The key function (custom comparison logic)
    /// - Memoization behavior (disable with `.no_memo()`)
    /// - Initial value
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Tokio executor with custom key function
    /// let resource = Resource::custom(|| user_id.get(), |id| fetch_user(id))
    ///     .tokio_spawn()
    ///     .key_fn(|id| *id)
    ///     .build();
    ///
    /// // Disable memoization
    /// let resource = Resource::custom(|| (), || fetch_latest())
    ///     .no_memo()
    ///     .build();
    /// ```
    pub fn custom<Fut, Dep>(
        source: impl Fn() -> Dep + 'static,
        fetcher: impl Fn(Dep) -> Fut + 'static,
    ) -> ResourceBuilder<Fut, Dep, T, DefaultHashKeyFn, EventLoopExecutor, WithMemo, NoInitial>
    where
        Fut: std::future::Future<Output = T> + 'static,
        Dep: Hash + 'static,
    {
        ResourceBuilder::new(source, fetcher)
    }

    /// Manually triggers a refetch of the resource, bypassing memoization.
    ///
    /// This will start a new async fetch operation using the current
    /// dependency value, even if that value hasn't changed since the last fetch.
    ///
    /// # Behavior
    ///
    /// - Sets the finished state to `false`
    /// - Spawns a new fetch operation
    /// - Bypasses memoization for this fetch
    pub fn refetch(&self) {
        use floem_reactive::SignalUpdate;
        self.refetch_trigger.update(|count| *count += 1);
    }

    /// Returns `true` if an async fetch operation is currently in progress.
    ///
    /// This can be used to show loading indicators in the UI.
    pub fn is_loading(&self) -> bool
    where
        T: 'static,
    {
        use floem_reactive::SignalGet;
        !self.finished.get()
    }
}

impl<T> floem_reactive::SignalWith<T> for Resource<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.data.id()
    }
}

impl<T> floem_reactive::SignalRead<T> for Resource<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.data.id()
    }
}

impl<T: Clone> floem_reactive::SignalGet<T> for Resource<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.data.id()
    }
}

impl<T> floem_reactive::SignalTrack<T> for Resource<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.data.id()
    }
}
