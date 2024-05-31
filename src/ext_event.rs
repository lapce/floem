use std::{cell::Cell, collections::VecDeque, sync::Arc};

use floem_reactive::{create_effect, untrack, with_scope, ReadSignal, Scope, Trigger, WriteSignal};
use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::{
    app::UserEvent,
    window_handle::{get_current_view, set_current_view},
    Application,
};

pub(crate) static EXT_EVENT_HANDLER: Lazy<ExtEventHandler> = Lazy::new(ExtEventHandler::default);

#[derive(Clone)]
pub struct ExtEventHandler {
    pub(crate) queue: Arc<Mutex<VecDeque<Trigger>>>,
}

impl Default for ExtEventHandler {
    fn default() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

impl ExtEventHandler {
    pub fn add_trigger(&self, trigger: Trigger) {
        EXT_EVENT_HANDLER.queue.lock().push_back(trigger);
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
