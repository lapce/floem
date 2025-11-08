use super::traits::{BlockingReceiver, PollableReceiver};
use crate::ext_event::{ExtSendTrigger, register_ext_trigger};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub use std_thread_receiver as std_thread_channel;
pub use std_thread_receiver_option as std_thread_channel_option;

#[cfg(feature = "tokio")]
pub use tokio_spawn_blocking_receiver as tokio_spawn_blocking_channel;
#[cfg(feature = "tokio")]
pub use tokio_spawn_blocking_receiver_option as tokio_spawn_blocking_channel_option;

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

#[cfg(feature = "tokio")]
pub fn tokio_spawn_poll_receiver_option<T: Send + 'static, E: Send + 'static>(
    receiver: impl PollableReceiver<Item = T, Error = E> + Send + 'static,
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

    tokio::spawn(async move {
        let mut receiver = receiver;
        loop {
            use std::future::poll_fn;

            match poll_fn(|cx| receiver.poll_recv(cx)).await {
                Ok(Some(event)) => {
                    data.lock().unwrap().push_back(Ok(event));
                    register_ext_trigger(trigger);
                }
                Ok(None) => break,
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
pub fn tokio_spawn_poll_receiver<T: Send + 'static, E: Send + 'static>(
    receiver: impl PollableReceiver<Item = T, Error = E> + Send + 'static,
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

    tokio::spawn(async move {
        let mut receiver = receiver;
        loop {
            use std::future::poll_fn;

            match poll_fn(|cx| receiver.poll_recv(cx)).await {
                Ok(Some(event)) => {
                    data.lock().unwrap().push_back(Ok(event));
                    register_ext_trigger(trigger);
                }
                Ok(None) => break,
                Err(e) => {
                    data.lock().unwrap().push_back(Err(e));
                    register_ext_trigger(trigger);
                    break;
                }
            }
        }
    });
}
