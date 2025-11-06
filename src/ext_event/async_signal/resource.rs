use crate::ext_event::ExtSendTrigger;
use crate::ext_event::async_signal::{EventLoopExecutor, TokioExecutor, CustomExecutor, NoInitial, WithInitialValue};
use floem_reactive::ReadSignal;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

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
    data: ReadSignal<T>,
    /// The signal indicating whether an async fetch operation is currently in progress.
    finished: ReadSignal<bool>,
    /// A trigger for manually rerunning the `fetcher` with the last used `source`.
    refetch_trigger: floem_reactive::RwSignal<u64>,
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

// ============================================================================
// Builder API
// ============================================================================

/// A builder for creating customized `Resource` instances.
///
/// Created via `Resource::custom()`. Allows fine-grained control over:
/// - Executor type (event loop, tokio, custom)
/// - Key function (for memoization)
/// - Memoization behavior
/// - Initial value
pub struct ResourceBuilder<Fut, Dep, T, K, E, M, I> {
    source: Box<dyn Fn() -> Dep + 'static>,
    fetcher: Box<dyn Fn(Dep) -> Fut + 'static>,
    key_fn: K,
    executor: E,
    initial: I,
    _phantom: std::marker::PhantomData<(T, M)>,
}

// Type-state markers specific to Resource
pub struct DefaultHashKeyFn;
pub struct CustomKeyFn<F>(F);
pub struct WithMemo;
pub struct NoMemoization;

impl<Fut, Dep, T>
    ResourceBuilder<Fut, Dep, T, DefaultHashKeyFn, EventLoopExecutor, WithMemo, NoInitial>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
    Dep: Hash + 'static,
{
    fn new(source: impl Fn() -> Dep + 'static, fetcher: impl Fn(Dep) -> Fut + 'static) -> Self {
        Self {
            source: Box::new(source),
            fetcher: Box::new(fetcher),
            key_fn: DefaultHashKeyFn,
            executor: EventLoopExecutor,
            initial: NoInitial,
            _phantom: std::marker::PhantomData,
        }
    }
}

