use std::{collections::HashMap, mem, rc::Rc};

use floem_reactive::SignalUpdate;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

#[cfg(target_arch = "wasm32")]
use wgpu::web_sys;

use floem_winit::{
    dpi::{LogicalPosition, LogicalSize},
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoopProxy, EventLoopWindowTarget},
    window::WindowId,
};

use peniko::kurbo::{Point, Size};

use crate::{
    action::{Timer, TimerToken},
    app::{AppUpdateEvent, UserEvent, APP_UPDATE_EVENTS},
    ext_event::EXT_EVENT_HANDLER,
    inspector::Capture,
    profiler::{Profile, ProfileEvent},
    view::View,
    window::WindowConfig,
    window_handle::WindowHandle,
    window_id::process_window_updates,
};

pub(crate) struct ApplicationHandle {
    window_handles: HashMap<floem_winit::window::WindowId, WindowHandle>,
    timers: HashMap<TimerToken, Timer>,
}

impl ApplicationHandle {
    pub(crate) fn new() -> Self {
        Self {
            window_handles: HashMap::new(),
            timers: HashMap::new(),
        }
    }

    pub(crate) fn handle_user_event(
        &mut self,
        event_loop: &EventLoopWindowTarget<UserEvent>,
        event_proxy: EventLoopProxy<UserEvent>,
        event: UserEvent,
    ) {
        match event {
            UserEvent::AppUpdate => {
                self.handle_update_event(event_loop, event_proxy);
            }
            UserEvent::Idle => {
                self.idle();
            }
            UserEvent::QuitApp => {
                event_loop.exit();
            }
            UserEvent::GpuResourcesUpdate { window_id } => {
                self.window_handles
                    .get_mut(&window_id)
                    .unwrap()
                    .init_renderer();
            }
        }
    }

    pub(crate) fn handle_update_event(
        &mut self,
        event_loop: &EventLoopWindowTarget<UserEvent>,
        event_proxy: EventLoopProxy<UserEvent>,
    ) {
        let events = APP_UPDATE_EVENTS.with(|events| {
            let mut events = events.borrow_mut();
            std::mem::take(&mut *events)
        });
        for event in events {
            match event {
                AppUpdateEvent::NewWindow { view_fn, config } => self.new_window(
                    event_loop,
                    event_proxy.clone(),
                    view_fn,
                    config.unwrap_or_default(),
                ),
                AppUpdateEvent::CloseWindow { window_id } => {
                    self.close_window(window_id, event_loop);
                }
                AppUpdateEvent::RequestTimer { timer } => {
                    self.request_timer(timer, event_loop);
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
                #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                AppUpdateEvent::MenuAction {
                    window_id,
                    action_id,
                } => {
                    let window_handle = match self.window_handles.get_mut(&window_id) {
                        Some(window_handle) => window_handle,
                        None => return,
                    };
                    window_handle.menu_action(action_id);
                }
            }
        }
    }

