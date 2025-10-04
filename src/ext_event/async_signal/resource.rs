use crate::ext_event::ExtSendTrigger;
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
pub struct Resource<T> {
    /// The signal containing the fetched data. `None` when loading or no data has been fetched yet.
    data: ReadSignal<Option<T>>,
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

impl<T> Resource<T> {
    /// Creates a new reactive resource using a custom executor function.
    ///
    /// This uses hash-based memoization - the dependency value is hashed to determine
    /// if it has changed.
    ///
    /// # Parameters
    ///
    /// * `source` - A function that returns the current dependency value(s).
    /// * `fetcher` - An async function that takes the dependency value and returns the fetched data.
    /// * `executor` - A function that executes the future.
    pub fn on_executor<Fut, Dep, F>(
        source: impl Fn() -> Dep + 'static,
        fetcher: impl Fn(Dep) -> Fut + 'static,
        executor: F,
    ) -> Self
    where
        T: 'static,
        Fut: std::future::Future<Output = T> + 'static,
        Dep: Hash + 'static,
        F: Fn(
                Fut,
                floem_reactive::WriteSignal<Option<T>>,
                floem_reactive::WriteSignal<bool>,
                ExtSendTrigger,
            ) + 'static,
    {
        Self::on_executor_with_key(
            source,
            |dep| {
                let mut hasher = DefaultHasher::new();
                dep.hash(&mut hasher);
                hasher.finish()
            },
            fetcher,
            executor,
        )
    }

