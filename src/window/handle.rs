#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
use std::{cell::RefCell, mem, rc::Rc, sync::Arc};

#[cfg(feature = "crossbeam")]
use crossbeam::channel::bounded as sync_channel;
#[cfg(not(feature = "crossbeam"))]
use std::sync::mpsc::sync_channel;

use crate::event::{CustomEvent, RouteKind, ScrollTo, UpdatePhaseEvent};
use crate::ElementIdExt;
use crate::platform::menu_types::{Menu as MudaMenu, MenuId};
use crate::style::recalc::StyleReason;
use crate::style::{StyleSelector, StyleSelectors};
#[cfg(target_os = "windows")]
use muda::MenuTheme as MudaMenuTheme;

use crate::platform::{Duration, Instant};
use ui_events::keyboard::{Key, KeyboardEvent, Modifiers, NamedKey};
use ui_events::pointer::PointerEvent;
use ui_events_winit::WindowEventReducer;

use winit::window::{
    ImeCapabilities, ImeEnableRequest, ImeHint, ImePurpose, ImeRequest, ImeRequestData,
};

use floem_reactive::{RwSignal, Scope, SignalGet, SignalUpdate};
use floem_renderer::Renderer;
use floem_renderer::gpu_resources::GpuResources;
use peniko::color::palette;
use peniko::kurbo::{self, Point, Size};
use winit::{
    cursor::CursorIcon,
    dpi::{LogicalPosition, LogicalSize},
    event::Ime,
    window::{Window, WindowId},
};

use super::state::WindowState;
use super::tracking::{remove_window_id_mapping, store_window_id_mapping};
use crate::app::{MenuWrapper, add_app_update_event};
#[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
use crate::platform::context_menu::context_menu_view;
#[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
use crate::reactive::SignalWith;
#[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
use crate::unit::UnitExt;
use crate::view::{LayoutTree, VIEW_STORAGE};
#[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
use crate::views::{Container, Decorators, Stack};
use crate::{
    Application,
    app::UserEvent,
    context::{FrameUpdate, LayoutChanged, PaintState, StyleCx, UpdateCx, VisualChanged},
    event::{
        Event, GlobalEventCx, ImeEvent, WindowEvent, clear_hit_test_cache,
        dropped_file::FileDragEvent,
    },
    inspector::{self, Capture, CaptureState, CapturedView, profiler::Profile},
    message::{
        CENTRAL_DEFERRED_UPDATE_MESSAGES, CENTRAL_UPDATE_MESSAGES, CURRENT_RUNNING_VIEW_HANDLE,
        DEFERRED_UPDATE_MESSAGES, UPDATE_MESSAGES, UpdateMessage,
    },
    style::{CursorStyle, Style},
    theme::default_theme,
    view::{IntoView, View, ViewId},
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
    /// The root view ID for this window.
    pub(crate) id: ViewId,
    /// Reactive Scope for this `WindowHandle`
    scope: Scope,
    pub(crate) window_state: WindowState,
    pub(crate) paint_state: PaintState,
    size: RwSignal<Size>,
    default_theme: Option<Style>,
    pub(crate) profile: Option<Profile>,
    is_maximized: bool,
    transparent: bool,
    pub(crate) modifiers: Modifiers,
    pub(crate) window_position: Point,
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
    pub(crate) context_menu: RwSignal<Option<(MudaMenu, Point, bool)>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) window_menu_actions: HashMap<MenuId, Box<dyn Fn()>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) window_menu: Option<MudaMenu>,
    pub(crate) event_reducer: WindowEventReducer,
    pub(crate) gpu_resources: Option<GpuResources>,
    last_presented_at: Instant,
    is_occluded: bool,
    live_resize_until: Option<Instant>,
}

impl Drop for WindowHandle {
    fn drop(&mut self) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            s.box_tree.remove(&self.id);
        })
    }
}

