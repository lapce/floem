use dpi::PhysicalPosition;
use floem_renderer::gpu_resources::GpuResources;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use ui_events_winit::WindowEventTranslation;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

#[cfg(target_arch = "wasm32")]
use wgpu::web_sys;

use floem_reactive::{Runtime, SignalUpdate};
use peniko::kurbo::{Point, Size};
use std::{collections::HashMap, rc::Rc, time::Duration};
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow},
    window::{Theme, WindowId},
};

use super::{APP_UPDATE_EVENTS, AppConfig, AppEventCallback, AppUpdateEvent, UserEvent};
#[cfg(not(feature = "skia"))]
use crate::context::PaintState;
use crate::{
    AppEvent, Application,
    action::{Timer, TimerToken},
    dropped_file,
    event::dropped_file::FileDragEvent,
    ext_event::EXT_EVENT_HANDLER,
    inspector::{
        Capture,
        profiler::{Profile, ProfileEvent},
    },
    view::View,
    window::{WindowConfig, handle::WindowHandle, id::process_window_updates},
};

struct PendingContextMenu {
    window_id: WindowId,
    menu: super::MenuWrapper,
    pos: Option<Point>,
}

pub(crate) struct ApplicationHandle {
    window_handles: HashMap<winit::window::WindowId, WindowHandle>,
    timers: HashMap<TimerToken, Timer>,
    animating_windows: std::collections::HashSet<winit::window::WindowId>,
    pending_context_menus: Vec<PendingContextMenu>,
    pub(crate) event_listener: Option<Box<AppEventCallback>>,
    pub(crate) gpu_resources: Option<GpuResources>,
    pub(crate) config: AppConfig,
}

impl ApplicationHandle {
    const UPDATE_BUDGET: Duration = Duration::from_millis(4);

    pub(crate) fn new(config: AppConfig) -> Self {
        Self {
            window_handles: HashMap::new(),
            timers: HashMap::new(),
            animating_windows: std::collections::HashSet::new(),
            pending_context_menus: Vec::new(),
            event_listener: None,
            gpu_resources: None,
            config,
        }
    }

