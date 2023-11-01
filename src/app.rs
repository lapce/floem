use std::{cell::RefCell, sync::Arc};

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use winit::{
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy},
    monitor::MonitorHandle,
    window::WindowId,
};

use crate::{action::Timer, app_handle::ApplicationHandle, view::View, window::WindowConfig};

type AppEventCallback = dyn Fn(AppEvent);

static EVENT_LOOP_PROXY: Lazy<Arc<Mutex<Option<EventLoopProxy<UserEvent>>>>> =
    Lazy::new(|| Arc::new(Mutex::new(None)));

thread_local! {
    pub(crate) static APP_UPDATE_EVENTS: RefCell<Vec<AppUpdateEvent>> = Default::default();
}

pub fn launch<V: View + 'static>(app_view: impl Fn() -> V + 'static) {
    Application::new().window(move |_| app_view(), None).run()
}

pub enum AppEvent {
    WillTerminate,
    Reopen { has_visible_windows: bool },
}

pub(crate) enum UserEvent {
    AppUpdate,
    Idle,
    QuitApp,
}

pub(crate) enum AppUpdateEvent {
    NewWindow {
        view_fn: Box<dyn FnOnce(WindowId) -> Box<dyn View>>,
        config: Option<WindowConfig>,
    },
    CloseWindow {
        window_id: WindowId,
    },
    RequestTimer {
        timer: Timer,
    },
    #[cfg(target_os = "linux")]
    MenuAction {
        window_id: WindowId,
        action_id: usize,
    },
}

pub(crate) fn add_app_update_event(event: AppUpdateEvent) {
    APP_UPDATE_EVENTS.with(|events| {
        events.borrow_mut().push(event);
    });
    Application::with_event_loop_proxy(|proxy| {
        let _ = proxy.send_event(UserEvent::AppUpdate);
    });
}

/// Floem top level application
/// This is the entry point of the application.
pub struct Application {
    handle: Option<ApplicationHandle>,
    event_listener: Option<Box<AppEventCallback>>,
    event_loop: EventLoop<UserEvent>,
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}

impl Application {
    pub fn new() -> Self {
        let event_loop = EventLoopBuilder::with_user_event()
            .build()
            .expect("can't start the event loop");
        let event_loop_proxy = event_loop.create_proxy();
        *EVENT_LOOP_PROXY.lock() = Some(event_loop_proxy.clone());
        let handle = ApplicationHandle::new();
        Self {
            handle: Some(handle),
            event_listener: None,
            event_loop,
        }
    }

    pub fn on_event(mut self, action: impl Fn(AppEvent) + 'static) -> Self {
        self.event_listener = Some(Box::new(action));
        self
    }

    /// create a new window for the application, if you want multiple windows,
    /// just chain more window method to the builder
    pub fn window<V: View + 'static>(
        mut self,
        app_view: impl FnOnce(WindowId) -> V + 'static,
        config: Option<WindowConfig>,
    ) -> Self {
        self.handle.as_mut().unwrap().new_window(
            &self.event_loop,
            Box::new(|window_id| Box::new(app_view(window_id))),
            config,
        );
        self
    }

    pub fn run(mut self) {
        let mut handle = self.handle.take().unwrap();
        handle.idle();
        let _ = self.event_loop.run(move |event, event_loop| {
            event_loop.set_control_flow(ControlFlow::Wait);
            handle.handle_timer(event_loop);

            match event {
                winit::event::Event::NewEvents(_) => {}
                winit::event::Event::WindowEvent { window_id, event } => {
                    handle.handle_window_event(window_id, event, event_loop);
                }
                winit::event::Event::DeviceEvent { .. } => {}
                winit::event::Event::UserEvent(event) => {
                    handle.handle_user_event(event_loop, event);
                }
                winit::event::Event::Suspended => {}
                winit::event::Event::Resumed => {}
                winit::event::Event::AboutToWait => {}
                winit::event::Event::LoopExiting => {
                    if let Some(action) = self.event_listener.as_ref() {
                        action(AppEvent::WillTerminate);
                    }
                }
                winit::event::Event::MemoryWarning => {}
            }
        });
    }

    pub(crate) fn with_event_loop_proxy(f: impl FnOnce(&EventLoopProxy<UserEvent>)) {
        if let Some(proxy) = EVENT_LOOP_PROXY.lock().as_ref() {
            f(proxy);
        }
    }

    pub fn available_monitors(&self) -> impl Iterator<Item = MonitorHandle> {
        self.event_loop.available_monitors()
    }

    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        self.event_loop.primary_monitor()
    }
}

pub fn quit_app() {
    Application::with_event_loop_proxy(|proxy| {
        let _ = proxy.send_event(UserEvent::QuitApp);
    });
}
