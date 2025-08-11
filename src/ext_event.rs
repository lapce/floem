use std::{cell::Cell, collections::VecDeque, sync::Arc};

use floem_reactive::{
    create_effect, create_rw_signal, untrack, with_scope, ReadSignal, RwSignal, Scope, SignalGet,
    SignalUpdate, SignalWith, WriteSignal,
};
use parking_lot::{Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{
    app::UserEvent,
    window_handle::{get_current_view, set_current_view},
    Application,
};

#[cfg(feature = "crossbeam")]
use crossbeam::channel::Receiver;
#[cfg(not(feature = "crossbeam"))]
use std::sync::mpsc::Receiver;

/// # SAFETY
///
/// **DO NOT USE THIS** trigger except for when using with `create_ext_action` or when you guarantee that
/// the signal is never used from a different thread than it was created on.
#[derive(Debug)]
pub struct ExtSendTrigger {
    signal: RwSignal<()>,
}

impl Copy for ExtSendTrigger {}

impl Clone for ExtSendTrigger {
    fn clone(&self) -> Self {
        *self
    }
}

impl ExtSendTrigger {
    pub fn notify(&self) {
        self.signal.set(());
    }

    pub fn track(&self) {
        self.signal.with(|_| {});
    }

    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        create_trigger()
    }
}

pub fn create_trigger() -> ExtSendTrigger {
    ExtSendTrigger {
        signal: create_rw_signal(()),
    }
}

unsafe impl Send for ExtSendTrigger {}
unsafe impl Sync for ExtSendTrigger {}

pub(crate) static EXT_EVENT_HANDLER: ExtEventHandler = ExtEventHandler::new();

pub(crate) struct ExtEventHandler {
    pub(crate) queue: Mutex<VecDeque<ExtSendTrigger>>,
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

    pub fn add_trigger(&self, trigger: ExtSendTrigger) {
        {
            // Run this in a short block to prevent any deadlock if running the trigger effects
            // causes another trigger to be registered
            EXT_EVENT_HANDLER.queue.lock().push_back(trigger);
        }
        Application::send_proxy_event(UserEvent::Idle);
    }
}

pub fn register_ext_trigger(trigger: ExtSendTrigger) {
    EXT_EVENT_HANDLER.add_trigger(trigger);
}

pub fn create_ext_action<T: Send + 'static>(
    cx: Scope,
    action: impl FnOnce(T) + 'static,
) -> impl FnOnce(T) {
    let view = get_current_view();
    let cx = cx.create_child();
    let trigger = with_scope(cx, ExtSendTrigger::new);
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
    rx: Receiver<T>,
) {
    let cx = Scope::new();
    let trigger = with_scope(cx, ExtSendTrigger::new);

    let channel_closed = cx.create_rw_signal(false);
    let data = Arc::new(Mutex::new(VecDeque::new()));

    {
        let data = data.clone();
        cx.create_effect(move |_| {
            trigger.track();
            while let Some(value) = data.lock().pop_front() {
                writer.set(value);
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

pub fn create_signal_from_channel<T: Send + 'static>(rx: Receiver<T>) -> ReadSignal<Option<T>> {
    let cx = Scope::new();
    let trigger = with_scope(cx, ExtSendTrigger::new);

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
    let trigger = with_scope(cx, ExtSendTrigger::new);

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
    initial_value: T,
    stream: impl futures::Stream<Item = T> + 'static,
) -> ReadSignal<T> {
    use std::{
        cell::RefCell,
        task::{Context, Poll},
    };

    use futures::task::{waker, ArcWake};

    let cx = Scope::current().create_child();
    let trigger = with_scope(cx, ExtSendTrigger::new);
    let (read, write) = cx.create_signal(initial_value);

    /// Waker that wakes by registering a trigger
    // TODO: since the trigger is just a `u64`, it could theoretically be changed to be a `usize`,
    //       Then the implementation of the std::task::RawWakerVTable could pass the `usize` as the data pointer,
    //       avoiding any allocation/reference counting
    struct TriggerWake(ExtSendTrigger);
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
            unreachable!("The waker registers events effecs to be run only at idle")
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

#[derive(Clone)]
pub struct ArcRwSignal<T> {
    inner: Arc<ArcRwSignalInner<T>>,
}

struct ArcRwSignalInner<T> {
    // The actual data, protected by a RwLock for thread safety
    data: RwLock<T>,
    // Trigger for notifying the reactive system of changes
    trigger: ExtSendTrigger,
    // Main-thread signal for reactive integration (created lazily)
    main_signal: parking_lot::Mutex<Option<RwSignal<()>>>,
}

impl<T> ArcRwSignal<T> {
    /// Create a new ArcRwSignal with the given initial value
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(ArcRwSignalInner {
                data: RwLock::new(value),
                trigger: ExtSendTrigger::new(),
                main_signal: parking_lot::Mutex::new(None),
            }),
        }
    }

    /// Get a read guard to the data (like Arc<RwLock<T>>::read())
    /// This does NOT subscribe to reactive effects.
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        self.inner.data.read()
    }

    /// Get a write guard to the data (like Arc<RwLock<T>>::write())
    /// This will notify reactive effects when the guard is dropped.
    pub fn write(&self) -> ArcRwSignalWriteGuard<'_, T> {
        let guard = self.inner.data.write();
        ArcRwSignalWriteGuard {
            guard,
            trigger: self.inner.trigger,
        }
    }

    /// Try to get a read guard without blocking
    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, T>> {
        self.inner.data.try_read()
    }

    /// Try to get a write guard without blocking
    pub fn try_write(&self) -> Option<ArcRwSignalWriteGuard<'_, T>> {
        self.inner
            .data
            .try_write()
            .map(|guard| ArcRwSignalWriteGuard {
                guard,
                trigger: self.inner.trigger,
            })
    }
}

