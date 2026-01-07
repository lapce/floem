#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
use std::{cell::RefCell, mem, rc::Rc, sync::Arc};

use crate::platform::menu_types::{Menu as MudaMenu, MenuId};
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
use peniko::kurbo::{Affine, Point, Size};
use winit::{
    cursor::CursorIcon,
    dpi::{LogicalPosition, LogicalSize},
    event::Ime,
    window::{Window, WindowId},
};

use super::state::WindowState;
use super::tracking::{remove_window_id_mapping, store_window_id_mapping};
use crate::event::dropped_file::FileDragEvent;
use crate::event::{GlobalEventCx, ImeEvent, WindowEvent};
use crate::layout::PostLayoutCx;
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
    context::{FrameUpdate, LayoutCx, PaintCx, PaintState, StyleCx, UpdateCx},
    event::{Event, clear_hit_test_cache},
    inspector::{self, Capture, CaptureState, CapturedView, profiler::Profile},
    message::{
        CENTRAL_DEFERRED_UPDATE_MESSAGES, CENTRAL_UPDATE_MESSAGES, CURRENT_RUNNING_VIEW_HANDLE,
        DEFERRED_UPDATE_MESSAGES, UPDATE_MESSAGES, UpdateMessage,
    },
    style::{CursorStyle, Style, StyleSelector},
    theme::default_theme,
    view::ViewId,
    view::stacking::clear_all_stacking_caches,
    view::{IntoView, View},
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
    main_view: ViewId,
    /// Reactive Scope for this `WindowHandle`
    scope: Scope,
    pub(crate) window_state: WindowState,
    pub(crate) paint_state: PaintState,
    size: RwSignal<Size>,
    default_theme: Option<Style>,
    pub(crate) profile: Option<Profile>,
    is_maximized: bool,
    transparent: bool,
    pub(crate) scale: f64,
    pub(crate) modifiers: Modifiers,
    pub(crate) cursor_position: Point,
    pub(crate) window_position: Point,
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
    pub(crate) context_menu: RwSignal<Option<(MudaMenu, Point, bool)>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) window_menu_actions: HashMap<MenuId, Box<dyn Fn()>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) window_menu: Option<MudaMenu>,
    pub(crate) event_reducer: WindowEventReducer,
}

impl Drop for WindowHandle {
    fn drop(&mut self) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            s.box_tree.remove(&self.id);
        })
    }
}

impl WindowHandle {
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
        let scope = Scope::new();
        let window_id = window.id();
        let id = ViewId::new_root();
        let scale = window.scale_factor();
        let size: LogicalSize<f64> = window.surface_size().to_logical(scale);
        let size = Size::new(size.width, size.height);
        let size = scope.create_rw_signal(Size::new(size.width, size.height));
        let os_theme = window.theme();
        // let current_theme = apply_theme.unwrap_or(os_theme.unwrap_or(winit::window::Theme::Light));
        let is_maximized = window.is_maximized();

        set_current_view(id);

        #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
        let context_menu = scope.create_rw_signal(None);

        #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32")))]
        let view = scope.enter(move || {
            let main_view = view_fn(window_id);
            let main_view_id = main_view.id();
            (main_view_id, main_view)
        });

