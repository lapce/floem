#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

#[cfg(target_arch = "wasm32")]
use wgpu::web_sys;

use floem_reactive::SignalUpdate;
use peniko::kurbo::{Point, Size};
use std::{collections::HashMap, rc::Rc};
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow},
    window::WindowId,
};

use crate::{
    action::{Timer, TimerToken},
    app::{AppEventCallback, AppUpdateEvent, UserEvent, APP_UPDATE_EVENTS},
    ext_event::EXT_EVENT_HANDLER,
    inspector::Capture,
    profiler::{Profile, ProfileEvent},
    view::View,
    window::WindowConfig,
    window_handle::WindowHandle,
    window_id::process_window_updates,
    AppEvent,
};

pub(crate) struct ApplicationHandle {
    window_handles: HashMap<winit::window::WindowId, WindowHandle>,
    timers: HashMap<TimerToken, Timer>,
    pub(crate) event_listener: Option<Box<AppEventCallback>>,
}

impl ApplicationHandle {
    pub(crate) fn new() -> Self {
        Self {
            window_handles: HashMap::new(),
            timers: HashMap::new(),
            event_listener: None,
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
            UserEvent::GpuResourcesUpdate { window_id } => {
                self.window_handles
                    .get_mut(&window_id)
                    .unwrap()
                    .init_renderer();
            }
        }
    }