impl<T: Clone> ArcRwSignal<T> {
    /// Get a clone of the current value and subscribe to changes in reactive contexts
    pub fn get(&self) -> T {
        self.track();
        self.inner.data.read().clone()
    }

    /// Get a clone of the current value without subscribing to changes
    pub fn get_untracked(&self) -> T {
        self.inner.data.read().clone()
    }

    /// Set the value, notifying all reactive subscribers
    pub fn set(&self, value: T) {
        *self.inner.data.write() = value;
        self.notify();
    }

    /// Update the value using a closure, notifying all reactive subscribers
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let result = f(&mut *self.inner.data.write());
        self.notify();
        result
    }

    /// Apply a closure to the current value, subscribing to changes in reactive contexts
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.track();
        f(&*self.inner.data.read())
    }

    /// Apply a closure to the current value without subscribing to changes
    pub fn with_untracked<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&*self.inner.data.read())
    }
}

impl<T> ArcRwSignal<T> {
    /// Subscribe to changes in reactive contexts (like signal.track())
    pub fn track(&self) {
        self.ensure_main_signal();
        self.inner.trigger.track();
    }

    /// Manually notify reactive subscribers of changes
    pub fn notify(&self) {
        EXT_EVENT_HANDLER.add_trigger(self.inner.trigger);
    }

    /// Ensure the main-thread signal exists for reactive integration
    fn ensure_main_signal(&self) {
        let mut main_signal = self.inner.main_signal.lock();
        if main_signal.is_none() {
            *main_signal = Some(create_rw_signal(()));
        }
    }

    /// Get a regular ReadSignal that updates when this ArcRwSignal changes.
    /// The returned signal's value is always `()` - it's just for tracking changes.
    pub fn to_read_signal(&self) -> ReadSignal<()> {
        self.ensure_main_signal();
        let main_signal = self.inner.main_signal.lock();
        main_signal.as_ref().unwrap().read_only()
    }

    /// Get a regular WriteSignal that can trigger updates.
    /// Writing to this signal will notify subscribers but won't change the actual data.
    pub fn to_write_signal(&self) -> WriteSignal<()> {
        self.ensure_main_signal();
        let main_signal = self.inner.main_signal.lock();
        main_signal.as_ref().unwrap().write_only()
    }
}

/// A write guard that notifies reactive subscribers when dropped
pub struct ArcRwSignalWriteGuard<'a, T> {
    guard: RwLockWriteGuard<'a, T>,
    trigger: ExtSendTrigger,
}

impl<T> Drop for ArcRwSignalWriteGuard<'_, T> {
    fn drop(&mut self) {
        EXT_EVENT_HANDLER.add_trigger(self.trigger);
    }
}

impl<T> std::ops::Deref for ArcRwSignalWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<T> std::ops::DerefMut for ArcRwSignalWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

// Make ArcRwSignal thread-safe
unsafe impl<T: Send> Send for ArcRwSignal<T> {}
unsafe impl<T: Send + Sync> Sync for ArcRwSignal<T> {}

/// Convenience function to create an ArcRwSignal
pub fn create_arc_rw_signal<T>(value: T) -> ArcRwSignal<T> {
    ArcRwSignal::new(value)
}