    /// Creates a new reactive resource using a custom executor function and key function.
    ///
    /// # Parameters
    ///
    /// * `source` - A function that returns the current dependency value(s). This function
    ///   is called reactively, and when its derived key changes (compared with `PartialEq`),
    ///   a new fetch operation is started.
    /// * `key_fn` - A function that extracts a comparable key from the dependency value.
    ///   The resource uses this key to determine if dependencies have changed, avoiding
    ///   unnecessary clones of the dependency data.
    /// * `fetcher` - An async function that takes the dependency value and returns the fetched data.
    /// * `executor` - A function that executes the future. This determines where and how the
    ///   async operation runs (event loop, tokio spawn, etc.).
    ///
    /// # Memoization
    ///
    /// The resource automatically prevents unnecessary fetches by comparing keys derived from
    /// the dependency value using the `key_fn`. Only when the key actually changes will a new
    /// async operation be started.
    pub fn on_executor_with_key<Fut, Dep, K, F>(
        source: impl Fn() -> Dep + 'static,
        key_fn: impl Fn(&Dep) -> K + 'static,
        fetcher: impl Fn(Dep) -> Fut + 'static,
        executor: F,
    ) -> Self
    where
        T: 'static,
        Fut: std::future::Future<Output = T> + 'static,
        Dep: 'static,
        K: PartialEq + 'static,
        F: Fn(
                Fut,
                floem_reactive::WriteSignal<Option<T>>,
                floem_reactive::WriteSignal<bool>,
                ExtSendTrigger,
            ) + 'static,
    {
        use floem_reactive::{with_scope, Scope, SignalGet, SignalUpdate};

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

    /// Creates a new reactive resource using a custom executor function, without memoization.
    ///
    /// Unlike `on_executor`, this method does not use a key function to determine if dependencies
    /// have changed. Instead, it unconditionally starts a new fetch operation whenever the
    /// `source` function returns or when `refetch()` is called.
    ///
    /// # Parameters
    ///
    /// * `source` - A function that returns the current dependency value(s). This function
    ///   is called reactively, and every time it runs, a new fetch operation is started.
    /// * `fetcher` - An async function that takes the dependency value and returns the fetched data.
    /// * `executor` - A function that executes the future.
    pub fn on_executor_no_memo<Fut, Dep, F>(
        source: impl Fn() -> Dep + 'static,
        fetcher: impl Fn(Dep) -> Fut + 'static,
        executor: F,
    ) -> Self
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
        use floem_reactive::{with_scope, Scope, SignalGet, SignalUpdate};

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

    /// Creates a new reactive resource on the main event loop.
    ///
    /// This uses hash-based memoization. The future is polled on the main event loop
    /// and does not need to be `Send`.
    pub fn on_event_loop<Fut, Dep>(
        source: impl Fn() -> Dep + 'static,
        fetcher: impl Fn(Dep) -> Fut + 'static,
    ) -> Self
    where
        T: 'static,
        Fut: std::future::Future<Output = T> + 'static,
        Dep: Hash + 'static,
    {
        Self::on_executor(source, fetcher, super::event_loop_future)
    }

    /// Creates a new reactive resource on the main event loop with a custom key function.
    ///
    /// The future is polled on the main event loop and does not need to be `Send`.
    pub fn on_event_loop_with_key<Fut, Dep, K>(
        source: impl Fn() -> Dep + 'static,
        key_fn: impl Fn(&Dep) -> K + 'static,
        fetcher: impl Fn(Dep) -> Fut + 'static,
    ) -> Self
    where
        T: 'static,
        Fut: std::future::Future<Output = T> + 'static,
        Dep: 'static,
        K: PartialEq + 'static,
    {
        Self::on_executor_with_key(source, key_fn, fetcher, super::event_loop_future)
    }

    /// Creates a new reactive resource on the main event loop, without memoization.
    ///
    /// The future is polled on the main event loop and does not need to be `Send`.
    pub fn on_event_loop_no_memo<Fut, Dep>(
        source: impl Fn() -> Dep + 'static,
        fetcher: impl Fn(Dep) -> Fut + 'static,
    ) -> Self
    where
        T: 'static,
        Fut: std::future::Future<Output = T> + 'static,
        Dep: 'static,
    {
        Self::on_executor_no_memo(source, fetcher, super::event_loop_future)
    }

    /// Creates a new reactive resource on tokio::spawn.
    ///
    /// This uses hash-based memoization. The future is spawned on tokio and
    /// requires `Send + 'static` bounds.
    #[cfg(feature = "tokio")]
    pub fn on_tokio_spawn<Fut, Dep>(
        source: impl Fn() -> Dep + 'static,
        fetcher: impl Fn(Dep) -> Fut + Send + 'static,
    ) -> Self
    where
        T: Send + 'static,
        Fut: std::future::Future<Output = T> + Send + 'static,
        Dep: Hash + Send + 'static,
    {
        Self::on_executor(source, fetcher, super::tokio_spawn_future)
    }

    /// Creates a new reactive resource on tokio::spawn with a custom key function.
    ///
    /// The future is spawned on tokio and requires `Send + 'static` bounds.
    #[cfg(feature = "tokio")]
    pub fn on_tokio_spawn_with_key<Fut, Dep, K>(
        source: impl Fn() -> Dep + 'static,
        key_fn: impl Fn(&Dep) -> K + 'static,
        fetcher: impl Fn(Dep) -> Fut + Send + 'static,
    ) -> Self
    where
        T: Send + 'static,
        Fut: std::future::Future<Output = T> + Send + 'static,
        Dep: 'static,
        K: PartialEq + 'static,
    {
        Self::on_executor_with_key(source, key_fn, fetcher, super::tokio_spawn_future)
    }

    /// Creates a new reactive resource on tokio::spawn, without memoization.
    ///
    /// The future is spawned on tokio and requires `Send + 'static` bounds.
    #[cfg(feature = "tokio")]
    pub fn on_tokio_spawn_no_memo<Fut, Dep>(
        source: impl Fn() -> Dep + 'static,
        fetcher: impl Fn(Dep) -> Fut + Send + 'static,
    ) -> Self
    where
        T: Send + 'static,
        Fut: std::future::Future<Output = T> + Send + 'static,
        Dep: 'static,
    {
        Self::on_executor_no_memo(source, fetcher, super::tokio_spawn_future)
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

impl<T> floem_reactive::SignalWith<Option<T>> for Resource<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.data.id()
    }
}

impl<T> floem_reactive::SignalRead<Option<T>> for Resource<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.data.id()
    }
}

impl<T: Clone> floem_reactive::SignalGet<Option<T>> for Resource<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.data.id()
    }
}

impl<T> floem_reactive::SignalTrack<Option<T>> for Resource<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.data.id()
    }
}
