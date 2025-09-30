use crate::ext_event::{register_ext_trigger, ExtSendTrigger};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub mod resource;
pub use resource::*;

/// A reactive signal for futures that resolve to a single value.
pub struct FutureSignal<T> {
    pub value: floem_reactive::ReadSignal<Option<T>>,
}

impl<T: 'static> FutureSignal<T> {
    /// Execute the future using a custom executor function.
    ///
    /// The executor function receives the future, write signals, and trigger,
    /// and is responsible for running the future and updating the signals.
    pub fn on_executor<F, Fut>(future: Fut, executor: F) -> Self
    where
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
        let (_read_finished, write_finished) = cx.create_signal(false);

        executor(future, write, write_finished, trigger);

        Self { value: read }
    }

    /// Execute the future on the main event loop by polling.
    /// The future does not need to be `Send`.
    pub fn on_event_loop(future: impl std::future::Future<Output = T> + 'static) -> Self
    where
        T: 'static,
    {
        Self::on_executor(future, event_loop_future)
    }

    /// Execute the future on tokio::spawn.
    /// Requires the future to be `Send + 'static`.
    #[cfg(feature = "tokio")]
    pub fn on_tokio_spawn(future: impl std::future::Future<Output = T> + Send + 'static) -> Self
    where
        T: Send + 'static,
    {
        Self::on_executor(future, tokio_spawn_future)
    }

    pub fn is_finished(&self) -> bool {
        use floem_reactive::SignalWith;
        self.value.with(|v| v.is_some())
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

/// A reactive signal for streams that produce multiple values.
pub struct StreamSignal<T> {
    pub value: floem_reactive::ReadSignal<Option<T>>,
    pub finished: floem_reactive::ReadSignal<bool>,
}

/// This implementation will be removed in the future
impl<T: 'static, S> From<S> for StreamSignal<T>
where
    S: futures::Stream<Item = T> + 'static,
{
    fn from(value: S) -> Self {
        StreamSignal::on_event_loop(value)
    }
}

impl<T: 'static> StreamSignal<T> {
    /// Execute the stream using a custom executor function.
    ///
    /// The executor function receives the stream, write signals, and trigger,
    /// and is responsible for running the stream and updating the signals.
    pub fn on_executor<F, S>(stream: S, executor: F) -> Self
    where
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

        Self {
            value: read,
            finished: read_finished,
        }
    }

    /// Execute the stream on the main event loop by driving the future using the event loop.
    /// The stream does not need to be `Send`.
    pub fn on_event_loop(stream: impl futures::Stream<Item = T> + 'static) -> Self
    where
        T: 'static,
    {
        Self::on_executor(stream, event_loop_stream)
    }

    /// Execute the stream on tokio::spawn.
    /// Requires the stream to be `Send + 'static`.
    #[cfg(feature = "tokio")]
    pub fn on_tokio_spawn(stream: impl futures::Stream<Item = T> + Send + 'static) -> Self
    where
        T: Send + 'static,
    {
        Self::on_executor(stream, tokio_spawn_stream)
    }

    pub fn is_finished(&self) -> bool {
        use floem_reactive::SignalGet;
        self.finished.get()
    }
}

impl<T> floem_reactive::SignalGet<Option<T>> for StreamSignal<T>
where
    T: Clone,
{
    fn id(&self) -> floem_reactive::ReactiveId {
        self.value.id()
    }
}

impl<T> floem_reactive::SignalWith<Option<T>> for StreamSignal<T> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.value.id()
    }
}

impl<T> floem_reactive::SignalRead<Option<T>> for StreamSignal<T> {
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
pub struct ChannelSignal<T, E> {
    pub value: floem_reactive::ReadSignal<Option<T>>,
    pub error: floem_reactive::ReadSignal<Option<E>>,
}

/// This implementation will be removed in the future
#[cfg(feature = "crossbeam")]
impl<T: std::marker::Send + 'static> From<crossbeam::channel::Receiver<T>>
    for ChannelSignal<T, crossbeam::channel::RecvError>
{
    fn from(value: crossbeam::channel::Receiver<T>) -> Self {
        ChannelSignal::on_std_thread(value)
    }
}
/// This implementation will be removed in the future
impl<T: std::marker::Send + 'static> From<std::sync::mpsc::Receiver<T>>
    for ChannelSignal<T, std::sync::mpsc::RecvError>
{
    fn from(value: std::sync::mpsc::Receiver<T>) -> Self {
        ChannelSignal::on_std_thread(value)
    }
}

impl<T: 'static, E: 'static> ChannelSignal<T, E> {
    /// Execute the channel receiver using a custom executor function.
    ///
    /// The executor function receives the receiver, write signals, and trigger,
    /// and is responsible for receiving messages and updating the signals.
    pub fn on_executor<F, R>(receiver: R, executor: F) -> Self
    where
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

        Self {
            value: read,
            error: read_error,
        }
    }

    /// Execute a pollable receiver on the main event loop.
    /// The receiver does not need to be `Send`.
    pub fn on_event_loop(receiver: impl PollableReceiver<Item = T, Error = E> + 'static) -> Self
    where
        T: 'static,
        E: 'static,
    {
        Self::on_executor(receiver, event_loop_receiver)
    }

    /// Execute a blocking receiver on a dedicated std::thread.
    /// Requires the receiver to be `Send + 'static`.
    pub fn on_std_thread(
        receiver: impl BlockingReceiver<Item = T, Error = E> + Send + 'static,
    ) -> Self
    where
        T: Send + 'static,
        E: Send + 'static,
    {
        Self::on_executor(receiver, std_thread_receiver)
    }

    /// Execute a blocking receiver on tokio::task::spawn_blocking.
    /// Requires the receiver to be `Send + 'static`.
    #[cfg(feature = "tokio")]
    pub fn on_tokio_spawn_blocking(
        receiver: impl BlockingReceiver<Item = T, Error = E> + Send + 'static,
    ) -> Self
    where
        T: Send + 'static,
        E: Send + 'static,
    {
        Self::on_executor(receiver, tokio_spawn_blocking_receiver)
    }

    pub fn error(&self) -> floem_reactive::ReadSignal<Option<E>> {
        self.error
    }
}

impl<T, E> floem_reactive::SignalGet<Option<T>> for ChannelSignal<T, E>
where
    T: Clone,
{
    fn id(&self) -> floem_reactive::ReactiveId {
        self.value.id()
    }
}

impl<T, E> floem_reactive::SignalWith<Option<T>> for ChannelSignal<T, E> {
    fn id(&self) -> floem_reactive::ReactiveId {
        self.value.id()
    }
}

impl<T, E> floem_reactive::SignalRead<Option<T>> for ChannelSignal<T, E> {
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

// Executor functions

/// Execute a future on the main event loop by polling.
/// The future does not need to be `Send`.
pub fn event_loop_future<T: 'static>(
    future: impl std::future::Future<Output = T> + 'static,
    write: floem_reactive::WriteSignal<Option<T>>,
    write_finished: floem_reactive::WriteSignal<bool>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::*;
    use futures::task::{waker, ArcWake};
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

/// Execute a stream on the main event loop by polling.
/// The stream does not need to be `Send`.
pub fn event_loop_stream<T: 'static>(
    stream: impl futures::Stream<Item = T> + 'static,
    write: floem_reactive::WriteSignal<Option<T>>,
    write_finished: floem_reactive::WriteSignal<bool>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::*;
    use futures::task::{waker, ArcWake};
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

/// Execute a pollable receiver on the main event loop.
/// The receiver does not need to be `Send`.
pub fn event_loop_receiver<T: 'static, E: 'static>(
    receiver: impl PollableReceiver<Item = T, Error = E> + 'static,
    write: floem_reactive::WriteSignal<Option<T>>,
    write_error: floem_reactive::WriteSignal<Option<E>>,
    trigger: ExtSendTrigger,
) {
    use floem_reactive::*;
    use futures::task::{waker, ArcWake};
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

/// Trait for receivers that can be polled (not blocking).
pub trait PollableReceiver {
    type Item;
    type Error;

    fn poll_recv(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<Option<Self::Item>, Self::Error>>;
}

/// Execute a blocking channel receiver on a dedicated std::thread.
pub fn std_thread_receiver<T: Send + 'static, E: Send + 'static>(
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
pub fn tokio_spawn_future<T: Send + 'static>(
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
pub fn tokio_spawn_stream<T: Send + 'static>(
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
pub fn tokio_spawn_blocking_receiver<T: Send + 'static, E: Send + 'static>(
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
