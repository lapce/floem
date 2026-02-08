use dpi::PhysicalPosition;
use floem_renderer::gpu_resources::GpuResources;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use ui_events_winit::WindowEventTranslation;
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
    window::{Theme, WindowId},
};

use super::{APP_UPDATE_EVENTS, AppConfig, AppEventCallback, AppUpdateEvent, UserEvent};
use crate::{
    AppEvent,
    action::{Timer, TimerToken},
    context::PaintState,
    event::dropped_file::FileDragEvent::{self, DragDropped},
    ext_event::EXT_EVENT_HANDLER,
    inspector::{
        Capture,
        profiler::{Profile, ProfileEvent},
    },
    view::View,
    window::WindowConfig,
    window::handle::WindowHandle,
    window::id::process_window_updates,
};

pub(crate) struct ApplicationHandle {
    window_handles: HashMap<winit::window::WindowId, WindowHandle>,
    timers: HashMap<TimerToken, Timer>,
    pub(crate) event_listener: Option<Box<AppEventCallback>>,
    pub(crate) gpu_resources: Option<GpuResources>,
    pub(crate) config: AppConfig,
}

impl ApplicationHandle {
    pub(crate) fn new(config: AppConfig) -> Self {
        Self {
            window_handles: HashMap::new(),
            timers: HashMap::new(),
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
                        renderer.scale(),
                        renderer.size(),
                        *font_embolden,
                    );
                    self.gpu_resources = Some(gpu_resources);
                    handle.paint_state = PaintState::Initialized { renderer };
                    handle.init_renderer(self.gpu_resources.clone());
                } else {
                    panic!("Sent a gpu resource update after it had already been initialized");
                }
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
                    self.config.global_theme_override,
                    window_creation.config.unwrap_or_default(),
                ),
                AppUpdateEvent::CloseWindow { window_id } => {
                    self.close_window(window_id, event_loop);
                }
                AppUpdateEvent::RequestTimer { timer } => {
                    self.request_timer(timer, event_loop);
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

        match window_handle
            .event_reducer
            .reduce(window_handle.scale, &event)
        {
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
            WindowEvent::DragDropped { paths, position } => {
                window_handle.file_drag_event(DragDropped {
                    paths,
                    position: PhysicalPosition::new(position.x, position.y),
                    scale_factor: window_handle.scale,
                });
            }
            WindowEvent::DragEntered { paths, position } => {
                window_handle.file_drag_event(FileDragEvent::DragEntered {
                    paths,
                    position: PhysicalPosition::new(position.x, position.y),
                    scale_factor: window_handle.scale,
                });
            }
            WindowEvent::DragMoved { position } => {
                window_handle.file_drag_event(FileDragEvent::DragMoved {
                    position: PhysicalPosition::new(position.x, position.y),
                    scale_factor: window_handle.scale,
                });
            }
            WindowEvent::DragLeft { position } => {
                let pos = position.map(|p| PhysicalPosition::new(p.x, p.y));
                window_handle.file_drag_event(FileDragEvent::DragLeft {
                    position: pos,
                    scale_factor: window_handle.scale,
                });
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
                window_handle.scale(scale_factor);
            }
            WindowEvent::ThemeChanged(theme) => {
                window_handle.set_theme(Some(theme), true);
            }
            WindowEvent::Occluded(_) => {}
            WindowEvent::RedrawRequested => {
                window_handle.render_frame(self.gpu_resources.clone());
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
        self.handle_updates_for_all_windows();
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
            .map(|handle| handle.capture(self.gpu_resources.clone()))
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
                if let Some(timer) = self.timers.remove(&token) {
                    (timer.action)(token);
                }
            }
            self.handle_updates_for_all_windows();
        }
        self.fire_timer(event_loop);
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