        #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
        let view = scope.enter(move || {
            let main_view = view_fn(window_id);
            let main_view_id = main_view.id();
            (
                main_view_id,
                Stack::new((
                    Container::new(main_view).style(|s| s.size(100.pct(), 100.pct())),
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
                backends,
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

        let window_state = WindowState::new(id, os_theme);

        let mut window_handle = Self {
            window,
            window_id,
            id,
            main_view: main_view_id,
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
            scale,
            modifiers: Modifiers::default(),
            cursor_position: Point::ZERO,
            window_position: Point::ZERO,
            #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
            context_menu,
            #[cfg(not(target_arch = "wasm32"))]
            window_menu_actions: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            window_menu: None,
            event_reducer: WindowEventReducer::default(),
        };
        if paint_state_initialized {
            window_handle.init_renderer(gpu_resources);
        }
        window_handle
            .window_state
            .set_root_size(size.get_untracked());
        window_handle.process_update_no_paint();

        window_handle.window_state.light_dark_theme =
            os_theme.unwrap_or(winit::window::Theme::Light);

        window_handle.event(Event::Window(WindowEvent::ThemeChanged(
            window_handle.window_state.light_dark_theme,
        )));
        window_handle.window_state.mark_dark_mode_changed();
        window_handle.size(size.get_untracked());
        window_handle
    }

    /// Creates a headless WindowHandle for testing purposes.
    ///
    /// This constructor creates a WindowHandle with a MockWindow and no GPU resources,
    /// suitable for testing the event handling and view update logic without a real window.
    ///
    /// # Arguments
    /// * `view` - The root view for this window
    /// * `size` - The virtual window size
    /// * `scale` - The window scale factor (default 1.0)
    pub(crate) fn new_headless(view: impl IntoView, size_val: Size, scale: f64) -> Self {
        use super::mock::MockWindow;

        let scope = Scope::new();
        let mock_window = MockWindow::with_size(size_val.width as u32, size_val.height as u32);
        let window_id = mock_window.id();
        let id = ViewId::new_root();
        let size = scope.create_rw_signal(size_val);
        let os_theme = mock_window.theme();
        let is_maximized = mock_window.is_maximized();

        set_current_view(id);

        // Convert the view
        let main_view = view.into_view();
        let main_view_id = main_view.id();
        let widget: Box<dyn View> = main_view.into_any();

        id.set_children([widget]);

        let window_view = WindowView { id };
        id.set_view(window_view.into_any());

        let window: Arc<dyn Window> = Arc::new(mock_window);
        store_window_id_mapping(id, window_id, &window);

        // Create a paint state that will never initialize (for headless testing)
        // We use a channel that will never receive a value
        let (tx, rx) = std::sync::mpsc::channel();
        drop(tx); // Drop sender so receiver will never receive
        let paint_state = PaintState::new_pending(
            window.clone(),
            rx,
            scale,
            size_val * scale,
            0.0, // font_embolden
        );

        let window_state = WindowState::new(id, os_theme);

        let mut window_handle = Self {
            window,
            window_id,
            id,
            main_view: main_view_id,
            scope,
            paint_state,
            size,
            default_theme: Some(default_theme(window_state.light_dark_theme)),
            window_state,
            is_maximized,
            transparent: false,
            profile: None,
            scale,
            modifiers: Modifiers::default(),
            cursor_position: Point::ZERO,
            window_position: Point::ZERO,
            #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
            context_menu: scope.create_rw_signal(None),
            #[cfg(not(target_arch = "wasm32"))]
            window_menu_actions: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            window_menu: None,
            event_reducer: WindowEventReducer::default(),
        };

        window_handle
            .window_state
            .set_root_size(size.get_untracked());

        window_handle.window_state.light_dark_theme =
            os_theme.unwrap_or(winit::window::Theme::Light);

        // Run initial style and layout passes
        window_handle.process_update_messages();
        // Mark root view as needing style so initial style pass runs compute_combined
        // and populates has_style_selectors for selector detection
        window_handle.id.request_style_recursive();
        window_handle.process_update_messages();
        window_handle.style();
        window_handle.layout();
        window_handle.commit_box_tree();

        window_handle
    }

    pub(crate) fn init_renderer(&mut self, gpu_resources: Option<GpuResources>) {
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
        self.render_frame(gpu_resources);
        self.window.set_visible(true);
    }

    pub fn event(&mut self, event: Event) {
        set_current_view(self.id.root());

        // Check event type for platform-specific context menu handling
        #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
        let is_pointer_down = matches!(&event, Event::Pointer(PointerEvent::Down { .. }));
        #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
        let is_pointer_up = matches!(&event, Event::Pointer(PointerEvent::Up { .. }));

        GlobalEventCx::new(&mut self.window_state).run(event);

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

        self.process_update();
    }

    pub(crate) fn scale(&mut self, scale: f64) {
        self.scale = scale;
        let scale = self.scale * self.window_state.scale;
        self.paint_state.set_scale(scale);
        self.event(Event::Window(WindowEvent::ScaleChanged(scale)));
        self.schedule_repaint();
    }

    pub(crate) fn set_theme(&mut self, theme: Option<winit::window::Theme>, change_from_os: bool) {
        if change_from_os && self.window_state.theme_overriden {
            // if the window theme has been set manually then changes from the os shouldn't do anything
            return;
        }
        self.window_state.mark_dark_mode_changed();
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
        self.id.request_style_recursive();
        self.id.request_all();
        if let Some(theme) = theme {
            self.event(Event::Window(WindowEvent::ThemeChanged(theme)));
        }
    }

    pub(crate) fn size(&mut self, size: Size) {
        self.size.set(size);
        self.window_state.update_screen_size_bp(size);
        self.event(Event::Window(WindowEvent::Resized(size)));
        let scale = self.scale * self.window_state.scale;
        self.paint_state.resize(scale, size * self.scale);
        self.window_state.set_root_size(size);

        let is_maximized = self.window.is_maximized();
        if is_maximized != self.is_maximized {
            self.is_maximized = is_maximized;
            self.event(Event::Window(WindowEvent::MaximizeChanged(is_maximized)));
        }

        self.style();
        self.layout();
        self.commit_box_tree();
        self.process_update();
        self.schedule_repaint();
    }

    pub(crate) fn position(&mut self, point: Point) {
        self.window_position = point;
        self.event(Event::Window(WindowEvent::Moved(point)));
    }

    pub(crate) fn file_drag_event(&mut self, file_drag_event: FileDragEvent) {
        self.event(Event::FileDrag(file_drag_event));
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
        if let PointerEvent::Move(pointer_update) = &pointer_event {
            let pos = pointer_update.current.logical_point();
            if self.cursor_position != pos {
                self.cursor_position = pos;
            }
        }

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
        // Take any pending global recalc (dark mode, responsive changes)
        let global_change = self.window_state.take_global_recalc();

        // Loop until no more views need styling
        // This handles the case where styling a parent marks children dirty
        // (e.g., when inherited properties change)
        loop {
            // Build explicit traversal order
            let traversal = self.window_state.build_style_traversal(self.id);
            if traversal.is_empty() {
                self.window_state.style_dirty.clear();
                self.window_state.view_style_dirty.clear();
                break;
            }

            // Style each view in order, passing the global change for first iteration
            // dbg!(traversal.len());
            for view_id in traversal {
                let cx = &mut StyleCx::new(&mut self.window_state, view_id);
                cx.style_view(view_id, global_change);
            }
            if self.window_state.capture.is_some() {
                // we need to break if capture because when capturing we style all views so no need to loop here.
                // we style all views so that the capture can accurately report how long a full style takes
                break;
            }
        }

        // Clear pending child changes after style pass completes
        self.window_state.pending_child_change.clear();
    }

    fn layout(&mut self) -> Duration {
        let cx = LayoutCx::new(&mut self.window_state);

        let start = Instant::now();
        cx.window_state.compute_layout();
        let taffy_duration = start.elapsed();

        let needs_post: Vec<_> = cx.window_state.needs_post_layout.iter().copied().collect();
        for id in needs_post {
            if let Some(layout) = id.get_layout() {
                let cx = PostLayoutCx::new(cx.window_state, &layout);
                id.view().borrow_mut().post_layout(cx);
            }
        }

        taffy_duration
    }

    fn commit_box_tree(&mut self) -> Duration {
        let start = Instant::now();
        self.window_state.commit_box_tree();
        self.window_state.needs_box_tree_commit = false;

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
                FrameUpdate::Style(id) => {
                    self.window_state.style_dirty.insert(id);
                }
                FrameUpdate::Paint(id) => self.window_state.request_paint(id),
            }
        }
    }

    pub(crate) fn render_frame(&mut self, gpu_resources: Option<GpuResources>) {
        // Processes updates scheduled on this frame.
        self.process_scheduled_updates();

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
            saved_transforms: Vec::new(),
            saved_clips: Vec::new(),
            pending_drag_paint: None,
            gpu_resources,
            window: self.window.clone(),
            #[cfg(feature = "vello")]
            saved_layer_counts: Vec::new(),
            #[cfg(feature = "vello")]
            layer_count: 0,
            record_paint_order: crate::paint::is_paint_order_tracking_enabled(),
        };
        cx.paint_state
            .renderer_mut()
            .begin(cx.window_state.capture.is_some());
        if !self.transparent {
            let scale = cx.window_state.scale;
            let color = self
                .default_theme
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
        // Paint registered overlays above all regular content
        cx.paint_overlays(self.id);
        // Paint drag overlay last to ensure it appears on top of all content
        cx.paint_pending_drag();
        if cx.window_state.capture.is_none() {
            self.window.pre_present_notify();
        }
        cx.paint_state.renderer_mut().finish()
    }

    pub(crate) fn capture(&mut self, gpu_resources: Option<GpuResources>) -> Capture {
        // Capture the view before we run `style` and `layout` to catch missing `request_style`` or
        // `request_layout` flags.
        let root_layout = self.id.get_layout_rect();
        let root = CapturedView::capture(self.id, &mut self.window_state, root_layout);

        self.window_state.capture = Some(CaptureState::default());

        // Trigger painting to create a Vger renderer which can capture the output.
        // This can be expensive so it could skew the paint time measurement.
        self.paint(gpu_resources.clone());

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
        let needs_paint = self.process_update_no_paint();
        if needs_paint {
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
                let needs_style = self.needs_style();
                let needs_layout = self.needs_layout();
                let needs_box = self.needs_box_tree_commit();
                if !needs_layout && !needs_style && !needs_box {
                    break;
                }

                if needs_style {
                    paint = true;
                    self.style();
                }

                if self.needs_layout() {
                    paint = true;
                    self.layout();
                }

                if self.needs_box_tree_commit() {
                    paint = true;
                    self.commit_box_tree();
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
                        let msgs = msgs.entry(id.root()).or_default();
                        msgs.push(msg);
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
                        let msgs = msgs.entry(id.root()).or_default();
                        msgs.push((id, msg));
                    }
                });
            }
        });
    }

    pub(crate) fn process_update_messages(&mut self) {
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
                    UpdateMessage::RequestStyle(id) => {
                        self.window_state.style_dirty.insert(id);
                    }
                    UpdateMessage::RequestViewStyle(id) => {
                        self.window_state.view_style_dirty.insert(id);
                    }
                    UpdateMessage::RequestLayout => {
                        self.window_state.needs_layout = true;
                    }
                    UpdateMessage::RequestBoxTreeCommit => {
                        self.window_state.needs_box_tree_commit = true;
                    }
                    UpdateMessage::RequestPaint => {
                        cx.window_state.request_paint = true;
                    }
                    UpdateMessage::Focus(id) => {
                        GlobalEventCx::new(cx.window_state).update_focus(id, false);
                    }
                    UpdateMessage::ClearFocus => {
                        GlobalEventCx::new(cx.window_state).update_focus_from_path(&[], false);
                    }
                    UpdateMessage::Active(id) => {
                        let old = cx.window_state.active;
                        cx.window_state.active = Some(id);

                        if let Some(old_id) = old.and_then(|id| id.exact_view_id()) {
                            // To remove the styles applied by the Active selector
                            // Use selector-aware method to only update views with :active styles
                            if cx
                                .window_state
                                .has_style_for_sel(old_id, StyleSelector::Active)
                            {
                                old_id.request_style_for_selector_recursive(StyleSelector::Active);
                            }
                        }

                        if let Some(id) = id.exact_view_id() {
                            if cx.window_state.has_style_for_sel(id, StyleSelector::Active) {
                                id.request_style_for_selector_recursive(StyleSelector::Active);
                            }
                        }
                    }
                    UpdateMessage::ClearActive => {
                        cx.window_state.active = None;
                    }
                    UpdateMessage::SetPointerCapture {
                        view_id,
                        pointer_id,
                    } => {
                        cx.window_state.set_pointer_capture(pointer_id, view_id);
                    }
                    UpdateMessage::ReleasePointerCapture {
                        view_id,
                        pointer_id,
                    } => {
                        cx.window_state.release_pointer_capture(pointer_id, view_id);
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
                        self.show_context_menu(menu, pos);
                    }
                    #[cfg(not(target_arch = "wasm32"))]
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
                    UpdateMessage::NeedsPostLayout(id) => {
                        self.window_state.needs_post_layout.insert(id);
                    }
                }
            }
        }
        // After all messages are processed, re-parent any scopes that couldn't find
        // a parent scope earlier (because the view tree wasn't fully assembled yet).
        crate::view::process_pending_scope_reparents();
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
    }

    fn needs_style(&mut self) -> bool {
        !self.window_state.style_dirty.is_empty() || !self.window_state.view_style_dirty.is_empty()
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
                if let Some(cursor) = hover
                    .exact_view_id()
                    .and_then(|id| id.state().borrow().cursor())
                {
                    temp = Some(cursor);
                }
                // it is important that the node cursors override the widget cursor because non View nodes will have a widget that maps to the parent View that they are associated with
                if let Some(cursor) = self.window_state.visual_id_cursors.get(hover) {
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
    fn show_context_menu(&self, menu: MudaMenu, pos: Option<Point>) {
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
    fn show_context_menu(&self, menu: MudaMenu, pos: Option<Point>) {
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

        if let RawWindowHandle::Win32(handle) = self.window.window_handle().unwrap().as_raw() {
            if let Some(menu) = &self.window_menu {
                unsafe {
                    let menu_theme = match theme {
                        winit::window::Theme::Light => MudaMenuTheme::Light,
                        winit::window::Theme::Dark => MudaMenuTheme::Dark,
                    };
                    let _ = menu.set_theme_for_hwnd(handle.hwnd.into(), menu_theme);
                }
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
    fn show_context_menu(&self, menu: MudaMenu, pos: Option<Point>) {
        let pos = pos.unwrap_or(self.cursor_position);
        let pos = Point::new(
            pos.x / self.window_state.scale,
            pos.y / self.window_state.scale,
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
        clear_all_stacking_caches();

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
    // dbg!(id.0);
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
    use crate::views::{Decorators, Empty};

    /// Test that we can create a headless WindowHandle.
    #[test]
    fn test_headless_window_handle_creation() {
        let view = Empty::new().style(|s| s.size(100.0, 100.0));
        let window_handle = WindowHandle::new_headless(view, Size::new(800.0, 600.0), 1.0);

        // Just verify creation doesn't panic
        assert!(window_handle.scale > 0.0);
    }

    /// Test that headless WindowHandle can dispatch events.
    #[test]
    fn test_headless_event_dispatch() {
        use crate::event::Event;
        use ui_events::pointer::{
            PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerType,
        };

        let view = Empty::new().style(|s| s.size(100.0, 100.0));
        let mut window_handle = WindowHandle::new_headless(view, Size::new(800.0, 600.0), 1.0);

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
        use crate::event::Event;
        use ui_events::pointer::{
            PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerType,
        };

        let view = Empty::new().style(|s| s.size(100.0, 100.0));
        let mut window_handle = WindowHandle::new_headless(view, Size::new(800.0, 600.0), 1.0);

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
}
