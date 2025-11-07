//! Reactive signals for channels that produce multiple values with error handling.

mod builder;

use super::common::{BlockingReceiver, NoInitial, PollableReceiver, StdThreadExecutor};
use builder::ChannelSignalBuilder;

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
    value: floem_reactive::ReadSignal<T>,
    error: floem_reactive::ReadSignal<Option<E>>,
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

    pub fn new_poll<R>(receiver: R) -> ChannelSignal<Option<T>, E>
    where
        R: PollableReceiver<Item = T, Error = E> + Send + 'static,
        T: Send + 'static,
        E: Send + 'static,
    {
        ChannelSignalBuilder::new(receiver).event_loop().build()
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

impl<T, E> floem_reactive::SignalTrack<T> for ChannelSignal<T, E> {
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