    pub(crate) fn handle_update_event(&mut self, event_loop: &dyn ActiveEventLoop) {
        let events = APP_UPDATE_EVENTS.with(|events| {
            let mut events = events.borrow_mut();
            std::mem::take(&mut *events)
        });

        for event in events {
            match event {
                AppUpdateEvent::NewWindow { window_creation } => self.new_window(
                    event_loop,
                    window_creation.view_fn,
                    window_creation.config.unwrap_or_default(),
                ),
                AppUpdateEvent::CloseWindow { window_id } => {
                    self.close_window(window_id, event_loop);
                }
                AppUpdateEvent::RequestTimer { timer } => {
                    self.request_timer(timer, event_loop);
                }
                AppUpdateEvent::CancelTimer { timer } => {
                    self.remove_timer(&timer);
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
                AppUpdateEvent::MenuAction { action_id } => {
                    for (_, handle) in self.window_handles.iter_mut() {
                        if handle.app_state.context_menu.contains_key(&action_id)
                            || handle.app_state.window_menu.contains_key(&action_id)
                        {
                            handle.menu_action(&action_id);
                            break;
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn handle_window_event(
        &mut self,
        window_id: winit::window::WindowId,
        event: WindowEvent,
        event_loop: &dyn ActiveEventLoop,
    ) {
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
                WindowEvent::DroppedFile(_) => "DroppedFile",
                WindowEvent::HoveredFile(_) => "HoveredFile",
                WindowEvent::HoveredFileCancelled => "HoveredFileCancelled",
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
                // WindowEvent::MenuAction(..) => "MenuAction",
            };
            (
                name,
                Instant::now(),
                matches!(event, WindowEvent::RedrawRequested),
            )
        });

        match event {
            WindowEvent::ActivationTokenDone { .. } => {}
            WindowEvent::SurfaceResized(size) => {
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
            WindowEvent::PointerMoved { position, .. } => {
                let position: LogicalPosition<f64> = position.to_logical(window_handle.scale);
                let point = Point::new(position.x, position.y);
                window_handle.pointer_move(point);
            }
            WindowEvent::PointerEntered { .. } => {}
            WindowEvent::PointerLeft { .. } => {
                window_handle.pointer_leave();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                window_handle.mouse_wheel(delta);
            }
            WindowEvent::PointerButton { state, button, .. } => {
                window_handle.pointer_button(button, state);
            }
            WindowEvent::PinchGesture { delta, phase, .. } => {
                window_handle.pinch_gesture(delta, phase);
            }
            WindowEvent::TouchpadPressure { .. } => {}
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                window_handle.scale(scale_factor);
            }
            WindowEvent::ThemeChanged(theme) => {
                window_handle.os_theme_changed(theme);
            }
            WindowEvent::Occluded(_) => {}
            WindowEvent::RedrawRequested => {
                window_handle.render_frame();
            }
            WindowEvent::PanGesture { .. } => {}
            WindowEvent::DoubleTapGesture { .. } => {}
            WindowEvent::RotationGesture { .. } => {} // WindowEvent::MenuAction(id) => {
                                                      //     window_handle.menu_action(id);
                                                      // }
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
        event_loop: &dyn ActiveEventLoop,
        view_fn: Box<dyn FnOnce(WindowId) -> Box<dyn View>>,
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
            apply_default_theme,
            mac_os_config,
            web_config,
            font_embolden,
        }: WindowConfig,
    ) {
        let logical_size = size.map(|size| LogicalSize::new(size.width, size.height));
        let logical_min_size = min_size.map(|size| LogicalSize::new(size.width, size.height));
        let logical_max_size = max_size.map(|size| LogicalSize::new(size.width, size.height));

        let mut window_attributes = winit::window::WindowAttributes::default()
            .with_visible(false)
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
            use wgpu::web_sys::wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWeb;

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

            window_attributes = window_attributes.with_canvas(Some(canvas));
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
            use winit::platform::windows::WindowAttributesExtWindows;
            window_attributes = window_attributes.with_undecorated_shadow(undecorated_shadow);
        }

        #[cfg(target_os = "macos")]
        if !show_titlebar {
            use winit::platform::macos::WindowAttributesExtMacOS;
            window_attributes = window_attributes
                .with_movable_by_window_background(false)
                .with_title_hidden(true)
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true);
            // .with_traffic_lights_offset(11.0, 16.0);
        }

        #[cfg(target_os = "macos")]
        if undecorated {
            use winit::platform::macos::WindowAttributesExtMacOS;
            // A palette-style window that will only obtain window focus but
            // not actually propagate the first mouse click it receives is
            // very unlikely to be expected behavior - these typically are
            // used for something that offers a quick choice and are closed
            // in a single pointer gesture.
            window_attributes = window_attributes.with_accepts_first_mouse(true);
        }

        #[cfg(target_os = "macos")]
        if let Some(mac) = mac_os_config {
            use winit::platform::macos::WindowAttributesExtMacOS;
            if let Some(val) = mac.movable_by_window_background {
                window_attributes = window_attributes.with_movable_by_window_background(val);
            }
            if let Some(val) = mac.titlebar_transparent {
                window_attributes = window_attributes.with_titlebar_transparent(val);
            }
            if let Some(val) = mac.titlebar_hidden {
                window_attributes = window_attributes.with_titlebar_hidden(val);
            }
            if let Some(val) = mac.title_hidden {
                window_attributes = window_attributes.with_title_hidden(val);
            }
            if let Some(val) = mac.full_size_content_view {
                window_attributes = window_attributes.with_fullsize_content_view(val);
            }
            if let Some(val) = mac.unified_titlebar {
                window_attributes = window_attributes.with_unified_titlebar(val);
            }
            if let Some(val) = mac.movable {
                window_attributes = window_attributes.with_movable_by_window_background(val);
            }
            // if let Some((x, y)) = mac.traffic_lights_offset {
            // TODO
            // window_attributes = window_attributes.with_traffic_lights_offset(x, y);
            // }
            if let Some(val) = mac.accepts_first_mouse {
                window_attributes = window_attributes.with_accepts_first_mouse(val);
            }
            if let Some(val) = mac.option_as_alt {
                window_attributes = window_attributes.with_option_as_alt(val.into());
            }
            if let Some(title) = mac.tabbing_identifier {
                window_attributes = window_attributes.with_tabbing_identifier(title.as_str());
            }
            if let Some(disallow_hidpi) = mac.disallow_high_dpi {
                window_attributes = window_attributes.with_disallow_hidpi(disallow_hidpi);
            }
            if let Some(shadow) = mac.has_shadow {
                window_attributes = window_attributes.with_has_shadow(shadow);
            }
            if let Some(hide) = mac.titlebar_buttons_hidden {
                window_attributes = window_attributes.with_titlebar_buttons_hidden(hide)
            }
            if let Some(panel) = mac.panel {
                window_attributes = window_attributes.with_panel(panel)
            }
        }

        let Ok(window) = event_loop.create_window(window_attributes) else {
            return;
        };
        let window_id = window.id();
        let window_handle = WindowHandle::new(
            window,
            view_fn,
            transparent,
            apply_default_theme,
            font_embolden,
        );
        self.window_handles.insert(window_id, window_handle);
    }

    fn close_window(
        &mut self,
        window_id: WindowId,
        #[cfg(target_os = "macos")] _event_loop: &dyn ActiveEventLoop,
        #[cfg(not(target_os = "macos"))] event_loop: &dyn ActiveEventLoop,
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
        let ext_events = { std::mem::take(&mut *EXT_EVENT_HANDLER.queue.lock()) };

        for trigger in ext_events {
            trigger.notify();
        }

        self.handle_updates_for_all_windows();
    }

    pub(crate) fn handle_updates_for_all_windows(&mut self) {
        for (window_id, handle) in self.window_handles.iter_mut() {
            handle.process_update();
            while process_window_updates(window_id) {}
        }
    }

    fn request_timer(&mut self, timer: Timer, event_loop: &dyn ActiveEventLoop) {
        self.timers.insert(timer.token, timer);
        self.fire_timer(event_loop);
    }

    fn remove_timer(&mut self, timer: &TimerToken) {
        self.timers.remove(timer);
    }

    fn fire_timer(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.timers.is_empty() {
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
                if let Some(timer) = self.timers.remove(&token) {
                    (timer.action)(token);
                }
            }
            self.handle_updates_for_all_windows();
        }
        self.fire_timer(event_loop);
    }
}