    pub(crate) fn handle_window_event(
        &mut self,
        window_id: floem_winit::window::WindowId,
        event: WindowEvent,
        event_loop: &EventLoopWindowTarget<UserEvent>,
    ) {
        let window_handle = match self.window_handles.get_mut(&window_id) {
            Some(window_handle) => window_handle,
            None => return,
        };

        // We only start reacting to events once the window is ready
        // I.e. once the renderer has acquired the necessary GPU resources (if any) and is initialized.
        if !window_handle.is_initialized() {
            return;
        }

        let start = window_handle.profile.is_some().then(|| {
            let name = match event {
                WindowEvent::ActivationTokenDone { .. } => "ActivationTokenDone",
                WindowEvent::Resized(..) => "Resized",
                WindowEvent::Moved(..) => "Moved",
                WindowEvent::CloseRequested => "CloseRequested",
                WindowEvent::Destroyed => "Destroyed",
                WindowEvent::DroppedFile(_) => "DroppedFile",
                WindowEvent::HoveredFile(_) => "HoveredFile",
                WindowEvent::HoveredFileCancelled => "HoveredFileCancelled",
                WindowEvent::Focused(..) => "Focused",
                WindowEvent::KeyboardInput { .. } => "KeyboardInput",
                WindowEvent::ModifiersChanged(..) => "ModifiersChanged",
                WindowEvent::Ime(..) => "Ime",
                WindowEvent::CursorMoved { .. } => "CursorMoved",
                WindowEvent::CursorEntered { .. } => "CursorEntered",
                WindowEvent::CursorLeft { .. } => "CursorLeft",
                WindowEvent::MouseWheel { .. } => "MouseWheel",
                WindowEvent::MouseInput { .. } => "MouseInput",
                WindowEvent::TouchpadMagnify { .. } => "TouchpadMagnify",
                WindowEvent::SmartMagnify { .. } => "SmartMagnify",
                WindowEvent::TouchpadRotate { .. } => "TouchpadRotate",
                WindowEvent::TouchpadPressure { .. } => "TouchpadPressure",
                WindowEvent::AxisMotion { .. } => "AxisMotion",
                WindowEvent::Touch(_) => "Touch",
                WindowEvent::ScaleFactorChanged { .. } => "ScaleFactorChanged",
                WindowEvent::ThemeChanged(..) => "ThemeChanged",
                WindowEvent::Occluded(..) => "Occluded",
                WindowEvent::MenuAction(..) => "MenuAction",
                WindowEvent::RedrawRequested => "RedrawRequested",
            };
            (
                name,
                Instant::now(),
                matches!(event, WindowEvent::RedrawRequested),
            )
        });

        match event {
            WindowEvent::ActivationTokenDone { .. } => {}
            WindowEvent::Resized(size) => {
                let size: LogicalSize<f64> = size.to_logical(window_handle.scale);
                let size = Size::new(size.width, size.height);
                window_handle.size(size);
            }
            WindowEvent::Moved(position) => {
                let position: LogicalPosition<f64> = position.to_logical(window_handle.scale);
                let point = Point::new(position.x, position.y);
                window_handle.position(point);
            }
            WindowEvent::CloseRequested => {
                self.close_window(window_id, event_loop);
            }
            WindowEvent::Destroyed => {
                self.close_window(window_id, event_loop);
            }
            WindowEvent::DroppedFile(path) => {
                window_handle.dropped_file(path);
            }
            WindowEvent::HoveredFile(_) => {}
            WindowEvent::HoveredFileCancelled => {}
            WindowEvent::Focused(focused) => {
                window_handle.focused(focused);
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                if !is_synthetic {
                    window_handle.key_event(event);
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                window_handle.modifiers_changed(modifiers.state());
            }
            WindowEvent::Ime(ime) => {
                window_handle.ime(ime);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let position: LogicalPosition<f64> = position.to_logical(window_handle.scale);
                let point = Point::new(position.x, position.y);
                window_handle.pointer_move(point);
            }
            WindowEvent::CursorEntered { .. } => {}
            WindowEvent::CursorLeft { .. } => {
                window_handle.pointer_leave();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                window_handle.mouse_wheel(delta);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                window_handle.mouse_input(button, state);
            }
            WindowEvent::TouchpadMagnify { .. } => {}
            WindowEvent::SmartMagnify { .. } => {}
            WindowEvent::TouchpadRotate { .. } => {}
            WindowEvent::TouchpadPressure { .. } => {}
            WindowEvent::AxisMotion { .. } => {}
            WindowEvent::Touch(_) => {}
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                window_handle.scale(scale_factor);
            }
            WindowEvent::ThemeChanged(theme) => {
                window_handle.os_theme_changed(theme);
            }
            WindowEvent::Occluded(_) => {}
            WindowEvent::MenuAction(id) => {
                window_handle.menu_action(id);
            }
            WindowEvent::RedrawRequested => {
                window_handle.render_frame();
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
        self.handle_updates_for_all_windows();
    }

    pub(crate) fn new_window(
        &mut self,
        event_loop: &EventLoopWindowTarget<UserEvent>,
        event_proxy: EventLoopProxy<UserEvent>,
        view_fn: Box<dyn FnOnce(WindowId) -> Box<dyn View>>,
        #[allow(unused_variables)] WindowConfig {
            size,
            position,
            show_titlebar,
            transparent,
            fullscreen,
            window_icon,
            title,
            enabled_buttons,
            resizable,
            undecorated,
            window_level,
            apply_default_theme,
            mac_os_config,
            web_config,
            font_embolden,
        }: WindowConfig,
    ) {
        let logical_size = size.map(|size| LogicalSize::new(size.width, size.height));

        let mut window_builder = floem_winit::window::WindowBuilder::new()
            .with_title(title)
            .with_decorations(!undecorated)
            .with_transparent(transparent)
            .with_fullscreen(fullscreen)
            .with_window_level(window_level)
            .with_window_icon(window_icon)
            .with_resizable(resizable)
            .with_enabled_buttons(enabled_buttons);

        #[cfg(target_arch = "wasm32")]
        {
            use floem_winit::platform::web::WindowBuilderExtWebSys;
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

            window_builder = window_builder.with_canvas(Some(canvas));
        };

        if let Some(Point { x, y }) = position {
            window_builder = window_builder.with_position(LogicalPosition::new(x, y));
        }

        if let Some(logical_size) = logical_size {
            window_builder = window_builder.with_inner_size(logical_size);
        }

        #[cfg(not(target_os = "macos"))]
        if !show_titlebar {
            window_builder = window_builder.with_decorations(false);
        }

        #[cfg(target_os = "macos")]
        if !show_titlebar {
            use floem_winit::platform::macos::WindowBuilderExtMacOS;
            window_builder = window_builder
                .with_movable(false)
                .with_title_hidden(true)
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true)
                .with_traffic_lights_offset(11.0, 16.0);
        }

        #[cfg(target_os = "macos")]
        if undecorated {
            use floem_winit::platform::macos::WindowBuilderExtMacOS;
            // A palette-style window that will only obtain window focus but
            // not actually propagate the first mouse click it receives is
            // very unlikely to be expected behavior - these typically are
            // used for something that offers a quick choice and are closed
            // in a single pointer gesture.
            window_builder = window_builder.with_accepts_first_mouse(true);
        }

        #[cfg(target_os = "macos")]
        if let Some(mac) = mac_os_config {
            use floem_winit::platform::macos::WindowBuilderExtMacOS;
            if let Some(val) = mac.movable_by_window_background {
                window_builder = window_builder.with_movable_by_window_background(val);
            }
            if let Some(val) = mac.titlebar_transparent {
                window_builder = window_builder.with_titlebar_transparent(val);
            }
            if let Some(val) = mac.titlebar_hidden {
                window_builder = window_builder.with_titlebar_hidden(val);
            }
            if let Some(val) = mac.full_size_content_view {
                window_builder = window_builder.with_fullsize_content_view(val);
            }
            if let Some(val) = mac.movable {
                window_builder = window_builder.with_movable(val);
            }
            if let Some((x, y)) = mac.traffic_lights_offset {
                window_builder = window_builder.with_traffic_lights_offset(x, y);
            }
            if let Some(val) = mac.accepts_first_mouse {
                window_builder = window_builder.with_accepts_first_mouse(val);
            }
            if let Some(val) = mac.option_as_alt {
                window_builder = window_builder.with_option_as_alt(val.into());
            }
            if let Some(title) = mac.tabbing_identifier {
                window_builder = window_builder.with_tabbing_identifier(title.as_str());
            }
            if let Some(disallow_hidpi) = mac.disallow_high_dpi {
                window_builder = window_builder.with_disallow_hidpi(disallow_hidpi);
            }
            if let Some(shadow) = mac.has_shadow {
                window_builder = window_builder.with_has_shadow(shadow);
            }
            if let Some(hide) = mac.titlebar_buttons_hidden {
                window_builder = window_builder.with_titlebar_buttons_hidden(hide)
            }
        }

        let Ok(window) = window_builder.build(event_loop) else {
            return;
        };
        let window_id = window.id();
        let window_handle = WindowHandle::new(
            window,
            event_proxy,
            view_fn,
            transparent,
            apply_default_theme,
            logical_size,
            font_embolden,
        );
        self.window_handles.insert(window_id, window_handle);
    }

