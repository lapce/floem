use std::{
    cell::RefCell,
    mem,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use floem_reactive::{with_scope, RwSignal, Scope};
use floem_renderer::Renderer;
use floem_winit::{
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{CursorIcon, WindowId},
};
use image::DynamicImage;
use peniko::kurbo::{Affine, Point, Rect, Size, Vec2};

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use crate::unit::UnitExt;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use crate::views::{container, stack};
use crate::{
    animate::{AnimPropKind, AnimUpdateMsg, AnimValue, AnimatedProp, SizeUnit},
    app_state::AppState,
    context::{
        ComputeLayoutCx, EventCx, FrameUpdate, LayoutCx, PaintCx, PaintState, StyleCx, UpdateCx,
    },
    event::{Event, EventListener},
    id::ViewId,
    inspector::{self, Capture, CaptureState, CapturedView},
    keyboard::{KeyEvent, Modifiers},
    menu::Menu,
    nav::view_arrow_navigation,
    pointer::{PointerButton, PointerInputEvent, PointerMoveEvent, PointerWheelEvent},
    profiler::Profile,
    style::{CursorStyle, Style, StyleSelector},
    theme::{default_theme, Theme},
    update::{
        UpdateMessage, ANIM_UPDATE_MESSAGES, CENTRAL_DEFERRED_UPDATE_MESSAGES,
        CENTRAL_UPDATE_MESSAGES, CURRENT_RUNNING_VIEW_HANDLE, DEFERRED_UPDATE_MESSAGES,
        UPDATE_MESSAGES,
    },
    view::{default_compute_layout, view_tab_navigation, IntoView, View},
    view_state::ChangeFlags,
    views::Decorators,
};

/// The top-level window handle that owns the winit Window.
/// Meant only for use with the root view of the application.
/// Owns the `AppState` and is responsible for
/// - processing all requests to update the AppState from the reactive system
/// - processing all requests to update the animation state from the reactive system
/// - requesting a new animation frame from the backend
pub(crate) struct WindowHandle {
    pub(crate) window: Option<Arc<floem_winit::window::Window>>,
    window_id: WindowId,
    id: ViewId,
    main_view: ViewId,
    /// Reactive Scope for this WindowHandle
    scope: Scope,
    app_state: AppState,
    paint_state: PaintState,
    size: RwSignal<Size>,
    theme: Option<Theme>,
    pub(crate) profile: Option<Profile>,
    os_theme: RwSignal<Option<floem_winit::window::Theme>>,
    is_maximized: bool,
    transparent: bool,
    pub(crate) scale: f64,
    pub(crate) modifiers: Modifiers,
    pub(crate) cursor_position: Point,
    pub(crate) window_position: Point,
    pub(crate) last_pointer_down: Option<(u8, Point, Instant)>,
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub(crate) context_menu: RwSignal<Option<(Menu, Point)>>,
}

impl WindowHandle {
    pub(crate) fn new(
        window: floem_winit::window::Window,
        view_fn: impl FnOnce(floem_winit::window::WindowId) -> Box<dyn View> + 'static,
        transparent: bool,
        apply_default_theme: bool,
    ) -> Self {
        let scope = Scope::new();
        let window_id = window.id();
        let id = ViewId::new();
        let scale = window.scale_factor();
        let size: LogicalSize<f64> = window.inner_size().to_logical(scale);
        let size = Size::new(size.width, size.height);
        let size = scope.create_rw_signal(Size::new(size.width, size.height));
        let theme = scope.create_rw_signal(window.theme());
        let is_maximized = window.is_maximized();

        set_current_view(id);

        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let context_menu = scope.create_rw_signal(None);

        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
        let view = with_scope(scope, move || view_fn(window_id));

        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let view = with_scope(scope, move || {
            stack((
                container(view_fn(window_id)).style(|s| s.size(100.pct(), 100.pct())),
                context_menu_view(scope, window_id, context_menu, size),
            ))
            .style(|s| s.size(100.pct(), 100.pct()))
            .into_any()
        });

        let widget = view;
        let main_id = widget.id();
        id.set_children(vec![widget]);

        let view = WindowView { id };
        id.set_view(view.into_any());

        let window = Arc::new(window);
        let paint_state = PaintState::new(window.clone(), scale, size.get_untracked() * scale);
        let mut window_handle = Self {
            window: Some(window),
            window_id,
            id,
            main_view: main_id,
            scope,
            app_state: AppState::new(id),
            paint_state,
            size,
            theme: apply_default_theme.then(default_theme),
            os_theme: theme,
            is_maximized,
            transparent,
            profile: None,
            scale,
            modifiers: Modifiers::default(),
            cursor_position: Point::ZERO,
            window_position: Point::ZERO,
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            context_menu,
            last_pointer_down: None,
        };
        window_handle.app_state.set_root_size(size.get_untracked());
        if let Some(theme) = theme.get_untracked() {
            window_handle.event(Event::ThemeChanged(theme));
        }
        window_handle
    }

    pub fn event(&mut self, event: Event) {
        set_current_view(self.id);
        let event = event.scale(self.app_state.scale);

        let mut cx = EventCx {
            app_state: &mut self.app_state,
        };

        let is_pointer_move = matches!(&event, Event::PointerMove(_));
        let (was_hovered, was_dragging_over) = if is_pointer_move {
            cx.app_state.cursor = None;
            let was_hovered = std::mem::take(&mut cx.app_state.hovered);
            let was_dragging_over = std::mem::take(&mut cx.app_state.dragging_over);

            (Some(was_hovered), Some(was_dragging_over))
        } else {
            (None, None)
        };

        let is_pointer_down = matches!(&event, Event::PointerDown(_));
        let was_focused = if is_pointer_down {
            cx.app_state.clicking.clear();
            cx.app_state.focus.take()
        } else {
            cx.app_state.focus
        };

        if event.needs_focus() {
            let mut processed = false;

            if !processed {
                if let Some(id) = cx.app_state.focus {
                    processed |= cx
                        .unconditional_view_event(id, event.clone(), true)
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
                            view_tab_navigation(self.id, cx.app_state, backwards);
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
                                view_arrow_navigation(name, cx.app_state, self.id);
                            }
                        }
                    }

                    let keyboard_trigger_end = cx.app_state.keyboard_navigation
                        && event.is_keyboard_trigger()
                        && matches!(event, Event::KeyUp(_));
                    if keyboard_trigger_end {
                        if let Some(id) = cx.app_state.active {
                            // To remove the styles applied by the Active selector
                            if cx.app_state.has_style_for_sel(id, StyleSelector::Active) {
                                id.request_style_recursive();
                            }

                            cx.app_state.active = None;
                        }
                    }
                }
            }
        } else if cx.app_state.active.is_some() && event.is_pointer() {
            if cx.app_state.is_dragging() {
                cx.unconditional_view_event(self.id, event.clone(), false);
            }

            let id = cx.app_state.active.unwrap();

            {
                let window_origin = id.state().borrow().window_origin;
                let layout = id.get_layout().unwrap_or_default();
                let viewport = id.state().borrow().viewport.unwrap_or_default();
                cx.unconditional_view_event(
                    id,
                    event.clone().offset((
                        window_origin.x - layout.location.x as f64 + viewport.x0,
                        window_origin.y - layout.location.y as f64 + viewport.y0,
                    )),
                    true,
                );
            }

            if let Event::PointerUp(_) = &event {
                // To remove the styles applied by the Active selector
                if cx.app_state.has_style_for_sel(id, StyleSelector::Active) {
                    id.request_style_recursive();
                }

                cx.app_state.active = None;
            }
        } else {
            cx.unconditional_view_event(self.id, event.clone(), false);
        }

        if let Event::PointerUp(_) = &event {
            cx.app_state.drag_start = None;
        }
        if is_pointer_move {
            let hovered = &cx.app_state.hovered.clone();
            for id in was_hovered.unwrap().symmetric_difference(hovered) {
                let view_state = id.state();
                if view_state.borrow().animation.is_some()
                    || view_state
                        .borrow()
                        .has_style_selectors
                        .has(StyleSelector::Hover)
                    || view_state
                        .borrow()
                        .has_style_selectors
                        .has(StyleSelector::Active)
                {
                    id.request_style_recursive();
                }
                if hovered.contains(id) {
                    id.apply_event(&EventListener::PointerEnter, &event);
                } else {
                    cx.unconditional_view_event(*id, Event::PointerLeave, true);
                }
            }
            let dragging_over = &cx.app_state.dragging_over.clone();
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
        if was_focused != cx.app_state.focus {
            cx.app_state.focus_changed(was_focused, cx.app_state.focus);
        }

        if is_pointer_down {
            for id in cx.app_state.clicking.clone() {
                if cx.app_state.has_style_for_sel(id, StyleSelector::Active) {
                    id.request_style_recursive();
                }
            }
        }
        if matches!(&event, Event::PointerUp(_)) {
            for id in cx.app_state.clicking.clone() {
                if cx.app_state.has_style_for_sel(id, StyleSelector::Active) {
                    id.request_style_recursive();
                }
            }
            cx.app_state.clicking.clear();
        }

        self.process_update();
    }

    pub(crate) fn scale(&mut self, scale: f64) {
        self.scale = scale;
        let scale = self.scale * self.app_state.scale;
        self.paint_state.set_scale(scale);
        self.schedule_repaint();
    }

    pub(crate) fn os_theme_changed(&mut self, theme: floem_winit::window::Theme) {
        self.os_theme.set(Some(theme));
        self.event(Event::ThemeChanged(theme));
    }

    pub(crate) fn size(&mut self, size: Size) {
        self.size.set(size);
        self.app_state.update_screen_size_bp(size);
        self.event(Event::WindowResized(size));
        let scale = self.scale * self.app_state.scale;
        self.paint_state.resize(scale, size * self.scale);
        self.app_state.set_root_size(size);

        if let Some(window) = self.window.as_ref() {
            let is_maximized = window.is_maximized();
            if is_maximized != self.is_maximized {
                self.is_maximized = is_maximized;
                self.event(Event::WindowMaximizeChanged(is_maximized));
            }
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

    pub(crate) fn key_event(&mut self, key_event: floem_winit::event::KeyEvent) {
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

    pub(crate) fn pointer_move(&mut self, pos: Point) {
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
            app_state: &mut self.app_state,
        };
        let was_hovered = std::mem::take(&mut cx.app_state.hovered);
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
                || view_state.borrow().animation.is_some()
            {
                id.request_style_recursive();
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

    pub(crate) fn mouse_input(&mut self, button: MouseButton, state: ElementState) {
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

    pub(crate) fn focused(&mut self, focused: bool) {
        if focused {
            self.event(Event::WindowGotFocus);
        } else {
            self.event(Event::WindowLostFocus);
        }
    }

    fn style(&mut self) {
        let mut cx = StyleCx::new(&mut self.app_state, self.id);
        if let Some(theme) = &self.theme {
            cx.current = theme.style.clone();
        }
        cx.style_view(self.id);
    }

    fn layout(&mut self) -> Duration {
        let mut cx = LayoutCx::new(&mut self.app_state);

        cx.app_state_mut().root = {
            let view = self.id.view();
            let mut view = view.borrow_mut();
            Some(cx.layout_view(view.as_mut()))
        };

        let start = Instant::now();
        cx.app_state_mut().compute_layout();
        let taffy_duration = Instant::now().saturating_duration_since(start);

        self.compute_layout();

        taffy_duration
    }

    fn compute_layout(&mut self) {
        self.app_state.request_compute_layout = false;
        let viewport = (self.app_state.root_size / self.app_state.scale).to_rect();
        let mut cx = ComputeLayoutCx::new(&mut self.app_state, viewport);
        cx.compute_view_layout(self.id);
    }

    pub(crate) fn render_frame(&mut self) {
        // Processes updates scheduled on this frame.
        for update in mem::take(&mut self.app_state.scheduled_updates) {
            match update {
                FrameUpdate::Style(id) => id.request_style(),
                FrameUpdate::Layout(id) => id.request_layout(),
                FrameUpdate::Paint(id) => self.app_state.request_paint(id),
            }
        }

        self.process_update_no_paint();
        self.paint();

        // Request a new frame if there's any scheduled updates.
        if !self.app_state.scheduled_updates.is_empty() {
            self.schedule_repaint();
        }
    }

    pub fn paint(&mut self) -> Option<DynamicImage> {
        let mut cx = PaintCx {
            app_state: &mut self.app_state,
            paint_state: &mut self.paint_state,
            transform: Affine::IDENTITY,
            clip: None,
            z_index: None,
            saved_transforms: Vec::new(),
            saved_clips: Vec::new(),
            saved_z_indexes: Vec::new(),
        };
        cx.paint_state
            .renderer
            .begin(cx.app_state.capture.is_some());
        if !self.transparent {
            let scale = cx.app_state.scale;
            let color = self
                .theme
                .as_ref()
                .map(|theme| theme.background)
                .unwrap_or(peniko::Color::WHITE);
            // fill window with default white background if it's not transparent
            cx.fill(
                &self
                    .size
                    .get_untracked()
                    .to_rect()
                    .scale_from_origin(1.0 / scale)
                    .expand(),
                color,
                0.0,
            );
        }
        cx.paint_view(self.id);
        if let Some(window) = self.window.as_ref() {
            if cx.app_state.capture.is_none() {
                window.pre_present_notify();
            }
        }
        cx.paint_state.renderer.finish()
    }

    pub(crate) fn capture(&mut self) -> Capture {
        // Capture the view before we run `style` and `layout` to catch missing `request_style`` or
        // `request_layout` flags.
        let root_layout = self.id.layout_rect();
        let root = CapturedView::capture(self.id, &mut self.app_state, root_layout);

        self.app_state.capture = Some(CaptureState::default());

        // Trigger painting to create a Vger renderer which can capture the output.
        // This can be expensive so it could skew the paint time measurement.
        self.paint();

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
        let window = self.paint().map(Rc::new);
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
            window_size: self.size.get_untracked() / self.app_state.scale,
            scale: self.scale * self.app_state.scale,
            root: Rc::new(root),
            state: self.app_state.capture.take().unwrap(),
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
            self.process_update_messages();
            if !self.needs_layout()
                && !self.needs_style()
                && !self.has_deferred_update_messages()
                && !self.has_anim_update_messages()
                && !self.app_state.request_compute_layout
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

            if self.app_state.request_compute_layout {
                self.compute_layout();
            }

            self.process_deferred_update_messages();
            self.process_anim_update_messages();
        }

        self.set_cursor();

        // TODO: This should only use `self.app_state.request_paint)`
        paint || mem::take(&mut self.app_state.request_paint)
    }

    fn process_central_messages(&self) {
        CENTRAL_UPDATE_MESSAGES.with_borrow_mut(|central_msgs| {
            if !central_msgs.is_empty() {
                UPDATE_MESSAGES.with_borrow_mut(|msgs| {
                    let central_msgs = std::mem::take(&mut *central_msgs);
                    for (id, msg) in central_msgs {
                        if let Some(root) = id.root() {
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
                    let central_msgs = std::mem::take(&mut *central_msgs.borrow_mut());
                    for (id, msg) in central_msgs {
                        if let Some(root) = id.root() {
                            let msgs = msgs.entry(root).or_default();
                            msgs.push((id, msg));
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
                    app_state: &mut self.app_state,
                };
                match msg {
                    UpdateMessage::RequestPaint => {
                        cx.app_state.request_paint = true;
                    }
                    UpdateMessage::Focus(id) => {
                        if cx.app_state.focus != Some(id) {
                            let old = cx.app_state.focus;
                            cx.app_state.focus = Some(id);
                            cx.app_state.focus_changed(old, cx.app_state.focus);
                        }
                    }
                    UpdateMessage::ClearFocus(id) => {
                        cx.app_state.clear_focus();
                        cx.app_state.focus_changed(Some(id), None);
                    }
                    UpdateMessage::Active(id) => {
                        let old = cx.app_state.active;
                        cx.app_state.active = Some(id);

                        if let Some(old_id) = old {
                            // To remove the styles applied by the Active selector
                            if cx
                                .app_state
                                .has_style_for_sel(old_id, StyleSelector::Active)
                            {
                                old_id.request_style_recursive();
                            }
                        }

                        if cx.app_state.has_style_for_sel(id, StyleSelector::Active) {
                            id.request_style_recursive();
                        }
                    }
                    UpdateMessage::ClearActive(id) => {
                        if Some(id) == cx.app_state.active {
                            cx.app_state.active = None;
                        }
                    }
                    UpdateMessage::ScrollTo { id, rect } => {
                        self.id
                            .view()
                            .borrow_mut()
                            .scroll_to(cx.app_state, id, rect);
                    }
                    UpdateMessage::Disabled { id, is_disabled } => {
                        if is_disabled {
                            cx.app_state.disabled.insert(id);
                            cx.app_state.hovered.remove(&id);
                        } else {
                            cx.app_state.disabled.remove(&id);
                        }
                        id.request_style_recursive();
                    }
                    UpdateMessage::State { id, state } => {
                        let view = id.view();
                        view.borrow_mut().update(&mut cx, state);
                    }
                    UpdateMessage::KeyboardNavigable { id } => {
                        cx.app_state.keyboard_navigable.insert(id);
                    }
                    UpdateMessage::Draggable { id } => {
                        cx.app_state.draggable.insert(id);
                    }
                    UpdateMessage::DragWindow => {
                        if let Some(window) = self.window.as_ref() {
                            let _ = window.drag_window();
                        }
                    }
                    UpdateMessage::FocusWindow => {
                        if let Some(window) = self.window.as_ref() {
                            window.focus_window();
                        }
                    }
                    UpdateMessage::DragResizeWindow(direction) => {
                        if let Some(window) = self.window.as_ref() {
                            let _ = window.drag_resize_window(direction);
                        }
                    }
                    UpdateMessage::ToggleWindowMaximized => {
                        if let Some(window) = self.window.as_ref() {
                            window.set_maximized(!window.is_maximized());
                        }
                    }
                    UpdateMessage::SetWindowMaximized(maximized) => {
                        if let Some(window) = self.window.as_ref() {
                            window.set_maximized(maximized);
                        }
                    }
                    UpdateMessage::MinimizeWindow => {
                        if let Some(window) = self.window.as_ref() {
                            window.set_minimized(true);
                        }
                    }
                    UpdateMessage::SetWindowDelta(delta) => {
                        if let Some(window) = self.window.as_ref() {
                            let pos = self.window_position + delta;
                            window.set_outer_position(floem_winit::dpi::Position::Logical(
                                floem_winit::dpi::LogicalPosition::new(pos.x, pos.y),
                            ));
                        }
                    }
                    UpdateMessage::Animation { id, animation } => {
                        let view_state = id.state();
                        if let Some(ref listener) = animation.on_create_listener {
                            listener(animation.id)
                        }
                        view_state.borrow_mut().animation = Some(animation);
                        id.request_style();
                    }
                    UpdateMessage::WindowScale(scale) => {
                        cx.app_state.scale = scale;
                        self.id.request_layout();
                        let scale = self.scale * cx.app_state.scale;
                        self.paint_state.set_scale(scale);
                    }
                    UpdateMessage::ShowContextMenu { menu, pos } => {
                        let mut menu = menu.popup();
                        let platform_menu = menu.platform_menu();
                        cx.app_state.context_menu.clear();
                        cx.app_state.update_context_menu(&mut menu);
                        #[cfg(target_os = "macos")]
                        self.show_context_menu(platform_menu, pos);
                        #[cfg(target_os = "windows")]
                        self.show_context_menu(platform_menu, pos);
                        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                        self.show_context_menu(menu, platform_menu, pos);
                    }
                    UpdateMessage::WindowMenu { menu } => {
                        // let platform_menu = menu.platform_menu();
                        self.update_window_menu(menu);
                        // self.handle.set_menu(platform_menu);
                    }
                    UpdateMessage::SetWindowTitle { title } => {
                        if let Some(window) = self.window.as_ref() {
                            window.set_title(&title);
                        }
                    }
                    UpdateMessage::SetImeAllowed { allowed } => {
                        if let Some(window) = self.window.as_ref() {
                            window.set_ime_allowed(allowed);
                        }
                    }
                    UpdateMessage::SetImeCursorArea { position, size } => {
                        if let Some(window) = self.window.as_ref() {
                            window.set_ime_cursor_area(
                                floem_winit::dpi::Position::Logical(
                                    floem_winit::dpi::LogicalPosition::new(
                                        position.x * self.app_state.scale,
                                        position.y * self.app_state.scale,
                                    ),
                                ),
                                floem_winit::dpi::Size::Logical(
                                    floem_winit::dpi::LogicalSize::new(
                                        size.width * self.app_state.scale,
                                        size.height * self.app_state.scale,
                                    ),
                                ),
                            );
                        }
                    }
                    UpdateMessage::Inspect => {
                        inspector::capture(self.window_id);
                    }
                    UpdateMessage::AddOverlay { id, position, view } => {
                        let scope = self.scope.create_child();

                        let view = with_scope(scope, view);
                        let child = view.id();
                        id.set_children(vec![view]);

                        let view = OverlayView {
                            id,
                            position,
                            child,
                            size: Size::ZERO,
                            parent_size: Size::ZERO,
                            window_origin: Point::ZERO,
                        };
                        self.id.add_child(
                            view.on_cleanup(move || {
                                scope.dispose();
                            })
                            .into_any(),
                        );
                        self.id.request_all();
                    }
                    UpdateMessage::RemoveOverlay { id } => {
                        cx.app_state.remove_view(id);
                        self.id.request_all();
                    }
                    UpdateMessage::WindowVisible(visible) => {
                        if let Some(window) = self.window.as_ref() {
                            window.set_visible(visible);
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
            app_state: &mut self.app_state,
        };
        for (id, state) in msgs {
            let view = id.view();
            view.borrow_mut().update(&mut cx, state);
        }
    }

    fn process_anim_update_messages(&mut self) {
        let msgs: Vec<AnimUpdateMsg> = ANIM_UPDATE_MESSAGES.with(|msgs| {
            let mut msgs = msgs.borrow_mut();
            let len = msgs.len();
            msgs.drain(0..len).collect()
        });

        for msg in msgs {
            match msg {
                AnimUpdateMsg::Prop {
                    id: anim_id,
                    kind,
                    val,
                } => {
                    let view_id = self.app_state.get_view_id_by_anim_id(anim_id);
                    self.process_update_anim_prop(view_id, kind, val);
                }
                AnimUpdateMsg::Resume(anim_id) => {
                    let view_id = self.app_state.get_view_id_by_anim_id(anim_id);
                    if let Some(anim) = view_id.state().borrow_mut().animation.as_mut() {
                        anim.resume();
                        view_id.request_style();
                    }
                }
                AnimUpdateMsg::Pause(anim_id) => {
                    let view_id = self.app_state.get_view_id_by_anim_id(anim_id);
                    if let Some(anim) = view_id.state().borrow_mut().animation.as_mut() {
                        anim.pause();
                    }
                }
                AnimUpdateMsg::Start(anim_id) => {
                    let view_id = self.app_state.get_view_id_by_anim_id(anim_id);
                    if let Some(anim) = view_id.state().borrow_mut().animation.as_mut() {
                        anim.start();
                        view_id.request_style();
                    }
                }
                AnimUpdateMsg::Stop(anim_id) => {
                    let view_id = self.app_state.get_view_id_by_anim_id(anim_id);
                    if let Some(anim) = view_id.state().borrow_mut().animation.as_mut() {
                        anim.stop();
                        view_id.request_style();
                    }
                }
            }
        }
    }

    fn process_update_anim_prop(&mut self, view_id: ViewId, kind: AnimPropKind, val: AnimValue) {
        let layout = view_id.get_layout().unwrap_or_default();
        let view_state = view_id.state();
        let prop = match kind {
            AnimPropKind::Scale => todo!(),
            AnimPropKind::Width => {
                let width = layout.size.width;
                AnimatedProp::Width {
                    from: width as f64,
                    to: val.get_f64(),
                    unit: SizeUnit::Px,
                }
            }
            AnimPropKind::Height => {
                let height = layout.size.height;
                AnimatedProp::Width {
                    from: height as f64,
                    to: val.get_f64(),
                    unit: SizeUnit::Px,
                }
            }
            AnimPropKind::Prop { prop } => {
                //TODO:  get from cx
                let from = view_state
                    .borrow()
                    .combined_style
                    .map
                    .get(&prop.key)
                    .cloned()
                    .unwrap_or_else(|| (prop.info().default_as_any)());
                AnimatedProp::Prop {
                    prop,
                    from,
                    to: val.get_any(),
                }
            }
        };

        // Overrides the old value
        // TODO: logic based on the old val to make the animation smoother when overriding an old
        // animation that was in progress
        if let Some(anim) = view_state.borrow_mut().animation.as_mut() {
            anim.props_mut().insert(kind, prop);
            anim.start();
        }

        view_id.request_style();
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

    fn has_anim_update_messages(&mut self) -> bool {
        ANIM_UPDATE_MESSAGES.with(|m| !m.borrow().is_empty())
    }

    fn update_window_menu(&mut self, _menu: Menu) {
        // if let Some(action) = menu.item.action.take() {
        //     self.window_menu.insert(menu.item.id as u32, action);
        // }
        // for child in menu.children {
        //     match child {
        //         crate::menu::MenuEntry::Separator => {}
        //         crate::menu::MenuEntry::Item(mut item) => {
        //             if let Some(action) = item.action.take() {
        //                 self.window_menu.insert(item.id as u32, action);
        //             }
        //         }
        //         crate::menu::MenuEntry::SubMenu(m) => {
        //             self.update_window_menu(m);
        //         }
        //     }
        // }
    }

    fn set_cursor(&mut self) {
        let cursor = match self.app_state.cursor {
            Some(CursorStyle::Default) => CursorIcon::Default,
            Some(CursorStyle::Pointer) => CursorIcon::Pointer,
            Some(CursorStyle::Text) => CursorIcon::Text,
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
        if cursor != self.app_state.last_cursor {
            if let Some(window) = self.window.as_ref() {
                window.set_cursor_icon(cursor);
            }
            self.app_state.last_cursor = cursor;
        }
    }

    fn schedule_repaint(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(crate) fn destroy(&mut self) {
        self.event(Event::WindowClosed);
        self.scope.dispose();
    }

    #[cfg(target_os = "macos")]
    fn show_context_menu(&self, menu: floem_winit::menu::Menu, pos: Option<Point>) {
        if let Some(window) = self.window.as_ref() {
            {
                use floem_winit::platform::macos::WindowExtMacOS;
                window.show_context_menu(
                    menu,
                    pos.map(|pos| {
                        floem_winit::dpi::Position::Logical(floem_winit::dpi::LogicalPosition::new(
                            pos.x * self.app_state.scale,
                            pos.y * self.app_state.scale,
                        ))
                    }),
                );
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn show_context_menu(&self, menu: floem_winit::menu::Menu, pos: Option<Point>) {
        use floem_winit::platform::windows::WindowExtWindows;

        if let Some(window) = self.window.as_ref() {
            {
                window.show_context_menu(
                    menu,
                    pos.map(|pos| {
                        floem_winit::dpi::Position::Logical(floem_winit::dpi::LogicalPosition::new(
                            pos.x * self.app_state.scale,
                            pos.y * self.app_state.scale,
                        ))
                    }),
                );
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn show_context_menu(
        &self,
        menu: Menu,
        _platform_menu: floem_winit::menu::Menu,
        pos: Option<Point>,
    ) {
        let pos = pos.unwrap_or(self.cursor_position);
        let pos = Point::new(pos.x / self.app_state.scale, pos.y / self.app_state.scale);
        self.context_menu.set(Some((menu, pos)));
    }

    pub(crate) fn menu_action(&mut self, id: usize) {
        set_current_view(self.id);
        if let Some(action) = self.app_state.window_menu.get(&id) {
            (*action)();
            self.process_update();
        } else if let Some(action) = self.app_state.context_menu.get(&id) {
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
    window_id: WindowId,
    context_menu: RwSignal<Option<(Menu, Point)>>,
    window_size: RwSignal<Size>,
) -> impl IntoView {
    use floem_reactive::{create_effect, create_rw_signal};
    use peniko::Color;

    use crate::{
        app::{add_app_update_event, AppUpdateEvent},
        views::{dyn_stack, empty, svg, text},
    };

    #[derive(Clone, PartialEq, Eq, Hash)]
    enum MenuDisplay {
        Separator(usize),
        Item {
            id: Option<u64>,
            enabled: bool,
            title: String,
            children: Option<Vec<MenuDisplay>>,
        },
    }

    fn format_menu(menu: &Menu) -> Vec<MenuDisplay> {
        menu.children
            .iter()
            .enumerate()
            .map(|(s, e)| match e {
                crate::menu::MenuEntry::Separator => MenuDisplay::Separator(s),
                crate::menu::MenuEntry::Item(i) => MenuDisplay::Item {
                    id: Some(i.id),
                    enabled: i.enabled,
                    title: i.title.clone(),
                    children: None,
                },
                crate::menu::MenuEntry::SubMenu(m) => MenuDisplay::Item {
                    id: None,
                    enabled: m.item.enabled,
                    title: m.item.title.clone(),
                    children: Some(format_menu(m)),
                },
            })
            .collect()
    }

    let context_menu_items = cx.create_memo(move |_| {
        context_menu.with(|menu| {
            menu.as_ref()
                .map(|(menu, _): &(Menu, Point)| format_menu(menu))
        })
    });
    let context_menu_size = cx.create_rw_signal(Size::ZERO);
    let focus_count = cx.create_rw_signal(0);

    fn view_fn(
        window_id: WindowId,
        menu: MenuDisplay,
        context_menu: RwSignal<Option<(Menu, Point)>>,
        focus_count: RwSignal<i32>,
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
                            text(title),
                            svg(|| submenu_svg.to_string()).style(move |s| {
                                s.size(20.0, 20.0)
                                    .color(Color::rgb8(201, 201, 201))
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
                        .on_click_stop(move |_| {
                            context_menu.set(None);
                            focus_count.set(0);
                            if let Some(id) = id {
                                add_app_update_event(AppUpdateEvent::MenuAction {
                                    window_id,
                                    action_id: id as usize,
                                });
                            }
                        })
                        .on_secondary_click_stop(move |_| {
                            context_menu.set(None);
                            focus_count.set(0);
                            if let Some(id) = id {
                                add_app_update_event(AppUpdateEvent::MenuAction {
                                    window_id,
                                    action_id: id as usize,
                                });
                            }
                        })
                        .disabled(move || !enabled)
                        .style(|s| {
                            s.width(100.pct())
                                .min_width(100.pct())
                                .padding_horiz(20.0)
                                .justify_between()
                                .items_center()
                                .hover(|s| {
                                    s.border_radius(10.0).background(Color::rgb8(65, 65, 65))
                                })
                                .active(|s| {
                                    s.border_radius(10.0).background(Color::rgb8(92, 92, 92))
                                })
                                .disabled(|s| s.color(Color::rgb8(92, 92, 92)))
                        }),
                        dyn_stack(
                            move || children.clone().unwrap_or_default(),
                            move |s| s.clone(),
                            move |menu| {
                                view_fn(
                                    window_id,
                                    menu,
                                    context_menu,
                                    focus_count,
                                    on_child_submenu,
                                )
                            },
                        )
                        .keyboard_navigatable()
                        .on_event_stop(EventListener::FocusGained, move |_| {
                            focus_count.update(|count| {
                                *count += 1;
                            });
                        })
                        .on_event_stop(EventListener::FocusLost, move |_| {
                            let count = focus_count
                                .try_update(|count| {
                                    *count -= 1;
                                    *count
                                })
                                .unwrap();
                            if count < 1 {
                                context_menu.set(None);
                            }
                        })
                        .on_event_stop(EventListener::KeyDown, move |event| {
                            if let Event::KeyDown(event) = event {
                                if event.key.logical_key == Key::Named(NamedKey::Escape) {
                                    context_menu.set(None);
                                }
                            }
                        })
                        .on_event_stop(EventListener::PointerDown, move |_| {})
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
                                .min_width(200.0)
                                .margin_top(-5.0)
                                .margin_left(menu_width.get() as f32)
                                .flex_col()
                                .border_radius(10.0)
                                .background(Color::rgb8(44, 44, 44))
                                .padding(5.0)
                                .cursor(CursorStyle::Default)
                                .box_shadow_blur(5.0)
                                .box_shadow_color(Color::BLACK)
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
                    .background(Color::rgb8(92, 92, 92))
            }))
            .style(|s| s.min_width(100.pct()).padding_horiz(20.0))
            .into_any(),
        }
    }

    let on_child_submenu = create_rw_signal(false);
    let view = dyn_stack(
        move || context_menu_items.get().unwrap_or_default(),
        move |s| s.clone(),
        move |menu| view_fn(window_id, menu, context_menu, focus_count, on_child_submenu),
    )
    .on_resize(move |rect| {
        context_menu_size.set(rect.size());
    })
    .on_event_stop(EventListener::PointerDown, move |_| {})
    .on_event_stop(EventListener::PointerMove, move |_| {})
    .keyboard_navigatable()
    .on_event_stop(EventListener::KeyDown, move |event| {
        if let Event::KeyDown(event) = event {
            if event.key.logical_key == Key::Named(NamedKey::Escape) {
                context_menu.set(None);
            }
        }
    })
    .on_event_stop(EventListener::FocusGained, move |_| {
        focus_count.update(|count| {
            *count += 1;
        });
    })
    .on_event_stop(EventListener::FocusLost, move |_| {
        let count = focus_count
            .try_update(|count| {
                *count -= 1;
                *count
            })
            .unwrap();
        if count < 1 {
            context_menu.set(None);
        }
    })
    .style(move |s| {
        let window_size = window_size.get();
        let menu_size = context_menu_size.get();
        let is_acitve = context_menu.with(|m| m.is_some());
        let mut pos = context_menu.with(|m| m.as_ref().map(|(_, pos)| *pos).unwrap_or_default());
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
            .background(Color::rgb8(44, 44, 44))
            .color(Color::rgb8(201, 201, 201))
            .z_index(999)
            .line_height(2.0)
            .padding(5.0)
            .margin_left(pos.x as f32)
            .margin_top(pos.y as f32)
            .cursor(CursorStyle::Default)
            .apply_if(!is_acitve, |s| s.hide())
            .box_shadow_blur(5.0)
            .box_shadow_color(Color::BLACK)
    });

    let id = view.id();

    create_effect(move |_| {
        if context_menu.with(|m| m.is_some()) {
            id.request_focus();
        }
    });

    view
}

struct OverlayView {
    id: ViewId,
    child: ViewId,
    position: Point,
    window_origin: Point,
    parent_size: Size,
    size: Size,
}

impl View for OverlayView {
    fn id(&self) -> ViewId {
        self.id
    }

    fn view_style(&self) -> Option<crate::style::Style> {
        Some(
            Style::new()
                .absolute()
                .inset_left(self.position.x)
                .inset_top(self.position.y),
        )
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Overlay".into()
    }

    fn compute_layout(&mut self, cx: &mut ComputeLayoutCx) -> Option<Rect> {
        self.window_origin = cx.window_origin;
        if let Some(parent_size) = self.id.parent_size() {
            self.parent_size = parent_size;
        }
        if let Some(layout) = self.id.get_layout() {
            self.size = Size::new(layout.size.width as f64, layout.size.height as f64);
        }
        default_compute_layout(self.id, cx)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        cx.save();
        let x = if (self.window_origin.x + self.size.width) > self.parent_size.width - 5.0 {
            (self.window_origin.x + self.size.width) - (self.parent_size.width - 5.0)
        } else {
            0.0
        };
        let y = if (self.window_origin.y + self.size.height) > self.parent_size.height - 5.0 {
            (self.window_origin.y + self.size.height) - (self.parent_size.height - 5.0)
        } else {
            0.0
        };
        cx.offset((-x, -y));
        cx.paint_view(self.child);
        cx.restore();
    }
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
