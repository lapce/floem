//! Reactive signals for streams that produce multiple values.

mod builder;
use builder::StreamSignalBuilder;

use super::common::{EventLoopExecutor, NoInitial};

/// A reactive signal for streams that produce multiple values.
///
/// `StreamSignal` provides a way to reactively consume values from async streams and automatically
/// update UI when new values arrive. It manages the stream lifecycle and provides access to both
/// the current value and completion state.
///
/// # Examples
///
/// ```rust,ignore
/// // Simple case: event loop executor, returns StreamSignal<Option<T>>
/// let stream_signal = StreamSignal::new(my_stream);
///
/// // With initial value
/// let stream_signal = StreamSignal::with_initial(my_stream, initial_value);
///
/// // Full customization
/// let stream_signal = StreamSignal::custom(my_stream)
///     .tokio_spawn()
///     .initial(initial_value)
///     .build();
/// ```
pub struct StreamSignal<T> {
    value: floem_reactive::ReadSignal<T>,
    finished: floem_reactive::ReadSignal<bool>,
}

/// This implementation will be removed in the future
impl<T: 'static, S> From<S> for StreamSignal<Option<T>>
where
    S: futures::Stream<Item = T> + 'static,
{
    fn from(value: S) -> Self {
        StreamSignal::new(value)
    }
}

impl<T: 'static> StreamSignal<T> {
    /// Creates a new reactive stream signal with sensible defaults.
    ///
    /// - **Executor**: Main event loop (stream does not need to be `Send`)
    /// - **Initial value**: None (returns `StreamSignal<Option<U>>`)
    ///
    /// # Parameters
    ///
    /// * `stream` - The async stream to consume.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let stream_signal = StreamSignal::new(my_stream);
    /// ```
    pub fn new<S>(stream: S) -> StreamSignal<Option<T>>
    where
        S: futures::Stream<Item = T> + 'static,
        T: 'static,
    {
        StreamSignalBuilder::new(stream).build()
    }

    /// Creates a new reactive stream signal with an initial value.
    ///
    /// Uses the same defaults as `new()` but allows you to specify an initial value
    /// so the signal is never `None`.
    ///
    /// - **Executor**: Main event loop (stream does not need to be `Send`)
    ///
    /// # Parameters
    ///
    /// * `stream` - The async stream to consume.
    /// * `initial` - The initial value for the signal before any stream item arrives.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let stream_signal = StreamSignal::with_initial(my_stream, initial_value);
    /// ```
    pub fn with_initial<S>(stream: S, initial: T) -> Self
    where
        S: futures::Stream<Item = T> + 'static,
    {
        StreamSignalBuilder::new(stream).initial(initial).build()
    }

    /// Creates a stream signal builder for full customization.
    ///
    /// Use this when you need to customize:
    /// - The executor (tokio, custom, etc.)
    /// - Initial value
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Tokio executor with initial value
    /// let stream_signal = StreamSignal::custom(my_stream)
    ///     .tokio_spawn()
    ///     .initial(initial_value)
    ///     .build();
    /// ```
    pub fn custom<S>(stream: S) -> StreamSignalBuilder<S, T, EventLoopExecutor, NoInitial>
    where
        S: futures::Stream<Item = T> + 'static,
        T: 'static,
    {
        StreamSignalBuilder::new(stream)
    }

    pub fn is_finished(&self) -> bool {
        use floem_reactive::SignalGet;
        self.finished.get()
    }
}

impl<T> floem_reactive::SignalGet<T> for StreamSignal<T>
where
    T: Clone,
{
    fn id(&self) -> floem_reactive::ReactiveId {
        self.value.id()
    }
}

impl<T> floem_reactive::SignalWith<T> for StreamSignal<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.value.id()
    }
}

impl<T> floem_reactive::SignalRead<T> for StreamSignal<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.value.id()
    }
}

impl<T> floem_reactive::SignalTrack<T> for StreamSignal<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.value.id()
    }
}

impl<T> Copy for StreamSignal<T> {}

impl<T> Clone for StreamSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}
