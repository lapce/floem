use crate::ext_event::{ExtSendTrigger, register_ext_trigger};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

// ============================================================================
// Type-state markers (shared across all async signal types)
// ============================================================================

/// Type-state marker for event loop executor.
pub struct EventLoopExecutor;

/// Type-state marker for tokio executor.
pub struct TokioExecutor;

/// Type-state marker for custom executor.
pub struct CustomExecutor<F>(pub F);

/// Type-state marker for no initial value.
pub struct NoInitial;

/// Type-state marker for having an initial value.
pub struct WithInitialValue<T>(pub T);

pub mod resource;
pub use resource::*;

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
    pub value: floem_reactive::ReadSignal<Option<T>>,
    pub finished: floem_reactive::ReadSignal<bool>,
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

impl<T> Copy for FutureSignal<T> {}

impl<T> Clone for FutureSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

// ============================================================================
// FutureSignal Builder API
// ============================================================================

/// A builder for creating customized `FutureSignal` instances.
///
/// Created via `FutureSignal::custom()`. Allows fine-grained control over:
/// - Executor type (event loop, tokio, custom)
pub struct FutureSignalBuilder<Fut, T, E> {
    future: Fut,
    executor: E,
    _phantom: std::marker::PhantomData<T>,
}

impl<Fut, T> FutureSignalBuilder<Fut, T, EventLoopExecutor>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
{
    fn new(future: Fut) -> Self {
        Self {
            future,
            executor: EventLoopExecutor,
            _phantom: std::marker::PhantomData,
        }
    }
}

