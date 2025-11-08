use crate::ext_event::ExtSendTrigger;

use super::{
    super::common::{CustomExecutor, EventLoopExecutor},
    super::executors::*,
    FutureSignal,
};

#[cfg(feature = "tokio")]
use super::super::common::TokioExecutor;

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
    pub(super) fn new(future: Fut) -> Self {
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
