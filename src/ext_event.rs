use std::{cell::Cell, collections::VecDeque, sync::Arc};

use floem_reactive::{create_effect, untrack, with_scope, ReadSignal, Scope, Trigger, WriteSignal};
use parking_lot::Mutex;

use crate::{
    app::UserEvent,
    window_handle::{get_current_view, set_current_view},
    Application,
};

pub(crate) static EXT_EVENT_HANDLER: ExtEventHandler = ExtEventHandler::new();

pub(crate) struct ExtEventHandler {
    pub(crate) queue: Mutex<VecDeque<Trigger>>,
}

impl Default for ExtEventHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtEventHandler {
    pub const fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
        }
    }

    pub fn add_trigger(&self, trigger: Trigger) {
        {
            // Run this in a short block to prevent any deadlock if running the trigger effects
            // causes another trigger to be registered
            EXT_EVENT_HANDLER.queue.lock().push_back(trigger);
        }
        Application::with_event_loop_proxy(|proxy| {
            let _ = proxy.send_event(UserEvent::Idle);
        });
    }
}

pub fn register_ext_trigger(trigger: Trigger) {
    EXT_EVENT_HANDLER.add_trigger(trigger);
}

pub fn create_ext_action<T: Send + 'static>(
    cx: Scope,
    action: impl FnOnce(T) + 'static,
) -> impl FnOnce(T) {
    let view = get_current_view();
    let cx = cx.create_child();
    let trigger = cx.create_trigger();
    let data = Arc::new(Mutex::new(None));

    {
        let data = data.clone();
        let action = Cell::new(Some(action));
        with_scope(cx, move || {
            create_effect(move |_| {
                trigger.track();
                if let Some(event) = data.lock().take() {
                    untrack(|| {
                        let current_view = get_current_view();
                        set_current_view(view);
                        let action = action.take().unwrap();
                        action(event);
                        set_current_view(current_view);
                    });
                    cx.dispose();
                }
            });
        });
    }

    move |event| {
        *data.lock() = Some(event);
        EXT_EVENT_HANDLER.add_trigger(trigger);
    }
}

pub fn update_signal_from_channel<T: Send + 'static>(
    writer: WriteSignal<Option<T>>,
    rx: crossbeam_channel::Receiver<T>,
) {
    let cx = Scope::new();
    let trigger = cx.create_trigger();

    let channel_closed = cx.create_rw_signal(false);
    let data = Arc::new(Mutex::new(VecDeque::new()));

    {
        let data = data.clone();
        cx.create_effect(move |_| {
            trigger.track();
            while let Some(value) = data.lock().pop_front() {
                writer.try_set(value);
            }

            if channel_closed.get() {
                cx.dispose();
            }
        });
    }

    let send = create_ext_action(cx, move |_| {
        channel_closed.set(true);
    });

    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            data.lock().push_back(Some(event));
            EXT_EVENT_HANDLER.add_trigger(trigger);
        }
        send(());
    });
}

pub fn create_signal_from_channel<T: Send + 'static>(
    rx: crossbeam_channel::Receiver<T>,
) -> ReadSignal<Option<T>> {
    let cx = Scope::new();
    let trigger = cx.create_trigger();

    let channel_closed = cx.create_rw_signal(false);
    let (read, write) = cx.create_signal(None);
    let data = Arc::new(Mutex::new(VecDeque::new()));

    {
        let data = data.clone();
        cx.create_effect(move |_| {
            trigger.track();
            while let Some(value) = data.lock().pop_front() {
                write.set(value);
            }

            if channel_closed.get() {
                cx.dispose();
            }
        });
    }

    let send = create_ext_action(cx, move |_| {
        channel_closed.set(true);
    });

    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            data.lock().push_back(Some(event));
            EXT_EVENT_HANDLER.add_trigger(trigger);
        }
        send(());
    });

    read
}

#[cfg(feature = "tokio")]
pub fn create_signal_from_tokio_channel<T: Send + 'static>(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<T>,
) -> ReadSignal<Option<T>> {
    let cx = Scope::new();
    let trigger = cx.create_trigger();

    let channel_closed = cx.create_rw_signal(false);
    let (read, write) = cx.create_signal(None);
    let data = std::sync::Arc::new(std::sync::Mutex::new(VecDeque::new()));

    {
        let data = data.clone();
        cx.create_effect(move |_| {
            trigger.track();
            while let Some(value) = data.lock().unwrap().pop_front() {
                write.set(value);
            }

            if channel_closed.get() {
                cx.dispose();
            }
        });
    }

    let send = create_ext_action(cx, move |_| {
        channel_closed.set(true);
    });

    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            data.lock().unwrap().push_back(Some(event));
            crate::ext_event::register_ext_trigger(trigger);
        }
        send(());
    });

    read
}

#[cfg(feature = "futures")]
pub fn create_signal_from_stream<T: 'static>(
    inital_value: T,
    stream: impl futures::Stream<Item = T> + 'static,
) -> ReadSignal<T> {
    use std::{
        cell::RefCell,
        task::{Context, Poll},
    };

    use futures::task::{waker, ArcWake};

    let cx = Scope::current().create_child();
    let trigger = cx.create_trigger();
    let (read, write) = cx.create_signal(inital_value);

    /// Waker that wakes by registering a trigger

    // TODO: since the trigger is just a `u64`, it could theorically be changed to be a `usize`,
    //       Then the implementation of the std::task::RawWakerVTable could pass the `usize` as the data pointer,
    //       avoiding any allocation/reference counting
    struct TriggerWake(Trigger);
    impl ArcWake for TriggerWake {
        fn wake_by_ref(arc_self: &Arc<Self>) {
            EXT_EVENT_HANDLER.add_trigger(arc_self.0);
        }
    }

    // We need a refcell because effects are `Fn` and not `FnMut`
    let stream = RefCell::new(Box::pin(stream));
    let arc_trigger = Arc::new(TriggerWake(trigger));

    cx.create_effect(move |_| {
        // Run the effect when the waker is called
        trigger.track();
        let Ok(mut stream) = stream.try_borrow_mut() else {
            // Race condition, we need to re-schedule-add the trigger?
            // I'm not sure if this does not risks creating an infinite loop if the effect is re-run before it is itself over
            // TODO: test with stream where the implementation returns pending but calls the waker immediately
            EXT_EVENT_HANDLER.add_trigger(arc_trigger.0);
            return;
        };

        let waker = waker(arc_trigger.clone());
        let mut context = Context::from_waker(&waker);

        let mut last_value = None;
        // Wee need to loop because if the stream returns `Poll::Ready`, it can discard the waker until
        // `poll_next` is called again, because it assumes that the task is performing other things
        loop {
            let poll = stream.as_mut().poll_next(&mut context);
            match poll {
                Poll::Pending => break,
                Poll::Ready(Some(v)) => last_value = Some(v),
                Poll::Ready(None) => {
                    // The stream is closed, the effect and the trigger will not be used anymore
                    cx.dispose();
                    break;
                }
            }
        }
        // Only write once to the signal
        if let Some(v) = last_value {
            write.set(v);
        }
    });

    read
}