impl WindowHandle {
    const LIVE_RESIZE_IDLE_TIMEOUT: Duration = Duration::from_millis(120);

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        window: Box<dyn winit::window::Window>,
        gpu_resources: Option<GpuResources>,
        required_features: wgpu::Features,
        backends: Option<wgpu::Backends>,
        view_fn: impl FnOnce(winit::window::WindowId) -> Box<dyn View> + 'static,
        transparent: bool,
        apply_default_theme: bool,
        font_embolden: f32,
    ) -> Self {
        let id = ViewId::new_root();
        let window_id = window.id();
        let scope = Scope::new();
        let os_scale = window.scale_factor();
        let size: LogicalSize<f64> = window.surface_size().to_logical(os_scale);
        let size = Size::new(size.width, size.height);
        let size = scope.create_rw_signal(Size::new(size.width, size.height));
        let os_theme = window.theme();
        // let current_theme = apply_theme.unwrap_or(os_theme.unwrap_or(winit::window::Theme::Light));
        let is_maximized = window.is_maximized();

        set_current_view(id);

        #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
        let context_menu = scope.create_rw_signal(None);

        #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32")))]
        let view = scope.enter(move || view_fn(window_id));

        #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
        let view = scope.enter(move || {
            let main_view = view_fn(window_id);
            Stack::new((
                Container::new(main_view).style(|s| s.size(100.pct(), 100.pct())),
                context_menu_view(scope, context_menu, size),
            ))
            .style(|s| s.size(100.pct(), 100.pct()))
            .into_any()
        });

        id.add_child(view);

        let view = WindowView { id };
        id.set_view(view.into_any());

        let window: Arc<dyn Window> = window.into();
        store_window_id_mapping(id, window_id, &window);

        let paint_state = Self::new_paint_state(
            window.clone(),
            gpu_resources.clone(),
            os_scale,
            size.get_untracked() * os_scale,
            font_embolden,
            required_features,
            backends,
        );

        let paint_state_initialized = matches!(paint_state, PaintState::Initialized { .. });

        let window_state = WindowState::new(id, os_theme, os_scale);

        let mut window_handle = Self {
            window,
            window_id,
            id,
            scope,
            paint_state,
            size,
            default_theme: match apply_default_theme {
                true => Some(default_theme(window_state.light_dark_theme)),
                false => None,
            },
            window_state,
            is_maximized,
            transparent,
            profile: None,
            modifiers: Modifiers::default(),
            window_position: Point::ZERO,
            #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
            context_menu,
            #[cfg(not(target_arch = "wasm32"))]
            window_menu_actions: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            window_menu: None,
            event_reducer: WindowEventReducer::default(),
            gpu_resources,
            last_presented_at: Instant::now(),
            is_occluded: false,
            live_resize_until: None,
        };
        if paint_state_initialized {
            window_handle.init_renderer();
        }
        window_handle
            .window_state
            .set_root_size(size.get_untracked());
        window_handle
            .window_state
            .update_screen_size_bp(size.get_untracked());
        window_handle.process_update_no_paint();

        window_handle.window_state.light_dark_theme =
            os_theme.unwrap_or(winit::window::Theme::Light);

        window_handle.event(Event::Window(WindowEvent::ThemeChanged(
            window_handle.window_state.light_dark_theme,
        )));
        window_handle
            .window_state
            .mark_style_dirty_selector(window_handle.id.get_element_id(), StyleSelector::DarkMode);
        window_handle.size(size.get_untracked());
        window_handle
    }

    #[cfg(feature = "skia")]
    fn new_paint_state(
        window: Arc<dyn Window>,
        _gpu_resources: Option<GpuResources>,
        os_scale: f64,
        size: Size,
        font_embolden: f32,
        _required_features: wgpu::Features,
        _backends: Option<wgpu::Backends>,
    ) -> PaintState {
        PaintState::new_skia(window, os_scale, size, font_embolden)
    }

    #[cfg(not(feature = "skia"))]
    fn new_paint_state(
        window: Arc<dyn Window>,
        gpu_resources: Option<GpuResources>,
        os_scale: f64,
        size: Size,
        font_embolden: f32,
        required_features: wgpu::Features,
        backends: Option<wgpu::Backends>,
    ) -> PaintState {
        if let Some(resources) = gpu_resources {
            let surface = resources
                .instance
                .create_surface(Arc::clone(&window))
                .expect("can create second window");
            PaintState::new(window, surface, resources, os_scale, size, font_embolden)
        } else {
            let gpu_resources_rx = GpuResources::request(
                move |window_id| {
                    Application::send_proxy_event(UserEvent::GpuResourcesUpdate { window_id });
                },
                required_features,
                backends,
                window.clone(),
            );
            PaintState::new_pending(window, gpu_resources_rx, size, font_embolden)
        }
    }

    /// Creates a headless WindowHandle for testing purposes.
    ///
    /// This constructor creates a WindowHandle with a MockWindow and no GPU resources,
    /// suitable for testing the event handling and view update logic without a real window.
    ///
    /// # Arguments
    /// * `root_id` - The root ViewId (from TestRoot)
    /// * `view` - The root view for this window
    /// * `size` - The virtual window size
    /// * `scale` - The window scale factor (default 1.0)
    pub(crate) fn new_headless(
        root_id: ViewId,
        view: impl IntoView,
        size_val: Size,
        os_scale: f64,
    ) -> Self {
        use super::mock::MockWindow;

        let scope = Scope::new();
        let mock_window = MockWindow::with_size(size_val.width as u32, size_val.height as u32);
        let window_id = mock_window.id();
        let id = root_id;
        let size = scope.create_rw_signal(size_val);
        let os_theme = mock_window.theme();
        let is_maximized = mock_window.is_maximized();

        // Root is already set by TestRoot, but set it again to be safe
        set_current_view(id);

        // Convert the view
        let main_view = view.into_view();
        let widget: Box<dyn View> = main_view.into_any();

        id.set_children([widget]);

        let window_view = WindowView { id };
        id.set_view(window_view.into_any());

        let window: Arc<dyn Window> = Arc::new(mock_window);
        store_window_id_mapping(id, window_id, &window);

        // Create a paint state that will never initialize (for headless testing)
        // We use a channel that will never receive a value
        let (tx, rx) = sync_channel(1);
        drop(tx); // Drop sender so receiver will never receive
        let paint_state = PaintState::new_pending(
            window.clone(),
            rx,
            size_val * os_scale,
            0.0, // font_embolden
        );

        let window_state = WindowState::new(id, os_theme, os_scale);

        let mut window_handle = Self {
            window,
            window_id,
            id,
            scope,
            paint_state,
            size,
            default_theme: Some(default_theme(window_state.light_dark_theme)),
            window_state,
            is_maximized,
            transparent: false,
            profile: None,
            modifiers: Modifiers::default(),
            window_position: Point::ZERO,
            #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
            context_menu: scope.create_rw_signal(None),
            #[cfg(not(target_arch = "wasm32"))]
            window_menu_actions: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            window_menu: None,
            event_reducer: WindowEventReducer::default(),
            gpu_resources: None,
            last_presented_at: Instant::now(),
            is_occluded: false,
            live_resize_until: None,
        };

        window_handle
            .window_state
            .set_root_size(size.get_untracked());

        window_handle.window_state.light_dark_theme =
            os_theme.unwrap_or(winit::window::Theme::Light);

        // Run initial style and layout passes
        window_handle.process_update_messages();
        // Mark root view as needing style so the initial style pass drives the
        // StyleTree cascade and populates has_style_selectors for selector detection
        window_handle.id.request_style(StyleReason::full_recalc());
        window_handle.process_update_messages();
        window_handle.style();
        window_handle.layout();
        window_handle.commit_box_tree();

        window_handle
    }

    pub(crate) fn init_renderer(&mut self) {
        // On the web, we need to get the canvas size once. The size will be updated automatically
        // when the canvas element is resized subsequently. This is the correct place to do so
        // because the renderer is not initialized until now.
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWeb;

            let rect = self.window.canvas().unwrap().get_bounding_client_rect();
            // let rect = canvas.get_bounding_client_rect();
            let size = LogicalSize::new(rect.width(), rect.height());
            self.size(Size::new(size.width, size.height));
        }
        // Now that the renderer is initialized, draw the first frame
        self.render_frame();
        self.window.set_visible(true);
    }

    pub fn event(&mut self, event: Event) {
        set_current_view(self.id.root());

        // Check event type for platform-specific context menu handling
        #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
        let is_pointer_down = matches!(&event, Event::Pointer(PointerEvent::Down { .. }));
        #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
        let is_pointer_up = matches!(&event, Event::Pointer(PointerEvent::Up { .. }));

        let root_element_id = self.window_state.root_view_id.get_element_id();
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();

        // Platform-specific context menu handling
        #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
        {
            if is_pointer_down
                && self.context_menu.with_untracked(|c| {
                    c.as_ref()
                        .map(|(_, _, had_pointer_down)| !*had_pointer_down)
                        .unwrap_or(false)
                })
            {
                // we had a pointer down event
                // if context menu is still shown
                // we should hide it
                self.context_menu.set(None);
            }
            if is_pointer_up && self.context_menu.with_untracked(|c| c.is_some()) {
                // we had a pointer up event
                // if context menu is still shown
                // we should hide it
                self.context_menu.set(None);
            }
        }
    }

    pub(crate) fn window_id(&self) -> WindowId {
        self.window_id
    }

    pub(crate) fn os_scale(&mut self, os_scale: f64) {
        self.window_state.os_scale = os_scale;
        let scale = self.window_state.effective_scale();
        self.paint_state.set_scale(scale);
        self.event(Event::Window(WindowEvent::ScaleChanged(scale)));
        self.window_state.request_paint = true;
        self.schedule_repaint();
    }

    pub(crate) fn set_theme(&mut self, theme: Option<winit::window::Theme>, change_from_os: bool) {
        if change_from_os && self.window_state.theme_overriden {
            // if the window theme has been set manually then changes from the os shouldn't do anything
            return;
        }
        self.window_state.mark_style_dirty_selector(
            self.window_state.root_view_id.get_element_id(),
            StyleSelector::DarkMode,
        );
        if let Some(theme) = theme {
            // Only override the theme with the default if the user did not provide one
            if self.default_theme.is_some() {
                self.default_theme = Some(default_theme(theme));
            }
            // Update the default theme in WindowState for style computation
            self.window_state.update_default_theme(theme);
            self.window_state.light_dark_theme = theme;
            if !change_from_os {
                self.window_state.theme_overriden = true
            }
            #[cfg(target_os = "windows")]
            {
                self.set_menu_theme_for_windows(theme);
            }
            // Mark dark mode changed if theme actually changed
            // if theme != old_theme {}
        } else {
            self.window_state.theme_overriden = false;
        }
        if !change_from_os {
            self.window.set_theme(theme);
        }
        self.id.request_style(StyleReason::inherited());
        if let Some(theme) = theme {
            self.event(Event::Window(WindowEvent::ThemeChanged(theme)));
        }
    }

    pub(crate) fn size(&mut self, size: Size) {
        let width_changed = (self.window_state.root_size.width - size.width).abs() > f64::EPSILON;
        self.size.set(size);

        // Update root size first so any style work triggered by resize observes
        // the new width instead of the previous frame's value.
        self.window_state.set_root_size(size);
        if width_changed {
            self.window_state.mark_style_dirty_with(
                self.window_state.root_view_id.get_element_id(),
                StyleReason::with_selectors(StyleSelectors::empty().responsive()),
            );
        }

        self.window_state.update_screen_size_bp(size);
        self.event(Event::Window(WindowEvent::Resized(size)));
        let scale = self.window_state.effective_scale();
        self.paint_state
            .resize(scale, size * self.window_state.os_scale);

        let is_maximized = self.window.is_maximized();
        if is_maximized != self.is_maximized {
            self.is_maximized = is_maximized;
            self.event(Event::Window(WindowEvent::MaximizeChanged(is_maximized)));
        }

        self.style();
        self.layout();
        self.commit_box_tree();
        self.process_update_no_paint();
        self.window_state.request_paint = true;
        self.schedule_repaint();
    }

    pub(crate) fn refresh_live_resize(&mut self) {
        self.live_resize_until = Some(Instant::now() + Self::LIVE_RESIZE_IDLE_TIMEOUT);
    }

    fn live_resize_active(&self) -> bool {
        self.live_resize_until
            .is_some_and(|deadline| Instant::now() < deadline)
    }

    pub(crate) fn position(&mut self, point: Point) {
        self.window_position = point;
        self.event(Event::Window(WindowEvent::Moved(point)));
    }

    pub(crate) fn file_drag_dropped(&mut self, file_drag_event: FileDragEvent) {
        // Store paths in window state for tracking during drag
        self.window_state.file_drag_paths = None;
        self.event(Event::FileDrag(file_drag_event));
    }

    pub(crate) fn file_drag_start(&mut self, paths: Vec<std::path::PathBuf>, position: Point) {
        // Store paths and dispatch as a move event to trigger hit testing
        let paths_rc: Rc<[std::path::PathBuf]> = paths.into();
        self.window_state.file_drag_paths = Some(paths_rc.clone());
        self.event(Event::FileDrag(FileDragEvent::Move(
            crate::event::dropped_file::FileDragMove {
                paths: paths_rc,
                position,
            },
        )));
    }

    pub(crate) fn file_drag_move(&mut self, position: Point) {
        if let Some(paths) = &self.window_state.file_drag_paths {
            self.event(Event::FileDrag(FileDragEvent::Move(
                crate::event::dropped_file::FileDragMove {
                    paths: paths.clone(),
                    position,
                },
            )));
        }
    }

    pub(crate) fn file_drag_end(&mut self) {
        // Clear paths and file hover state
        self.window_state.file_drag_paths = None;
        set_current_view(self.id.root());
        let root_element_id = self.window_state.root_view_id.get_element_id();
        GlobalEventCx::new(&mut self.window_state, root_element_id, Event::Extracted)
            .update_hover_from_path(&[]);
        self.process_update();
    }

    pub(crate) fn key_event(&mut self, key_event: KeyboardEvent) {
        let is_altgr = key_event.key == Key::Named(NamedKey::AltGraph);
        if key_event.state.is_down() {
            if is_altgr {
                self.modifiers.set(Modifiers::ALT_GRAPH, true);
            }
        } else if is_altgr {
            self.modifiers.set(Modifiers::ALT_GRAPH, false);
        }
        self.event(Event::Key(key_event));
    }

    pub(crate) fn pointer_event(&mut self, pointer_event: PointerEvent) {
        self.event(Event::Pointer(pointer_event));
    }

    pub(crate) fn focused(&mut self, focused: bool) {
        if focused {
            #[cfg(target_os = "macos")]
            if let Some(window_menu) = &self.window_menu {
                window_menu.init_for_nsapp();
            }
            self.event(Event::Window(WindowEvent::FocusGained));
        } else {
            self.event(Event::Window(WindowEvent::FocusLost));
        }
    }

    fn style(&mut self) {
        // Capture a single timestamp for the entire style pass.
        // All views in this frame see the same `now`, which is both cheaper
        // (avoids per-view syscall) and more correct (no sub-frame jitter).
        self.window_state.frame_start = std::time::Instant::now();

        // Loop until no more views need styling
        // This handles the case where styling a parent marks children dirty
        // (e.g., when inherited properties change)
        loop {
            // Build explicit traversal order
            let traversal = self.window_state.build_style_traversal(self.id);
            if traversal.is_empty() {
                break;
            }

            // Sync host state into the engine's StyleTree, run the
            // cascade, and copy outputs back into each view's
            // `style_storage`. See `WindowState::run_style_cascade` for
            // the three-pass breakdown.
            self.window_state.run_style_cascade(self.id, &traversal);

            // Per-view loop: animations, taffy, View::style_pass. The
            // cascade has already run; `style_view` reads results from
            // `style_storage` on the fast path (no `compute_combined`).
            for (view_id, traversal_reason) in traversal {
                let cx = &mut StyleCx::new(&mut self.window_state, view_id, traversal_reason);
                cx.style_view();
            }
            if self.window_state.capture.is_some() {
                self.window_state.style_dirty.clear();
                // we need to break if capture because when capturing we style all views so no need to loop here.
                // we style all views so that the capture can accurately report how long a full style takes
                break;
            }
        }

        // Clear pending child changes after style pass completes
        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Style));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();
    }

    fn layout(&mut self) -> Duration {
        let start = Instant::now();
        self.window_state.compute_layout();
        let taffy_duration = start.elapsed();

        // Update box tree from layout after layout completes
        self.window_state.update_box_tree_from_layout();

        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Layout));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();

        taffy_duration
    }

    fn update_box_tree_from_layout(&mut self) {
        self.window_state.update_box_tree_from_layout();
        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::BoxTreeUpdate));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();
    }

    fn process_pending_box_tree_updates(&mut self) {
        self.window_state.process_pending_box_tree_updates();
        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(
            UpdatePhaseEvent::BoxTreePendingUpdates,
        ));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();
    }

    fn commit_box_tree(&mut self) -> Duration {
        let start = Instant::now();
        self.window_state.commit_box_tree();
        self.window_state.needs_box_tree_commit = false;

        let has_layout_listener: smallvec::SmallVec<[ViewId; 64]> = self
            .window_state
            .listeners
            .get(&LayoutChanged::listener_key())
            .into_iter()
            .flatten()
            .copied()
            .collect();
        for id in has_layout_listener {
            if let Some(layout) = id.get_layout() {
                let window_origin = id.get_layout_window_origin();
                let new_box = kurbo::Rect::from_origin_size(
                    (layout.location.x as f64, layout.location.y as f64),
                    (layout.size.width as f64, layout.size.height as f64),
                );
                let new_content_box = kurbo::Rect::from_origin_size(
                    (layout.content_box_x() as f64, layout.content_box_y() as f64),
                    (
                        layout.content_box_width() as f64,
                        layout.content_box_height() as f64,
                    ),
                );
                let new_layout = LayoutChanged {
                    new_box,
                    new_content_box,
                    new_window_origin: window_origin,
                };
                let (old_layout, element_id) = {
                    let state = id.state();
                    let mut state = state.borrow_mut();
                    let old: Option<LayoutChanged> = state.layout;
                    state.layout = Some(new_layout);
                    let element_id = state.element_id;
                    (old, element_id)
                };
                if old_layout.is_none_or(|old| old != new_layout) {
                    use crate::context::Phases;
                    use crate::event::RouteKind;
                    GlobalEventCx::new(
                        &mut self.window_state,
                        element_id,
                        Event::new_custom(new_layout),
                    )
                    .route_normal(
                        RouteKind::Directed {
                            target: element_id,
                            phases: Phases::TARGET,
                        },
                        None,
                    );
                }
            }
        }

        let needs_moved: smallvec::SmallVec<[ViewId; 64]> = self
            .window_state
            .listeners
            .get(&VisualChanged::listener_key())
            .into_iter()
            .flatten()
            .copied()
            .collect();
        for id in needs_moved {
            let transform = id.get_visual_transform();
            let visual_aabb = id.get_visual_rect();
            let element_id = id.get_element_id();

            let new_visual = VisualChanged {
                new_visual_aabb: visual_aabb,
                new_world_transform: transform,
            };

            let old_visual = {
                let state = id.state();
                let mut state = state.borrow_mut();
                let old = state.visual_change;
                state.visual_change = Some(new_visual);
                old
            };

            if old_visual.is_none_or(|old| old != new_visual) {
                use crate::context::Phases;
                use crate::event::RouteKind;
                GlobalEventCx::new(
                    &mut self.window_state,
                    element_id,
                    Event::new_custom(new_visual),
                )
                .route_normal(
                    RouteKind::Directed {
                        target: element_id,
                        phases: Phases::TARGET,
                    },
                    None,
                );
            }
        }

        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::BoxTreeCommit));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();

        start.elapsed()
    }

    /// Process any scheduled updates (style/layout/paint requests from previous frame).
    /// This converts scheduled updates to immediate requests.
    pub(crate) fn process_scheduled_updates(&mut self) {
        for update in mem::take(&mut self.window_state.scheduled_updates) {
            match update {
                FrameUpdate::Layout => {
                    self.window_state.needs_layout = true;
                }
                FrameUpdate::BoxTreeCommit => {
                    self.window_state.needs_box_tree_commit = true;
                }
                FrameUpdate::Style(id, reason) => {
                    self.window_state.mark_style_dirty_with(id, reason);
                }
                FrameUpdate::Paint(id) => self.window_state.request_paint(id),
            }
        }
    }

    pub(crate) fn render_frame(&mut self) {
        let renderer_ready = matches!(self.paint_state, PaintState::Initialized { .. });
        if self.window_state.request_paint && renderer_ready {
            self.window_state.request_paint = false;
            self.paint();
            self.last_presented_at = Instant::now();
        }

        if self.live_resize_active() {
            self.window_state.schedule_paint(self.id);
            self.window_state.request_paint = true;
            self.schedule_repaint();
        } else {
            self.live_resize_until = None;
        }

        // Keep animation control flow in sync with scheduled updates.
        let window_id = self.window.id();
        if !self.window_state.scheduled_updates.is_empty() {
            add_app_update_event(crate::app::AppUpdateEvent::AnimationFrame(true, window_id));
        } else {
            add_app_update_event(crate::app::AppUpdateEvent::AnimationFrame(false, window_id));
        }
    }

    pub fn paint(&mut self) -> Option<peniko::ImageBrush> {
        // Create GlobalPaintCx (global/shared state)
        let mut cx = crate::paint::GlobalPaintCx {
            window_state: &mut self.window_state,
            paint_state: &mut self.paint_state,
            gpu_resources: self.gpu_resources.clone(),
            window: self.window.clone(),
            record_paint_order: crate::paint::is_paint_order_tracking_enabled(),
        };

        cx.paint_state
            .renderer_mut()
            .begin(cx.window_state.capture.is_some());

        // Background fill (unchanged)
        if !self.transparent {
            let color = self
                .default_theme
                .as_ref()
                .and_then(|theme| theme.get(crate::style::Background))
                .unwrap_or(peniko::Brush::Solid(palette::css::WHITE));

            // Fill the full render target. The renderer now operates in device space
            // during paint, so this must use the physical surface size, not logical size.
            let renderer = cx.paint_state.renderer_mut();
            renderer.fill(&renderer.size().to_rect().expand(), &color, 0.0);
        }

        // Paint main tree with overlays using explicit traversal
        cx.paint_with_traversal(self.id);

        self.window.pre_present_notify();
        let root_element_id = cx.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::PaintPresent));
        GlobalEventCx::new(cx.window_state, root_element_id, event).route_window_event();

        cx.paint_state.renderer_mut().finish()
    }

    pub(crate) fn capture(&mut self) -> Capture {
        // Capture the view before we run `style` and `layout` to catch missing `request_style`` or
        // `request_layout` flags.
        let root = CapturedView::capture(self.id, &mut self.window_state);

        self.window_state.capture = Some(CaptureState::default());

        // Trigger painting to create a Vger renderer which can capture the output.
        // This can be expensive so it could skew the paint time measurement.
        self.paint();

        // Ensure we run layout and styling again for accurate timing. We also need to ensure
        // styles are recomputed to capture them.
        fn request_changes(id: ViewId) {
            id.request_all();
            for child in id.children() {
                request_changes(child);
            }
        }
        request_changes(self.id);

        fn get_taffy_depth(taffy: Rc<RefCell<LayoutTree>>, root: taffy::tree::NodeId) -> usize {
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

        let taffy_root_node = self.id.state().borrow().layout_id;
        let taffy_duration = self.layout();
        let _box_tree_duration = self.commit_box_tree();
        let post_layout = Instant::now();
        let window = self.paint();
        let end = Instant::now();
        let window_size = self.paint_state.renderer().size() / self.window_state.effective_scale();

        let capture = Capture {
            start,
            post_style,
            post_layout,
            end,
            taffy_duration,
            taffy_node_count: self.id.taffy().borrow().total_node_count(),
            taffy_depth: get_taffy_depth(self.id.taffy(), taffy_root_node),
            window,
            window_size,
            root: Rc::new(root),
            state: self.window_state.capture.take().unwrap(),
            renderer: self.paint_state.renderer().debug_info(),
        };
        // Process any updates produced by capturing
        self.process_update();

        capture
    }

    pub(crate) fn process_update(&mut self) {
        self.process_update_no_paint();
    }

    pub(crate) fn render_frame_if_due(&mut self, min_frame_interval: Duration) -> bool {
        if !self.window_state.request_paint {
            return false;
        }
        if self.last_presented_at.elapsed() < min_frame_interval {
            return false;
        }
        self.render_frame();
        true
    }

    pub(crate) fn set_occluded(&mut self, is_occluded: bool) {
        self.is_occluded = is_occluded;
    }

    pub(crate) fn can_render_now(&self) -> bool {
        !self.is_occluded && self.window.is_visible().unwrap_or(true)
    }

    /// Processes updates up to a shared budget and returns whether this window is quiescent.
    pub(crate) fn process_update_budgeted(&mut self, start: Instant, budget: Duration) -> bool {
        let mut iterations = 0usize;
        const MAX_ITERS: usize = 32;

        loop {
            loop {
                self.process_update_messages();
                let needs_style = self.needs_style();
                let needs_layout = self.needs_layout();
                let needs_box_update = self.needs_box_tree_update();
                let needs_box = self.needs_box_tree_commit();
                let has_pending_box_updates =
                    !self.window_state.views_needing_box_tree_update.is_empty();
                if !needs_layout
                    && !needs_style
                    && !needs_box
                    && !has_pending_box_updates
                    && !needs_box_update
                {
                    break;
                }

                if needs_style {
                    self.style();
                }

                if self.needs_layout() {
                    self.layout();
                }

                if self.needs_box_tree_update() {
                    self.update_box_tree_from_layout();
                }

                if !self.window_state.views_needing_box_tree_update.is_empty() {
                    self.process_pending_box_tree_updates();
                }

                if self.needs_box_tree_commit() {
                    self.commit_box_tree();
                }

                iterations += 1;
                if iterations >= MAX_ITERS || start.elapsed() >= budget {
                    return false;
                }
            }

            if !self.has_deferred_update_messages() {
                break;
            }
            self.process_deferred_update_messages();

            iterations += 1;
            if iterations >= MAX_ITERS || start.elapsed() >= budget {
                return false;
            }
        }

        self.set_cursor();

        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Complete));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();

        true
    }

    /// Processes updates and runs style and layout if needed.
    pub(crate) fn process_update_no_paint(&mut self) {
        loop {
            loop {
                self.process_update_messages();
                let needs_style = self.needs_style();
                let needs_layout = self.needs_layout();
                let needs_box_update = self.needs_box_tree_update();
                let needs_box = self.needs_box_tree_commit();
                let has_pending_box_updates =
                    !self.window_state.views_needing_box_tree_update.is_empty();
                if !needs_layout
                    && !needs_style
                    && !needs_box
                    && !has_pending_box_updates
                    && !needs_box_update
                {
                    break;
                }

                if needs_style {
                    self.style();
                }

                if self.needs_layout() {
                    self.layout();
                }

                if self.needs_box_tree_update() {
                    self.update_box_tree_from_layout();
                }

                // Process any pending individual box tree updates after layout
                if !self.window_state.views_needing_box_tree_update.is_empty() {
                    self.process_pending_box_tree_updates();
                }

                if self.needs_box_tree_commit() {
                    self.commit_box_tree();
                }
            }
            if !self.has_deferred_update_messages() {
                break;
            }
            self.process_deferred_update_messages();
        }

        self.set_cursor();

        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Complete));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();
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
                        if let Some(root) = id.try_root() {
                            let msgs = msgs.entry(root).or_default();
                            msgs.push(msg);
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
                    for (id, msg) in removed_central_msgs {
                        if let Some(root) = id.try_root() {
                            let msgs = msgs.entry(root).or_default();
                            msgs.push((id, msg));
                        }
                    }
                });
            }
        });
    }

    pub(crate) fn process_update_messages(&mut self) {
        set_current_view(self.id.root());
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
                    UpdateMessage::RequestStyle(id, reason) => {
                        self.window_state.mark_style_dirty_with(id, reason);
                    }
                    UpdateMessage::RequestLayout => {
                        self.window_state.needs_layout = true;
                    }
                    UpdateMessage::MarkViewLayoutDirty(id) => {
                        let _ = id.mark_view_layout_dirty();
                    }
                    UpdateMessage::RequestBoxTreeUpdate => {
                        self.window_state.needs_box_tree_from_layout = true;
                    }
                    UpdateMessage::RequestBoxTreeUpdateForView(view_id) => {
                        self.window_state
                            .views_needing_box_tree_update
                            .insert(view_id);
                    }
                    UpdateMessage::RequestBoxTreeCommit => {
                        self.window_state.needs_box_tree_commit = true;
                    }
                    UpdateMessage::RequestPaint => {
                        cx.window_state.request_paint = true;
                    }
                    UpdateMessage::Focus(id) => {
                        // because we do not call route, the processing messages event is not sent.
                        // this is desirable because the process messages event will be sent explicitly another time.
                        let keyboard_navigation = cx.window_state.keyboard_navigation;
                        let root_element_id = cx.window_state.root_view_id.get_element_id();
                        GlobalEventCx::new(
                            cx.window_state,
                            root_element_id,
                            Event::Window(WindowEvent::UpdatePhase(
                                UpdatePhaseEvent::ProcessingMessages,
                            )),
                        )
                        .update_focus(id, keyboard_navigation);
                    }
                    UpdateMessage::ClearFocus => {
                        // because we do not call route, the processing messages event is not sent.
                        // this is desirable because the process messages event will be sent explicitly another time.
                        let root_element_id = cx.window_state.root_view_id.get_element_id();
                        GlobalEventCx::new(
                            cx.window_state,
                            root_element_id,
                            Event::Window(WindowEvent::UpdatePhase(
                                UpdatePhaseEvent::ProcessingMessages,
                            )),
                        )
                        .clear_focus();
                    }
                    UpdateMessage::SetPointerCapture {
                        element_id: view_id,
                        pointer_id,
                    } => {
                        cx.window_state.set_pointer_capture(pointer_id, view_id);
                    }
                    UpdateMessage::ReleasePointerCapture {
                        element_id: view_id,
                        pointer_id,
                    } => {
                        cx.window_state.release_pointer_capture(pointer_id, view_id);
                    }
                    UpdateMessage::ScrollTo { id, rect } => {
                        let event = ScrollTo { id, rect };
                        let event = Event::new_custom(event);
                        GlobalEventCx::new(&mut self.window_state, self.id.get_element_id(), event)
                            .route_normal(RouteKind::bubble_from(id), None);
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
                        cx.window_state.user_scale = scale;
                        self.id.request_layout();
                        let scale = cx.window_state.effective_scale();
                        self.paint_state.set_scale(scale);
                    }
                    UpdateMessage::ShowContextMenu { menu, pos } => {
                        let (menu, registry) = menu.build();
                        cx.window_state.context_menu.clear();
                        cx.window_state.update_context_menu(registry);

                        // Queue the context menu to show after this event completes
                        Application::send_proxy_event(UserEvent::ShowContextMenu {
                            window_id: self.window_id,
                            menu: MenuWrapper(menu),
                            pos,
                        });
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
                                    position.x * self.window_state.user_scale,
                                    position.y * self.window_state.user_scale,
                                ));
                            let size = winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
                                size.width * self.window_state.user_scale,
                                size.height * self.window_state.user_scale,
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
                    UpdateMessage::RegisterListener(key, id) => {
                        cx.window_state.listeners.entry(key).or_default().push(id);
                        id.state().borrow_mut().registered_listener_keys.push(key);
                    }
                    UpdateMessage::RemoveListener(key, id) => {
                        if let Some(ids) = cx.window_state.listeners.get_mut(&key) {
                            ids.retain(|v| *v != id);
                        }
                        if let Ok(mut state) = id.state().try_borrow_mut() {
                            state.registered_listener_keys.retain(|k| *k != key);
                        }
                    }
                    UpdateMessage::WindowVisible(visible) => {
                        self.window.set_visible(visible);
                    }
                    UpdateMessage::ViewTransitionAnimComplete(id) => {
                        let num_waiting =
                            id.state().borrow().style_storage.num_waiting_animations.saturating_sub(1);
                        id.state().borrow_mut().style_storage.num_waiting_animations = num_waiting;
                    }
                    UpdateMessage::SetTheme(theme) => {
                        self.set_theme(theme, false);

                        #[cfg(target_os = "windows")]
                        if let Some(new) = theme {
                            self.set_menu_theme_for_windows(new);
                        }
                    }
                    UpdateMessage::RemoveViews(view_ids) => {
                        for view_id in view_ids {
                            cx.window_state.remove_view(view_id);
                        }
                    }
                    UpdateMessage::AddChild {
                        parent_id,
                        mut child,
                    } => {
                        // Resolve scope by looking up parent's scope
                        let scope = parent_id.find_scope().unwrap_or_else(Scope::current);
                        let view = child.build(scope);
                        parent_id.add_child(view);
                        parent_id.request_all();
                    }
                    UpdateMessage::AddChildren {
                        parent_id,
                        mut children,
                    } => {
                        // Resolve scope by looking up parent's scope
                        let scope = parent_id.find_scope().unwrap_or_else(Scope::current);
                        let views = children.build(scope);
                        parent_id.append_children(views);
                        parent_id.request_all();
                    }
                    UpdateMessage::SetupReactiveChildren { mut setup } => {
                        setup.run();
                    }
                    UpdateMessage::RouteEvent {
                        id,
                        event,
                        route_kind: dispatch_kind,
                        triggered_by,
                    } => {
                        let cx = GlobalEventCx::new(&mut self.window_state, id, *event);
                        cx.route_normal(dispatch_kind, triggered_by.as_deref());
                    }
                }
            }
        }
        // After all messages are processed, re-parent any scopes that couldn't find
        // a parent scope earlier (because the view tree wasn't fully assembled yet).
        crate::view::process_pending_scope_reparents();
        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(
            UpdatePhaseEvent::ProcessingMessages,
        ));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();
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
        self.window_state.needs_layout
    }

    fn needs_box_tree_commit(&mut self) -> bool {
        self.window_state.needs_box_tree_commit
            || self.window_state.box_tree.borrow().needs_commit()
    }

    fn needs_box_tree_update(&mut self) -> bool {
        self.window_state.needs_box_tree_from_layout
    }

    fn needs_style(&mut self) -> bool {
        !self.window_state.style_dirty.is_empty()
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
        if self.window_state.needs_cursor_resolution {
            let mut temp = None;
            for hover in self.window_state.hover_state.current_path() {
                if hover.is_view()
                    && let Some(cursor) = hover.owning_id().state().borrow().cursor()
                {
                    temp = Some(cursor);
                }
                // it is important that the node cursors override the widget cursor because non View nodes will have a widget that maps to the parent View that they are associated with
                if let Some(cursor) = self.window_state.element_id_cursors.get(hover) {
                    temp = Some(*cursor);
                }
            }
            self.window_state.needs_cursor_resolution = false;
            self.window_state.cursor = temp;
        }
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
        if cursor != self.window_state.last_cursor_icon {
            self.window.set_cursor(cursor.into());
            self.window_state.last_cursor_icon = cursor;
        }
    }

    fn schedule_repaint(&self) {
        self.window.request_redraw();
    }

    pub(crate) fn destroy(&mut self) {
        self.event(Event::Window(WindowEvent::Closed));
        self.scope.dispose();
        remove_window_id_mapping(&self.id, &self.window_id);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn show_context_menu(&self, menu: MudaMenu, pos: Option<Point>) {
        use dispatch2::DispatchQueue;
        use muda::{
            ContextMenu,
            dpi::{LogicalPosition, Position},
        };
        use raw_window_handle::HasWindowHandle;
        use raw_window_handle::RawWindowHandle;

        if let RawWindowHandle::AppKit(handle) = self.window.window_handle().unwrap().as_raw() {
            let ns_view = handle.ns_view.as_ptr() as usize;
            let scale = self.window_state.user_scale;
            let height = self.size.get_untracked().height;
            let logical_pos = pos.map(|pos| (pos.x * scale, (height - pos.y) * scale));

            struct SendMenu(MudaMenu);
            unsafe impl Send for SendMenu {}
            impl SendMenu {
                unsafe fn show(self, ns_view: usize, logical_pos: Option<(f64, f64)>) {
                    unsafe {
                        self.0.show_context_menu_for_nsview(
                            ns_view as _,
                            logical_pos.map(|(x, y)| Position::Logical(LogicalPosition::new(x, y))),
                        );
                    }
                }
            }

            let menu = SendMenu(menu);
            DispatchQueue::main().exec_async(move || {
                unsafe {
                    menu.show(ns_view, logical_pos);
                };
            });
        }
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn show_context_menu(&self, menu: MudaMenu, pos: Option<Point>) {
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
                            pos.x * self.window_state.user_scale,
                            pos.y * self.window_state.user_scale,
                        ))
                    }),
                );
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn init_menu_for_windows(&self, menu: &MudaMenu) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        if let RawWindowHandle::Win32(handle) = self.window.window_handle().unwrap().as_raw() {
            unsafe {
                let menu_theme = match (
                    self.window_state.theme_overriden,
                    self.window_state.light_dark_theme,
                ) {
                    (false, winit::window::Theme::Light) => MudaMenuTheme::Light,
                    (false, winit::window::Theme::Dark) => MudaMenuTheme::Dark,
                    (true, winit::window::Theme::Light) => MudaMenuTheme::Light,
                    (true, winit::window::Theme::Dark) => MudaMenuTheme::Dark,
                };
                let _ = menu.init_for_hwnd_with_theme(isize::from(handle.hwnd), menu_theme);
                let _ = menu.show_for_hwnd(isize::from(handle.hwnd));
            }
        }
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn set_menu_theme_for_windows(&self, theme: winit::window::Theme) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        if let RawWindowHandle::Win32(handle) = self.window.window_handle().unwrap().as_raw()
            && let Some(menu) = &self.window_menu
        {
            unsafe {
                let menu_theme = match theme {
                    winit::window::Theme::Light => MudaMenuTheme::Light,
                    winit::window::Theme::Dark => MudaMenuTheme::Dark,
                };
                let _ = menu.set_theme_for_hwnd(handle.hwnd.into(), menu_theme);
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
    pub(crate) fn show_context_menu(&self, menu: MudaMenu, pos: Option<Point>) {
        let pos = pos.unwrap_or(self.window_state.last_pointer.0);
        let pos = Point::new(
            pos.x / self.window_state.user_scale,
            pos.y / self.window_state.user_scale,
        );
        self.context_menu.set(Some((menu, pos, false)));
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn menu_action(&mut self, id: &MenuId) {
        set_current_view(self.id);
        if let Some(action) = self.window_state.context_menu.get(id) {
            (*action)();
            self.process_update();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
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
        let floem_ime = match ime {
            Ime::Enabled => ImeEvent::Enabled,
            Ime::Preedit(text, cursor) => ImeEvent::Preedit { text, cursor },
            Ime::Commit(text) => ImeEvent::Commit(text),
            Ime::Disabled => ImeEvent::Disabled,
            Ime::DeleteSurrounding {
                before_bytes,
                after_bytes,
            } => ImeEvent::DeleteSurrounding {
                before_bytes,
                after_bytes,
            },
        };
        self.event(Event::Ime(floem_ime));
    }

    pub(crate) fn modifiers_changed(&mut self, modifiers: Modifiers) {
        let is_altgr = self.modifiers.contains(Modifiers::ALT_GRAPH);
        let mut modifiers: Modifiers = modifiers;
        if is_altgr {
            modifiers.set(Modifiers::ALT_GRAPH, true);
        }
        self.modifiers = modifiers;
    }

    /// Clean up the window's view tree and reactive scope.
    ///
    /// This removes all views from VIEW_STORAGE and disposes the reactive scope,
    /// ensuring proper cleanup for test isolation.
    pub(crate) fn cleanup(&mut self) {
        // Dispose the reactive scope FIRST to clean up effects.
        // This stops any reactive effects from running during cleanup.
        self.scope.dispose();

        // Clear ALL message queues to prevent stale messages from affecting
        // future windows that might reuse the same ViewId slots.
        // We clear all messages, not just those for views we're removing,
        // because the reactive scope disposal above might have queued new messages.
        CENTRAL_UPDATE_MESSAGES.with_borrow_mut(|msgs| {
            msgs.clear();
        });
        UPDATE_MESSAGES.with_borrow_mut(|msgs| {
            msgs.clear();
        });
        CENTRAL_DEFERRED_UPDATE_MESSAGES.with_borrow_mut(|msgs| {
            msgs.clear();
        });
        DEFERRED_UPDATE_MESSAGES.with_borrow_mut(|msgs| {
            msgs.clear();
        });

        // Remove all views starting from the root
        self.window_state.remove_view(self.id);

        // Clear all caches that might hold stale ViewId references.
        // This is crucial for test isolation when tests run on the same thread.
        clear_hit_test_cache();

        // Remove the window from the global window tracking map.
        // This is crucial for test isolation - if not done, the old root ViewId
        // will still be considered a "known root" when the ViewId slot is reused.
        remove_window_id_mapping(&self.id, &self.window_id);
    }
}

pub(crate) fn get_current_view() -> ViewId {
    CURRENT_RUNNING_VIEW_HANDLE
        .with(|running| *running.borrow())
        .expect("view id must have been set before getting")
}
/// Set this view handle to the current running view handle
pub(crate) fn set_current_view(id: ViewId) {
    CURRENT_RUNNING_VIEW_HANDLE.with(|running| {
        *running.borrow_mut() = Some(id);
    });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{Event, WindowEvent, listener},
        views::{Decorators, Empty},
    };
    use std::{cell::Cell, rc::Rc};

    /// Test that we can create a headless WindowHandle.
    #[test]
    fn test_headless_window_handle_creation() {
        let root_id = ViewId::new_root();
        set_current_view(root_id);
        let view = Empty::new().style(|s| s.size(100.0, 100.0));
        let window_handle = WindowHandle::new_headless(root_id, view, Size::new(800.0, 600.0), 1.0);

        // Just verify creation doesn't panic
        assert!(window_handle.window_state.os_scale > 0.0);
    }

    /// Test that headless WindowHandle can dispatch events.
    #[test]
    fn test_headless_event_dispatch() {
        use ui_events::pointer::{
            PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerType,
        };

        let root_id = ViewId::new_root();
        set_current_view(root_id);
        let view = Empty::new().style(|s| s.size(100.0, 100.0));
        let mut window_handle =
            WindowHandle::new_headless(root_id, view, Size::new(800.0, 600.0), 1.0);

        // Create a pointer down event
        let event = Event::Pointer(PointerEvent::Down(PointerButtonEvent {
            state: ui_events::pointer::PointerState {
                position: dpi::PhysicalPosition::new(50.0, 50.0),
                count: 1,
                ..Default::default()
            },
            button: Some(PointerButton::Primary),
            pointer: PointerInfo {
                pointer_id: Some(PointerId::PRIMARY),
                persistent_device_id: None,
                pointer_type: PointerType::Mouse,
            },
        }));

        // Dispatch should not panic
        window_handle.event(event);
    }

    /// Test that headless WindowHandle runs process_update correctly.
    #[test]
    fn test_headless_process_update() {
        use ui_events::pointer::{
            PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerType,
        };

        let root_id = ViewId::new_root();
        set_current_view(root_id);
        let view = Empty::new().style(|s| s.size(100.0, 100.0));
        let mut window_handle =
            WindowHandle::new_headless(root_id, view, Size::new(800.0, 600.0), 1.0);

        // Dispatch pointer down
        window_handle.event(Event::Pointer(PointerEvent::Down(PointerButtonEvent {
            state: ui_events::pointer::PointerState {
                position: dpi::PhysicalPosition::new(50.0, 50.0),
                count: 1,
                ..Default::default()
            },
            button: Some(PointerButton::Primary),
            pointer: PointerInfo {
                pointer_id: Some(PointerId::PRIMARY),
                persistent_device_id: None,
                pointer_type: PointerType::Mouse,
            },
        })));

        // Dispatch pointer up
        window_handle.event(Event::Pointer(PointerEvent::Up(PointerButtonEvent {
            state: ui_events::pointer::PointerState {
                position: dpi::PhysicalPosition::new(50.0, 50.0),
                count: 1,
                ..Default::default()
            },
            button: Some(PointerButton::Primary),
            pointer: PointerInfo {
                pointer_id: Some(PointerId::PRIMARY),
                persistent_device_id: None,
                pointer_type: PointerType::Mouse,
            },
        })));

        // All should complete without panic
    }

    #[test]
    fn test_headless_window_destroy_emits_window_closed() {
        let root_id = ViewId::new_root();
        set_current_view(root_id);

        let closed_count = Rc::new(Cell::new(0));
        let closed_count_for_listener = closed_count.clone();

        let view = Empty::new().style(|s| s.size(100.0, 100.0)).on_event_cont(
            listener::WindowClosed,
            move |_cx, _| {
                closed_count_for_listener.set(closed_count_for_listener.get() + 1);
            },
        );

        let mut window_handle =
            WindowHandle::new_headless(root_id, view, Size::new(800.0, 600.0), 1.0);

        window_handle.event(Event::Window(WindowEvent::CloseRequested));
        assert_eq!(closed_count.get(), 0);

        window_handle.destroy();

        assert_eq!(closed_count.get(), 1);
    }

    #[test]
    fn test_budgeted_update_quiesces_with_unreachable_style_dirty_view() {
        let root_id = ViewId::new_root();
        set_current_view(root_id);
        let view = Empty::new().style(|s| s.size(100.0, 100.0));
        let mut window_handle =
            WindowHandle::new_headless(root_id, view, Size::new(800.0, 600.0), 1.0);

        // Create a view ID that belongs to this root but is not in the tree.
        let orphan = ViewId::new();
        window_handle
            .window_state
            .mark_style_dirty(orphan.get_element_id());

        // Must quiesce immediately instead of repeatedly trying to style an unreachable view.
        let quiescent =
            window_handle.process_update_budgeted(Instant::now(), Duration::from_millis(10));
        assert!(
            quiescent,
            "process_update_budgeted should quiesce when style dirty contains unreachable views"
        );
        assert!(
            window_handle.window_state.style_dirty.is_empty(),
            "unreachable style dirty entries should be drained"
        );
    }
}
