//! Reactive signals for futures that resolve to a single value.

mod builder;

use super::common::EventLoopExecutor;
use builder::FutureSignalBuilder;

/// A reactive signal for futures that resolve to a single value.
///
/// `FutureSignal` provides a way to reactively consume values from async futures and automatically
/// update UI when the future completes. It manages the future lifecycle and provides access to
/// both the current value and completion state.
///
/// The value is always `Option<T>` where `None` means the future hasn't completed yet
/// and `Some(T)` means the future has completed with the result.
///
/// # Examples
///
/// ```rust,ignore
/// // Simple case: event loop executor
/// let future_signal = FutureSignal::new(my_future);
///
/// // Full customization
/// let future_signal = FutureSignal::custom(my_future)
///     .tokio_spawn()
///     .build();
/// ```
pub struct FutureSignal<T> {
    pub(super) value: floem_reactive::ReadSignal<Option<T>>,
    pub(super) finished: floem_reactive::ReadSignal<bool>,
}

impl<T: 'static> FutureSignal<T> {
    /// Creates a new reactive future signal with sensible defaults.
    ///
    /// - **Executor**: Main event loop (future does not need to be `Send`)
    /// - **Initial value**: Always `None` until the future completes
    ///
    /// # Parameters
    ///
    /// * `future` - The async future to consume.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let future_signal = FutureSignal::new(my_future);
    /// ```
    pub fn new<Fut>(future: Fut) -> Self
    where
        Fut: std::future::Future<Output = T> + 'static,
        T: 'static,
    {
        FutureSignalBuilder::new(future).build()
    }

    /// Creates a future signal builder for full customization.
    ///
    /// Use this when you need to customize:
    /// - The executor (tokio, custom, etc.)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Tokio executor
    /// let future_signal = FutureSignal::custom(my_future)
    ///     .tokio_spawn()
    ///     .build();
    /// ```
    pub fn custom<Fut>(future: Fut) -> FutureSignalBuilder<Fut, T, EventLoopExecutor>
    where
        Fut: std::future::Future<Output = T> + 'static,
        T: 'static,
    {
        FutureSignalBuilder::new(future)
    }

    pub fn is_finished(&self) -> bool {
        use floem_reactive::SignalGet;
        self.finished.get()
    }
}

impl<T> floem_reactive::SignalGet<Option<T>> for FutureSignal<T>
where
    T: Clone,
{
    fn id(&self) -> floem_reactive::ReactiveId {
        self.value.id()
    }
}

impl<T> floem_reactive::SignalWith<Option<T>> for FutureSignal<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.value.id()
    }
}

impl<T> floem_reactive::SignalRead<Option<T>> for FutureSignal<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.value.id()
    }
}

impl<T> floem_reactive::SignalTrack<Option<T>> for FutureSignal<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.value.id()
    }
}

impl<T> Copy for FutureSignal<T> {}

impl<T> Clone for FutureSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}
