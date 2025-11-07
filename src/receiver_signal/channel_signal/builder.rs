use crate::ext_event::ExtSendTrigger;

use super::{
    super::common::{
        BlockingReceiver, CustomExecutor, EventLoopExecutor, NoInitial, PollableReceiver,
        StdThreadExecutor, WithInitialValue, executors::*,
    },
    ChannelSignal,
};

#[cfg(feature = "tokio")]
use super::super::common::{TokioBlockingExecutor, TokioExecutor};

/// A builder for creating customized `ChannelSignal` instances.
///
/// Created via `ChannelSignal::custom()`. Allows fine-grained control over:
/// - Executor type (event loop, tokio, std::thread, custom)
/// - Initial value
pub struct ChannelSignalBuilder<R, T, E, Ex, I> {
    receiver: R,
    executor: Ex,
    initial: I,
    _phantom: std::marker::PhantomData<(T, E)>,
}

impl<R, T, E> ChannelSignalBuilder<R, T, E, StdThreadExecutor, NoInitial>
where
    T: Send + 'static,
    E: Send + 'static,
{
    pub(super) fn new(receiver: R) -> Self {
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
    T: Send + 'static,
    E: Send + 'static,
{
    /// Use the main event loop as the executor.
    ///
    /// The receiver must implement `PollableReceiver`. The receiver does not need to be `Send`.
    pub fn event_loop(self) -> ChannelSignalBuilder<R, T, E, EventLoopExecutor, I>
    where
        R: PollableReceiver<Item = T, Error = E> + 'static,
    {
        ChannelSignalBuilder {
            receiver: self.receiver,
            executor: EventLoopExecutor,
            initial: self.initial,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Use std::thread::spawn as the executor.
    ///
    /// This is the default for blocking receivers.
    pub fn std_thread(self) -> ChannelSignalBuilder<R, T, E, StdThreadExecutor, I> {
        ChannelSignalBuilder {
            receiver: self.receiver,
            executor: StdThreadExecutor,
            initial: self.initial,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Use tokio::spawn_blocking as the executor.
    #[cfg(feature = "tokio")]
    pub fn tokio_spawn_blocking(self) -> ChannelSignalBuilder<R, T, E, TokioBlockingExecutor, I> {
        ChannelSignalBuilder {
            receiver: self.receiver,
            executor: TokioBlockingExecutor,
            initial: self.initial,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Use a custom executor function.
    ///
    /// The executor receives the receiver and signals to update when values arrive.
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

impl<R, T, E, Ex, I> ChannelSignalBuilder<R, T, E, Ex, I>
where
    R: PollableReceiver<Item = T, Error = E>,
    T: Send + 'static,
    E: Send + 'static,
{
    /// Use tokio::spawn as the executor.
    ///
    /// The receiver must be `Send + 'static`. Requires the `tokio` feature.
    #[cfg(feature = "tokio")]
    pub fn tokio_spawn(self) -> ChannelSignalBuilder<R, T, E, TokioExecutor, I>
    where
        R: Send,
    {
        ChannelSignalBuilder {
            receiver: self.receiver,
            executor: TokioExecutor,
            initial: self.initial,
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
        build_channel_signal_option(self.receiver, std_thread_channel_option)
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
        build_channel_signal_with_initial(self.receiver, self.initial.0, std_thread_channel)
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

// Build with tokio::spawn_blocking, no initial -> ChannelSignal<Option<T>, E>
#[cfg(feature = "tokio")]
impl<R, T, E> ChannelSignalBuilder<R, T, E, TokioBlockingExecutor, NoInitial>
where
    T: Send + 'static,
    E: Send + 'static,
    R: BlockingReceiver<Item = T, Error = E> + Send + 'static,
{
    pub fn build(self) -> ChannelSignal<Option<T>, E> {
        build_channel_signal_option(self.receiver, tokio_spawn_blocking_channel_option)
    }
}

// Build with tokio::spawn_blocking, no initial -> ChannelSignal<Option<T>, E>
#[cfg(feature = "tokio")]
impl<R, T, E> ChannelSignalBuilder<R, T, E, TokioBlockingExecutor, WithInitialValue<T>>
where
    T: Send + 'static,
    E: Send + 'static,
    R: BlockingReceiver<Item = T, Error = E> + Send + 'static,
{
    pub fn build(self) -> ChannelSignal<T, E> {
        build_channel_signal_with_initial(
            self.receiver,
            self.initial.0,
            tokio_spawn_blocking_channel,
        )
    }
}

// Build with tokio, no initial -> ChannelSignal<Option<T>, E>
#[cfg(feature = "tokio")]
impl<R, T, E> ChannelSignalBuilder<R, T, E, TokioExecutor, NoInitial>
where
    T: Send + 'static,
    E: Send + 'static,
    R: PollableReceiver<Item = T, Error = E> + Send + 'static,
{
    pub fn build(self) -> ChannelSignal<Option<T>, E> {
        build_channel_signal_option(self.receiver, tokio_spawn_poll_receiver_option)
    }
}

// Build with tokio, with initial -> ChannelSignal<T, E>
#[cfg(feature = "tokio")]
impl<R, T, E> ChannelSignalBuilder<R, T, E, TokioExecutor, WithInitialValue<T>>
where
    T: Send + 'static,
    E: Send + 'static,
    R: PollableReceiver<Item = T, Error = E> + Send + 'static,
{
    pub fn build(self) -> ChannelSignal<T, E> {
        build_channel_signal_with_initial(self.receiver, self.initial.0, tokio_spawn_poll_receiver)
    }
}

// Build with custom executor, no initial -> ChannelSignal<Option<T>, E>
impl<R, T, E, F> ChannelSignalBuilder<R, T, E, CustomExecutor<F>, NoInitial>
where
    T: 'static,
    E: 'static,
    R: BlockingReceiver<Item = T, Error = E> + 'static,
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
    R: BlockingReceiver<Item = T, Error = E> + 'static,
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
    R: 'static,
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
    R: 'static,
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