    fn close_window(
        &mut self,
        window_id: WindowId,
        #[cfg(target_os = "macos")] _event_loop: &EventLoopWindowTarget<UserEvent>,
        #[cfg(not(target_os = "macos"))] event_loop: &EventLoopWindowTarget<UserEvent>,
    ) {
        if let Some(handle) = self.window_handles.get_mut(&window_id) {
            handle.window = None;
            handle.destroy();
        }
        self.window_handles.remove(&window_id);
        #[cfg(not(target_os = "macos"))]
        if self.window_handles.is_empty() {
            event_loop.exit();
        }
    }

    fn capture_window(&mut self, window_id: WindowId) -> Option<Capture> {
        self.window_handles
            .get_mut(&window_id)
            .map(|handle| handle.capture())
    }

    pub(crate) fn idle(&mut self) {
        let ext_events = { mem::take(&mut *EXT_EVENT_HANDLER.queue.lock()) };

        for trigger in ext_events {
            trigger.notify();
        }

        self.handle_updates_for_all_windows();
    }

    fn handle_updates_for_all_windows(&mut self) {
        for (window_id, handle) in self.window_handles.iter_mut() {
            handle.process_update();
            while process_window_updates(window_id) {}
        }
    }

    fn request_timer(&mut self, timer: Timer, event_loop: &EventLoopWindowTarget<UserEvent>) {
        self.timers.insert(timer.token, timer);
        self.fire_timer(event_loop);
    }

    fn fire_timer(&mut self, event_loop: &EventLoopWindowTarget<UserEvent>) {
        if self.timers.is_empty() {
            return;
        }

        let deadline = self.timers.values().map(|timer| timer.deadline).min();
        if let Some(deadline) = deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
    }

    pub(crate) fn handle_timer(&mut self, event_loop: &EventLoopWindowTarget<UserEvent>) {
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
                if let Some(timer) = self.timers.remove(&token) {
                    (timer.action)(token);
                }
            }
            self.handle_updates_for_all_windows();
        }
        self.fire_timer(event_loop);
    }
}
