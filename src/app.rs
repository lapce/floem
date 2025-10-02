use std::{cell::RefCell, rc::Rc};

#[cfg(feature = "crossbeam")]
use crossbeam::channel::{unbounded as channel, Receiver, Sender};
use muda::MenuId;
#[cfg(not(feature = "crossbeam"))]
use std::sync::mpsc::{channel, Receiver, Sender};

use floem_reactive::WriteSignal;
use parking_lot::Mutex;
use raw_window_handle::HasDisplayHandle;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::WindowId,
};

use crate::{
    action::{Timer, TimerToken},
    app_handle::ApplicationHandle,
    clipboard::Clipboard,
    inspector::Capture,
    profiler::Profile,
    view::IntoView,
    window::{WindowConfig, WindowCreation},
};

pub(crate) type AppEventCallback = dyn Fn(AppEvent);

static EVENT_LOOP_PROXY: Mutex<Option<(EventLoopProxy, Sender<UserEvent>)>> = Mutex::new(None);

thread_local! {
    pub(crate) static APP_UPDATE_EVENTS: RefCell<Vec<AppUpdateEvent>> = Default::default();
}

#[derive(Debug)]
pub struct AppConfig {
    pub(crate) exit_on_close: bool,
    pub(crate) wgpu_features: wgpu::Features,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            exit_on_close: !cfg!(target_os = "macos"),
            wgpu_features: wgpu::Features::default(),
        }
    }
}

impl AppConfig {
    /// Sets whether the application should exit when the last window is closed.
    #[inline]
    pub fn exit_on_close(mut self, exit_on_close: bool) -> Self {
        self.exit_on_close = exit_on_close;
        self
    }
    /// Sets the WGPU features to be used by the application.
    #[inline]
    pub fn wgpu_features(mut self, features: wgpu::Features) -> Self {
        self.wgpu_features = features;
        self
    }
}

/// Initializes and runs an application with a single window.
///
/// This function creates a new `Application`, sets up a window with the provided view,
/// and starts the application event loop. The `app_view` closure is used to define
/// the root view of the application window.
///
/// Example:
/// ```no_run
/// floem::launch(|| "Hello, World!")
/// ```
///
/// To build an application and windows with more configuration, see [`Application`].
pub fn launch<V: IntoView + 'static>(app_view: impl FnOnce() -> V + 'static) {
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
    #[allow(dead_code)]
    Reopen {
        has_visible_windows: bool,
    },
    GpuResourcesUpdate {
        window_id: WindowId,
    },
}

#[allow(clippy::large_enum_variant)]
pub(crate) enum AppUpdateEvent {
    NewWindow {
        window_creation: WindowCreation,
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
    CancelTimer {
        timer: TimerToken,
    },
    MenuAction {
        action_id: MenuId,
    },
}

pub(crate) fn add_app_update_event(event: AppUpdateEvent) {
    APP_UPDATE_EVENTS.with(|events| {
        events.borrow_mut().push(event);
    });
    Application::send_proxy_event(UserEvent::AppUpdate);
}

/// Floem top level application
/// This is the entry point of the application.
pub struct Application {
    receiver: Receiver<UserEvent>,
    handle: ApplicationHandle,
    event_loop: Option<EventLoop>,
    initial_windows: Vec<WindowCreation>,
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplicationHandler for Application {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        while let Some(window_creation) = self.initial_windows.pop() {
            self.handle.new_window(
                event_loop,
                window_creation.view_fn,
                window_creation.config.unwrap_or_default(),
            );
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        self.handle.handle_timer(event_loop);
        self.handle
            .handle_window_event(window_id, event, event_loop);
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.handle.handle_timer(event_loop);
        for event in self.receiver.try_iter() {
            self.handle.handle_user_event(event_loop, event);
        }
        self.handle.handle_updates_for_all_windows();
    }

    fn exiting(&mut self, _event_loop: &dyn ActiveEventLoop) {
        if let Some(action) = self.handle.event_listener.as_ref() {
            action(AppEvent::WillTerminate);
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.handle.handle_timer(event_loop);
    }
}

impl Application {
    pub fn new() -> Self {
        Self::new_with_config(AppConfig::default())
    }
    pub fn new_with_config(config: AppConfig) -> Self {
        let event_loop = EventLoop::new().expect("can't start the event loop");

        #[cfg(target_os = "macos")]
        crate::app_delegate::set_app_delegate();

        let event_loop_proxy = event_loop.create_proxy();
        let (sender, receiver) = channel();

        *EVENT_LOOP_PROXY.lock() = Some((event_loop_proxy.clone(), sender));
        unsafe {
            Clipboard::init(event_loop.display_handle().unwrap().as_raw());
        }
        let handle = ApplicationHandle::new(config);

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
            add_app_update_event(AppUpdateEvent::MenuAction {
                action_id: event.id,
            });
        }));

        Self {
            receiver,
            handle,
            event_loop: Some(event_loop),
            initial_windows: Vec::new(),
        }
    }

    pub fn on_event(mut self, action: impl Fn(AppEvent) + 'static) -> Self {
        self.handle.event_listener = Some(Box::new(action));
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
        self.initial_windows.push(WindowCreation {
            view_fn: Box::new(move |window_id: WindowId| app_view(window_id).into_any()),
            config,
        });
        self
    }

    pub fn run(mut self) {
        let event_loop = self.event_loop.take().unwrap();
        let _ = event_loop.run_app(self);
    }

    pub(crate) fn send_proxy_event(event: UserEvent) {
        if let Some((proxy, sender)) = EVENT_LOOP_PROXY.lock().as_ref() {
            let _ = sender.send(event);
            proxy.wake_up();
        }
    }
}

/// Initiates the application shutdown process.
///
/// This function sends a `QuitApp` event to the application's event loop,
/// triggering the application to close gracefully.
pub fn quit_app() {
    Application::send_proxy_event(UserEvent::QuitApp);
}

/// Signals the application to reopen.
///
/// This function sends a `Reopen` event to the application's event loop.
/// It is safe to call from any thread.
pub fn reopen() {
    Application::send_proxy_event(UserEvent::Reopen {
        has_visible_windows: false,
    });
}
