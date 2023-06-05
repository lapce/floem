use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use glazier::{IdleHandle, IdleToken};
use leptos_reactive::{create_signal, ReadSignal, Scope, SignalSet};
use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::id::Id;

pub static EXT_EVENT_HANDLER: Lazy<ExtEventHandler> = Lazy::new(ExtEventHandler::default);

thread_local! {
    pub(crate) static IDLE_ACTIONS: RefCell<HashMap<ExtId, FnOrFnOnce>> = RefCell::new(HashMap::new());
}

pub type ExtId = Id;

pub struct ExtEvent(ExtId);

pub(crate) enum FnOrFnOnce {
    Fn(Box<dyn Fn()>),
    FnOnce(Box<dyn FnOnce()>),
}

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
    cx: Scope,
    rx: crossbeam_channel::Receiver<T>,
) -> ReadSignal<Option<T>> {
    let ext_id = ExtId::next();

    let (read, write) = create_signal(cx, None);
    let data = Arc::new(Mutex::new(VecDeque::new()));

    let action = {
        let data = data.clone();
        move || {
            while let Some(value) = data.lock().pop_front() {
                write.set(value);
            }
        }
    };
    IDLE_ACTIONS.with(|actions| {
        actions
            .borrow_mut()
            .insert(ext_id, FnOrFnOnce::Fn(Box::new(action)))
    });

    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            data.lock().push_back(Some(event));
            EXT_EVENT_HANDLER.send_event(ext_id);
        }
    });

    read
}

pub fn create_ext_action<T: Send + 'static>(action: impl Fn(T) + 'static) -> impl Fn(T) {
    let ext_id = ExtId::next();
    let data = Arc::new(Mutex::new(None));

    let action = {
        let data = data.clone();
        move || {
            let event = data.lock().take().unwrap();
            action(event);
        }
    };
    IDLE_ACTIONS.with(|actions| {
        actions
            .borrow_mut()
            .insert(ext_id, FnOrFnOnce::FnOnce(Box::new(action)))
    });

    move |event| {
        *data.lock() = Some(event);
        EXT_EVENT_HANDLER.send_event(ext_id);
    }
}
