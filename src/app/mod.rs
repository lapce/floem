#[cfg(target_os = "macos")]
pub(crate) mod delegate;
pub(crate) mod handle;

use std::{
    cell::RefCell,
    rc::Rc,
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::platform::menu_types::MenuId;
#[cfg(feature = "crossbeam")]
use crossbeam::channel::{Receiver, Sender, unbounded as channel};
use peniko::kurbo::Point;
#[cfg(not(feature = "crossbeam"))]
use std::sync::mpsc::{Receiver, Sender, channel};

use floem_reactive::{Runtime, WriteSignal};
use parking_lot::Mutex;
use raw_window_handle::HasDisplayHandle;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::{Theme, WindowId},
};

use crate::{
    action::{Timer, TimerToken},
    compositor_surface::{
        CompositorSurfaceContent, CompositorSurfaceId, CompositorSurfaceProviderHandle,
    },
    frame::FrameTime,
    inspector::{Capture, profiler::Profile},
    paint::composition::CompositionKey,
    platform::clipboard::Clipboard,
    view::IntoView,
    window::{WindowConfig, WindowCreation, compositor::SceneRenderSignature},
};
use handle::ApplicationHandle;
use subduction_core::timing::FrameTick;

pub(crate) type AppEventCallback = dyn Fn(AppEvent);

static EVENT_LOOP_PROXY: Mutex<Option<(EventLoopProxy, Sender<UserEvent>)>> = Mutex::new(None);
static APP_UPDATE_POSTED: AtomicBool = AtomicBool::new(false);

thread_local! {
    pub(crate) static APP_UPDATE_EVENTS: RefCell<Vec<AppUpdateEvent>> = Default::default();
}

pub struct AppConfig {
    pub(crate) exit_on_close: bool,
    pub(crate) wgpu_features: wgpu::Features,
    pub(crate) wgpu_backends: Option<wgpu::Backends>,
    pub(crate) gpu_resources: Option<crate::gpu_resources::GpuResources>,
    pub(crate) global_theme_override: Option<Theme>,
    pub(crate) renderer_chooser: crate::paint::renderer::RendererChooser,
}

impl std::fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppConfig")
            .field("exit_on_close", &self.exit_on_close)
            .field("wgpu_features", &self.wgpu_features)
            .field("wgpu_backends", &self.wgpu_backends)
            .field("gpu_resources", &self.gpu_resources)
            .field("global_theme_override", &self.global_theme_override)
            .field("renderer_chooser", &"<closure>")
            .finish()
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            exit_on_close: !cfg!(target_os = "macos"),
            wgpu_features: wgpu::Features::default(),
            wgpu_backends: None,
            gpu_resources: None,
            global_theme_override: None,
            renderer_chooser: crate::paint::renderer::default_renderer(),
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

    /// Uses an existing WGPU instance, adapter, device, and queue for Floem.
    ///
    /// Floem-created window surfaces and compositor-owned compositor surfaces
    /// will be configured against these resources instead of requesting a
    /// separate WGPU context.
    #[inline]
    pub fn gpu_resources(mut self, gpu_resources: crate::gpu_resources::GpuResources) -> Self {
        self.gpu_resources = Some(gpu_resources);
        self
    }

    /// Sets the global theme.
    #[inline]
    pub fn set_global_theme(mut self, theme: Theme) -> Self {
        self.global_theme_override = Some(theme);
        self
    }

    /// Override the application-wide renderer chooser.
    #[inline]
    pub fn renderer_chooser(
        mut self,
        renderer_chooser: impl Fn(
            crate::paint::renderer::NewRendererCx,
        ) -> crate::paint::renderer::RendererSpec
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.renderer_chooser = Arc::new(renderer_chooser);
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
#[cfg_attr(debug_assertions, track_caller)]
pub fn launch<V: IntoView + 'static>(app_view: impl FnOnce() -> V + 'static) {
    Application::new().window(move |_| app_view(), None).run()
}

pub enum AppEvent {
    WillTerminate,
    Reopen { has_visible_windows: bool },
}

