use std::collections::HashMap;
use std::{cell::RefCell, mem, path::PathBuf, rc::Rc, sync::Arc};

use muda::MenuId;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};
use winit::window::{
    ImeCapabilities, ImeEnableRequest, ImeHint, ImePurpose, ImeRequest, ImeRequestData,
};

use floem_reactive::{RwSignal, Scope, SignalGet, SignalUpdate, with_scope};
use floem_renderer::Renderer;
use floem_renderer::gpu_resources::GpuResources;
use peniko::color::palette;
use peniko::kurbo::{Affine, Point, Size, Vec2};
use winit::{
    cursor::CursorIcon,
    dpi::{LogicalPosition, LogicalSize},
    event::{ButtonSource, ElementState, Ime, MouseScrollDelta, TouchPhase},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use crate::menu::MudaMenu;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use crate::reactive::SignalWith;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use crate::unit::UnitExt;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use crate::views::{Decorators, container, stack};
use crate::{
    Application,
    app::UserEvent,
    context::{
        ComputeLayoutCx, EventCx, FrameUpdate, LayoutCx, PaintCx, PaintState, StyleCx, UpdateCx,
    },
    dropped_file::DroppedFilesEvent,
    event::{Event, EventListener},
    id::ViewId,
    inspector::{self, Capture, CaptureState, CapturedView},
    keyboard::{KeyEvent, Modifiers},
    nav::view_arrow_navigation,
    pointer::{PointerButton, PointerInputEvent, PointerMoveEvent, PointerWheelEvent},
    profiler::Profile,
    style::{CursorStyle, Style, StyleSelector},
    theme::default_theme,
    touchpad::PinchGestureEvent,
    update::{
        CENTRAL_DEFERRED_UPDATE_MESSAGES, CENTRAL_UPDATE_MESSAGES, CURRENT_RUNNING_VIEW_HANDLE,
        DEFERRED_UPDATE_MESSAGES, UPDATE_MESSAGES, UpdateMessage,
    },
    view::{IntoView, View, view_tab_navigation},
    view_state::ChangeFlags,
    window_state::WindowState,
    window_tracking::{remove_window_id_mapping, store_window_id_mapping},
};

/// The top-level window handle that owns the winit `Window`.
/// Meant only for use with the root view of the application.
/// Owns the [`WindowState`] and is responsible for
/// - processing all requests to update the [`WindowState`] from the reactive system
/// - processing all requests to update the animation state from the reactive system
/// - requesting a new animation frame from the backend
pub(crate) struct WindowHandle {
    pub(crate) window: Arc<dyn winit::window::Window>,
    window_id: WindowId,
    id: ViewId,
    main_view: ViewId,
    /// Reactive Scope for this `WindowHandle`
    scope: Scope,
    pub(crate) window_state: WindowState,
    pub(crate) paint_state: PaintState,
    size: RwSignal<Size>,
    theme: Option<Style>,
    pub(crate) profile: Option<Profile>,
    os_theme: Option<winit::window::Theme>,
    pub(crate) current_theme: winit::window::Theme,
    is_maximized: bool,
    transparent: bool,
    pub(crate) scale: f64,
    pub(crate) modifiers: Modifiers,
    pub(crate) cursor_position: Point,
    pub(crate) window_position: Point,
    pub(crate) last_pointer_down: Option<(u8, Point, Instant)>,
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub(crate) context_menu: RwSignal<Option<(muda::Menu, Point, bool)>>,
    pub(crate) window_menu_actions: HashMap<MenuId, Box<dyn Fn()>>,
    pub(crate) window_menu: Option<muda::Menu>,
    dropper_files: Option<Vec<PathBuf>>,
}

impl WindowHandle {
    pub(crate) fn new(
        window: Box<dyn winit::window::Window>,
        gpu_resources: Option<GpuResources>,
        required_features: wgpu::Features,
        view_fn: impl FnOnce(winit::window::WindowId) -> Box<dyn View> + 'static,
        transparent: bool,
        apply_theme: Option<winit::window::Theme>,
        font_embolden: f32,
    ) -> Self {
        let scope = Scope::new();
        let window_id = window.id();
        let id = ViewId::new();
        let scale = window.scale_factor();
        let size: LogicalSize<f64> = window.surface_size().to_logical(scale);
        let size = Size::new(size.width, size.height);
        let size = scope.create_rw_signal(Size::new(size.width, size.height));
        let os_theme = window.theme();
        let current_theme = apply_theme.unwrap_or(os_theme.unwrap_or(winit::window::Theme::Light));
        let is_maximized = window.is_maximized();

        set_current_view(id);

        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let context_menu = scope.create_rw_signal(None);

        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
        let view = with_scope(scope, move || {
            let main_view = view_fn(window_id);
            let main_view_id = main_view.id();
            (main_view_id, main_view)
        });

        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let view = with_scope(scope, move || {
            let main_view = view_fn(window_id);
            let main_view_id = main_view.id();
            (
                main_view_id,
                stack((
                    container(main_view).style(|s| s.size(100.pct(), 100.pct())),
                    context_menu_view(scope, context_menu, size),
                ))
                .style(|s| s.size(100.pct(), 100.pct()))
                .into_any(),
            )
        });

        let (main_view_id, widget) = view;
        id.set_children([widget]);

        let view = WindowView { id };
        id.set_view(view.into_any());

        let window: Arc<dyn Window> = window.into();
        store_window_id_mapping(id, window_id, &window);

        let paint_state = if let Some(resources) = gpu_resources.clone() {
            let surface = resources
                .instance
                .create_surface(Arc::clone(&window))
                .expect("can create second window");
            PaintState::new(
                window.clone(),
                surface,
                resources,
                scale,
                size.get_untracked() * scale,
                font_embolden,
            )
        } else {
            let gpu_resources_rx = GpuResources::request(
                move |window_id| {
                    Application::send_proxy_event(UserEvent::GpuResourcesUpdate { window_id });
                },
                required_features,
                window.clone(),
            );
            PaintState::new_pending(
                window.clone(),
                gpu_resources_rx,
                scale,
                size.get_untracked() * scale,
                font_embolden,
            )
        };

        let paint_state_initialized = matches!(paint_state, PaintState::Initialized { .. });

        let mut window_handle = Self {
            window,
            window_id,
            id,
            main_view: main_view_id,
            scope,
            window_state: WindowState::new(id),
            paint_state,
            size,
            theme: match apply_theme {
                Some(t) => Some(default_theme(t)),
                None => Some(default_theme(
                    os_theme.unwrap_or(winit::window::Theme::Light),
                )),
            },
            os_theme,
            current_theme,
            is_maximized,
            transparent,
            profile: None,
            scale,
            modifiers: Modifiers::default(),
            cursor_position: Point::ZERO,
            window_position: Point::ZERO,
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            context_menu,
            window_menu_actions: HashMap::new(),
            window_menu: None,
            last_pointer_down: None,
            dropper_files: None,
        };
        if paint_state_initialized {
            window_handle.init_renderer(gpu_resources);
        }
        window_handle
            .window_state
            .set_root_size(size.get_untracked());
        window_handle.window_state.os_theme = os_theme;
        if let Some(theme) = apply_theme {
            window_handle.event(Event::ThemeChanged(theme));
        }
        window_handle.size(size.get_untracked());
        window_handle
    }

    pub(crate) fn init_renderer(&mut self, gpu_resources: Option<GpuResources>) {
        // On the web, we need to get the canvas size once. The size will be updated automatically
        // when the canvas element is resized subsequently. This is the correct place to do so
        // because the renderer is not initialized until now.
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWeb;

            let rect = self
                .window
                .as_ref()
                .unwrap()
                .canvas()
                .unwrap()
                .get_bounding_client_rect();
            // let rect = canvas.get_bounding_client_rect();
            let size = LogicalSize::new(rect.width(), rect.height());
            self.size(Size::new(size.width, size.height));
        }
        // Now that the renderer is initialized, draw the first frame
        self.render_frame(gpu_resources);
        self.window.set_visible(true);
    }

    pub fn event(&mut self, event: Event) {
        set_current_view(self.id);
        let event = event.transform(Affine::scale(self.window_state.scale));

        let mut cx = EventCx {
            window_state: &mut self.window_state,
        };

        let is_pointer_move = if let Event::PointerMove(pme) = &event {
            cx.window_state.last_cursor_location = pme.pos;
            true
        } else {
            false
        };
        let (was_hovered, was_dragging_over) = if is_pointer_move {
            cx.window_state.cursor = None;
            let was_hovered = std::mem::take(&mut cx.window_state.hovered);
            let was_dragging_over = std::mem::take(&mut cx.window_state.dragging_over);

            (Some(was_hovered), Some(was_dragging_over))
        } else {
            (None, None)
        };

        let is_pointer_down = matches!(&event, Event::PointerDown(_));
        let was_focused = if is_pointer_down {
            cx.window_state.clicking.clear();
            cx.window_state.focus.take()
        } else {
            cx.window_state.focus
        };

        if event.needs_focus() {
            let mut processed = false;

            if !processed {
                if let Some(id) = cx.window_state.focus {
                    processed |= cx
                        .unconditional_view_event(id, event.clone(), true)
                        .0
                        .is_processed();
                }

                if !processed {
                    if let Some(listener) = event.listener() {
                        processed |= self
                            .main_view
                            .apply_event(&listener, &event)
                            .is_some_and(|prop| prop.is_processed());
                    }
                }

                if !processed {
                    if let Event::KeyDown(KeyEvent { key, modifiers }) = &event {
                        if key.logical_key == Key::Named(NamedKey::Tab)
                            && (modifiers.is_empty() || *modifiers == Modifiers::SHIFT)
                        {
                            let backwards = modifiers.contains(Modifiers::SHIFT);
                            view_tab_navigation(self.id, cx.window_state, backwards);
                            // view_debug_tree(&self.view);
                        } else if let Key::Character(character) = &key.logical_key {
                            // 'I' displays some debug information
                            if character.eq_ignore_ascii_case("i") {
                                // view_debug_tree(&self.view);
                            }
                        } else if *modifiers == Modifiers::ALT {
                            if let Key::Named(
                                name @ (NamedKey::ArrowUp
                                | NamedKey::ArrowDown
                                | NamedKey::ArrowLeft
                                | NamedKey::ArrowRight),
                            ) = key.logical_key
                            {
                                view_arrow_navigation(name, cx.window_state, self.id);
                            }
                        }
                    }

                    let keyboard_trigger_end = cx.window_state.keyboard_navigation
                        && event.is_keyboard_trigger()
                        && matches!(event, Event::KeyUp(_));
                    if keyboard_trigger_end {
                        if let Some(id) = cx.window_state.active {
                            // To remove the styles applied by the Active selector
                            if cx.window_state.has_style_for_sel(id, StyleSelector::Active) {
                                id.request_style_recursive();
                            }

                            cx.window_state.active = None;
                        }
                    }
                }
            }
        } else if cx.window_state.active.is_some() && event.is_pointer() {
            if cx.window_state.is_dragging() {
                cx.unconditional_view_event(self.id, event.clone(), false);
            }

            let id = cx.window_state.active.unwrap();

            {
                let window_origin = id.state().borrow().window_origin;
                let layout = id.get_layout().unwrap_or_default();
                let viewport = id.state().borrow().viewport.unwrap_or_default();
                let transform = Affine::translate((
                    window_origin.x - layout.location.x as f64 + viewport.x0,
                    window_origin.y - layout.location.y as f64 + viewport.y0,
                ));
                cx.unconditional_view_event(id, event.clone().transform(transform), true);
            }

            if let Event::PointerUp(_) = &event {
                // To remove the styles applied by the Active selector
                if cx.window_state.has_style_for_sel(id, StyleSelector::Active) {
                    id.request_style_recursive();
                }

                cx.window_state.active = None;
            }
        } else {
            cx.unconditional_view_event(self.id, event.clone(), false);
        }

        if let Event::PointerUp(_) = &event {
            cx.window_state.drag_start = None;
        }
        if is_pointer_move {
            let hovered = &cx.window_state.hovered.clone();
            for id in was_hovered.unwrap().symmetric_difference(hovered) {
                let view_state = id.state();
                if view_state.borrow().has_active_animation()
                    || view_state
                        .borrow()
                        .has_style_selectors
                        .has(StyleSelector::Hover)
                    || view_state
                        .borrow()
                        .has_style_selectors
                        .has(StyleSelector::Active)
                {
                    id.request_style();
                }
                if hovered.contains(id) {
                    id.apply_event(&EventListener::PointerEnter, &event);
                } else {
                    cx.unconditional_view_event(*id, Event::PointerLeave, true);
                }
            }
            let dragging_over = &cx.window_state.dragging_over.clone();
            for id in was_dragging_over
                .unwrap()
                .symmetric_difference(dragging_over)
            {
                if dragging_over.contains(id) {
                    id.apply_event(&EventListener::DragEnter, &event);
                } else {
                    id.apply_event(&EventListener::DragLeave, &event);
                }
            }
        }
        if was_focused != cx.window_state.focus {
            cx.window_state
                .focus_changed(was_focused, cx.window_state.focus);
        }

        if is_pointer_down {
            for id in cx.window_state.clicking.clone() {
                if cx.window_state.has_style_for_sel(id, StyleSelector::Active) {
                    id.request_style_recursive();
                }
            }

            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            if self.context_menu.with_untracked(|c| {
                c.as_ref()
                    .map(|(_, _, had_pointer_down)| !*had_pointer_down)
                    .unwrap_or(false)
            }) {
                // we had a pointer down event
                // if context menu is still shown
                // we should hide it
                self.context_menu.set(None);
            }
        }
        if matches!(&event, Event::PointerUp(_)) {
            for id in cx.window_state.clicking.clone() {
                if cx.window_state.has_style_for_sel(id, StyleSelector::Active) {
                    id.request_style_recursive();
                }
            }
            cx.window_state.clicking.clear();

            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            if self.context_menu.with_untracked(|c| c.is_some()) {
                // we had a pointer up event
                // if context menu is still shown
                // we should hide it
                self.context_menu.set(None);
            }
        }

        self.process_update();
    }

    pub(crate) fn scale(&mut self, scale: f64) {
        self.scale = scale;
        let scale = self.scale * self.window_state.scale;
        self.paint_state.set_scale(scale);
        self.event(Event::WindowScaleChanged(scale));
        self.schedule_repaint();
    }

    pub(crate) fn theme_changed(&mut self, theme: winit::window::Theme) {
        self.os_theme = Some(theme);
        self.window_state.os_theme = Some(theme);
        self.current_theme = theme;
        self.theme = Some(default_theme(theme));
        self.window.set_theme(Some(theme));
        self.id.request_all();
        request_recursive_changes(self.id, ChangeFlags::STYLE);
        self.event(Event::ThemeChanged(theme));
    }

    pub(crate) fn size(&mut self, size: Size) {
        self.size.set(size);
        self.window_state.update_screen_size_bp(size);
        self.event(Event::WindowResized(size));
        let scale = self.scale * self.window_state.scale;
        self.paint_state.resize(scale, size * self.scale);
        self.window_state.set_root_size(size);

        let is_maximized = self.window.is_maximized();
        if is_maximized != self.is_maximized {
            self.is_maximized = is_maximized;
            self.event(Event::WindowMaximizeChanged(is_maximized));
        }

        self.style();
        self.layout();
        self.process_update();
        self.schedule_repaint();
    }

    pub(crate) fn position(&mut self, point: Point) {
        self.window_position = point;
        self.event(Event::WindowMoved(point));
    }

    pub(crate) fn key_event(&mut self, key_event: winit::event::KeyEvent) {
        let event = KeyEvent {
            key: key_event,
            modifiers: self.modifiers,
        };
        let is_altgr = matches!(event.key.logical_key, Key::Named(NamedKey::AltGraph));
        if event.key.state.is_pressed() {
            self.event(Event::KeyDown(event));
            if is_altgr {
                self.modifiers.set(Modifiers::ALTGR, true);
            }
        } else {
            self.event(Event::KeyUp(event));
            if is_altgr {
                self.modifiers.set(Modifiers::ALTGR, false);
            }
        }
    }

    pub(crate) fn dropped_files(&mut self, paths: Vec<PathBuf>) {
        self.dropper_files = Some(paths);
    }

    pub(crate) fn pointer_move(&mut self, pos: Point) {
        if let Some(path) = self.dropper_files.take() {
            self.event(Event::DroppedFiles(DroppedFilesEvent { path, pos }));
        }
        if self.cursor_position != pos {
            self.cursor_position = pos;
            let event = PointerMoveEvent {
                pos,
                modifiers: self.modifiers,
            };
            self.event(Event::PointerMove(event));
        }
    }

    pub(crate) fn pointer_leave(&mut self) {
        set_current_view(self.id);
        let mut cx = EventCx {
            window_state: &mut self.window_state,
        };
        let was_hovered = std::mem::take(&mut cx.window_state.hovered);
        for id in was_hovered {
            let view_state = id.state();
            if view_state
                .borrow()
                .has_style_selectors
                .has(StyleSelector::Hover)
                || view_state
                    .borrow()
                    .has_style_selectors
                    .has(StyleSelector::Active)
                || view_state.borrow().has_active_animation()
            {
                id.request_style();
            }
            cx.unconditional_view_event(id, Event::PointerLeave, true);
        }
        self.process_update();
    }

    pub(crate) fn mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let delta = match delta {
            MouseScrollDelta::LineDelta(x, y) => Vec2::new(-x as f64 * 60.0, -y as f64 * 60.0),
            MouseScrollDelta::PixelDelta(delta) => {
                let position: LogicalPosition<f64> = delta.to_logical(self.scale);
                Vec2::new(-position.x, -position.y)
            }
        };
        let event = PointerWheelEvent {
            pos: self.cursor_position,
            delta,
            modifiers: self.modifiers,
        };
        self.event(Event::PointerWheel(event));
    }

    pub(crate) fn pointer_button(&mut self, button: ButtonSource, state: ElementState) {
        let button: PointerButton = button.into();
        let count = if state.is_pressed() && button.is_primary() {
            if let Some((count, last_pos, instant)) = self.last_pointer_down.as_mut() {
                if *count == 4 {
                    *count = 1;
                } else if instant.elapsed().as_millis() < 500
                    && last_pos.distance(self.cursor_position) < 4.0
                {
                    *count += 1;
                } else {
                    *count = 1;
                }
                *instant = Instant::now();
                *last_pos = self.cursor_position;
                *count
            } else {
                self.last_pointer_down = Some((1, self.cursor_position, Instant::now()));
                1
            }
        } else {
            0
        };
        let event = PointerInputEvent {
            pos: self.cursor_position,
            button,
            modifiers: self.modifiers,
            count,
        };
        match state {
            ElementState::Pressed => {
                self.event(Event::PointerDown(event));
            }
            ElementState::Released => {
                self.event(Event::PointerUp(event));
            }
        }
    }

    pub(crate) fn pinch_gesture(&mut self, delta: f64, phase: TouchPhase) {
        let event = PinchGestureEvent { delta, phase };
        self.event(Event::PinchGesture(event));
    }

    pub(crate) fn focused(&mut self, focused: bool) {
        if focused {
            #[cfg(target_os = "macos")]
            if let Some(window_menu) = &self.window_menu {
                window_menu.init_for_nsapp();
            }
            self.event(Event::WindowGotFocus);
        } else {
            self.event(Event::WindowLostFocus);
        }
    }

    fn style(&mut self) {
        let mut cx = StyleCx::new(&mut self.window_state, self.id);
        if let Some(theme) = &self.theme {
            cx.current = Rc::new(theme.inherited());
        }
        cx.style_view(self.id);
    }

    fn layout(&mut self) -> Duration {
        let mut cx = LayoutCx::new(&mut self.window_state);

        cx.window_state.root = {
            let view = self.id.view();
            let mut view = view.borrow_mut();
            Some(cx.layout_view(view.as_mut()))
        };

        let start = Instant::now();
        cx.window_state.compute_layout();
        let taffy_duration = Instant::now().saturating_duration_since(start);

        self.compute_layout();

        taffy_duration
    }

    fn compute_layout(&mut self) {
        self.window_state.request_compute_layout = false;
        let viewport = (self.window_state.root_size / self.window_state.scale).to_rect();
        let mut cx = ComputeLayoutCx::new(&mut self.window_state, viewport);
        cx.compute_view_layout(self.id);
    }

    pub(crate) fn render_frame(&mut self, gpu_resources: Option<GpuResources>) {
        // Processes updates scheduled on this frame.
        for update in mem::take(&mut self.window_state.scheduled_updates) {
            match update {
                FrameUpdate::Style(id) => id.request_style(),
                FrameUpdate::Layout(id) => id.request_layout(),
                FrameUpdate::Paint(id) => self.window_state.request_paint(id),
            }
        }

        self.process_update_no_paint();
        self.paint(gpu_resources);

        // Request a new frame if there's any scheduled updates.
        if !self.window_state.scheduled_updates.is_empty() {
            self.schedule_repaint();
        }
    }

    pub fn paint(&mut self, gpu_resources: Option<GpuResources>) -> Option<peniko::ImageBrush> {
        let mut cx = PaintCx {
            window_state: &mut self.window_state,
            paint_state: &mut self.paint_state,
            transform: Affine::IDENTITY,
            clip: None,
            z_index: None,
            saved_transforms: Vec::new(),
            saved_clips: Vec::new(),
            saved_z_indexes: Vec::new(),
            gpu_resources,
            window: self.window.clone(),
            #[cfg(feature = "vello")]
            saved_layer_counts: Vec::new(),
            #[cfg(feature = "vello")]
            layer_count: 0,
        };
        cx.paint_state
            .renderer_mut()
            .begin(cx.window_state.capture.is_some());
        if !self.transparent {
            let scale = cx.window_state.scale;
            let color = self
                .theme
                .as_ref()
                .and_then(|theme| theme.get(crate::style::Background))
                .unwrap_or(peniko::Brush::Solid(palette::css::WHITE));
            // fill window with default white background if it's not transparent
            cx.fill(
                &self
                    .size
                    .get_untracked()
                    .to_rect()
                    .scale_from_origin(1.0 / scale)
                    .expand(),
                &color,
                0.0,
            );
        }
        cx.paint_view(self.id);
        if cx.window_state.capture.is_none() {
            self.window.pre_present_notify();
        }
        cx.paint_state.renderer_mut().finish()
    }

    pub(crate) fn capture(&mut self, gpu_resources: Option<GpuResources>) -> Capture {
        // Capture the view before we run `style` and `layout` to catch missing `request_style`` or
        // `request_layout` flags.
        let root_layout = self.id.layout_rect();
        let root = CapturedView::capture(self.id, &mut self.window_state, root_layout);

        self.window_state.capture = Some(CaptureState::default());

        // Trigger painting to create a Vger renderer which can capture the output.
        // This can be expensive so it could skew the paint time measurement.
        self.paint(gpu_resources.clone());

        // Ensure we run layout and styling again for accurate timing. We also need to ensure
        // styles are recomputed to capture them.
        fn request_changes(id: ViewId) {
            id.state().borrow_mut().requested_changes = ChangeFlags::all();
            for child in id.children() {
                request_changes(child);
            }
        }
        request_changes(self.id);

        fn get_taffy_depth(
            taffy: Rc<RefCell<taffy::TaffyTree>>,
            root: taffy::tree::NodeId,
        ) -> usize {
            let children = taffy.borrow().children(root).unwrap();
            if children.is_empty() {
                1
            } else {
                children
                    .iter()
                    .map(|child| get_taffy_depth(taffy.clone(), *child))
                    .max()
                    .unwrap()
                    + 1
            }
        }

        let start = Instant::now();
        self.style();
        let post_style = Instant::now();

        let taffy_root_node = self.id.state().borrow().node;
        let taffy_duration = self.layout();
        let post_layout = Instant::now();
        let window = self.paint(gpu_resources);
        let end = Instant::now();

        let capture = Capture {
            start,
            post_style,
            post_layout,
            end,
            taffy_duration,
            taffy_node_count: self.id.taffy().borrow().total_node_count(),
            taffy_depth: get_taffy_depth(self.id.taffy(), taffy_root_node),
            window,
            window_size: self.size.get_untracked() / self.window_state.scale,
            scale: self.scale * self.window_state.scale,
            root: Rc::new(root),
            state: self.window_state.capture.take().unwrap(),
            renderer: self.paint_state.renderer().debug_info(),
        };
        // Process any updates produced by capturing
        self.process_update();

        capture
    }

    pub(crate) fn process_update(&mut self) {
        if self.process_update_no_paint() {
            self.schedule_repaint();
        }
    }

    /// Processes updates and runs style and layout if needed.
    /// Returns `true` if painting is required.
    pub(crate) fn process_update_no_paint(&mut self) -> bool {
        let mut paint = false;

        loop {
            loop {
                self.process_update_messages();
                if !self.needs_layout()
                    && !self.needs_style()
                    && !self.window_state.request_compute_layout
                {
                    break;
                }

                if self.needs_style() {
                    paint = true;
                    self.style();
                }

                if self.needs_layout() {
                    paint = true;
                    self.layout();
                }

                if self.window_state.request_compute_layout {
                    self.compute_layout();
                }
            }
            if !self.has_deferred_update_messages() {
                break;
            }
            self.process_deferred_update_messages();
        }

        self.set_cursor();

        // TODO: This should only use `self.window_state.request_paint)`
        paint || mem::take(&mut self.window_state.request_paint)
    }

    fn process_central_messages(&self) {
        CENTRAL_UPDATE_MESSAGES.with_borrow_mut(|central_msgs| {
            if !central_msgs.is_empty() {
                UPDATE_MESSAGES.with_borrow_mut(|msgs| {
                    // We need to retain any messages which are for a view that either belongs
                    // to a different window, or which does not yet have a root
                    let removed_central_msgs =
                        std::mem::replace(central_msgs, Vec::with_capacity(central_msgs.len()));
                    for (id, msg) in removed_central_msgs {
                        if let Some(root) = id.root() {
                            let msgs = msgs.entry(root).or_default();
                            msgs.push(msg);
                        } else {
                            // Messages that are not for our root get put back - they may
                            // belong to another window, or may be construction-time messages
                            // for a View that does not yet have a window but will momentarily.
                            //
                            // Note that if there is a plethora of events for ids which were created
                            // but never assigned to any view, they will probably pile up in here,
                            // and if that becomes a real problem, we may want a garbage collection
                            // mechanism, or give every message a max-touch-count and discard it
                            // if it survives too many iterations through here. Unclear if there
                            // are real-world app development patterns where that could actually be
                            // an issue. Since any such mechanism would have some overhead, there
                            // should be a proven need before building one.
                            central_msgs.push((id, msg));
                        }
                    }
                });
            }
        });

        CENTRAL_DEFERRED_UPDATE_MESSAGES.with(|central_msgs| {
            if !central_msgs.borrow().is_empty() {
                DEFERRED_UPDATE_MESSAGES.with(|msgs| {
                    let mut msgs = msgs.borrow_mut();
                    let removed_central_msgs = std::mem::replace(
                        &mut *central_msgs.borrow_mut(),
                        Vec::with_capacity(msgs.len()),
                    );
                    let unprocessed = &mut *central_msgs.borrow_mut();
                    for (id, msg) in removed_central_msgs {
                        if let Some(root) = id.root() {
                            let msgs = msgs.entry(root).or_default();
                            msgs.push((id, msg));
                        } else {
                            unprocessed.push((id, msg));
                        }
                    }
                });
            }
        });
    }

    fn process_update_messages(&mut self) {
        loop {
            self.process_central_messages();
            let msgs =
                UPDATE_MESSAGES.with(|msgs| msgs.borrow_mut().remove(&self.id).unwrap_or_default());
            if msgs.is_empty() {
                break;
            }
            for msg in msgs {
                let mut cx = UpdateCx {
                    window_state: &mut self.window_state,
                };
                match msg {
                    UpdateMessage::RequestPaint => {
                        cx.window_state.request_paint = true;
                    }
                    UpdateMessage::Focus(id) => {
                        if cx.window_state.focus != Some(id) {
                            let old = cx.window_state.focus;
                            cx.window_state.focus = Some(id);
                            cx.window_state.focus_changed(old, cx.window_state.focus);
                        }
                    }
                    UpdateMessage::ClearFocus(id) => {
                        if cx.window_state.focus == Some(id) {
                            cx.window_state.clear_focus();
                            cx.window_state.focus_changed(Some(id), None);
                        }
                    }
                    UpdateMessage::ClearAppFocus => {
                        let focus = cx.window_state.focus;
                        cx.window_state.clear_focus();
                        if let Some(id) = focus {
                            cx.window_state.focus_changed(Some(id), None);
                        }
                    }
                    UpdateMessage::Active(id) => {
                        let old = cx.window_state.active;
                        cx.window_state.active = Some(id);

                        if let Some(old_id) = old {
                            // To remove the styles applied by the Active selector
                            if cx
                                .window_state
                                .has_style_for_sel(old_id, StyleSelector::Active)
                            {
                                old_id.request_style_recursive();
                            }
                        }

                        if cx.window_state.has_style_for_sel(id, StyleSelector::Active) {
                            id.request_style_recursive();
                        }
                    }
                    UpdateMessage::ClearActive(id) => {
                        if Some(id) == cx.window_state.active {
                            cx.window_state.active = None;
                        }
                    }
                    UpdateMessage::ScrollTo { id, rect } => {
                        self.id
                            .view()
                            .borrow_mut()
                            .scroll_to(cx.window_state, id, rect);
                    }
                    UpdateMessage::State { id, state } => {
                        let view = id.view();
                        view.borrow_mut().update(&mut cx, state);
                    }
                    UpdateMessage::DragWindow => {
                        let _ = self.window.drag_window();
                    }
                    UpdateMessage::FocusWindow => {
                        self.window.focus_window();
                    }
                    UpdateMessage::DragResizeWindow(direction) => {
                        let _ = self.window.drag_resize_window(direction);
                    }
                    UpdateMessage::ToggleWindowMaximized => {
                        self.window.set_maximized(!self.window.is_maximized());
                    }
                    UpdateMessage::SetWindowMaximized(maximized) => {
                        self.window.set_maximized(maximized);
                    }
                    UpdateMessage::MinimizeWindow => {
                        self.window.set_minimized(true);
                    }
                    UpdateMessage::SetWindowDelta(delta) => {
                        let pos = self.window_position + delta;
                        self.window
                            .set_outer_position(winit::dpi::Position::Logical(
                                winit::dpi::LogicalPosition::new(pos.x, pos.y),
                            ));
                    }
                    UpdateMessage::WindowScale(scale) => {
                        cx.window_state.scale = scale;
                        self.id.request_layout();
                        let scale = self.scale * cx.window_state.scale;
                        self.paint_state.set_scale(scale);
                    }
                    UpdateMessage::ShowContextMenu { menu, pos } => {
                        let (menu, registry) = menu.build();
                        cx.window_state.context_menu.clear();
                        cx.window_state.update_context_menu(registry);

                        #[cfg(any(target_os = "windows", target_os = "macos"))]
                        {
                            self.show_context_menu(menu, pos);
                        }
                        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                        self.show_context_menu(menu, pos);
                    }
                    UpdateMessage::WindowMenu { menu } => {
                        self.window_menu_actions.clear();
                        let (menu, registry) = menu.build();
                        self.window_menu_actions = registry;
                        #[cfg(target_os = "macos")]
                        {
                            menu.init_for_nsapp();
                        }
                        #[cfg(target_os = "windows")]
                        {
                            self.init_menu_for_windows(&menu);
                        }
                        self.window_menu = Some(menu);
                    }
                    UpdateMessage::SetWindowTitle { title } => {
                        self.window.set_title(&title);
                    }
                    UpdateMessage::SetImeAllowed { allowed } => {
                        if self.window.ime_capabilities().is_some() != allowed {
                            let ime = if allowed {
                                let position = LogicalPosition::new(0, 0);
                                let size = LogicalSize::new(0, 0);
                                let request_data = ImeRequestData::default()
                                    .with_cursor_area(position.into(), size.into())
                                    .with_hint_and_purpose(ImeHint::NONE, ImePurpose::Normal);

                                ImeRequest::Enable(
                                    ImeEnableRequest::new(
                                        ImeCapabilities::new()
                                            .with_hint_and_purpose()
                                            .with_cursor_area(),
                                        request_data,
                                    )
                                    .unwrap(),
                                )
                            } else {
                                ImeRequest::Disable
                            };

                            self.window.request_ime_update(ime).unwrap();
                        }
                    }
                    UpdateMessage::SetImeCursorArea { position, size } => {
                        if self
                            .window
                            .ime_capabilities()
                            .map(|caps| caps.cursor_area())
                            .unwrap_or(false)
                        {
                            let position =
                                winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(
                                    position.x * self.window_state.scale,
                                    position.y * self.window_state.scale,
                                ));
                            let size = winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
                                size.width * self.window_state.scale,
                                size.height * self.window_state.scale,
                            ));
                            self.window
                                .request_ime_update(ImeRequest::Update(
                                    ImeRequestData::default().with_cursor_area(position, size),
                                ))
                                .unwrap();
                        }
                    }
                    UpdateMessage::Inspect => {
                        inspector::capture(self.window_id);
                    }
                    UpdateMessage::AddOverlay { view } => {
                        self.id.add_child(view);
                        self.id.request_all();
                    }
                    UpdateMessage::RemoveOverlay { id } => {
                        cx.window_state.remove_view(id);
                        self.id.request_all();
                    }
                    UpdateMessage::WindowVisible(visible) => {
                        self.window.set_visible(visible);
                    }
                    UpdateMessage::ViewTransitionAnimComplete(id) => {
                        let num_waiting =
                            id.state().borrow().num_waiting_animations.saturating_sub(1);
                        id.state().borrow_mut().num_waiting_animations = num_waiting;
                    }
                    UpdateMessage::ToggleTheme => {
                        use winit::window::Theme;

                        let new = match self.current_theme {
                            Theme::Light => Theme::Dark,
                            Theme::Dark => Theme::Light,
                        };
                        self.theme_changed(new);

                        #[cfg(target_os = "windows")]
                        {
                            self.set_menu_theme_for_windows(new);
                        }
                    }
                }
            }
        }
    }

    fn process_deferred_update_messages(&mut self) {
        self.process_central_messages();
        let msgs = DEFERRED_UPDATE_MESSAGES
            .with(|msgs| msgs.borrow_mut().remove(&self.id).unwrap_or_default());
        let mut cx = UpdateCx {
            window_state: &mut self.window_state,
        };
        for (id, state) in msgs {
            let view = id.view();
            view.borrow_mut().update(&mut cx, state);
        }
    }

    fn needs_layout(&mut self) -> bool {
        self.id
            .state()
            .borrow()
            .requested_changes
            .contains(ChangeFlags::LAYOUT)
    }

    fn needs_style(&mut self) -> bool {
        self.id
            .state()
            .borrow()
            .requested_changes
            .contains(ChangeFlags::STYLE)
    }

    fn has_deferred_update_messages(&self) -> bool {
        DEFERRED_UPDATE_MESSAGES.with(|m| {
            m.borrow()
                .get(&self.id)
                .map(|m| !m.is_empty())
                .unwrap_or(false)
        })
    }

    fn set_cursor(&mut self) {
        let cursor = match self.window_state.cursor {
            Some(CursorStyle::Default) => CursorIcon::Default,
            Some(CursorStyle::Pointer) => CursorIcon::Pointer,
            Some(CursorStyle::Progress) => CursorIcon::Progress,
            Some(CursorStyle::Wait) => CursorIcon::Wait,
            Some(CursorStyle::Crosshair) => CursorIcon::Crosshair,
            Some(CursorStyle::Text) => CursorIcon::Text,
            Some(CursorStyle::Move) => CursorIcon::Move,
            Some(CursorStyle::Grab) => CursorIcon::Grab,
            Some(CursorStyle::Grabbing) => CursorIcon::Grabbing,
            Some(CursorStyle::ColResize) => CursorIcon::ColResize,
            Some(CursorStyle::RowResize) => CursorIcon::RowResize,
            Some(CursorStyle::WResize) => CursorIcon::WResize,
            Some(CursorStyle::EResize) => CursorIcon::EResize,
            Some(CursorStyle::NwResize) => CursorIcon::NwResize,
            Some(CursorStyle::NeResize) => CursorIcon::NeResize,
            Some(CursorStyle::SwResize) => CursorIcon::SwResize,
            Some(CursorStyle::SeResize) => CursorIcon::SeResize,
            Some(CursorStyle::SResize) => CursorIcon::SResize,
            Some(CursorStyle::NResize) => CursorIcon::NResize,
            Some(CursorStyle::NeswResize) => CursorIcon::NeswResize,
            Some(CursorStyle::NwseResize) => CursorIcon::NwseResize,
            None => CursorIcon::Default,
        };
        if cursor != self.window_state.last_cursor {
            self.window.set_cursor(cursor.into());
            self.window_state.last_cursor = cursor;
        }
    }

    fn schedule_repaint(&self) {
        self.window.request_redraw();
    }

    pub(crate) fn destroy(&mut self) {
        self.event(Event::WindowClosed);
        self.scope.dispose();
        remove_window_id_mapping(&self.id, &self.window_id);
    }

    #[cfg(target_os = "macos")]
    fn show_context_menu(&self, menu: muda::Menu, pos: Option<Point>) {
        use muda::{
            ContextMenu,
            dpi::{LogicalPosition, Position},
        };
        use raw_window_handle::HasWindowHandle;
        use raw_window_handle::RawWindowHandle;

        if let RawWindowHandle::AppKit(handle) = self.window.window_handle().unwrap().as_raw() {
            unsafe {
                menu.show_context_menu_for_nsview(
                    handle.ns_view.as_ptr() as _,
                    pos.map(|pos| {
                        Position::Logical(LogicalPosition::new(
                            pos.x * self.window_state.scale,
                            (self.size.get_untracked().height - pos.y) * self.window_state.scale,
                        ))
                    }),
                )
            };
        }
    }

    #[cfg(target_os = "windows")]
    fn show_context_menu(&self, menu: muda::Menu, pos: Option<Point>) {
        use muda::{
            ContextMenu,
            dpi::{LogicalPosition, Position},
        };
        use raw_window_handle::HasWindowHandle;
        use raw_window_handle::RawWindowHandle;

        if let RawWindowHandle::Win32(handle) = self.window.window_handle().unwrap().as_raw() {
            unsafe {
                menu.show_context_menu_for_hwnd(
                    isize::from(handle.hwnd),
                    pos.map(|pos| {
                        Position::Logical(LogicalPosition::new(
                            pos.x * self.window_state.scale,
                            pos.y * self.window_state.scale,
                        ))
                    }),
                );
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn init_menu_for_windows(&self, menu: &muda::Menu) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        if let RawWindowHandle::Win32(handle) = self.window.window_handle().unwrap().as_raw() {
            unsafe {
                let menu_theme = match self.os_theme {
                    Some(winit::window::Theme::Light) => muda::MenuTheme::Light,
                    Some(winit::window::Theme::Dark) => muda::MenuTheme::Dark,
                    None => muda::MenuTheme::Auto,
                };
                let _ = menu.init_for_hwnd_with_theme(isize::from(handle.hwnd), menu_theme);
                let _ = menu.show_for_hwnd(isize::from(handle.hwnd));
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn set_menu_theme_for_windows(&self, theme: winit::window::Theme) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        if let RawWindowHandle::Win32(handle) = self.window.window_handle().unwrap().as_raw() {
            if let Some(menu) = &self.window_menu {
                unsafe {
                    let menu_theme = match theme {
                        winit::window::Theme::Light => muda::MenuTheme::Light,
                        winit::window::Theme::Dark => muda::MenuTheme::Dark,
                    };
                    let _ = menu.set_theme_for_hwnd(handle.hwnd.into(), menu_theme);
                }
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn show_context_menu(&self, menu: muda::Menu, pos: Option<Point>) {
        let pos = pos.unwrap_or(self.cursor_position);
        let pos = Point::new(
            pos.x / self.window_state.scale,
            pos.y / self.window_state.scale,
        );
        self.context_menu.set(Some((menu, pos, false)));
    }

    pub(crate) fn menu_action(&mut self, id: &MenuId) {
        set_current_view(self.id);
        if let Some(action) = self.window_state.context_menu.get(id) {
            (*action)();
            self.process_update();
        } else if let Some(action) = self.window_menu_actions.get(id) {
            (*action)();
            self.process_update();
        }
    }

    pub(crate) fn ime(&mut self, ime: Ime) {
        match ime {
            Ime::Enabled => {
                self.event(Event::ImeEnabled);
            }
            Ime::Preedit(text, cursor) => {
                self.event(Event::ImePreedit { text, cursor });
            }
            Ime::Commit(text) => {
                self.event(Event::ImeCommit(text));
            }
            Ime::Disabled => {
                self.event(Event::ImeDisabled);
            }
            Ime::DeleteSurrounding { .. } => {
                // TODO?
            }
        }
    }

    pub(crate) fn modifiers_changed(&mut self, modifiers: ModifiersState) {
        let is_altgr = self.modifiers.altgr();
        let mut modifiers: Modifiers = modifiers.into();
        if is_altgr {
            modifiers.set(Modifiers::ALTGR, true);
        }
        self.modifiers = modifiers;
    }
}

fn request_recursive_changes(id: ViewId, changes: ChangeFlags) {
    id.state().borrow_mut().requested_changes = changes;
    for child in id.children() {
        request_recursive_changes(child, changes);
    }
}

pub(crate) fn get_current_view() -> ViewId {
    CURRENT_RUNNING_VIEW_HANDLE.with(|running| *running.borrow())
}
/// Set this view handle to the current running view handle
pub(crate) fn set_current_view(id: ViewId) {
    CURRENT_RUNNING_VIEW_HANDLE.with(|running| {
        *running.borrow_mut() = id;
    });
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn context_menu_view(
    cx: Scope,
    context_menu: RwSignal<Option<(muda::Menu, Point, bool)>>,
    window_size: RwSignal<Size>,
) -> impl IntoView {
    use floem_reactive::{create_effect, create_rw_signal};
    use peniko::Color;

    use crate::{
        app::{AppUpdateEvent, add_app_update_event},
        views::{dyn_stack, empty, svg, text},
    };

    #[derive(Clone, PartialEq, Eq, Hash)]
    enum MenuDisplay {
        Separator(usize),
        Item {
            id: Option<String>,
            enabled: bool,
            title: String,
            children: Option<Vec<MenuDisplay>>,
        },
    }

    fn format_menu(menu: &MudaMenu) -> Vec<MenuDisplay> {
        menu.items()
            .iter()
            .enumerate()
            .map(|(s, item)| match item {
                muda::MenuItemKind::MenuItem(menu_item) => MenuDisplay::Item {
                    id: Some(menu_item.id().as_ref().to_string()),
                    enabled: menu_item.is_enabled(),
                    title: menu_item.text().to_string(),
                    children: None,
                },
                muda::MenuItemKind::Submenu(submenu) => MenuDisplay::Item {
                    id: None,
                    enabled: submenu.is_enabled(),
                    title: submenu.text().to_string(),
                    children: Some(format_submenu(submenu)),
                },
                muda::MenuItemKind::Predefined(_) => MenuDisplay::Separator(s),
                muda::MenuItemKind::Check(check_item) => MenuDisplay::Item {
                    id: Some(check_item.id().as_ref().to_string()),
                    enabled: check_item.is_enabled(),
                    title: check_item.text().to_string(),
                    children: None,
                },
                muda::MenuItemKind::Icon(icon_item) => MenuDisplay::Item {
                    id: Some(icon_item.id().as_ref().to_string()),
                    enabled: icon_item.is_enabled(),
                    title: icon_item.text().to_string(),
                    children: None,
                },
            })
            .collect()
    }

    fn format_submenu(submenu: &muda::Submenu) -> Vec<MenuDisplay> {
        submenu
            .items()
            .iter()
            .enumerate()
            .map(|(s, item)| match item {
                muda::MenuItemKind::MenuItem(menu_item) => MenuDisplay::Item {
                    id: Some(menu_item.id().as_ref().to_string()),
                    enabled: menu_item.is_enabled(),
                    title: menu_item.text().to_string(),
                    children: None,
                },
                muda::MenuItemKind::Submenu(nested_submenu) => MenuDisplay::Item {
                    id: None,
                    enabled: nested_submenu.is_enabled(),
                    title: nested_submenu.text().to_string(),
                    children: Some(format_submenu(nested_submenu)),
                },
                muda::MenuItemKind::Predefined(_) => MenuDisplay::Separator(s),
                muda::MenuItemKind::Check(check_item) => MenuDisplay::Item {
                    id: Some(check_item.id().as_ref().to_string()),
                    enabled: check_item.is_enabled(),
                    title: check_item.text().to_string(),
                    children: None,
                },
                muda::MenuItemKind::Icon(icon_item) => MenuDisplay::Item {
                    id: Some(icon_item.id().as_ref().to_string()),
                    enabled: icon_item.is_enabled(),
                    title: icon_item.text().to_string(),
                    children: None,
                },
            })
            .collect()
    }

    let context_menu_items = cx.create_memo(move |_| {
        context_menu.with(|menu| {
            menu.as_ref()
                .map(|(menu, _, _): &(MudaMenu, Point, bool)| format_menu(menu))
        })
    });
    let context_menu_size = cx.create_rw_signal(Size::ZERO);

    fn view_fn(
        menu: MenuDisplay,
        context_menu: RwSignal<Option<(MudaMenu, Point, bool)>>,
        on_child_submenu_for_parent: RwSignal<bool>,
    ) -> impl IntoView {
        match menu {
            MenuDisplay::Item {
                id,
                enabled,
                title,
                children,
            } => {
                let menu_width = create_rw_signal(0.0);
                let show_submenu = create_rw_signal(false);
                let on_submenu = create_rw_signal(false);
                let on_child_submenu = create_rw_signal(false);
                let has_submenu = children.is_some();
                let submenu_svg = r#"<svg width="16" height="16" viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="currentColor"><path fill-rule="evenodd" clip-rule="evenodd" d="M10.072 8.024L5.715 3.667l.618-.62L11 7.716v.618L6.333 13l-.618-.619 4.357-4.357z"/></svg>"#;
                container(
                    stack((
                        stack((
                            text(title).style(|s| s.selectable(false)),
                            svg(submenu_svg).style(move |s| {
                                s.size(20.0, 20.0)
                                    .color(Color::from_rgb8(201, 201, 201))
                                    .margin_right(10.0)
                                    .margin_left(20.0)
                                    .apply_if(!has_submenu, |s| s.hide())
                            }),
                        ))
                        .on_event_stop(EventListener::PointerEnter, move |_| {
                            if has_submenu {
                                show_submenu.set(true);
                            }
                        })
                        .on_event_stop(EventListener::PointerLeave, move |_| {
                            if has_submenu {
                                show_submenu.set(false);
                            }
                        })
                        .on_resize(move |rect| {
                            let width = rect.width();
                            if menu_width.get_untracked() != width {
                                menu_width.set(width);
                            }
                        })
                        .on_event_stop(EventListener::PointerDown, move |_| {
                            context_menu.update(|context_menu| {
                                if let Some((_, _, had_pointer_down)) = context_menu.as_mut() {
                                    *had_pointer_down = true;
                                }
                            });
                        })
                        .on_event_stop(EventListener::PointerUp, move |_| {
                            if has_submenu {
                                // don't handle the click if there's submenu
                                return;
                            }
                            context_menu.set(None);
                            if let Some(id) = id.clone() {
                                add_app_update_event(AppUpdateEvent::MenuAction {
                                    action_id: id.into(),
                                });
                            }
                        })
                        .style(move |s| {
                            s.width(100.pct())
                                .min_width(100.pct())
                                .padding_horiz(20.0)
                                .justify_between()
                                .items_center()
                                .hover(|s| {
                                    s.border_radius(10.0)
                                        .background(Color::from_rgb8(65, 65, 65))
                                })
                                .active(|s| {
                                    s.border_radius(10.0)
                                        .background(Color::from_rgb8(92, 92, 92))
                                })
                                .set_disabled(!enabled)
                                .disabled(|s| s.color(Color::from_rgb8(92, 92, 92)))
                        }),
                        dyn_stack(
                            move || children.clone().unwrap_or_default(),
                            move |s| s.clone(),
                            move |menu| view_fn(menu, context_menu, on_child_submenu),
                        )
                        .on_event_stop(EventListener::KeyDown, move |event| {
                            if let Event::KeyDown(event) = event {
                                if event.key.logical_key == Key::Named(NamedKey::Escape) {
                                    context_menu.set(None);
                                }
                            }
                        })
                        .on_event_stop(EventListener::PointerEnter, move |_| {
                            if has_submenu {
                                on_submenu.set(true);
                                on_child_submenu_for_parent.set(true);
                            }
                        })
                        .on_event_stop(EventListener::PointerLeave, move |_| {
                            if has_submenu {
                                on_submenu.set(false);
                                on_child_submenu_for_parent.set(false);
                            }
                        })
                        .style(move |s| {
                            s.absolute()
                                .focusable(true)
                                .min_width(200.0)
                                .margin_top(-5.0)
                                .margin_left(menu_width.get() as f32)
                                .flex_col()
                                .border_radius(10.0)
                                .background(Color::from_rgb8(44, 44, 44))
                                .padding(5.0)
                                .cursor(CursorStyle::Default)
                                .box_shadow_blur(5.0)
                                .box_shadow_color(palette::css::BLACK)
                                .apply_if(
                                    !show_submenu.get()
                                        && !on_submenu.get()
                                        && !on_child_submenu.get(),
                                    |s| s.hide(),
                                )
                        }),
                    ))
                    .style(|s| s.min_width(100.pct())),
                )
                .style(|s| s.min_width(100.pct()))
                .into_any()
            }

            MenuDisplay::Separator(_) => container(empty().style(|s| {
                s.width(100.pct())
                    .height(1.0)
                    .margin_vert(5.0)
                    .background(Color::from_rgb8(92, 92, 92))
            }))
            .style(|s| s.min_width(100.pct()).padding_horiz(20.0))
            .into_any(),
        }
    }

    let on_child_submenu = create_rw_signal(false);
    let view = dyn_stack(
        move || context_menu_items.get().unwrap_or_default(),
        move |s| s.clone(),
        move |menu| view_fn(menu, context_menu, on_child_submenu),
    )
    .on_resize(move |rect| {
        context_menu_size.set(rect.size());
    })
    .on_event_stop(EventListener::PointerDown, move |_| {
        context_menu.update(|context_menu| {
            if let Some((_, _, had_pointer_down)) = context_menu.as_mut() {
                *had_pointer_down = true;
            }
        });
    })
    .on_event_stop(EventListener::PointerUp, move |_| {
        context_menu.update(|context_menu| {
            if let Some((_, _, had_pointer_down)) = context_menu.as_mut() {
                *had_pointer_down = false;
            }
        });
    })
    .on_event_stop(EventListener::PointerMove, move |_| {})
    .on_event_stop(EventListener::KeyDown, move |event| {
        if let Event::KeyDown(event) = event {
            if event.key.logical_key == Key::Named(NamedKey::Escape) {
                context_menu.set(None);
            }
        }
    })
    .style(move |s| {
        let window_size = window_size.get();
        let menu_size = context_menu_size.get();
        let is_active = context_menu.with(|m| m.is_some());
        let mut pos = context_menu.with(|m| m.as_ref().map(|(_, pos, _)| *pos).unwrap_or_default());
        if pos.x + menu_size.width > window_size.width {
            pos.x = window_size.width - menu_size.width;
        }
        if pos.y + menu_size.height > window_size.height {
            pos.y = window_size.height - menu_size.height;
        }
        s.absolute()
            .min_width(200.0)
            .flex_col()
            .border_radius(10.0)
            .focusable(true)
            .background(Color::from_rgb8(44, 44, 44))
            .color(Color::from_rgb8(201, 201, 201))
            .z_index(999)
            .line_height(2.0)
            .padding(5.0)
            .margin_left(pos.x as f32)
            .margin_top(pos.y as f32)
            .cursor(CursorStyle::Default)
            .apply_if(!is_active, |s| s.hide())
            .box_shadow_blur(5.0)
            .box_shadow_color(palette::css::BLACK)
    });

    let id = view.id();

    create_effect(move |_| {
        if context_menu.with(|m| m.is_some()) {
            id.request_focus();
        }
    });

    view
}

/// A view representing a window which manages the main window view and any overlays.
struct WindowView {
    id: ViewId,
}

impl View for WindowView {
    fn id(&self) -> ViewId {
        self.id
    }

    fn view_style(&self) -> Option<crate::style::Style> {
        Some(Style::new().width_full().height_full())
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Window".into()
    }
}
