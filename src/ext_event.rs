use std::{collections::VecDeque, sync::Arc};

use crossbeam_channel::TryRecvError;
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

pub fn create_ext_action<T: Send + 'static>(
    cx: Scope,
    action: impl Fn(T) + 'static,
) -> impl FnOnce(T) {
    let view = get_current_view();
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
                        let current_view = get_current_view();
                        set_current_view(view);
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

    cx.create_effect(move |_| {
        trigger.track();
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    writer.try_set(Some(event))
                }
                Err(TryRecvError::Empty) => {
                    EXT_EVENT_HANDLER.add_trigger(trigger);
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    cx.dispose();
                    break;
                }
            }
        }
    });
}

pub fn create_signal_from_channel<T: Send + 'static>(
    rx: crossbeam_channel::Receiver<T>,
) -> ReadSignal<Option<T>> {
    let cx = Scope::new();
    let trigger = cx.create_trigger();
    let (read, write) = cx.create_signal(None);

    cx.create_effect(move |_| {
        trigger.track();
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    write.set(Some(event))
                }
                Err(TryRecvError::Empty) => {
                    EXT_EVENT_HANDLER.add_trigger(trigger);
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    cx.dispose();
                    break;
                }
            }
        }
    });

    read
}