pub(crate) struct MenuWrapper(pub(crate) muda::Menu);
// SAFETY: these unsafe wappers are needed so that we can send the muda memu.
// The muda menu internally uses RC on a String ID and it's Vec of children.
// This unsafe wrapper is memory safe but the race condition could potentially (unlikely)
// lead to bad reference counts and leaked memory.
// I think this is fine for this case.
unsafe impl Send for MenuWrapper {}
unsafe impl Sync for MenuWrapper {}

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
    ShowContextMenu {
        window_id: WindowId,
        menu: MenuWrapper,
        pos: Option<Point>,
    },
    CompositorSurfaceContent {
        window_id: WindowId,
        surface_id: CompositorSurfaceId,
        content: CompositorSurfaceContent,
    },
    CompositorSurfaceRequestFrame {
        window_id: WindowId,
        surface_id: CompositorSurfaceId,
    },
    CompositorSurfaceProvider {
        window_id: WindowId,
        surface_id: CompositorSurfaceId,
        provider: CompositorSurfaceProviderHandle,
    },
    SceneFragmentReady {
        window_id: WindowId,
        key: CompositionKey,
        signature: SceneRenderSignature,
        rendered: bool,
        worker_index: usize,
        render_start: crate::platform::Instant,
        render_end: crate::platform::Instant,
    },
    LayerHostCommit {
        window_id: WindowId,
        committed_at: crate::platform::Instant,
    },
    CompositorCommitDeadline {
        window_id: WindowId,
        generation: u64,
        token: TimerToken,
    },
    FrameTick {
        window_id: WindowId,
        tick: FrameTick,
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
    RequestCloseWindow {
        window_id: WindowId,
    },
    CaptureWindow {
        window_id: WindowId,
        capture: WriteSignal<Option<Rc<Capture>>>,
    },
    CaptureMetalFrame {
        window_id: WindowId,
    },
    ProfileWindow {
        window_id: WindowId,
        end_profile: Option<WriteSignal<Option<Rc<Profile>>>>,
    },
    RequestTimer {
        timer: Timer,
    },
    RequestAnimationFrame {
        window_id: WindowId,
        callback: Box<dyn FnOnce(FrameTime)>,
    },
    CancelTimer {
        timer: TimerToken,
    },
    MenuAction {
        action_id: MenuId,
    },
    ThemeChanged {
        theme: Theme,
    },
}

pub(crate) fn add_app_update_event(event: AppUpdateEvent) {
    APP_UPDATE_EVENTS.with(|events| {
        events.borrow_mut().push(event);
    });
    Application::request_update();
}

/// Drain the pending app update events and return how many of them were
/// `CloseWindow` for the given `window_id`.  This is intended for testing
/// the `handle_default_behaviors` close logic.
#[doc(hidden)]
pub fn take_close_window_event_count(window_id: WindowId) -> usize {
    APP_UPDATE_EVENTS.with(|events| {
        let mut events = events.borrow_mut();
        let count = events
            .iter()
            .filter(
                |e| matches!(e, AppUpdateEvent::CloseWindow { window_id: id } if *id == window_id),
            )
            .count();
        events.retain(
            |e| !matches!(e, AppUpdateEvent::CloseWindow { window_id: id } if *id == window_id),
        );
        count
    })
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
                self.handle.config.global_theme_override,
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
        self.drain_proxy_events(event_loop);
        if Runtime::has_pending_work() {
            self.handle.request_update();
        }
    }

    fn destroy_surfaces(&mut self, _event_loop: &dyn ActiveEventLoop) {
        if let Some(action) = self.handle.event_listener.as_ref() {
            action(AppEvent::WillTerminate);
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.handle.flush_deferred_context_menus();
        self.handle.handle_timer(event_loop);
        if Runtime::has_pending_work() {
            self.handle.request_update();
        }
    }
}

impl Application {
    pub fn new() -> Self {
        Self::new_with_config(AppConfig::default())
    }
    pub fn new_with_config(config: AppConfig) -> Self {
        let event_loop = EventLoop::new().expect("can't start the event loop");

        #[cfg(target_os = "macos")]
        delegate::set_app_delegate();

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

    fn drain_proxy_events(&mut self, event_loop: &dyn ActiveEventLoop) {
        for event in self.receiver.try_iter() {
            self.handle.handle_user_event(event_loop, event);
        }
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

    #[cfg_attr(debug_assertions, track_caller)]
    pub fn run(mut self) {
        Runtime::init_on_ui_thread();
        // Nudge UI when sync signals are updated from other threads.
        Runtime::set_sync_effect_waker(|| Application::send_proxy_event(UserEvent::Idle));
        let event_loop = self.event_loop.take().unwrap();
        let _ = event_loop.run_app(self);
    }

    pub(crate) fn send_proxy_event(event: UserEvent) {
        if let Some((proxy, sender)) = EVENT_LOOP_PROXY.lock().as_ref() {
            let _ = sender.send(event);
            proxy.wake_up();
        }
    }

    pub(crate) fn request_update() {
        if !APP_UPDATE_POSTED.swap(true, Ordering::AcqRel) {
            Self::send_proxy_event(UserEvent::AppUpdate);
        }
    }

    pub(crate) fn clear_update_posted() {
        APP_UPDATE_POSTED.store(false, Ordering::Release);
    }
}

/// Immediately terminates the application.
///
/// This is an unconditional exit: no windows receive a `CloseRequested` event
/// and no `WindowCloseRequested` handlers are consulted. The event loop exits
/// immediately.
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
