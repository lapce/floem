use std::{
    any::Any,
    cell::RefCell,
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use glazier::{IdleHandle, IdleToken};
use leptos_reactive::{
    create_effect, create_signal, ReadSignal, Scope, ScopeDisposer, WriteSignal,
};
use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::{app::AppContext, id::Id};

pub static EXT_EVENT_HANDLER: Lazy<ExtEventHandler> = Lazy::new(|| ExtEventHandler::default());

thread_local! {
    pub(crate) static WRITE_SIGNALS: RefCell<HashMap<ExtId, WriteSignal<Option<()>>>> = RefCell::new(HashMap::new());
}

pub type ExtId = Id;

pub struct ExtEvent(ExtId);

#[derive(Clone)]
pub struct ExtEventHandler {
    pub(crate) queue: Arc<Mutex<VecDeque<ExtId>>>,
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
    pub fn send_event(&self, ext_id: ExtId) {
        EXT_EVENT_HANDLER.queue.lock().push_back(ext_id);
        if let Some(handle) = EXT_EVENT_HANDLER.handle.lock().as_mut() {
            handle.schedule_idle(IdleToken::new(0));
        }
    }
}

pub fn create_signal_from_channel<T: Send>(
    cx: AppContext,
    rx: crossbeam_channel::Receiver<T>,
) -> ReadSignal<Option<T>> {
    let (read_notify, write_notify) = create_signal(cx.scope, None);
    let ext_id = ExtId::next();
    WRITE_SIGNALS.with(|signals| signals.borrow_mut().insert(ext_id, write_notify));

    let data = Arc::new(Mutex::new(VecDeque::new()));
    let (read, write) = create_signal(cx.scope, None);

    {
        let data = data.clone();
        create_effect(cx.scope, move |_| {
            if read_notify.get().is_some() {
                while let Some(value) = data.lock().pop_front() {
                    write.set(value);
                }
            }
        });
    }

    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            EXT_EVENT_HANDLER.send_event(ext_id);
            data.lock().push_back(Some(event));
        }
    });

    read
}

pub fn create_ext_action<T: Send + 'static>(
    cx: AppContext,
    action: impl Fn(T) + 'static,
) -> impl Fn(T) {
    let ext_id = ExtId::next();
    let data = Arc::new(Mutex::new(None));

    let (cx, _) = cx.scope.run_child_scope(|cx| cx);
    let (read_notify, write_notify) = create_signal(cx, None);
    WRITE_SIGNALS.with(|signals| signals.borrow_mut().insert(ext_id, write_notify));

    {
        let data = data.clone();
        create_effect(cx, move |_| {
            if read_notify.get().is_some() {
                let event = data.lock().take().unwrap();
                action(event);
                cx.dispose();
            }
        });
    }

    move |event| {
        *data.lock() = Some(event);
        EXT_EVENT_HANDLER.send_event(ext_id);
    }
}

pub fn create_signal_from_channel_oneshot<T: Send>(
    cx: AppContext,
    rx: crossbeam_channel::Receiver<T>,
) -> (ReadSignal<Option<T>>, Scope) {
    let ext_id = ExtId::next();
    let data = Arc::new(Mutex::new(VecDeque::new()));
    let ((read, child_scope), _child_scope_disposer) = cx.scope.run_child_scope(|cx| {
        let (read_notify, write_notify) = create_signal(cx, None);
        WRITE_SIGNALS.with(|signals| signals.borrow_mut().insert(ext_id, write_notify));

        let (read, write) = create_signal(cx, None);

        {
            let data = data.clone();
            create_effect(cx, move |_| {
                if read_notify.get().is_some() {
                    while let Some(value) = data.lock().pop_front() {
                        write.set(value);
                    }
                }
            });
        }

        (read, cx)
    });

    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            EXT_EVENT_HANDLER.send_event(ext_id);
            data.lock().push_back(Some(event));
        }
    });

    (read, child_scope)
}