// Builder methods for customization
impl<Fut, T, E> FutureSignalBuilder<Fut, T, E>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
{
    /// Use the main event loop as the executor.
    ///
    /// The future does not need to be `Send`. This is the default.
    pub fn event_loop(self) -> FutureSignalBuilder<Fut, T, EventLoopExecutor> {
        FutureSignalBuilder {
            future: self.future,
            executor: EventLoopExecutor,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Use tokio::spawn as the executor.
    ///
    /// The future must be `Send + 'static`. Requires the `tokio` feature.
    #[cfg(feature = "tokio")]
    pub fn tokio_spawn(self) -> FutureSignalBuilder<Fut, T, TokioExecutor>
    where
        T: Send,
        Fut: Send,
    {
        FutureSignalBuilder {
            future: self.future,
            executor: TokioExecutor,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Use a custom executor function.
    ///
    /// The executor receives the future and signals to update when it completes.
    pub fn executor<F>(self, executor: F) -> FutureSignalBuilder<Fut, T, CustomExecutor<F>>
    where
        F: FnOnce(
                Fut,
                floem_reactive::WriteSignal<Option<T>>,
                floem_reactive::WriteSignal<bool>,
                ExtSendTrigger,
            ) + 'static,
    {
        FutureSignalBuilder {
            future: self.future,
            executor: CustomExecutor(executor),
            _phantom: std::marker::PhantomData,
        }
    }
}

// Build with event loop -> FutureSignal<T>
impl<Fut, T> FutureSignalBuilder<Fut, T, EventLoopExecutor>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
{
    pub fn build(self) -> FutureSignal<T> {
        build_future_signal(self.future, event_loop_future_option)
    }
}

// Build with tokio -> FutureSignal<T>
#[cfg(feature = "tokio")]
impl<Fut, T> FutureSignalBuilder<Fut, T, TokioExecutor>
where
    T: Send + 'static,
    Fut: std::future::Future<Output = T> + Send + 'static,
{
    pub fn build(self) -> FutureSignal<T> {
        build_future_signal(self.future, tokio_spawn_future_option)
    }
}

// Build with custom executor -> FutureSignal<T>
impl<Fut, T, F> FutureSignalBuilder<Fut, T, CustomExecutor<F>>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
    F: FnOnce(
            Fut,
            floem_reactive::WriteSignal<Option<T>>,
            floem_reactive::WriteSignal<bool>,
            ExtSendTrigger,
        ) + 'static,
{
    pub fn build(self) -> FutureSignal<T> {
        build_future_signal(self.future, self.executor.0)
    }
}

fn build_future_signal<Fut, T, F>(future: Fut, executor: F) -> FutureSignal<T>
where
    T: 'static,
    Fut: std::future::Future<Output = T> + 'static,
    F: FnOnce(
        Fut,
        floem_reactive::WriteSignal<Option<T>>,
        floem_reactive::WriteSignal<bool>,
        ExtSendTrigger,
    ),
{
    use floem_reactive::*;

    let cx = Scope::current();
    let trigger = with_scope(cx, ExtSendTrigger::new);
    let (read, write) = cx.create_signal(None);
    let (read_finished, write_finished) = cx.create_signal(false);

    executor(future, write, write_finished, trigger);

    FutureSignal {
        value: read,
        finished: read_finished,
    }
}

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
    pub value: floem_reactive::ReadSignal<T>,
    pub finished: floem_reactive::ReadSignal<bool>,
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
        T: 'static,
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

impl<T> Copy for StreamSignal<T> {}

impl<T> Clone for StreamSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

// ============================================================================
// StreamSignal Builder API
// ============================================================================

/// A builder for creating customized `StreamSignal` instances.
///
/// Created via `StreamSignal::custom()`. Allows fine-grained control over:
/// - Executor type (event loop, tokio, custom)
/// - Initial value
pub struct StreamSignalBuilder<S, T, E, I> {
    stream: S,
    executor: E,
    initial: I,
    _phantom: std::marker::PhantomData<T>,
}

impl<S, T> StreamSignalBuilder<S, T, EventLoopExecutor, NoInitial>
where
    T: 'static,
    S: futures::Stream<Item = T> + 'static,
{
    fn new(stream: S) -> Self {
        Self {
            stream,
            executor: EventLoopExecutor,
            initial: NoInitial,
            _phantom: std::marker::PhantomData,
        }
    }
}

// Builder methods for customization
impl<S, T, E, I> StreamSignalBuilder<S, T, E, I>
where
    T: 'static,
    S: futures::Stream<Item = T> + 'static,
{
    /// Use the main event loop as the executor.
    ///
    /// The stream does not need to be `Send`. This is the default.
    pub fn event_loop(self) -> StreamSignalBuilder<S, T, EventLoopExecutor, I> {
        StreamSignalBuilder {
            stream: self.stream,
            executor: EventLoopExecutor,
            initial: self.initial,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Use tokio::spawn as the executor.
    ///
    /// The stream must be `Send + 'static`. Requires the `tokio` feature.
    #[cfg(feature = "tokio")]
    pub fn tokio_spawn(self) -> StreamSignalBuilder<S, T, TokioExecutor, I>
    where
        T: Send,
        S: Send,
    {
        StreamSignalBuilder {
            stream: self.stream,
            executor: TokioExecutor,
            initial: self.initial,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Use a custom executor function.
    ///
    /// The executor receives the stream and signals to update when items arrive.
    pub fn executor<F>(self, executor: F) -> StreamSignalBuilder<S, T, CustomExecutor<F>, I>
    where
        F: FnOnce(
                S,
                floem_reactive::WriteSignal<T>,
                floem_reactive::WriteSignal<bool>,
                ExtSendTrigger,
            ) + 'static,
    {
        StreamSignalBuilder {
            stream: self.stream,
            executor: CustomExecutor(executor),
            initial: self.initial,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Set an initial value for the signal.
    ///
    /// Without this, the signal returns `Option<T>` and starts as `None`.
    /// With this, the signal returns `T` and starts with your initial value.
    pub fn initial(self, initial: T) -> StreamSignalBuilder<S, T, E, WithInitialValue<T>> {
        StreamSignalBuilder {
            stream: self.stream,
            executor: self.executor,
            initial: WithInitialValue(initial),
            _phantom: std::marker::PhantomData,
        }
    }
}

// Build with event loop, no initial -> StreamSignal<Option<T>>
impl<S, T> StreamSignalBuilder<S, T, EventLoopExecutor, NoInitial>
where
    T: 'static,
    S: futures::Stream<Item = T> + 'static,
{
    pub fn build(self) -> StreamSignal<Option<T>> {
        build_stream_signal_option(self.stream, event_loop_stream_option)
    }
}

// Build with event loop, with initial -> StreamSignal<T>
impl<S, T> StreamSignalBuilder<S, T, EventLoopExecutor, WithInitialValue<T>>
where
    T: 'static,
    S: futures::Stream<Item = T> + 'static,
{
    pub fn build(self) -> StreamSignal<T> {
        build_stream_signal_with_initial(self.stream, self.initial.0, event_loop_stream)
    }
}

// Build with tokio, no initial -> StreamSignal<Option<T>>
#[cfg(feature = "tokio")]
impl<S, T> StreamSignalBuilder<S, T, TokioExecutor, NoInitial>
where
    T: Send + 'static,
    S: futures::Stream<Item = T> + Send + 'static,
{
    pub fn build(self) -> StreamSignal<Option<T>> {
        build_stream_signal_option(self.stream, tokio_spawn_stream_option)
    }
}

// Build with tokio, with initial -> StreamSignal<T>
#[cfg(feature = "tokio")]
impl<S, T> StreamSignalBuilder<S, T, TokioExecutor, WithInitialValue<T>>
where
    T: Send + 'static,
    S: futures::Stream<Item = T> + Send + 'static,
{
    pub fn build(self) -> StreamSignal<T> {
        build_stream_signal_with_initial(self.stream, self.initial.0, tokio_spawn_stream)
    }
}

// Build with custom executor, no initial -> StreamSignal<Option<T>>
impl<S, T, F> StreamSignalBuilder<S, T, CustomExecutor<F>, NoInitial>
where
    T: 'static,
    S: futures::Stream<Item = T> + 'static,
    F: FnOnce(
            S,
            floem_reactive::WriteSignal<Option<T>>,
            floem_reactive::WriteSignal<bool>,
            ExtSendTrigger,
        ) + 'static,
{
    pub fn build(self) -> StreamSignal<Option<T>> {
        build_stream_signal_option(self.stream, self.executor.0)
    }
}

// Build with custom executor, with initial -> StreamSignal<T>
impl<S, T, F> StreamSignalBuilder<S, T, CustomExecutor<F>, WithInitialValue<T>>
where
    T: 'static,
    S: futures::Stream<Item = T> + 'static,
    F: FnOnce(S, floem_reactive::WriteSignal<T>, floem_reactive::WriteSignal<bool>, ExtSendTrigger)
        + 'static,
{
    pub fn build(self) -> StreamSignal<T> {
        build_stream_signal_with_initial(self.stream, self.initial.0, self.executor.0)
    }
}

fn build_stream_signal_with_initial<S, T, F>(stream: S, initial: T, executor: F) -> StreamSignal<T>
where
    T: 'static,
    S: futures::Stream<Item = T> + 'static,
    F: FnOnce(S, floem_reactive::WriteSignal<T>, floem_reactive::WriteSignal<bool>, ExtSendTrigger),
{
    use floem_reactive::*;

    let cx = Scope::current();
    let trigger = with_scope(cx, ExtSendTrigger::new);
    let (read, write) = cx.create_signal(initial);
    let (read_finished, write_finished) = cx.create_signal(false);

    executor(stream, write, write_finished, trigger);

    StreamSignal {
        value: read,
        finished: read_finished,
    }
}

fn build_stream_signal_option<S, T, F>(stream: S, executor: F) -> StreamSignal<Option<T>>
where
    T: 'static,
    S: futures::Stream<Item = T> + 'static,
    F: FnOnce(
        S,
        floem_reactive::WriteSignal<Option<T>>,
        floem_reactive::WriteSignal<bool>,
        ExtSendTrigger,
    ),
{
    use floem_reactive::*;

    let cx = Scope::current();
    let trigger = with_scope(cx, ExtSendTrigger::new);
    let (read, write) = cx.create_signal(None);
    let (read_finished, write_finished) = cx.create_signal(false);

    executor(stream, write, write_finished, trigger);

    StreamSignal {
        value: read,
        finished: read_finished,
    }
}

/// Error type indicating a channel has closed.
#[derive(Debug, Clone, Copy)]
pub struct ChannelClosed;

impl std::fmt::Display for ChannelClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Channel Closed")
    }
}

impl core::error::Error for ChannelClosed {}

/// A reactive signal for channels that produce multiple values with error handling.
///
/// `ChannelSignal` provides a way to reactively consume values from channels and automatically
/// update UI when new values arrive or errors occur. It manages the channel lifecycle and provides
/// access to both the current value and any errors.
///
/// # Examples
///
/// ```rust,ignore
/// // Simple case: std::thread executor, returns ChannelSignal<Option<T>, E>
/// let channel_signal = ChannelSignal::new(receiver);
///
/// // With initial value
/// let channel_signal = ChannelSignal::with_initial(receiver, initial_value);
///
/// // Full customization
/// let channel_signal = ChannelSignal::custom(receiver)
///     .event_loop()
///     .initial(initial_value)
///     .build();
/// ```
pub struct ChannelSignal<T, E> {
    pub value: floem_reactive::ReadSignal<T>,
    pub error: floem_reactive::ReadSignal<Option<E>>,
}

/// This implementation will be removed in the future
#[cfg(feature = "crossbeam")]
impl<T: std::marker::Send + 'static> From<crossbeam::channel::Receiver<T>>
    for ChannelSignal<Option<T>, crossbeam::channel::RecvError>
{
    fn from(value: crossbeam::channel::Receiver<T>) -> Self {
        ChannelSignal::new(value)
    }
}
/// This implementation will be removed in the future
impl<T: std::marker::Send + 'static> From<std::sync::mpsc::Receiver<T>>
    for ChannelSignal<Option<T>, std::sync::mpsc::RecvError>
{
    fn from(value: std::sync::mpsc::Receiver<T>) -> Self {
        ChannelSignal::new(value)
    }
}

impl<T: 'static, E: 'static> ChannelSignal<T, E> {
    /// Creates a new reactive channel signal with sensible defaults.
    ///
    /// - **Executor**: Std::thread for blocking receivers, event loop for pollable receivers
    /// - **Initial value**: None (returns `ChannelSignal<Option<U>, E>`)
    ///
    /// # Parameters
    ///
    /// * `receiver` - The channel receiver to consume.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let channel_signal = ChannelSignal::new(receiver);
    /// ```
    pub fn new<R>(receiver: R) -> ChannelSignal<Option<T>, E>
    where
        R: BlockingReceiver<Item = T, Error = E> + Send + 'static,
        T: Send + 'static,
        E: Send + 'static,
    {
        ChannelSignalBuilder::new(receiver).build()
    }

    /// Creates a new reactive channel signal with an initial value.
    ///
    /// Uses the same defaults as `new()` but allows you to specify an initial value
    /// so the signal is never `None`.
    ///
    /// - **Executor**: Std::thread for blocking receivers, event loop for pollable receivers
    ///
    /// # Parameters
    ///
    /// * `receiver` - The channel receiver to consume.
    /// * `initial` - The initial value for the signal before any message arrives.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let channel_signal = ChannelSignal::with_initial(receiver, initial_value);
    /// ```
    pub fn with_initial<R>(receiver: R, initial: T) -> Self
    where
        R: BlockingReceiver<Item = T, Error = E> + Send + 'static,
        T: Send + 'static,
        E: Send + 'static,
    {
        ChannelSignalBuilder::new(receiver).initial(initial).build()
    }

    /// Creates a channel signal builder for full customization.
    ///
    /// Use this when you need to customize:
    /// - The executor (event loop, tokio, std::thread, etc.)
    /// - Initial value
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Event loop executor with initial value
    /// let channel_signal = ChannelSignal::custom(receiver)
    ///     .event_loop()
    ///     .initial(initial_value)
    ///     .build();
    /// ```
    pub fn custom<R>(receiver: R) -> ChannelSignalBuilder<R, T, E, StdThreadExecutor, NoInitial>
    where
        R: BlockingReceiver<Item = T, Error = E> + Send + 'static,
        T: Send + 'static,
        E: Send + 'static,
    {
        ChannelSignalBuilder::new(receiver)
    }

    pub fn error(&self) -> floem_reactive::ReadSignal<Option<E>> {
        self.error
    }
}

impl<T, E> floem_reactive::SignalGet<T> for ChannelSignal<T, E>
where
    T: Clone,
{
    fn id(&self) -> floem_reactive::ReactiveId {
        self.value.id()
    }
}

impl<T, E> floem_reactive::SignalWith<T> for ChannelSignal<T, E> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.value.id()
    }
}

impl<T, E> floem_reactive::SignalRead<T> for ChannelSignal<T, E> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.value.id()
    }
}

impl<T, E> Copy for ChannelSignal<T, E> {}

impl<T, E> Clone for ChannelSignal<T, E> {
    fn clone(&self) -> Self {
        *self
    }
}

// ============================================================================
// ChannelSignal Builder API
// ============================================================================

/// A builder for creating customized `ChannelSignal` instances.
///
/// Created via `ChannelSignal::custom()`. Allows fine-grained control over:
/// - Executor type (event loop, std::thread, tokio, custom)
/// - Initial value
pub struct ChannelSignalBuilder<R, T, E, Ex, I> {
    receiver: R,
    executor: Ex,
    initial: I,
    _phantom: std::marker::PhantomData<(T, E)>,
}

// Additional type-state marker for channel signal
pub struct StdThreadExecutor;

impl<R, T, E> ChannelSignalBuilder<R, T, E, StdThreadExecutor, NoInitial>
where
    T: Send + 'static,
    E: Send + 'static,
    R: BlockingReceiver<Item = T, Error = E> + Send + 'static,
{
    fn new(receiver: R) -> Self {
        Self {
            receiver,
            executor: StdThreadExecutor,
            initial: NoInitial,
            _phantom: std::marker::PhantomData,
        }
    }
}

// Builder methods for customization
impl<R, T, E, Ex, I> ChannelSignalBuilder<R, T, E, Ex, I>
where
    T: 'static,
    E: 'static,
{
    /// Use the main event loop as the executor.
    ///
    /// The receiver must implement `PollableReceiver` and does not need to be `Send`.
    pub fn event_loop<PR>(self) -> ChannelSignalBuilder<PR, T, E, EventLoopExecutor, I>
    where
        PR: PollableReceiver<Item = T, Error = E> + 'static,
        R: Into<PR>,
    {
        ChannelSignalBuilder {
            receiver: self.receiver.into(),
            executor: EventLoopExecutor,
            initial: self.initial,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Use a dedicated std::thread as the executor.
    ///
    /// The receiver must implement `BlockingReceiver` and be `Send + 'static`. This is the default.
    pub fn std_thread<BR>(self) -> ChannelSignalBuilder<BR, T, E, StdThreadExecutor, I>
    where
        BR: BlockingReceiver<Item = T, Error = E> + Send + 'static,
        R: Into<BR>,
        T: Send,
        E: Send,
    {
        ChannelSignalBuilder {
            receiver: self.receiver.into(),
            executor: StdThreadExecutor,
            initial: self.initial,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Use tokio::task::spawn_blocking as the executor.
    ///
    /// The receiver must implement `BlockingReceiver` and be `Send + 'static`. Requires the `tokio` feature.
    #[cfg(feature = "tokio")]
    pub fn tokio_spawn_blocking<BR>(self) -> ChannelSignalBuilder<BR, T, E, TokioExecutor, I>
    where
        BR: BlockingReceiver<Item = T, Error = E> + Send + 'static,
        R: Into<BR>,
        T: Send,
        E: Send,
    {
        ChannelSignalBuilder {
            receiver: self.receiver.into(),
            executor: TokioExecutor,
            initial: self.initial,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Use a custom executor function.
    ///
    /// The executor receives the receiver and signals to update when messages arrive.
    pub fn executor<F>(self, executor: F) -> ChannelSignalBuilder<R, T, E, CustomExecutor<F>, I>
    where
        F: FnOnce(
                R,
                floem_reactive::WriteSignal<T>,
                floem_reactive::WriteSignal<Option<E>>,
                ExtSendTrigger,
            ) + 'static,
    {
        ChannelSignalBuilder {
            receiver: self.receiver,
            executor: CustomExecutor(executor),
            initial: self.initial,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Set an initial value for the signal.
    ///
    /// Without this, the signal returns `Option<T>` and starts as `None`.
    /// With this, the signal returns `T` and starts with your initial value.
    pub fn initial(self, initial: T) -> ChannelSignalBuilder<R, T, E, Ex, WithInitialValue<T>> {
        ChannelSignalBuilder {
            receiver: self.receiver,
            executor: self.executor,
            initial: WithInitialValue(initial),
            _phantom: std::marker::PhantomData,
        }
    }
}

// Build with std::thread, no initial -> ChannelSignal<Option<T>, E>
impl<R, T, E> ChannelSignalBuilder<R, T, E, StdThreadExecutor, NoInitial>
where
    T: Send + 'static,
    E: Send + 'static,
    R: BlockingReceiver<Item = T, Error = E> + Send + 'static,
{
    pub fn build(self) -> ChannelSignal<Option<T>, E> {
        build_channel_signal_option(self.receiver, std_thread_receiver_option)
    }
}

// Build with std::thread, with initial -> ChannelSignal<T, E>
impl<R, T, E> ChannelSignalBuilder<R, T, E, StdThreadExecutor, WithInitialValue<T>>
where
    T: Send + 'static,
    E: Send + 'static,
    R: BlockingReceiver<Item = T, Error = E> + Send + 'static,
{
    pub fn build(self) -> ChannelSignal<T, E> {
        build_channel_signal_with_initial(self.receiver, self.initial.0, std_thread_receiver)
    }
}

// Build with event loop, no initial -> ChannelSignal<Option<T>, E>
impl<R, T, E> ChannelSignalBuilder<R, T, E, EventLoopExecutor, NoInitial>
where
    T: 'static,
    E: 'static,
    R: PollableReceiver<Item = T, Error = E> + 'static,
{
    pub fn build(self) -> ChannelSignal<Option<T>, E> {
        build_channel_signal_option(self.receiver, event_loop_receiver_option)
    }
}

// Build with event loop, with initial -> ChannelSignal<T, E>
impl<R, T, E> ChannelSignalBuilder<R, T, E, EventLoopExecutor, WithInitialValue<T>>
where
    T: 'static,
    E: 'static,
    R: PollableReceiver<Item = T, Error = E> + 'static,
{
    pub fn build(self) -> ChannelSignal<T, E> {
        build_channel_signal_with_initial(self.receiver, self.initial.0, event_loop_receiver)
    }
}

// Build with tokio, no initial -> ChannelSignal<Option<T>, E>
#[cfg(feature = "tokio")]
impl<R, T, E> ChannelSignalBuilder<R, T, E, TokioExecutor, NoInitial>
where
    T: Send + 'static,
    E: Send + 'static,
    R: BlockingReceiver<Item = T, Error = E> + Send + 'static,
{
    pub fn build(self) -> ChannelSignal<Option<T>, E> {
        build_channel_signal_option(self.receiver, tokio_spawn_blocking_receiver_option)
    }
}

// Build with tokio, with initial -> ChannelSignal<T, E>
#[cfg(feature = "tokio")]
impl<R, T, E> ChannelSignalBuilder<R, T, E, TokioExecutor, WithInitialValue<T>>
where
    T: Send + 'static,
    E: Send + 'static,
    R: BlockingReceiver<Item = T, Error = E> + Send + 'static,
{
    pub fn build(self) -> ChannelSignal<T, E> {
        build_channel_signal_with_initial(
            self.receiver,
            self.initial.0,
            tokio_spawn_blocking_receiver,
        )
    }
}

// Build with custom executor, no initial -> ChannelSignal<Option<T>, E>
impl<R, T, E, F> ChannelSignalBuilder<R, T, E, CustomExecutor<F>, NoInitial>
where
    T: 'static,
    E: 'static,
    F: FnOnce(
            R,
            floem_reactive::WriteSignal<Option<T>>,
            floem_reactive::WriteSignal<Option<E>>,
            ExtSendTrigger,
        ) + 'static,
{
    pub fn build(self) -> ChannelSignal<Option<T>, E> {
        build_channel_signal_option(self.receiver, self.executor.0)
    }
}

// Build with custom executor, with initial -> ChannelSignal<T, E>
impl<R, T, E, F> ChannelSignalBuilder<R, T, E, CustomExecutor<F>, WithInitialValue<T>>
where
    T: 'static,
    E: 'static,
    F: FnOnce(
            R,
            floem_reactive::WriteSignal<T>,
            floem_reactive::WriteSignal<Option<E>>,
            ExtSendTrigger,
        ) + 'static,
{
    pub fn build(self) -> ChannelSignal<T, E> {
        build_channel_signal_with_initial(self.receiver, self.initial.0, self.executor.0)
    }
}

fn build_channel_signal_with_initial<R, T, E, F>(
    receiver: R,
    initial: T,
    executor: F,
) -> ChannelSignal<T, E>
where
    T: 'static,
    E: 'static,
    F: FnOnce(
        R,
        floem_reactive::WriteSignal<T>,
        floem_reactive::WriteSignal<Option<E>>,
        ExtSendTrigger,
    ),
{
    use floem_reactive::*;

    let cx = Scope::current();
    let trigger = with_scope(cx, ExtSendTrigger::new);
    let (read, write) = cx.create_signal(initial);
    let (read_error, write_error) = cx.create_signal(None);

    executor(receiver, write, write_error, trigger);

    ChannelSignal {
        value: read,
        error: read_error,
    }
}

fn build_channel_signal_option<R, T, E, F>(receiver: R, executor: F) -> ChannelSignal<Option<T>, E>
where
    T: 'static,
    E: 'static,
    F: FnOnce(
        R,
        floem_reactive::WriteSignal<Option<T>>,
        floem_reactive::WriteSignal<Option<E>>,
        ExtSendTrigger,
    ),
{
    use floem_reactive::*;

    let cx = Scope::current();
    let trigger = with_scope(cx, ExtSendTrigger::new);
    let (read, write) = cx.create_signal(None);
    let (read_error, write_error) = cx.create_signal(None);

    executor(receiver, write, write_error, trigger);

    ChannelSignal {
        value: read,
        error: read_error,
    }
}

// Executor functions

/// Execute a future on the main event loop by polling, for Option<T> signals.
/// The future does not need to be `Send`.
pub fn event_loop_future_option<T: 'static>(
    future: impl std::future::Future<Output = T> + 'static,
    write: floem_reactive::WriteSignal<Option<T>>,
    write_finished: floem_reactive::WriteSignal<bool>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::*;
    use futures::task::{ArcWake, waker};
    use std::{
        cell::RefCell,
        task::{Context, Poll},
    };

    struct TriggerWake(ExtSendTrigger);
    impl ArcWake for TriggerWake {
        fn wake_by_ref(arc_self: &Arc<Self>) {
            register_ext_trigger(arc_self.0);
        }
    }

    let future = RefCell::new(Box::pin(future));
    let arc_trigger = Arc::new(TriggerWake(trigger));

    let cx = Scope::current();
    cx.create_effect(move |_| {
        trigger.track();

        let Ok(mut future) = future.try_borrow_mut() else {
            unreachable!("The waker registers events to be run only at idle")
        };

        let waker = waker(arc_trigger.clone());
        let mut context = Context::from_waker(&waker);

        match future.as_mut().poll(&mut context) {
            Poll::Pending => {}
            Poll::Ready(v) => {
                write.set(Some(v));
                write_finished.set(true);
            }
        }
    });
}

/// Execute a future on the main event loop by polling.
/// The future does not need to be `Send`.
pub fn event_loop_future<T: 'static>(
    future: impl std::future::Future<Output = T> + 'static,
    write: floem_reactive::WriteSignal<T>,
    write_finished: floem_reactive::WriteSignal<bool>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::*;
    use futures::task::{ArcWake, waker};
    use std::{
        cell::RefCell,
        task::{Context, Poll},
    };

    struct TriggerWake(ExtSendTrigger);
    impl ArcWake for TriggerWake {
        fn wake_by_ref(arc_self: &Arc<Self>) {
            register_ext_trigger(arc_self.0);
        }
    }

    let future = RefCell::new(Box::pin(future));
    let arc_trigger = Arc::new(TriggerWake(trigger));

    let cx = Scope::current();
    cx.create_effect(move |_| {
        trigger.track();

        let Ok(mut future) = future.try_borrow_mut() else {
            unreachable!("The waker registers events to be run only at idle")
        };

        let waker = waker(arc_trigger.clone());
        let mut context = Context::from_waker(&waker);

        match future.as_mut().poll(&mut context) {
            Poll::Pending => {}
            Poll::Ready(v) => {
                write.set(v);
                write_finished.set(true);
            }
        }
    });
}

/// Execute a stream on the main event loop by polling, for Option<T> signals.
/// The stream does not need to be `Send`.
pub fn event_loop_stream_option<T: 'static>(
    stream: impl futures::Stream<Item = T> + 'static,
    write: floem_reactive::WriteSignal<Option<T>>,
    write_finished: floem_reactive::WriteSignal<bool>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::*;
    use futures::task::{ArcWake, waker};
    use std::{
        cell::RefCell,
        task::{Context, Poll},
    };

    struct TriggerWake(ExtSendTrigger);
    impl ArcWake for TriggerWake {
        fn wake_by_ref(arc_self: &Arc<Self>) {
            register_ext_trigger(arc_self.0);
        }
    }

    let stream = RefCell::new(Box::pin(stream));
    let arc_trigger = Arc::new(TriggerWake(trigger));

    let cx = Scope::current();
    cx.create_effect(move |_| {
        trigger.track();

        let Ok(mut stream) = stream.try_borrow_mut() else {
            unreachable!("The waker registers events to be run only at idle")
        };

        let waker = waker(arc_trigger.clone());
        let mut context = Context::from_waker(&waker);
        let mut last_value = None;

        loop {
            let poll = stream.as_mut().poll_next(&mut context);
            match poll {
                Poll::Pending => break,
                Poll::Ready(Some(v)) => last_value = Some(v),
                Poll::Ready(None) => {
                    write_finished.set(true);
                    break;
                }
            }
        }

        if let Some(v) = last_value {
            write.set(Some(v));
        }
    });
}

/// Execute a stream on the main event loop by polling.
/// The stream does not need to be `Send`.
pub fn event_loop_stream<T: 'static>(
    stream: impl futures::Stream<Item = T> + 'static,
    write: floem_reactive::WriteSignal<T>,
    write_finished: floem_reactive::WriteSignal<bool>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::*;
    use futures::task::{ArcWake, waker};
    use std::{
        cell::RefCell,
        task::{Context, Poll},
    };

    struct TriggerWake(ExtSendTrigger);
    impl ArcWake for TriggerWake {
        fn wake_by_ref(arc_self: &Arc<Self>) {
            register_ext_trigger(arc_self.0);
        }
    }

    let stream = RefCell::new(Box::pin(stream));
    let arc_trigger = Arc::new(TriggerWake(trigger));

    let cx = Scope::current();
    cx.create_effect(move |_| {
        trigger.track();

        let Ok(mut stream) = stream.try_borrow_mut() else {
            unreachable!("The waker registers events to be run only at idle")
        };

        let waker = waker(arc_trigger.clone());
        let mut context = Context::from_waker(&waker);
        let mut last_value = None;

        loop {
            let poll = stream.as_mut().poll_next(&mut context);
            match poll {
                Poll::Pending => break,
                Poll::Ready(Some(v)) => last_value = Some(v),
                Poll::Ready(None) => {
                    write_finished.set(true);
                    break;
                }
            }
        }

        if let Some(v) = last_value {
            write.set(v);
        }
    });
}

/// Execute a pollable receiver on the main event loop, for Option<T> signals.
/// The receiver does not need to be `Send`.
pub fn event_loop_receiver_option<T: 'static, E: 'static>(
    receiver: impl PollableReceiver<Item = T, Error = E> + 'static,
    write: floem_reactive::WriteSignal<Option<T>>,
    write_error: floem_reactive::WriteSignal<Option<E>>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::*;
    use futures::task::{ArcWake, waker};
    use std::{
        cell::RefCell,
        task::{Context, Poll},
    };

    struct TriggerWake(ExtSendTrigger);
    impl ArcWake for TriggerWake {
        fn wake_by_ref(arc_self: &Arc<Self>) {
            register_ext_trigger(arc_self.0);
        }
    }

    let receiver = RefCell::new(receiver);
    let arc_trigger = Arc::new(TriggerWake(trigger));

    let cx = Scope::current();
    cx.create_effect(move |_| {
        trigger.track();

        let Ok(mut receiver) = receiver.try_borrow_mut() else {
            unreachable!("The waker registers events to be run only at idle")
        };

        let waker = waker(arc_trigger.clone());
        let mut context = Context::from_waker(&waker);
        let mut last_value = None;

        loop {
            match receiver.poll_recv(&mut context) {
                Poll::Pending => break,
                Poll::Ready(Ok(Some(v))) => last_value = Some(v),
                Poll::Ready(Ok(None)) => {
                    break;
                }
                Poll::Ready(Err(e)) => {
                    write_error.set(Some(e));
                    break;
                }
            }
        }

        if let Some(v) = last_value {
            write.set(Some(v));
        }
    });
}

/// Execute a pollable receiver on the main event loop.
/// The receiver does not need to be `Send`.
pub fn event_loop_receiver<T: 'static, E: 'static>(
    receiver: impl PollableReceiver<Item = T, Error = E> + 'static,
    write: floem_reactive::WriteSignal<T>,
    write_error: floem_reactive::WriteSignal<Option<E>>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::*;
    use futures::task::{ArcWake, waker};
    use std::{
        cell::RefCell,
        task::{Context, Poll},
    };

    struct TriggerWake(ExtSendTrigger);
    impl ArcWake for TriggerWake {
        fn wake_by_ref(arc_self: &Arc<Self>) {
            register_ext_trigger(arc_self.0);
        }
    }

    let receiver = RefCell::new(receiver);
    let arc_trigger = Arc::new(TriggerWake(trigger));

    let cx = Scope::current();
    cx.create_effect(move |_| {
        trigger.track();

        let Ok(mut receiver) = receiver.try_borrow_mut() else {
            unreachable!("The waker registers events to be run only at idle")
        };

        let waker = waker(arc_trigger.clone());
        let mut context = Context::from_waker(&waker);
        let mut last_value = None;

        loop {
            match receiver.poll_recv(&mut context) {
                Poll::Pending => break,
                Poll::Ready(Ok(Some(v))) => last_value = Some(v),
                Poll::Ready(Ok(None)) => {
                    break;
                }
                Poll::Ready(Err(e)) => {
                    write_error.set(Some(e));
                    break;
                }
            }
        }

        if let Some(v) = last_value {
            write.set(v);
        }
    });
}

/// Trait for receivers that can be polled (not blocking).
pub trait PollableReceiver {
    type Item;
    type Error;

    fn poll_recv(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<Option<Self::Item>, Self::Error>>;
}

/// Execute a blocking channel receiver on a dedicated std::thread, for Option<T> signals.
pub fn std_thread_receiver_option<T: Send + 'static, E: Send + 'static>(
    rx: impl BlockingReceiver<Item = T, Error = E> + Send + 'static,
    write: floem_reactive::WriteSignal<Option<T>>,
    write_error: floem_reactive::WriteSignal<Option<E>>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::SignalUpdate;
    let data = Arc::new(Mutex::new(VecDeque::new()));

    {
        let data = data.clone();
        let cx = floem_reactive::Scope::current();
        cx.create_effect(move |_| {
            trigger.track();
            while let Some(result) = data.lock().unwrap().pop_front() {
                match result {
                    Ok(v) => write.set(Some(v)),
                    Err(e) => write_error.set(Some(e)),
                }
            }
        });
    }

    std::thread::spawn(move || {
        let mut rx = rx;
        loop {
            match rx.recv() {
                Ok(event) => {
                    data.lock().unwrap().push_back(Ok(event));
                    register_ext_trigger(trigger);
                }
                Err(e) => {
                    data.lock().unwrap().push_back(Err(e));
                    register_ext_trigger(trigger);
                    break;
                }
            }
        }
    });
}

/// Execute a blocking channel receiver on a dedicated std::thread.
pub fn std_thread_receiver<T: Send + 'static, E: Send + 'static>(
    rx: impl BlockingReceiver<Item = T, Error = E> + Send + 'static,
    write: floem_reactive::WriteSignal<T>,
    write_error: floem_reactive::WriteSignal<Option<E>>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::SignalUpdate;
    let data = Arc::new(Mutex::new(VecDeque::new()));

    {
        let data = data.clone();
        let cx = floem_reactive::Scope::current();
        cx.create_effect(move |_| {
            trigger.track();
            while let Some(result) = data.lock().unwrap().pop_front() {
                match result {
                    Ok(v) => write.set(v),
                    Err(e) => write_error.set(Some(e)),
                }
            }
        });
    }

    std::thread::spawn(move || {
        let mut rx = rx;
        loop {
            match rx.recv() {
                Ok(event) => {
                    data.lock().unwrap().push_back(Ok(event));
                    register_ext_trigger(trigger);
                }
                Err(e) => {
                    data.lock().unwrap().push_back(Err(e));
                    register_ext_trigger(trigger);
                    break;
                }
            }
        }
    });
}

/// Trait for blocking receivers.
pub trait BlockingReceiver {
    type Item;
    type Error;

    fn recv(&mut self) -> Result<Self::Item, Self::Error>;
}

impl<T> BlockingReceiver for std::sync::mpsc::Receiver<T> {
    type Item = T;
    type Error = std::sync::mpsc::RecvError;

    fn recv(&mut self) -> Result<Self::Item, Self::Error> {
        std::sync::mpsc::Receiver::recv(self)
    }
}

#[cfg(feature = "crossbeam")]
impl<T> BlockingReceiver for crossbeam::channel::Receiver<T> {
    type Item = T;
    type Error = crossbeam::channel::RecvError;

    fn recv(&mut self) -> Result<Self::Item, Self::Error> {
        crossbeam::channel::Receiver::recv(self)
    }
}

// Tokio executors

#[cfg(feature = "tokio")]
pub fn tokio_spawn_future_option<T: Send + 'static>(
    future: impl std::future::Future<Output = T> + Send + 'static,
    write: floem_reactive::WriteSignal<Option<T>>,
    write_finished: floem_reactive::WriteSignal<bool>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::SignalUpdate;

    let data = Arc::new(Mutex::new(None));
    let finished = Arc::new(Mutex::new(false));

    {
        let data = data.clone();
        let finished = finished.clone();
        let cx = floem_reactive::Scope::current();
        cx.create_effect(move |_| {
            trigger.track();

            if let Some(value) = data.lock().unwrap().take() {
                write.set(Some(value));
            }

            if *finished.lock().unwrap() {
                write_finished.set(true);
            }
        });
    }

    tokio::spawn(async move {
        let value = future.await;
        *data.lock().unwrap() = Some(value);
        *finished.lock().unwrap() = true;
        register_ext_trigger(trigger);
    });
}

#[cfg(feature = "tokio")]
pub fn tokio_spawn_stream_option<T: Send + 'static>(
    stream: impl futures::Stream<Item = T> + Send + 'static,
    write: floem_reactive::WriteSignal<Option<T>>,
    write_finished: floem_reactive::WriteSignal<bool>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::SignalUpdate;
    use futures::StreamExt;

    let data = Arc::new(Mutex::new(VecDeque::new()));
    let finished = Arc::new(Mutex::new(false));

    {
        let data = data.clone();
        let finished = finished.clone();
        let cx = floem_reactive::Scope::current();
        cx.create_effect(move |_| {
            trigger.track();

            while let Some(value) = data.lock().unwrap().pop_front() {
                write.set(Some(value));
            }

            if *finished.lock().unwrap() {
                write_finished.set(true);
            }
        });
    }

    tokio::spawn(async move {
        let mut stream = Box::pin(stream);

        while let Some(value) = stream.next().await {
            data.lock().unwrap().push_back(value);
            register_ext_trigger(trigger);
        }

        *finished.lock().unwrap() = true;
        register_ext_trigger(trigger);
    });
}

#[cfg(feature = "tokio")]
pub fn tokio_spawn_future<T: Send + 'static>(
    future: impl std::future::Future<Output = T> + Send + 'static,
    write: floem_reactive::WriteSignal<T>,
    write_finished: floem_reactive::WriteSignal<bool>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::SignalUpdate;

    let data = Arc::new(Mutex::new(None));
    let finished = Arc::new(Mutex::new(false));

    {
        let data = data.clone();
        let finished = finished.clone();
        let cx = floem_reactive::Scope::current();
        cx.create_effect(move |_| {
            trigger.track();

            if let Some(value) = data.lock().unwrap().take() {
                write.set(value);
            }

            if *finished.lock().unwrap() {
                write_finished.set(true);
            }
        });
    }

    tokio::spawn(async move {
        let value = future.await;
        *data.lock().unwrap() = Some(value);
        *finished.lock().unwrap() = true;
        register_ext_trigger(trigger);
    });
}

#[cfg(feature = "tokio")]
pub fn tokio_spawn_stream<T: Send + 'static>(
    stream: impl futures::Stream<Item = T> + Send + 'static,
    write: floem_reactive::WriteSignal<T>,
    write_finished: floem_reactive::WriteSignal<bool>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::SignalUpdate;
    use futures::StreamExt;

    let data = Arc::new(Mutex::new(VecDeque::new()));
    let finished = Arc::new(Mutex::new(false));

    {
        let data = data.clone();
        let finished = finished.clone();
        let cx = floem_reactive::Scope::current();
        cx.create_effect(move |_| {
            trigger.track();

            while let Some(value) = data.lock().unwrap().pop_front() {
                write.set(value);
            }

            if *finished.lock().unwrap() {
                write_finished.set(true);
            }
        });
    }

    tokio::spawn(async move {
        let mut stream = Box::pin(stream);

        while let Some(value) = stream.next().await {
            data.lock().unwrap().push_back(value);
            register_ext_trigger(trigger);
        }

        *finished.lock().unwrap() = true;
        register_ext_trigger(trigger);
    });
}

#[cfg(feature = "tokio")]
pub fn tokio_spawn_blocking_receiver_option<T: Send + 'static, E: Send + 'static>(
    rx: impl BlockingReceiver<Item = T, Error = E> + Send + 'static,
    write: floem_reactive::WriteSignal<Option<T>>,
    write_error: floem_reactive::WriteSignal<Option<E>>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::SignalUpdate;

    let data = Arc::new(Mutex::new(VecDeque::new()));

    {
        let data = data.clone();
        let cx = floem_reactive::Scope::current();
        cx.create_effect(move |_| {
            trigger.track();
            while let Some(result) = data.lock().unwrap().pop_front() {
                match result {
                    Ok(v) => write.set(Some(v)),
                    Err(e) => write_error.set(Some(e)),
                }
            }
        });
    }

    tokio::task::spawn_blocking(move || {
        let mut rx = rx;
        loop {
            match rx.recv() {
                Ok(event) => {
                    data.lock().unwrap().push_back(Ok(event));
                    register_ext_trigger(trigger);
                }
                Err(e) => {
                    data.lock().unwrap().push_back(Err(e));
                    register_ext_trigger(trigger);
                    break;
                }
            }
        }
    });
}

#[cfg(feature = "tokio")]
pub fn tokio_spawn_blocking_receiver<T: Send + 'static, E: Send + 'static>(
    rx: impl BlockingReceiver<Item = T, Error = E> + Send + 'static,
    write: floem_reactive::WriteSignal<T>,
    write_error: floem_reactive::WriteSignal<Option<E>>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::SignalUpdate;

    let data = Arc::new(Mutex::new(VecDeque::new()));

    {
        let data = data.clone();
        let cx = floem_reactive::Scope::current();
        cx.create_effect(move |_| {
            trigger.track();
            while let Some(result) = data.lock().unwrap().pop_front() {
                match result {
                    Ok(v) => write.set(v),
                    Err(e) => write_error.set(Some(e)),
                }
            }
        });
    }

    tokio::task::spawn_blocking(move || {
        let mut rx = rx;
        loop {
            match rx.recv() {
                Ok(event) => {
                    data.lock().unwrap().push_back(Ok(event));
                    register_ext_trigger(trigger);
                }
                Err(e) => {
                    data.lock().unwrap().push_back(Err(e));
                    register_ext_trigger(trigger);
                    break;
                }
            }
        }
    });
}

// Implement PollableReceiver for tokio channels

#[cfg(feature = "tokio")]
impl<T> PollableReceiver for tokio::sync::mpsc::Receiver<T> {
    type Item = T;
    type Error = ChannelClosed;

    fn poll_recv(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<Option<Self::Item>, Self::Error>> {
        use std::future::Future;
        let recv_fut = self.recv();
        let mut recv_fut = std::pin::pin!(recv_fut);

        match recv_fut.as_mut().poll(cx) {
            std::task::Poll::Pending => std::task::Poll::Pending,
            std::task::Poll::Ready(Some(v)) => std::task::Poll::Ready(Ok(Some(v))),
            std::task::Poll::Ready(None) => std::task::Poll::Ready(Err(ChannelClosed)),
        }
    }
}

#[cfg(feature = "tokio")]
impl<T> PollableReceiver for tokio::sync::mpsc::UnboundedReceiver<T> {
    type Item = T;
    type Error = ChannelClosed;

    fn poll_recv(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<Option<Self::Item>, Self::Error>> {
        use std::future::Future;
        let recv_fut = self.recv();
        let mut recv_fut = std::pin::pin!(recv_fut);

        match recv_fut.as_mut().poll(cx) {
            std::task::Poll::Pending => std::task::Poll::Pending,
            std::task::Poll::Ready(Some(v)) => std::task::Poll::Ready(Ok(Some(v))),
            std::task::Poll::Ready(None) => std::task::Poll::Ready(Err(ChannelClosed)),
        }
    }
}

#[cfg(feature = "tokio")]
impl<T: Clone> PollableReceiver for tokio::sync::broadcast::Receiver<T> {
    type Item = T;
    type Error = tokio::sync::broadcast::error::RecvError;

    fn poll_recv(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<Option<Self::Item>, Self::Error>> {
        use std::future::Future;
        let recv_fut = self.recv();
        let mut recv_fut = std::pin::pin!(recv_fut);

        match recv_fut.as_mut().poll(cx) {
            std::task::Poll::Pending => std::task::Poll::Pending,
            std::task::Poll::Ready(Ok(v)) => std::task::Poll::Ready(Ok(Some(v))),
            std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Err(e)),
        }
    }
}