    pub(crate) fn handle_user_event(&mut self, event_loop: &dyn ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::AppUpdate => {
                self.handle_update_event(event_loop);
            }
            UserEvent::Idle => {
                self.idle();
            }
            UserEvent::QuitApp => {
                event_loop.exit();
            }
            UserEvent::Reopen {
                has_visible_windows,
            } => {
                if let Some(action) = self.event_listener.as_ref() {
                    action(AppEvent::Reopen {
                        has_visible_windows,
                    });
                }
            }
            #[cfg(not(feature = "skia"))]
            UserEvent::GpuResourcesUpdate { window_id } => {
                let handle = self.window_handles.get_mut(&window_id).unwrap();
                if let PaintState::PendingGpuResources {
                    window,
                    rx,
                    font_embolden,
                    renderer,
                } = &handle.paint_state
                {
                    let (gpu_resources, surface) = rx.recv().unwrap().unwrap();
                    let renderer = crate::paint::Renderer::new(
                        window.clone(),
                        gpu_resources.clone(),
                        surface,
                        handle.window_state.effective_scale(),
                        renderer.size(),
                        *font_embolden,
                    );
                    self.gpu_resources = Some(gpu_resources);
                    handle.paint_state = PaintState::Initialized { renderer };
                    handle.gpu_resources = self.gpu_resources.clone();
                    handle.init_renderer();
                } else {
                    panic!("Sent a gpu resource update after it had already been initialized");
                }
            }
            UserEvent::ShowContextMenu {
                window_id,
                menu,
                pos,
            } => {
                self.pending_context_menus.push(PendingContextMenu {
                    window_id,
                    menu,
                    pos,
                });
            }
        }
    }

    pub(crate) fn handle_update_event(&mut self, event_loop: &dyn ActiveEventLoop) {
        Application::clear_update_posted();

        let events = APP_UPDATE_EVENTS.with(|events| {
            let mut events = events.borrow_mut();
            std::mem::take(&mut *events)
        });

        for event in events {
            match event {
                AppUpdateEvent::NewWindow { window_creation } => self.new_window(
                    event_loop,
                    window_creation.view_fn,
                    self.config.global_theme_override,
                    window_creation.config.unwrap_or_default(),
                ),
                AppUpdateEvent::CloseWindow { window_id } => {
                    self.close_window(window_id, event_loop);
                }
                AppUpdateEvent::RequestCloseWindow { window_id } => {
                    if self.should_close_window_on_request(window_id) {
                        self.close_window(window_id, event_loop);
                    }
                }
                AppUpdateEvent::RequestTimer { timer } => {
                    self.request_timer(timer, event_loop);
                }
                AppUpdateEvent::RequestAnimationTimer {
                    mut timer,
                    window_id,
                } => {
                    if !self.window_can_render(&window_id) {
                        continue;
                    }
                    timer.deadline = Instant::now() + self.frame_duration_for_window(&window_id);
                    self.request_timer(timer, event_loop);
                }
                AppUpdateEvent::AnimationFrame(animate, window_id) => {
                    if animate && self.window_can_render(&window_id) {
                        self.animating_windows.insert(window_id);
                    } else {
                        self.animating_windows.remove(&window_id);
                    }
                    self.update_control_flow(event_loop);
                }
                AppUpdateEvent::CancelTimer { timer } => {
                    self.remove_timer(&timer, event_loop);
                }
                AppUpdateEvent::CaptureWindow { window_id, capture } => {
                    capture.set(self.capture_window(window_id).map(Rc::new));
                }
                AppUpdateEvent::ProfileWindow {
                    window_id,
                    end_profile,
                } => {
                    let handle = self.window_handles.get_mut(&window_id);
                    if let Some(handle) = handle {
                        if let Some(profile) = end_profile {
                            profile.set(handle.profile.take().map(|mut profile| {
                                profile.next_frame();
                                Rc::new(profile)
                            }));
                        } else {
                            handle.profile = Some(Profile::default());
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                AppUpdateEvent::MenuAction { action_id } => {
                    for (_, handle) in self.window_handles.iter_mut() {
                        if handle.window_state.context_menu.contains_key(&action_id)
                            || handle.window_menu_actions.contains_key(&action_id)
                        {
                            handle.menu_action(&action_id);
                            break;
                        }
                    }
                }
                #[cfg(target_arch = "wasm32")]
                AppUpdateEvent::MenuAction { action_id } => {
                    for (_, handle) in self.window_handles.iter_mut() {
                        if handle.window_state.context_menu.contains_key(&action_id) {
                            handle.menu_action(&action_id);
                            break;
                        }
                    }
                }
                AppUpdateEvent::ThemeChanged { theme } => {
                    self.config.global_theme_override = Some(theme);
                    for window_handle in self.window_handles.values_mut() {
                        window_handle.window_state.light_dark_theme = theme;
                        window_handle.set_theme(Some(theme), false);
                    }
                }
            }
        }

        let start = Instant::now();
        let mut any_work_remaining =
            self.handle_updates_for_all_windows_budgeted(start, Self::UPDATE_BUDGET);

        if start.elapsed() < Self::UPDATE_BUDGET && Runtime::has_pending_work() {
            Runtime::drain_pending_work();
            if Runtime::has_pending_work() {
                any_work_remaining = true;
            }
        }

        if any_work_remaining {
            self.request_update();
        }
    }

    pub(crate) fn handle_window_event(
        &mut self,
        window_id: winit::window::WindowId,
        event: WindowEvent,
        event_loop: &dyn ActiveEventLoop,
    ) {
        let is_redraw = matches!(event, WindowEvent::RedrawRequested);
        let window_handle = match self.window_handles.get_mut(&window_id) {
            Some(window_handle) => window_handle,
            None => return,
        };

        let start = window_handle.profile.is_some().then(|| {
            let name = match event {
                WindowEvent::ActivationTokenDone { .. } => "ActivationTokenDone",
                WindowEvent::SurfaceResized(..) => "Resized",
                WindowEvent::Moved(..) => "Moved",
                WindowEvent::CloseRequested => "CloseRequested",
                WindowEvent::Destroyed => "Destroyed",
                WindowEvent::Focused(..) => "Focused",
                WindowEvent::KeyboardInput { .. } => "KeyboardInput",
                WindowEvent::ModifiersChanged(..) => "ModifiersChanged",
                WindowEvent::Ime(..) => "Ime",
                WindowEvent::PointerMoved { .. } => "PointerMoved",
                WindowEvent::PointerEntered { .. } => "PointerEntered",
                WindowEvent::PointerLeft { .. } => "PointerLeft",
                WindowEvent::MouseWheel { .. } => "MouseWheel",
                WindowEvent::PointerButton { .. } => "PointerButton",
                WindowEvent::TouchpadPressure { .. } => "TouchpadPressure",
                WindowEvent::ScaleFactorChanged { .. } => "ScaleFactorChanged",
                WindowEvent::ThemeChanged(..) => "ThemeChanged",
                WindowEvent::Occluded(..) => "Occluded",
                WindowEvent::RedrawRequested => "RedrawRequested",
                WindowEvent::PinchGesture { .. } => "PinchGesture",
                WindowEvent::PanGesture { .. } => "PanGesture",
                WindowEvent::DoubleTapGesture { .. } => "DoubleTapGesture",
                WindowEvent::RotationGesture { .. } => "RotationGesture",
                WindowEvent::DragDropped { .. } => "DroppedFile",
                WindowEvent::DragEntered { .. } => "DragEntered",
                WindowEvent::DragLeft { .. } => "DragLeft",
                WindowEvent::DragMoved { .. } => "DragMoved",
            };
            (
                name,
                Instant::now(),
                matches!(event, WindowEvent::RedrawRequested),
            )
        });

        let event_scale = window_handle.window_state.effective_scale();

        match window_handle.event_reducer.reduce(event_scale, &event) {
            Some(WindowEventTranslation::Keyboard(ke)) => {
                if let WindowEvent::KeyboardInput { is_synthetic, .. } = event
                    && !is_synthetic
                {
                    window_handle.key_event(ke)
                }
            }
            Some(WindowEventTranslation::Pointer(pe)) => {
                window_handle.pointer_event(pe);
            }
            None => {}
        }

        match event {
            WindowEvent::ActivationTokenDone { .. } => {}
            WindowEvent::SurfaceResized(size) => {
                window_handle.refresh_live_resize();
                let size: LogicalSize<f64> = size.to_logical(window_handle.window_state.os_scale);
                let size = Size::new(size.width, size.height);
                window_handle.size(size);
            }
            WindowEvent::Moved(position) => {
                let position: LogicalPosition<f64> =
                    position.to_logical(window_handle.window_state.os_scale);
                let point = Point::new(position.x, position.y);
                window_handle.position(point);
            }
            WindowEvent::CloseRequested => {
                if self.should_close_window_on_request(window_id) {
                    self.close_window(window_id, event_loop);
                }
            }
            WindowEvent::Destroyed => {
                self.close_window(window_id, event_loop);
            }
            WindowEvent::DragDropped { paths, position } => {
                let logical_pos =
                    PhysicalPosition::new(position.x, position.y).to_logical(event_scale);
                let paths_rc: std::rc::Rc<[std::path::PathBuf]> = paths.clone().into();
                window_handle.file_drag_dropped(FileDragEvent::Drop(
                    dropped_file::FileDragDropped {
                        paths: paths_rc,
                        position: Point::new(logical_pos.x, logical_pos.y),
                    },
                ));
            }
            WindowEvent::DragEntered { paths, position } => {
                let logical_pos =
                    PhysicalPosition::new(position.x, position.y).to_logical(event_scale);
                window_handle.file_drag_start(paths, Point::new(logical_pos.x, logical_pos.y));
            }
            WindowEvent::DragMoved { position } => {
                let logical_pos =
                    PhysicalPosition::new(position.x, position.y).to_logical(event_scale);
                window_handle.file_drag_move(Point::new(logical_pos.x, logical_pos.y));
            }
            WindowEvent::DragLeft { .. } => {
                window_handle.file_drag_end();
            }
            WindowEvent::Focused(focused) => {
                window_handle.focused(focused);
            }
            WindowEvent::KeyboardInput { .. } => {
                // already handled by the ui-events reducer
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                window_handle.modifiers_changed(
                    ui_events_winit::keyboard::from_winit_modifier_state(modifiers.state()),
                );
            }
            WindowEvent::Ime(ime) => {
                window_handle.ime(ime);
            }
            WindowEvent::MouseWheel { .. } => {}
            WindowEvent::PinchGesture {
                delta: _, phase: _, ..
            } => {}
            WindowEvent::TouchpadPressure { .. } => {}
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                window_handle.os_scale(scale_factor);
            }
            WindowEvent::ThemeChanged(theme) => {
                window_handle.set_theme(Some(theme), true);
            }
            WindowEvent::Occluded(occluded) => {
                window_handle.set_occluded(occluded);
                if occluded {
                    self.animating_windows.remove(&window_id);
                }
            }
            WindowEvent::RedrawRequested => {
                window_handle.render_frame();
            }
            WindowEvent::PanGesture { .. } => {}
            WindowEvent::DoubleTapGesture { .. } => {}
            WindowEvent::RotationGesture { .. } => {}
            WindowEvent::PointerMoved { .. } => {
                //already handled by the ui-events reducer
            }
            WindowEvent::PointerEntered { .. } => {
                //already handled by the ui-events reducer
            }
            WindowEvent::PointerLeft { .. } => {
                //already handled by the ui-events reducer
            }
            WindowEvent::PointerButton { .. } => {
                //already handled by the ui-events reducer
            }
        }

        if let Some((name, start, new_frame)) = start {
            let end = Instant::now();

            if let Some(window_handle) = self.window_handles.get_mut(&window_id) {
                let profile = window_handle.profile.as_mut().unwrap();

                profile
                    .current
                    .events
                    .push(ProfileEvent { start, end, name });

                if new_frame {
                    profile.next_frame();
                }
            }
        }
        if !is_redraw {
            self.request_update();
        }
    }

    pub(crate) fn new_window(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        view_fn: Box<dyn FnOnce(WindowId) -> Box<dyn View>>,
        override_theme: Option<Theme>,
        #[allow(unused_variables)] WindowConfig {
            size,
            min_size,
            max_size,
            position,
            show_titlebar,
            transparent,
            fullscreen,
            window_icon,
            title,
            enabled_buttons,
            resizable,
            undecorated,
            undecorated_shadow,
            window_level,
            theme_override,
            apply_default_theme,
            mac_os_config,
            win_os_config,
            web_config,
            font_embolden,
        }: WindowConfig,
    ) {
        let logical_size = size.map(|size| LogicalSize::new(size.width, size.height));
        let logical_min_size = min_size.map(|size| LogicalSize::new(size.width, size.height));
        let logical_max_size = max_size.map(|size| LogicalSize::new(size.width, size.height));

        #[cfg(target_os = "macos")]
        let mut mac_attrs = winit::platform::macos::WindowAttributesMacOS::default();

        let mut window_attributes = winit::window::WindowAttributes::default()
            .with_visible(false)
            .with_title(title)
            .with_decorations(!undecorated)
            .with_transparent(transparent)
            .with_fullscreen(fullscreen)
            .with_window_level(window_level)
            .with_window_icon(window_icon)
            .with_resizable(resizable)
            // .with_theme(theme_override)
            .with_enabled_buttons(enabled_buttons);
        if theme_override.is_none() {
            window_attributes = window_attributes.with_theme(override_theme);
        } else {
            window_attributes = window_attributes.with_theme(theme_override);
        }

        #[cfg(target_arch = "wasm32")]
        {
            use wgpu::web_sys::wasm_bindgen::JsCast;

            let parent_id = web_config.expect("Specify an id for the canvas.").canvas_id;
            let doc = web_sys::window()
                .and_then(|win| win.document())
                .expect("Couldn't get document.");
            let canvas = doc
                .get_element_by_id(&parent_id)
                .expect("Couldn't get canvas by supplied id.");
            let canvas = canvas
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .expect("Element behind supplied id is not a canvas.");

            if let Some(size) = logical_size {
                canvas.set_width(size.width as u32);
                canvas.set_height(size.height as u32);
            }

            let web_attrs =
                winit::platform::web::WindowAttributesWeb::default().with_canvas(Some(canvas));
            window_attributes = window_attributes.with_platform_attributes(Box::new(web_attrs));
        };

        if let Some(Point { x, y }) = position {
            window_attributes = window_attributes.with_position(LogicalPosition::new(x, y));
        }

        if let Some(logical_size) = logical_size {
            window_attributes = window_attributes.with_surface_size(logical_size);
        }
        if let Some(logical_min_size) = logical_min_size {
            window_attributes = window_attributes.with_min_surface_size(logical_min_size);
        }
        if let Some(logical_max_size) = logical_max_size {
            window_attributes = window_attributes.with_max_surface_size(logical_max_size);
        }

        #[cfg(not(target_os = "macos"))]
        if !show_titlebar {
            window_attributes = window_attributes.with_decorations(false);
        }

        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowAttributesWindows;
            let mut win =
                WindowAttributesWindows::default().with_undecorated_shadow(undecorated_shadow);
            if let Some(cfg) = win_os_config {
                use crate::window::convert_to_win;
                win = win
                    .with_title_background_color(convert_to_win(cfg.set_title_background_color))
                    .with_border_color(convert_to_win(cfg.set_border_color))
                    .with_skip_taskbar(cfg.set_skip_taskbar)
                    .with_corner_preference(cfg.corner_preference.into())
                    .with_system_backdrop(cfg.set_system_backdrop.into())
                    .with_title_text_color(
                        convert_to_win(cfg.set_title_text_color).unwrap_or_default(),
                    );
            }
            window_attributes = window_attributes.with_platform_attributes(Box::new(win));
        }

        #[cfg(target_os = "macos")]
        if !show_titlebar {
            mac_attrs = mac_attrs
                .with_movable_by_window_background(false)
                .with_title_hidden(true)
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true);
            // .with_traffic_lights_offset(11.0, 16.0);
        }

        #[cfg(target_os = "macos")]
        if undecorated {
            // A palette-style window that will only obtain window focus but
            // not actually propagate the first mouse click it receives is
            // very unlikely to be expected behavior - these typically are
            // used for something that offers a quick choice and are closed
            // in a single pointer gesture.
            mac_attrs = mac_attrs.with_accepts_first_mouse(true);
        }

        #[cfg(target_os = "macos")]
        if let Some(mac) = &mac_os_config {
            if let Some(val) = mac.movable_by_window_background {
                mac_attrs = mac_attrs.with_movable_by_window_background(val);
            }
            if let Some(val) = mac.titlebar_transparent {
                mac_attrs = mac_attrs.with_titlebar_transparent(val);
            }
            if let Some(val) = mac.titlebar_hidden {
                mac_attrs = mac_attrs.with_titlebar_hidden(val);
            }
            if let Some(val) = mac.title_hidden {
                mac_attrs = mac_attrs.with_title_hidden(val);
            }
            if let Some(val) = mac.full_size_content_view {
                mac_attrs = mac_attrs.with_fullsize_content_view(val);
            }
            if let Some(val) = mac.unified_titlebar {
                mac_attrs = mac_attrs.with_unified_titlebar(val);
            }
            if let Some(val) = mac.movable {
                mac_attrs = mac_attrs.with_movable_by_window_background(val);
            }
            if let Some(val) = mac.accepts_first_mouse {
                mac_attrs = mac_attrs.with_accepts_first_mouse(val);
            }
            if let Some(val) = mac.option_as_alt {
                mac_attrs = mac_attrs.with_option_as_alt(val.into());
            }
            if let Some(title) = &mac.tabbing_identifier {
                mac_attrs = mac_attrs.with_tabbing_identifier(title.as_str());
            }
            if let Some(disallow_hidpi) = mac.disallow_high_dpi {
                mac_attrs = mac_attrs.with_disallow_hidpi(disallow_hidpi);
            }
            if let Some(shadow) = mac.has_shadow {
                mac_attrs = mac_attrs.with_has_shadow(shadow);
            }
            if let Some(hide) = mac.titlebar_buttons_hidden {
                mac_attrs = mac_attrs.with_titlebar_buttons_hidden(hide)
            }
            // if let Some(panel) = mac.panel {
            //     window_attributes = window_attributes.with_panel(panel)
            // }
            window_attributes = window_attributes.with_platform_attributes(Box::new(mac_attrs));
        }

        let Ok(window) = event_loop.create_window(window_attributes) else {
            return;
        };
        #[cfg(target_os = "macos")]
        if let Some(mac) = &mac_os_config
            && let Some((x, y)) = mac.traffic_lights_offset
        {
            use raw_window_handle::HasWindowHandle;

            if let Ok(wh) = window.window_handle() {
                use raw_window_handle::RawWindowHandle;

                if let RawWindowHandle::AppKit(app_kit) = wh.as_raw() {
                    let _ = setup_traffic_light_constraints_all_pixels(&app_kit, x, y, 6.);
                }
            }
        }
        let window_id = window.id();
        let window_handle = WindowHandle::new(
            window,
            self.gpu_resources.clone(),
            self.config.wgpu_features,
            self.config.wgpu_backends,
            view_fn,
            transparent,
            apply_default_theme,
            font_embolden,
        );
        self.window_handles.insert(window_id, window_handle);
    }

    /// Dispatch a `CloseRequested` event to the window's view tree and return
    /// whether the window should be closed.
    ///
    /// Returns `true` if no handler called `cx.prevent_default()` (the window
    /// should close). Returns `false` if any handler prevented the default
    /// (the window should stay open). Returns `false` if the window does not
    /// exist.
    fn should_close_window_on_request(&mut self, window_id: WindowId) -> bool {
        let Some(handle) = self.window_handles.get_mut(&window_id) else {
            return false;
        };

        !handle.event(crate::event::Event::Window(
            crate::event::WindowEvent::CloseRequested,
        ))
    }

    fn close_window(&mut self, window_id: WindowId, event_loop: &dyn ActiveEventLoop) {
        if let Some(handle) = self.window_handles.get_mut(&window_id) {
            handle.destroy();
        }
        self.window_handles.remove(&window_id);
        if self.window_handles.is_empty() && self.config.exit_on_close {
            event_loop.exit();
        }
    }

    fn capture_window(&mut self, window_id: WindowId) -> Option<Capture> {
        self.window_handles
            .get_mut(&window_id)
            .map(|handle| handle.capture())
    }

    pub(crate) fn idle(&mut self) {
        let ext_events = { std::mem::take(&mut *EXT_EVENT_HANDLER.queue.lock()) };

        for trigger in ext_events {
            trigger.notify();
        }
        self.request_update();
    }

    pub(crate) fn request_update(&self) {
        Application::request_update();
    }

    fn handle_updates_for_all_windows_budgeted(
        &mut self,
        start: Instant,
        budget: Duration,
    ) -> bool {
        let mut any_work_remaining = false;

        for (window_id, handle) in self.window_handles.iter_mut() {
            handle.process_scheduled_updates();
            let done = handle.process_update_budgeted(start, budget);
            if !done {
                any_work_remaining = true;
            }

            if handle.window_state.request_paint && handle.can_render_now() {
                handle.window.request_redraw();
                let frame_interval = handle
                    .window
                    .current_monitor()
                    .and_then(|m| m.current_video_mode())
                    .and_then(|v| v.refresh_rate_millihertz())
                    .map(|mhz| Duration::from_nanos(1_000_000_000_000 / mhz.get() as u64))
                    .unwrap_or(Duration::from_millis(8));
                handle.render_frame_if_due(frame_interval);
            }

            if !done || start.elapsed() >= budget {
                any_work_remaining = true;
                break;
            }

            // Keep window updates in the same update phase but bound by the same budget.
            while process_window_updates(window_id) {
                if start.elapsed() >= budget {
                    any_work_remaining = true;
                    break;
                }
            }
            if start.elapsed() >= budget {
                any_work_remaining = true;
                break;
            }
        }

        any_work_remaining
    }

    fn frame_duration_for_window(&self, window_id: &winit::window::WindowId) -> Duration {
        self.window_handles
            .get(window_id)
            .and_then(|h| h.window.current_monitor())
            .and_then(|m| m.current_video_mode())
            .and_then(|v| v.refresh_rate_millihertz())
            .map(|mhz| Duration::from_nanos(1_000_000_000_000 / mhz.get() as u64))
            .unwrap_or(Duration::from_millis(8))
    }

    fn window_can_render(&self, window_id: &winit::window::WindowId) -> bool {
        self.window_handles
            .get(window_id)
            .map(|h| h.can_render_now())
            .unwrap_or(false)
    }

    fn update_control_flow(&self, event_loop: &dyn ActiveEventLoop) {
        let timer_deadline = self.timers.values().map(|t| t.deadline).min();

        let animation_deadline = if self.animating_windows.is_empty() {
            None
        } else {
            let now = Instant::now();
            self.animating_windows
                .iter()
                .filter(|wid| self.window_can_render(wid))
                .map(|wid| now + self.frame_duration_for_window(wid))
                .min()
        };

        match (timer_deadline, animation_deadline) {
            (Some(t), Some(a)) => {
                event_loop.set_control_flow(ControlFlow::WaitUntil(t.min(a)));
            }
            (Some(t), None) => {
                event_loop.set_control_flow(ControlFlow::WaitUntil(t));
            }
            (None, Some(a)) => {
                event_loop.set_control_flow(ControlFlow::WaitUntil(a));
            }
            (None, None) => {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
    }

    fn request_timer(&mut self, timer: Timer, event_loop: &dyn ActiveEventLoop) {
        self.timers.insert(timer.token, timer);
        self.fire_timer(event_loop);
    }

    fn remove_timer(&mut self, timer: &TimerToken, event_loop: &dyn ActiveEventLoop) {
        self.timers.remove(timer);
        if self.timers.is_empty() {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn fire_timer(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.timers.is_empty() {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }

        let deadline = self.timers.values().map(|timer| timer.deadline).min();
        if let Some(deadline) = deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
    }

    pub(crate) fn handle_timer(&mut self, event_loop: &dyn ActiveEventLoop) {
        let now = Instant::now();
        let tokens: Vec<TimerToken> = self
            .timers
            .iter()
            .filter_map(|(token, timer)| {
                if timer.deadline <= now {
                    Some(*token)
                } else {
                    None
                }
            })
            .collect();
        if !tokens.is_empty() {
            for token in tokens {
                if let Some(mut timer) = self.timers.remove(&token) {
                    if timer.is_animation
                        && timer
                            .window_id
                            .is_some_and(|window_id| !self.window_can_render(&window_id))
                    {
                        // Keep animation timers dormant while hidden/occluded.
                        timer.deadline = now + Duration::from_millis(100);
                        self.timers.insert(token, timer);
                        continue;
                    }
                    (timer.action)(token);
                }
            }
            self.request_update();
        }
        self.fire_timer(event_loop);
    }

    pub(crate) fn flush_deferred_context_menus(&mut self) {
        let pending = std::mem::take(&mut self.pending_context_menus);
        for item in pending {
            if let Some(handle) = self.window_handles.get_mut(&item.window_id) {
                handle.show_context_menu(item.menu.0, item.pos);
            }
        }
    }
}

/// Sets up traffic light button constraints with precise pixel positioning.
///
/// # Parameters
/// - `leading_pixels`: Distance from left edge of title bar to close button
///   (typically 10.0 for standard macOS positioning)
/// - `top_pixels`: Distance from top edge of title bar to **top edge** of
///   buttons
/// - `button_spacing_pixels`: Spacing between traffic light buttons (typically
///   6.0 for native macOS appearance)
///
/// # Calculating `top_pixels` for vertical centering
/// Traffic light buttons are typically 13pt tall, so to center them:
/// `top_pixels = (top_bar_height - 13.0) / 2.0`
///
/// # Example for centering in a 30pt top bar
/// ```rust,ignore
/// // Standard horizontal position (10pt), centered vertically (8.5pt from top), standard spacing (6pt)
/// setup_traffic_light_constraints_all_pixels(view_handle, 10.0, 8.5, 6.0)?;
/// ```
///
/// # Common values
/// - Standard positioning: `(10.0, 8.0, 6.0)`
/// - Centered in 30pt bar: `(10.0, 8.5, 6.0)`
/// - Centered in 40pt bar: `(10.0, 13.5, 6.0)`
/// - Centered in 50pt bar: `(10.0, 18.5, 6.0)`
#[cfg(target_os = "macos")]
fn setup_traffic_light_constraints_all_pixels(
    view_handle: &raw_window_handle::AppKitWindowHandle,
    leading_pixels: f64,
    top_pixels: f64,
    button_spacing_pixels: f64,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use {
        objc2_app_kit::{NSLayoutAttribute, NSLayoutConstraint, NSLayoutRelation, NSWindowButton},
        objc2_foundation::NSArray,
    };

    let ns_view = view_handle.ns_view.cast::<objc2_app_kit::NSView>();
    let ns_view = unsafe { &*ns_view.as_ptr() };
    let window = ns_view
        .window()
        .ok_or("View must be attached to a window")?;

    let close_button = window.standardWindowButton(NSWindowButton::CloseButton);
    let miniaturize_button = window.standardWindowButton(NSWindowButton::MiniaturizeButton);
    let zoom_button = window.standardWindowButton(NSWindowButton::ZoomButton);
    let title_bar_view = close_button
        .as_ref()
        .and_then(|button| unsafe { button.superview() })
        .ok_or("Could not find title bar container view")?;

    unsafe {
        // Set up close button with exact pixel positioning
        if let Some(close_btn) = &close_button {
            close_btn.setTranslatesAutoresizingMaskIntoConstraints(false);

            let leading = NSLayoutConstraint::constraintWithItem_attribute_relatedBy_toItem_attribute_multiplier_constant(
                close_btn,
                NSLayoutAttribute::Leading,
                NSLayoutRelation::Equal,
                Some(&title_bar_view),
                NSLayoutAttribute::Leading,
                1.0,
                leading_pixels,
            );

            let top = NSLayoutConstraint::constraintWithItem_attribute_relatedBy_toItem_attribute_multiplier_constant(
                close_btn,
                NSLayoutAttribute::Top,
                NSLayoutRelation::Equal,
                Some(&title_bar_view),
                NSLayoutAttribute::Top,
                1.0,
                top_pixels,
            );

            title_bar_view.addConstraints(&NSArray::from_slice(&[&*leading, &*top]));
        }

        // Set up other buttons with custom spacing
        if let (Some(mini_btn), Some(close_btn)) = (&miniaturize_button, &close_button) {
            mini_btn.setTranslatesAutoresizingMaskIntoConstraints(false);

            let leading = NSLayoutConstraint::constraintWithItem_attribute_relatedBy_toItem_attribute_multiplier_constant(
                mini_btn,
                NSLayoutAttribute::Leading,
                NSLayoutRelation::Equal,
                Some(close_btn),
                NSLayoutAttribute::Trailing,
                1.0,
                button_spacing_pixels,
            );

            let center_y = NSLayoutConstraint::constraintWithItem_attribute_relatedBy_toItem_attribute_multiplier_constant(
                mini_btn,
                NSLayoutAttribute::CenterY,
                NSLayoutRelation::Equal,
                Some(close_btn),
                NSLayoutAttribute::CenterY,
                1.0,
                0.0,
            );

            title_bar_view.addConstraints(&NSArray::from_slice(&[&*leading, &*center_y]));
        }

        if let (Some(zoom_btn), Some(mini_btn)) = (&zoom_button, &miniaturize_button) {
            zoom_btn.setTranslatesAutoresizingMaskIntoConstraints(false);

            let leading = NSLayoutConstraint::constraintWithItem_attribute_relatedBy_toItem_attribute_multiplier_constant(
                zoom_btn,
                NSLayoutAttribute::Leading,
                NSLayoutRelation::Equal,
                Some(mini_btn),
                NSLayoutAttribute::Trailing,
                1.0,
                button_spacing_pixels,
            );

            let center_y = NSLayoutConstraint::constraintWithItem_attribute_relatedBy_toItem_attribute_multiplier_constant(
                zoom_btn,
                NSLayoutAttribute::CenterY,
                NSLayoutRelation::Equal,
                Some(mini_btn),
                NSLayoutAttribute::CenterY,
                1.0,
                0.0,
            );

            title_bar_view.addConstraints(&NSArray::from_slice(&[&*leading, &*center_y]));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        view::ViewId,
        views::{Decorators, Empty},
    };

    #[test]
    fn close_requested_defaults_to_closing_when_unhandled() {
        let mut app = ApplicationHandle::new(AppConfig::default());

        let root_id = ViewId::new_root();
        crate::window::handle::set_current_view(root_id);
        let view = Empty::new().style(|s| s.size(100.0, 100.0));
        let window_handle = WindowHandle::new_headless(root_id, view, Size::new(400.0, 300.0), 1.0);
        let window_id = window_handle.window_id();

        app.window_handles.insert(window_id, window_handle);

        assert!(app.should_close_window_on_request(window_id));
    }

    #[test]
    fn close_requested_can_be_prevented() {
        let mut app = ApplicationHandle::new(AppConfig::default());

        let root_id = ViewId::new_root();
        crate::window::handle::set_current_view(root_id);
        let view = Empty::new().style(|s| s.size(100.0, 100.0)).on_event_cont(
            crate::event::listener::WindowCloseRequested,
            |cx, _| {
                cx.prevent_default();
            },
        );
        let window_handle = WindowHandle::new_headless(root_id, view, Size::new(400.0, 300.0), 1.0);
        let window_id = window_handle.window_id();

        app.window_handles.insert(window_id, window_handle);

        assert!(!app.should_close_window_on_request(window_id));
        assert_eq!(app.window_handles.len(), 1);
    }

    #[test]
    fn close_requested_stop_only_does_not_cancel_close() {
        let mut app = ApplicationHandle::new(AppConfig::default());

        let root_id = ViewId::new_root();
        crate::window::handle::set_current_view(root_id);
        let view = Empty::new()
            .style(|s| s.size(100.0, 100.0))
            .on_event_stop(crate::event::listener::WindowCloseRequested, |_cx, _| {});
        let window_handle = WindowHandle::new_headless(root_id, view, Size::new(400.0, 300.0), 1.0);
        let window_id = window_handle.window_id();

        app.window_handles.insert(window_id, window_handle);

        assert!(app.should_close_window_on_request(window_id));
    }

    #[test]
    fn preventing_one_window_close_does_not_affect_another() {
        let mut app = ApplicationHandle::new(AppConfig::default());

        let prevented_root = ViewId::new_root();
        crate::window::handle::set_current_view(prevented_root);
        let prevented_view = Empty::new().style(|s| s.size(100.0, 100.0)).on_event_cont(
            crate::event::listener::WindowCloseRequested,
            |cx, _| {
                cx.prevent_default();
            },
        );
        let prevented_window = WindowHandle::new_headless(
            prevented_root,
            prevented_view,
            Size::new(400.0, 300.0),
            1.0,
        );
        let prevented_id = prevented_window.window_id();

        let plain_root = ViewId::new_root();
        crate::window::handle::set_current_view(plain_root);
        let plain_view = Empty::new().style(|s| s.size(100.0, 100.0));
        let plain_window =
            WindowHandle::new_headless(plain_root, plain_view, Size::new(400.0, 300.0), 1.0);
        let plain_id = plain_window.window_id();

        app.window_handles.insert(prevented_id, prevented_window);
        app.window_handles.insert(plain_id, plain_window);

        assert!(!app.should_close_window_on_request(prevented_id));
        assert!(app.should_close_window_on_request(plain_id));
        assert_eq!(app.window_handles.len(), 2);
    }

    #[test]
    fn request_close_window_allows_unhandled() {
        let mut app = ApplicationHandle::new(AppConfig::default());

        let root_id = ViewId::new_root();
        crate::window::handle::set_current_view(root_id);
        let view = Empty::new().style(|s| s.size(100.0, 100.0));
        let window_handle = WindowHandle::new_headless(root_id, view, Size::new(400.0, 300.0), 1.0);
        let window_id = window_handle.window_id();

        app.window_handles.insert(window_id, window_handle);

        // Simulate what AppUpdateEvent::RequestCloseWindow does
        assert!(app.should_close_window_on_request(window_id));
    }

    #[test]
    fn request_close_window_respects_prevent_default() {
        let mut app = ApplicationHandle::new(AppConfig::default());

        let root_id = ViewId::new_root();
        crate::window::handle::set_current_view(root_id);
        let view = Empty::new().style(|s| s.size(100.0, 100.0)).on_event_cont(
            crate::event::listener::WindowCloseRequested,
            |cx, _| {
                cx.prevent_default();
            },
        );
        let window_handle = WindowHandle::new_headless(root_id, view, Size::new(400.0, 300.0), 1.0);
        let window_id = window_handle.window_id();

        app.window_handles.insert(window_id, window_handle);

        // Simulate what AppUpdateEvent::RequestCloseWindow does
        assert!(!app.should_close_window_on_request(window_id));
        assert_eq!(app.window_handles.len(), 1);
    }
}
