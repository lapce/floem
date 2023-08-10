use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use floem_reactive::{create_effect, untrack, with_scope, ReadSignal, Scope, Trigger};
use glazier::{IdleHandle, IdleToken};
// use leptos_reactive::{create_signal, create_trigger, untrack, ReadSignal, SignalSet, Trigger};
use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::id::WindowId;

pub static EXT_EVENT_HANDLER: Lazy<ExtEventHandler> = Lazy::new(ExtEventHandler::default);

#[derive(Clone)]
pub struct ExtEventHandler {
    pub(crate) queue: Arc<Mutex<VecDeque<Trigger>>>,
    pub(crate) handle: Arc<Mutex<HashMap<WindowId, IdleHandle>>>,
}

impl Default for ExtEventHandler {
    fn default() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            handle: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl ExtEventHandler {
    pub fn add_trigger(&self, trigger: Trigger) {
        EXT_EVENT_HANDLER.queue.lock().push_back(trigger);
        for (_, handle) in EXT_EVENT_HANDLER.handle.lock().iter_mut() {
            handle.schedule_idle(IdleToken::new(0));
        }
    }
}

pub fn create_ext_action<T: Send + 'static>(
    cx: Scope,
    action: impl Fn(T) + 'static,
) -> impl FnOnce(T) {
    let cx = cx.create_child();
    let trigger = cx.create_trigger();
    let data = Arc::new(Mutex::new(None));

    {
        let data = data.clone();
        with_scope(cx, move || {
            create_effect(move |_| {
                trigger.track();
                if let Some(event) = data.lock().take() {
                    untrack(|| {
                        action(event);
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
