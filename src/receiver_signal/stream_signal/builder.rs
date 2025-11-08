use crate::ext_event::ExtSendTrigger;

use super::{
    super::common::{CustomExecutor, EventLoopExecutor, NoInitial, WithInitialValue, executors::*},
    StreamSignal,
};

#[cfg(feature = "tokio")]
use super::super::common::TokioExecutor;

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
    pub(super) fn new(stream: S) -> Self {
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