// Builder methods for customization
impl<Fut, Dep, T, K, E, M, I> ResourceBuilder<Fut, Dep, T, K, E, M, I>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
    Dep: 'static,
{
    /// Use a custom key function for memoization instead of hashing.
    ///
    /// The resource will only refetch when the key changes (using `PartialEq`).
    #[allow(clippy::type_complexity)]
    pub fn key_fn<NewK>(
        self,
        key_fn: impl Fn(&Dep) -> NewK + 'static,
    ) -> ResourceBuilder<Fut, Dep, T, CustomKeyFn<Box<dyn Fn(&Dep) -> NewK + 'static>>, E, M, I>
    where
        NewK: PartialEq + 'static,
    {
        ResourceBuilder {
            source: self.source,
            fetcher: self.fetcher,
            key_fn: CustomKeyFn(Box::new(key_fn)),
            executor: self.executor,
            initial: self.initial,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Use the main event loop as the executor.
    ///
    /// The future does not need to be `Send`. This is the default.
    pub fn event_loop(self) -> ResourceBuilder<Fut, Dep, T, K, EventLoopExecutor, M, I> {
        ResourceBuilder {
            source: self.source,
            fetcher: self.fetcher,
            key_fn: self.key_fn,
            executor: EventLoopExecutor,
            initial: self.initial,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Use tokio::spawn as the executor.
    ///
    /// The future must be `Send + 'static`. Requires the `tokio` feature.
    #[cfg(feature = "tokio")]
    pub fn tokio_spawn(self) -> ResourceBuilder<Fut, Dep, T, K, TokioExecutor, M, I>
    where
        T: Send,
        Fut: Send,
        Dep: Send,
    {
        ResourceBuilder {
            source: self.source,
            fetcher: self.fetcher,
            key_fn: self.key_fn,
            executor: TokioExecutor,
            initial: self.initial,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Use a custom executor function.
    ///
    /// The executor receives the future and signals to update when complete.
    pub fn executor<F>(
        self,
        executor: F,
    ) -> ResourceBuilder<Fut, Dep, T, K, CustomExecutor<F>, M, I>
    where
        F: Fn(
                Fut,
                floem_reactive::WriteSignal<T>,
                floem_reactive::WriteSignal<bool>,
                ExtSendTrigger,
            ) + 'static,
    {
        ResourceBuilder {
            source: self.source,
            fetcher: self.fetcher,
            key_fn: self.key_fn,
            executor: CustomExecutor(executor),
            initial: self.initial,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Disable memoization.
    ///
    /// The fetcher will run every time the source function is called,
    /// regardless of whether the dependency value has changed.
    pub fn no_memo(self) -> ResourceBuilder<Fut, Dep, T, K, E, NoMemoization, I> {
        ResourceBuilder {
            source: self.source,
            fetcher: self.fetcher,
            key_fn: self.key_fn,
            executor: self.executor,
            initial: self.initial,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Set an initial value for the resource.
    ///
    /// Without this, the resource returns `Option<T>` and starts as `None`.
    /// With this, the resource returns `T` and starts with your initial value.
    pub fn initial(self, initial: T) -> ResourceBuilder<Fut, Dep, T, K, E, M, WithInitialValue<T>> {
        ResourceBuilder {
            source: self.source,
            fetcher: self.fetcher,
            key_fn: self.key_fn,
            executor: self.executor,
            initial: WithInitialValue(initial),
            _phantom: std::marker::PhantomData,
        }
    }
}

// There is probably some way to reduce the code duplication below... but honestly this is fine. If you read this and get the itch to improve it, please do.

// Build with default hash key, event loop, with memo, no initial -> Resource<Option<T>>
impl<Fut, Dep, T>
    ResourceBuilder<Fut, Dep, T, DefaultHashKeyFn, EventLoopExecutor, WithMemo, NoInitial>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
    Dep: Hash + 'static,
{
    pub fn build(self) -> Resource<Option<T>> {
        let key_fn = |dep: &Dep| {
            let mut hasher = DefaultHasher::new();
            dep.hash(&mut hasher);
            hasher.finish()
        };

        build_resource_with_key(
            self.source,
            key_fn,
            self.fetcher,
            super::event_loop_future_option,
        )
    }
}

// Build with default hash key, event loop, with memo, with initial -> Resource<T>
impl<Fut, Dep, T>
    ResourceBuilder<Fut, Dep, T, DefaultHashKeyFn, EventLoopExecutor, WithMemo, WithInitialValue<T>>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
    Dep: Hash + 'static,
{
    pub fn build(self) -> Resource<T> {
        let key_fn = |dep: &Dep| {
            let mut hasher = DefaultHasher::new();
            dep.hash(&mut hasher);
            hasher.finish()
        };

        build_resource_with_key_and_initial(
            self.source,
            key_fn,
            self.fetcher,
            super::event_loop_future,
            self.initial.0,
        )
    }
}

// Build with custom key fn, event loop, with memo, no initial -> Resource<Option<T>>
impl<Fut, Dep, T, KeyFn, NewK>
    ResourceBuilder<Fut, Dep, T, CustomKeyFn<KeyFn>, EventLoopExecutor, WithMemo, NoInitial>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
    Dep: 'static,
    NewK: PartialEq + 'static,
    KeyFn: Fn(&Dep) -> NewK + 'static,
{
    pub fn build(self) -> Resource<Option<T>> {
        let key_fn = self.key_fn.0;
        build_resource_with_key(
            self.source,
            move |dep: &Dep| key_fn(dep),
            self.fetcher,
            super::event_loop_future_option,
        )
    }
}

// Build with custom key fn, event loop, with memo, with initial -> Resource<T>
impl<Fut, Dep, T, KeyFn, NewK>
    ResourceBuilder<
        Fut,
        Dep,
        T,
        CustomKeyFn<KeyFn>,
        EventLoopExecutor,
        WithMemo,
        WithInitialValue<T>,
    >
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
    Dep: 'static,
    NewK: PartialEq + 'static,
    KeyFn: Fn(&Dep) -> NewK + 'static,
{
    pub fn build(self) -> Resource<T> {
        let key_fn = self.key_fn.0;
        build_resource_with_key_and_initial(
            self.source,
            move |dep: &Dep| key_fn(dep),
            self.fetcher,
            super::event_loop_future,
            self.initial.0,
        )
    }
}

// Build with event loop, no memo, no initial -> Resource<Option<T>>
impl<Fut, Dep, T, K> ResourceBuilder<Fut, Dep, T, K, EventLoopExecutor, NoMemoization, NoInitial>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
    Dep: 'static,
{
    pub fn build(self) -> Resource<Option<T>> {
        build_resource_no_memo(self.source, self.fetcher, super::event_loop_future_option)
    }
}

// Build with event loop, no memo, with initial -> Resource<T>
impl<Fut, Dep, T, K>
    ResourceBuilder<Fut, Dep, T, K, EventLoopExecutor, NoMemoization, WithInitialValue<T>>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
    Dep: 'static,
{
    pub fn build(self) -> Resource<T> {
        build_resource_no_memo_with_initial(
            self.source,
            self.fetcher,
            super::event_loop_future,
            self.initial.0,
        )
    }
}

// Tokio builds
#[cfg(feature = "tokio")]
impl<Fut, Dep, T> ResourceBuilder<Fut, Dep, T, DefaultHashKeyFn, TokioExecutor, WithMemo, NoInitial>
where
    T: Send + 'static,
    Fut: std::future::Future<Output = T> + Send + 'static,
    Dep: Hash + Send + 'static,
{
    pub fn build(self) -> Resource<Option<T>> {
        let key_fn = |dep: &Dep| {
            let mut hasher = DefaultHasher::new();
            dep.hash(&mut hasher);
            hasher.finish()
        };

        build_resource_with_key(
            self.source,
            key_fn,
            self.fetcher,
            super::tokio_spawn_future_option,
        )
    }
}

#[cfg(feature = "tokio")]
impl<Fut, Dep, T>
    ResourceBuilder<Fut, Dep, T, DefaultHashKeyFn, TokioExecutor, WithMemo, WithInitialValue<T>>
where
    T: Send + 'static,
    Fut: std::future::Future<Output = T> + Send + 'static,
    Dep: Hash + Send + 'static,
{
    pub fn build(self) -> Resource<T> {
        let key_fn = |dep: &Dep| {
            let mut hasher = DefaultHasher::new();
            dep.hash(&mut hasher);
            hasher.finish()
        };

        build_resource_with_key_and_initial(
            self.source,
            key_fn,
            self.fetcher,
            super::tokio_spawn_future,
            self.initial.0,
        )
    }
}

#[cfg(feature = "tokio")]
impl<Fut, Dep, T, KeyFn, NewK>
    ResourceBuilder<Fut, Dep, T, CustomKeyFn<KeyFn>, TokioExecutor, WithMemo, NoInitial>
where
    T: Send + 'static,
    Fut: std::future::Future<Output = T> + Send + 'static,
    Dep: 'static,
    NewK: PartialEq + 'static,
    KeyFn: Fn(&Dep) -> NewK + 'static,
{
    pub fn build(self) -> Resource<Option<T>> {
        let key_fn = self.key_fn.0;
        build_resource_with_key(
            self.source,
            move |dep: &Dep| key_fn(dep),
            self.fetcher,
            super::tokio_spawn_future_option,
        )
    }
}

#[cfg(feature = "tokio")]
impl<Fut, Dep, T, KeyFn, NewK>
    ResourceBuilder<Fut, Dep, T, CustomKeyFn<KeyFn>, TokioExecutor, WithMemo, WithInitialValue<T>>
where
    T: Send + 'static,
    Fut: std::future::Future<Output = T> + Send + 'static,
    Dep: 'static,
    NewK: PartialEq + 'static,
    KeyFn: Fn(&Dep) -> NewK + 'static,
{
    pub fn build(self) -> Resource<T> {
        let key_fn = self.key_fn.0;
        build_resource_with_key_and_initial(
            self.source,
            move |dep: &Dep| key_fn(dep),
            self.fetcher,
            super::tokio_spawn_future,
            self.initial.0,
        )
    }
}

#[cfg(feature = "tokio")]
impl<Fut, Dep, T, K> ResourceBuilder<Fut, Dep, T, K, TokioExecutor, NoMemoization, NoInitial>
where
    T: Send + 'static,
    Fut: std::future::Future<Output = T> + Send + 'static,
    Dep: 'static,
{
    pub fn build(self) -> Resource<Option<T>> {
        build_resource_no_memo(self.source, self.fetcher, super::tokio_spawn_future_option)
    }
}

#[cfg(feature = "tokio")]
impl<Fut, Dep, T, K>
    ResourceBuilder<Fut, Dep, T, K, TokioExecutor, NoMemoization, WithInitialValue<T>>
where
    T: Send + 'static,
    Fut: std::future::Future<Output = T> + Send + 'static,
    Dep: 'static,
{
    pub fn build(self) -> Resource<T> {
        build_resource_no_memo_with_initial(
            self.source,
            self.fetcher,
            super::tokio_spawn_future,
            self.initial.0,
        )
    }
}

// Custom executor with no memo
impl<Fut, Dep, T, K, F> ResourceBuilder<Fut, Dep, T, K, CustomExecutor<F>, NoMemoization, NoInitial>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
    Dep: 'static,
    F: Fn(
            Fut,
            floem_reactive::WriteSignal<Option<T>>,
            floem_reactive::WriteSignal<bool>,
            ExtSendTrigger,
        ) + 'static,
{
    pub fn build(self) -> Resource<Option<T>> {
        build_resource_no_memo(self.source, self.fetcher, self.executor.0)
    }
}

// Custom executor with no memo and initial value
impl<Fut, Dep, T, K, F>
    ResourceBuilder<Fut, Dep, T, K, CustomExecutor<F>, NoMemoization, WithInitialValue<T>>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
    Dep: 'static,
    F: Fn(Fut, floem_reactive::WriteSignal<T>, floem_reactive::WriteSignal<bool>, ExtSendTrigger)
        + 'static,
{
    pub fn build(self) -> Resource<T> {
        build_resource_no_memo_with_initial(
            self.source,
            self.fetcher,
            self.executor.0,
            self.initial.0,
        )
    }
}

fn build_resource_with_key_and_initial<Fut, Dep, K, F, T>(
    source: Box<dyn Fn() -> Dep + 'static>,
    key_fn: impl Fn(&Dep) -> K + 'static,
    fetcher: Box<dyn Fn(Dep) -> Fut + 'static>,
    executor: F,
    initial: T,
) -> Resource<T>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
    Dep: 'static,
    K: PartialEq + 'static,
    F: Fn(Fut, floem_reactive::WriteSignal<T>, floem_reactive::WriteSignal<bool>, ExtSendTrigger)
        + 'static,
{
    use floem_reactive::{Scope, SignalGet, SignalUpdate, with_scope};

    let cx = Scope::current();
    let trigger = with_scope(cx, ExtSendTrigger::new);
    let (data_read, data_write) = cx.create_signal(initial);
    let (finished_read, finished_write) = cx.create_signal(false);
    let refetch_trigger = cx.create_rw_signal(0);

    cx.create_effect(move |last| {
        let refetch_count = refetch_trigger.get();
        let dep_value = source();
        let current_key = key_fn(&dep_value);

        let should_fetch = match last {
            None => true,
            Some((last_key, last_refetch_count)) => {
                last_key != current_key
                    || (last_refetch_count != refetch_count && refetch_count != 0)
            }
        };

        if !should_fetch {
            return (current_key, refetch_count);
        }

        finished_write.set(false);
        let future = fetcher(dep_value);
        executor(future, data_write, finished_write, trigger);

        (current_key, refetch_count)
    });

    Resource {
        data: data_read,
        finished: finished_read,
        refetch_trigger,
    }
}

fn build_resource_with_key<Fut, Dep, K, F, U>(
    source: Box<dyn Fn() -> Dep + 'static>,
    key_fn: impl Fn(&Dep) -> K + 'static,
    fetcher: Box<dyn Fn(Dep) -> Fut + 'static>,
    executor: F,
) -> Resource<Option<U>>
where
    U: 'static,
    Fut: std::future::Future<Output = U> + 'static,
    Dep: 'static,
    K: PartialEq + 'static,
    F: Fn(
            Fut,
            floem_reactive::WriteSignal<Option<U>>,
            floem_reactive::WriteSignal<bool>,
            ExtSendTrigger,
        ) + 'static,
{
    use floem_reactive::{Scope, SignalGet, SignalUpdate, with_scope};

    let cx = Scope::current();
    let trigger = with_scope(cx, ExtSendTrigger::new);
    let (data_read, data_write) = cx.create_signal(None);
    let (finished_read, finished_write) = cx.create_signal(false);
    let refetch_trigger = cx.create_rw_signal(0);

    cx.create_effect(move |last| {
        let refetch_count = refetch_trigger.get();
        let dep_value = source();
        let current_key = key_fn(&dep_value);

        let should_fetch = match last {
            None => true,
            Some((last_key, last_refetch_count)) => {
                last_key != current_key
                    || (last_refetch_count != refetch_count && refetch_count != 0)
            }
        };

        if !should_fetch {
            return (current_key, refetch_count);
        }

        finished_write.set(false);
        let future = fetcher(dep_value);
        executor(future, data_write, finished_write, trigger);

        (current_key, refetch_count)
    });

    Resource {
        data: data_read,
        finished: finished_read,
        refetch_trigger,
    }
}

fn build_resource_no_memo_with_initial<Fut, Dep, F, T>(
    source: Box<dyn Fn() -> Dep + 'static>,
    fetcher: Box<dyn Fn(Dep) -> Fut + 'static>,
    executor: F,
    initial: T,
) -> Resource<T>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
    Dep: 'static,
    F: Fn(Fut, floem_reactive::WriteSignal<T>, floem_reactive::WriteSignal<bool>, ExtSendTrigger)
        + 'static,
{
    use floem_reactive::{Scope, SignalGet, SignalUpdate, with_scope};

    let cx = Scope::current();
    let trigger = with_scope(cx, ExtSendTrigger::new);
    let (data_read, data_write) = cx.create_signal(initial);
    let (finished_read, finished_write) = cx.create_signal(false);
    let refetch_trigger = cx.create_rw_signal(0);

    cx.create_effect(move |_| {
        let _refetch_count = refetch_trigger.get();
        let dep_value = source();

        finished_write.set(false);
        let future = fetcher(dep_value);
        executor(future, data_write, finished_write, trigger);
    });

    Resource {
        data: data_read,
        finished: finished_read,
        refetch_trigger,
    }
}

fn build_resource_no_memo<Fut, Dep, F, U>(
    source: Box<dyn Fn() -> Dep + 'static>,
    fetcher: Box<dyn Fn(Dep) -> Fut + 'static>,
    executor: F,
) -> Resource<Option<U>>
where
    U: 'static,
    Fut: std::future::Future<Output = U> + 'static,
    Dep: 'static,
    F: Fn(
            Fut,
            floem_reactive::WriteSignal<Option<U>>,
            floem_reactive::WriteSignal<bool>,
            ExtSendTrigger,
        ) + 'static,
{
    use floem_reactive::{Scope, SignalGet, SignalUpdate, with_scope};

    let cx = Scope::current();
    let trigger = with_scope(cx, ExtSendTrigger::new);
    let (data_read, data_write) = cx.create_signal(None);
    let (finished_read, finished_write) = cx.create_signal(false);
    let refetch_trigger = cx.create_rw_signal(0);

    cx.create_effect(move |_| {
        let _refetch_count = refetch_trigger.get();
        let dep_value = source();

        finished_write.set(false);
        let future = fetcher(dep_value);
        executor(future, data_write, finished_write, trigger);
    });

    Resource {
        data: data_read,
        finished: finished_read,
        refetch_trigger,
    }
}
