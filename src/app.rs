use std::{cell::RefCell, rc::Rc};

use floem_reactive::WriteSignal;
use floem_winit::{
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy},
    monitor::MonitorHandle,
    window::WindowId,
};
use parking_lot::Mutex;
#[allow(deprecated)]
use raw_window_handle::HasRawDisplayHandle;

use crate::{
    action::Timer,
    app_handle::ApplicationHandle,
    clipboard::Clipboard,
    inspector::Capture,
    profiler::Profile,
    view::{IntoView, View},
    window::WindowConfig,
};

type AppEventCallback = dyn Fn(AppEvent);

static EVENT_LOOP_PROXY: Mutex<Option<EventLoopProxy<UserEvent>>> = Mutex::new(None);

thread_local! {
    pub(crate) static APP_UPDATE_EVENTS: RefCell<Vec<AppUpdateEvent>> = Default::default();
}

pub fn launch<V: IntoView + 'static>(app_view: impl FnOnce() -> V + 'static) {
    Application::new().window(move |_| app_view(), None).run()
}

pub enum AppEvent {
    WillTerminate,
    Reopen { has_visible_windows: bool },
}

#[derive(Debug)]
pub(crate) enum UserEvent {
    AppUpdate,
    Idle,
    QuitApp,
    GpuResourcesUpdate { window_id: WindowId },
}

pub(crate) enum AppUpdateEvent {
    NewWindow {
        view_fn: Box<dyn FnOnce(WindowId) -> Box<dyn View>>,
        config: Option<WindowConfig>,
    },
    CloseWindow {
        window_id: WindowId,
    },
    CaptureWindow {
        window_id: WindowId,
        capture: WriteSignal<Option<Rc<Capture>>>,
    },
    ProfileWindow {
        window_id: WindowId,
        end_profile: Option<WriteSignal<Option<Rc<Profile>>>>,
    },
    RequestTimer {
        timer: Timer,
    },
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
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
        unsafe {
            #[allow(deprecated)]
            Clipboard::init(event_loop.raw_display_handle().unwrap());
        }
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

    /// Create a new window for the application, if you want multiple windows,
    /// just chain more window method to the builder.
    ///
    /// # Note
    ///
    /// Using `None` as a configuration argument is equivalent to using
    /// `WindowConfig::default()`.
    pub fn window<V: IntoView + 'static>(
        mut self,
        app_view: impl FnOnce(WindowId) -> V + 'static,
        config: Option<WindowConfig>,
    ) -> Self {
        self.handle.as_mut().unwrap().new_window(
            &self.event_loop,
            self.event_loop.create_proxy(),
            Box::new(|window_id| app_view(window_id).into_any()),
            config.unwrap_or_default(),
        );
        self
    }

    pub fn run(mut self) {
        let mut handle = self.handle.take().unwrap();
        handle.idle();
        let event_loop_proxy = self.event_loop.create_proxy();
        let _ = self.event_loop.run(|event, event_loop| {
            event_loop.set_control_flow(ControlFlow::Wait);
            handle.handle_timer(event_loop);

            match event {
                floem_winit::event::Event::NewEvents(_) => {}
                floem_winit::event::Event::WindowEvent { window_id, event } => {
                    handle.handle_window_event(window_id, event, event_loop);
                }
                floem_winit::event::Event::DeviceEvent { .. } => {}
                floem_winit::event::Event::UserEvent(event) => {
                    handle.handle_user_event(event_loop, event_loop_proxy.clone(), event);
                }
                floem_winit::event::Event::Suspended => {}
                floem_winit::event::Event::Resumed => {}
                floem_winit::event::Event::AboutToWait => {}
                floem_winit::event::Event::LoopExiting => {
                    if let Some(action) = self.event_listener.as_ref() {
                        action(AppEvent::WillTerminate);
                    }
                }
                floem_winit::event::Event::MemoryWarning => {}
                floem_winit::event::Event::Reopen => {}
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
