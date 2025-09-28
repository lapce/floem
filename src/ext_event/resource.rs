#[cfg(feature = "tokio")]
use std::collections::hash_map::DefaultHasher;
#[cfg(feature = "tokio")]
use std::hash::{Hash, Hasher};

use floem_reactive::ReadSignal;

#[derive(Debug, Clone)]
#[allow(unused)]
enum ResourceState<T> {
    Loading,
    Ready(T),
}

/// A reactive resource that automatically fetches data when its dependencies change.
///
/// `Resource` provides a way to reactively fetch data from async operations  and automatically
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
    loading: ReadSignal<bool>,
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
    /// Creates a new reactive resource that fetches data when dependencies change.
    ///
    /// This is a convenience wrapper around [`new_full`](Self::new_full) that uses the hash of
    /// the dependency value as the comparison key. Use this when your dependency type implements
    /// `Hash`.
    ///
    /// # Parameters
    ///
    /// * `source` - A function that returns the current dependency value(s). This function
    ///   is called reactively, and when its return value changes (the hashed value changes),
    ///   a new fetch operation is started.
    /// * `fetcher` - An async function that takes the dependency value and returns the fetched data.
    ///   This can return any type `T`, including `Result<Data, Error>` for explicit error handling.
    ///
    /// # When to use `new` vs `new_full`
    ///
    /// Use `new` when your dependency is simple (e.g., `String`, `u64`, or small structs).
    /// Use [`new_full`](Self::new_full) when you need a custom key function to avoid cloning
    /// large dependency values or to compare based on specific fields.
    ///
    /// # Memoization
    ///
    /// The resource automatically prevents unnecessary fetches by comparing the current dependency
    /// value with the previous one by comparing hashed values. Only when the dependency actually changes
    /// will a new async operation be started.
    ///
    /// # Concurrency
    ///
    /// Each dependency change spawns a new tokio task. If multiple tasks complete while the main
    /// thread is busy, only the result from the most recent task is kept.
    #[cfg(feature = "tokio")]
    pub fn new<Fut, Dep>(
        source: impl Fn() -> Dep + 'static,
        fetcher: impl Fn(Dep) -> Fut + Clone + Send + 'static,
    ) -> Self
    where
        T: Send + 'static,
        Fut: std::future::Future<Output = T> + Send + 'static,
        Dep: Hash + Send + 'static,
    {
        Self::new_full(
            source,
            |dep| {
                let mut hasher = DefaultHasher::new();
                dep.hash(&mut hasher);
                hasher.finish()
            },
            fetcher,
        )
    }

    /// Shared setup for resource effects and state management.
    /// Returns a closure to update state and the signal handles.
    #[allow(clippy::type_complexity, dead_code)]
    fn setup_effects(
        cx: floem_reactive::Scope,
    ) -> (
        impl Fn(ResourceState<T>) + Clone,
        ReadSignal<Option<T>>,
        ReadSignal<bool>,
        floem_reactive::RwSignal<u64>,
    )
    where
        T: Send + 'static,
    {
        use super::ExtSendTrigger;
        use floem_reactive::{with_scope, SignalUpdate};
        use std::sync::{Arc, Mutex};

        let trigger = with_scope(cx, ExtSendTrigger::new);
        let (data_read, data_write) = cx.create_signal(None);
        let (loading_read, loading_write) = cx.create_signal(false);
        let refetch_trigger = cx.create_rw_signal(0);
        let last_state = Arc::new(Mutex::new(None::<ResourceState<T>>));

        {
            let last_state = Arc::clone(&last_state);
            cx.create_effect(move |_| {
                trigger.track();
                if let Some(state) = last_state.lock().unwrap().take() {
                    match state {
                        ResourceState::Loading => {
                            loading_write.set(true);
                        }
                        ResourceState::Ready(data) => {
                            loading_write.set(false);
                            data_write.set(Some(data));
                        }
                    }
                }
            });
        }

        let set_state = {
            let last_state = Arc::clone(&last_state);
            move |state: ResourceState<T>| {
                *last_state.lock().unwrap() = Some(state);
                crate::ext_event::register_ext_trigger(trigger);
            }
        };

        (set_state, data_read, loading_read, refetch_trigger)
    }

    /// Creates a new reactive resource that fetches data when dependencies change.
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
    ///   This can return any type `T`, including `Result<Data, Error>` for explicit error handling.
    ///
    /// # Memoization
    ///
    /// The resource automatically prevents unnecessary fetches by comparing keys derived from
    /// the dependency value using the `key_fn`. Only when the key actually changes will a new
    /// async operation be started. This approach avoids cloning potentially large dependency
    /// values for comparison.
    ///
    /// # Concurrency
    ///
    /// Each dependency change spawns a new tokio task. If multiple tasks complete while the main
    /// thread is busy, only the result from the most recent task is kept.
    #[cfg(feature = "tokio")]
    pub fn new_full<Fut, Dep, K>(
        source: impl Fn() -> Dep + 'static,
        key_fn: impl Fn(&Dep) -> K + 'static,
        fetcher: impl Fn(Dep) -> Fut + Clone + Send + 'static,
    ) -> Self
    where
        T: Send + 'static,
        Fut: std::future::Future<Output = T> + Send + 'static,
        Dep: Send + 'static,
        K: PartialEq + 'static,
    {
        use floem_reactive::{Scope, SignalGet};

        let cx = Scope::current();
        let (set_state, data, loading, refetch_trigger) = Self::setup_effects(cx);

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

            set_state(ResourceState::Loading);

            tokio::spawn({
                let set_state = set_state.clone();
                let fetcher = fetcher.clone();
                async move {
                    let result = fetcher(dep_value).await;
                    set_state(ResourceState::Ready(result));
                }
            });

            (current_key, refetch_count)
        });

        Resource {
            data,
            loading,
            refetch_trigger,
        }
    }

    /// Creates a new reactive resource that always fetches data when dependencies change,
    /// without any memoization.
    ///
    /// Unlike `new_full`, this method does not use a key function to determine if dependencies
    /// have changed. Instead, it unconditionally starts a new fetch operation whenever the
    /// `source` function returns or when `refetch()` is called.
    ///
    /// # Parameters
    ///
    /// * `source` - A function that returns the current dependency value(s). This function
    ///   is called reactively, and every time it runs, a new fetch operation is started.
    /// * `fetcher` - An async function that takes the dependency value and returns the fetched data.
    ///   This can return any type `T`, including `Result<Data, Error>` for explicit error handling.
    ///
    /// # Use Cases
    ///
    /// Use this method when:
    /// - You want to always fetch fresh data, even if dependencies appear unchanged
    /// - The dependency type doesn't have a meaningful equality comparison
    ///
    /// # Concurrency
    ///
    /// Each dependency change spawns a new tokio task. If multiple tasks complete while the main
    /// thread is busy, only the result from the most recent task is kept.
    #[cfg(feature = "tokio")]
    pub fn new_no_memo<Fut, Dep>(
        source: impl Fn() -> Dep + 'static,
        fetcher: impl Fn(Dep) -> Fut + Clone + Send + 'static,
    ) -> Self
    where
        T: Send + 'static,
        Fut: std::future::Future<Output = T> + Send + 'static,
        Dep: Send + 'static,
    {
        use floem_reactive::{Scope, SignalGet};

        let cx = Scope::current();
        let (set_state, data, loading, refetch_trigger) = Self::setup_effects(cx);

        cx.create_effect(move |_| {
            let _refetch_count = refetch_trigger.get();
            let dep_value = source();

            set_state(ResourceState::Loading);

            tokio::spawn({
                let set_state = set_state.clone();
                let fetcher = fetcher.clone();
                async move {
                    let result = fetcher(dep_value).await;
                    set_state(ResourceState::Ready(result));
                }
            });
        });

        Resource {
            data,
            loading,
            refetch_trigger,
        }
    }

    /// Manually triggers a refetch of the resource, bypassing memoization.
    ///
    /// This will start a new async fetch operation using the current
    /// dependency value, even if that value hasn't changed since the last fetch.
    ///
    /// # Behavior
    ///
    /// - Sets the loading state to `true`
    /// - Spawns a new fetch operation
    /// - Bypasses memoization for this fetch
    pub fn refetch(&self) {
        use floem_reactive::SignalUpdate;
        // Increment the refetch trigger to bypass memoization
        self.refetch_trigger.update(|count| *count += 1);
    }

    /// Returns `true` if an async fetch operation is currently in progress.
    ///
    /// This can be used to show loading indicators in the UI.
    pub fn is_loading(&self) -> bool {
        use floem_reactive::SignalGet as _;
        self.loading.get()
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
impl<T: std::clone::Clone> floem_reactive::SignalGet<Option<T>> for Resource<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.data.id()
    }
}
impl<T> floem_reactive::SignalTrack<Option<T>> for Resource<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.data.id()
    }
}
