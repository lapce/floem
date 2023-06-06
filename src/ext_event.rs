use std::{collections::VecDeque, sync::Arc};

use glazier::{IdleHandle, IdleToken};
use leptos_reactive::{
    create_effect, create_signal, create_trigger, ReadSignal, Scope, SignalSet, Trigger,
};
use once_cell::sync::Lazy;
use parking_lot::Mutex;

pub static EXT_EVENT_HANDLER: Lazy<ExtEventHandler> = Lazy::new(ExtEventHandler::default);

#[derive(Clone)]
pub struct ExtEventHandler {
    pub(crate) queue: Arc<Mutex<VecDeque<Trigger>>>,
    pub(crate) handle: Arc<Mutex<Option<IdleHandle>>>,
}

impl Default for ExtEventHandler {
    fn default() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            handle: Arc::new(Mutex::new(None)),
        }
    }
}

impl ExtEventHandler {
    pub fn add_trigger(&self, trigger: Trigger) {
        EXT_EVENT_HANDLER.queue.lock().push_back(trigger);
        if let Some(handle) = EXT_EVENT_HANDLER.handle.lock().as_mut() {
            handle.schedule_idle(IdleToken::new(0));
        }
    }
}

pub fn create_ext_action<T: Send + 'static>(
    cx: Scope,
    action: impl Fn(T) + 'static,
) -> impl FnOnce(T) {
    let (cx, _) = cx.run_child_scope(|cx| cx);
    let trigger = create_trigger(cx);
    let data = Arc::new(Mutex::new(None));

    {
        let data = data.clone();
        create_effect(cx, move |_| {
            trigger.track();
            if let Some(event) = data.lock().take() {
                cx.untrack(|| {
                    action(event);
                });
                cx.dispose();
            }
        });
    }

    move |event| {
        *data.lock() = Some(event);
        EXT_EVENT_HANDLER.add_trigger(trigger);
    }
}

pub fn create_signal_from_channel<T: Send>(
    cx: Scope,
    rx: crossbeam_channel::Receiver<T>,
) -> ReadSignal<Option<T>> {
    let (cx, _) = cx.run_child_scope(|cx| cx);
    let trigger = create_trigger(cx);

    let (read, write) = create_signal(cx, None);
    let data = Arc::new(Mutex::new(VecDeque::new()));

    {
        let data = data.clone();
        create_effect(cx, move |_| {
            trigger.track();
            while let Some(value) = data.lock().pop_front() {
                write.set(value);
            }
        });
    }

    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            data.lock().push_back(Some(event));
            EXT_EVENT_HANDLER.add_trigger(trigger);
        }
    });

    read
}
