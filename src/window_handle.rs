use std::time::{Duration, Instant};

use floem_reactive::{with_scope, RwSignal, Scope};
use floem_renderer::Renderer;
use kurbo::{Affine, Point, Rect, Size, Vec2};

#[cfg(target_os = "linux")]
use winit::window::WindowId;
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{CursorIcon, Theme},
};

#[cfg(target_os = "linux")]
use crate::unit::UnitExt;
#[cfg(target_os = "linux")]
use crate::views::{container_box, stack, Decorators};
use crate::{
    action::exec_after,
    animate::{AnimPropKind, AnimUpdateMsg, AnimValue, AnimatedProp, SizeUnit},
    context::{
        AppState, EventCx, LayoutCx, MoveListener, PaintCx, PaintState, ResizeListener, UpdateCx,
    },
    event::{Event, EventListener},
    id::{Id, IdPath, ID_PATHS},
    keyboard::KeyEvent,
    menu::Menu,
    pointer::{PointerButton, PointerInputEvent, PointerMoveEvent, PointerWheelEvent},
    style::{CursorStyle, StyleSelector},
    update::{
        UpdateMessage, ANIM_UPDATE_MESSAGES, CENTRAL_DEFERRED_UPDATE_MESSAGES,
        CENTRAL_UPDATE_MESSAGES, CURRENT_RUNNING_VIEW_HANDLE, DEFERRED_UPDATE_MESSAGES,
        UPDATE_MESSAGES,
    },
    view::{view_children_set_parent_id, ChangeFlags, View},
};

/// The top-level window handle that owns the winit Window.
/// Meant only for use with the root view of the application.
/// Owns the `AppState` and is responsible for
/// - processing all requests to update the AppState from the reactive system
/// - processing all requests to update the animation state from the reactive system
/// - requesting a new animation frame from the backend
pub(crate) struct WindowHandle {
    pub(crate) window: Option<winit::window::Window>,
    /// Reactive Scope for this WindowHandle
    scope: Scope,
    view: Box<dyn View>,
    app_state: AppState,
    paint_state: PaintState,
    size: RwSignal<Size>,
    theme: RwSignal<Option<Theme>>,
    is_maximized: bool,
    pub(crate) scale: f64,
    pub(crate) modifiers: ModifiersState,
    pub(crate) cursor_position: Point,
    pub(crate) window_position: Point,
    pub(crate) last_pointer_down: Option<(u8, Instant)>,
    #[cfg(target_os = "linux")]
    pub(crate) context_menu: RwSignal<Option<(Menu, Point)>>,
}

impl WindowHandle {
    pub(crate) fn new(
        window: winit::window::Window,
        view_fn: impl FnOnce(winit::window::WindowId) -> Box<dyn View> + 'static,
    ) -> Self {
        let scope = Scope::new();
        let window_id = window.id();
        let scale = window.scale_factor();
        let size: LogicalSize<f64> = window.inner_size().to_logical(scale);
        let size = Size::new(size.width, size.height);
        let size = scope.create_rw_signal(Size::new(size.width, size.height));
        let theme = scope.create_rw_signal(window.theme());
        let is_maximized = window.is_maximized();

        #[cfg(target_os = "linux")]
        let context_menu = scope.create_rw_signal(None);

        #[cfg(not(target_os = "linux"))]
        let view = with_scope(scope, move || view_fn(window_id));

        #[cfg(target_os = "linux")]
        let view = with_scope(scope, move || {
            Box::new(
                stack((
                    container_box(view_fn(window_id)).style(|s| s.size(100.pct(), 100.pct())),
                    context_menu_view(scope, window_id, context_menu, size),
                ))
                .style(|s| s.size(100.pct(), 100.pct())),
            )
        });

        ID_PATHS.with(|id_paths| {
            id_paths
                .borrow_mut()
                .insert(view.id(), IdPath(vec![view.id()]));
        });
        view_children_set_parent_id(&*view);

        let paint_state = PaintState::new(&window, scale, size.get_untracked() * scale);
        let mut window_handle = Self {
            window: Some(window),
            scope,
            view,
            app_state: AppState::new(),
            paint_state,
            size,
            theme,
            is_maximized,
            scale,
            modifiers: ModifiersState::default(),
            cursor_position: Point::ZERO,
            window_position: Point::ZERO,
            #[cfg(target_os = "linux")]
            context_menu,
            last_pointer_down: None,
        };
        window_handle.app_state.set_root_size(size.get_untracked());
        window_handle
    }

    pub fn event(&mut self, event: Event) {
        set_current_view(self.view.id());
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
            cx.app_state.focus.take()
        } else {
            cx.app_state.focus
        };

        if event.needs_focus() {
            let mut processed = false;

            if !processed {
                if let Some(id) = cx.app_state.focus {
                    let id_path = ID_PATHS.with(|paths| paths.borrow().get(&id).cloned());
                    if let Some(id_path) = id_path {
                        processed |= self
                            .view
                            .event_main(&mut cx, Some(&id_path.0), event.clone());
                    } else {
                        cx.app_state.focus = None;
                    }
                }

                if !processed {
                    if let Some(listener) = event.listener() {
                        if let Some(action) = cx.get_event_listener(self.view.id(), &listener) {
                            processed |= (*action)(&event);
                        }
                    }
                }

                if !processed {
                    if let Event::KeyDown(KeyEvent { key, modifiers }) = &event {
                        if key.logical_key == Key::Named(NamedKey::Tab) {
                            let _backwards = modifiers.contains(ModifiersState::SHIFT);
                            // view_tab_navigation(&self.view, cx.app_state, backwards);
                            // view_debug_tree(&self.view);
                        } else if let Key::Character(character) = &key.logical_key {
                            // 'I' displays some debug information
                            if character.eq_ignore_ascii_case("i") {
                                // view_debug_tree(&self.view);
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
                                cx.app_state.request_layout(id);
                            }

                            cx.app_state.active = None;
                        }
                    }
                }
            }
        } else if cx.app_state.active.is_some() && event.is_pointer() {
            if cx.app_state.is_dragging() {
                self.view.event_main(&mut cx, None, event.clone());
            }

            let id = cx.app_state.active.unwrap();
            let id_path = ID_PATHS.with(|paths| paths.borrow().get(&id).cloned());
            if let Some(id_path) = id_path {
                self.view
                    .event_main(&mut cx, Some(&id_path.0), event.clone());
            }
            if let Event::PointerUp(_) = &event {
                // To remove the styles applied by the Active selector
                if cx.app_state.has_style_for_sel(id, StyleSelector::Active) {
                    cx.app_state.request_layout(id);
                }

                cx.app_state.active = None;
            }
        } else {
            self.view.event_main(&mut cx, None, event.clone());
        }

        if let Event::PointerUp(_) = &event {
            cx.app_state.drag_start = None;
        }
        if is_pointer_move {
            let hovered = &cx.app_state.hovered.clone();
            for id in was_hovered.unwrap().symmetric_difference(hovered) {
                let view_state = cx.app_state.view_state(*id);
                if view_state.hover_style.is_some()
                    || view_state.active_style.is_some()
                    || view_state.animation.is_some()
                {
                    cx.app_state.request_layout(*id);
                }
                if hovered.contains(id) {
                    if let Some(action) = cx.get_event_listener(*id, &EventListener::PointerEnter) {
                        (*action)(&event);
                    }
                } else if let Some(action) =
                    cx.get_event_listener(*id, &EventListener::PointerLeave)
                {
                    (*action)(&event);
                }
            }
            let dragging_over = &cx.app_state.dragging_over.clone();
            for id in was_dragging_over
                .unwrap()
                .symmetric_difference(dragging_over)
            {
                if dragging_over.contains(id) {
                    if let Some(action) = cx.get_event_listener(*id, &EventListener::DragEnter) {
                        (*action)(&event);
                    }
                } else if let Some(action) = cx.get_event_listener(*id, &EventListener::DragLeave) {
                    (*action)(&event);
                }
            }
        }
        if was_focused != cx.app_state.focus {
            cx.app_state.focus_changed(was_focused, cx.app_state.focus);
        }

        self.process_update();
    }

    pub(crate) fn scale(&mut self, scale: f64) {
        self.scale = scale;
        let scale = self.scale * self.app_state.scale;
        self.paint_state.set_scale(scale);
        self.request_paint();
    }

    pub(crate) fn theme_changed(&mut self, theme: Theme) {
        self.theme.set(Some(theme));
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

        self.layout();
        self.process_update();
        self.request_paint();
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
        if event.key.state.is_pressed() {
            self.event(Event::KeyDown(event));
        } else {
            self.event(Event::KeyUp(event));
        }
    }

    pub(crate) fn pointer_move(&mut self, pos: Point) {
        if self.cursor_position != pos {
            self.last_pointer_down = None;
            self.cursor_position = pos;
            let event = PointerMoveEvent {
                pos,
                modifiers: self.modifiers,
            };
            self.event(Event::PointerMove(event));
        }
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
            if let Some((count, instant)) = self.last_pointer_down.as_mut() {
                if *count == 4 {
                    *count = 1;
                } else if instant.elapsed().as_millis() < 500 {
                    *count += 1;
                } else {
                    *count = 1;
                }
                *instant = Instant::now();
                *count
            } else {
                self.last_pointer_down = Some((1, Instant::now()));
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

    fn layout(&mut self) {
        let mut cx = LayoutCx::new(&mut self.app_state);

        cx.app_state_mut().root = Some(self.view.layout_main(&mut cx));
        cx.app_state_mut().compute_layout();

        cx.clear();
        self.view.compute_layout_main(&mut cx);

        // Currently we only need one ID with animation in progress to request layout, which will
        // advance the all the animations in progress.
        // This will be reworked once we change from request_layout to request_paint
        let id = self.app_state.ids_with_anim_in_progress().get(0).cloned();

        if let Some(id) = id {
            exec_after(Duration::from_millis(1), move |_| {
                id.request_layout();
            });
        }
    }

    pub fn paint(&mut self) {
        let mut cx = PaintCx {
            app_state: &mut self.app_state,
            paint_state: &mut self.paint_state,
            transform: Affine::IDENTITY,
            clip: None,
            color: None,
            font_size: None,
            font_family: None,
            font_weight: None,
            font_style: None,
            line_height: None,
            z_index: None,
            saved_transforms: Vec::new(),
            saved_clips: Vec::new(),
            saved_colors: Vec::new(),
            saved_font_sizes: Vec::new(),
            saved_font_families: Vec::new(),
            saved_font_weights: Vec::new(),
            saved_font_styles: Vec::new(),
            saved_line_heights: Vec::new(),
            saved_z_indexes: Vec::new(),
            scroll_bar_color: None,
            scroll_bar_rounded: None,
            scroll_bar_thickness: None,
            scroll_bar_edge_width: None,
            saved_scroll_bar_colors: Vec::new(),
            saved_scroll_bar_roundeds: Vec::new(),
            saved_scroll_bar_thicknesses: Vec::new(),
            saved_scroll_bar_edge_widths: Vec::new(),
        };
        cx.paint_state.renderer.begin();
        self.view.paint_main(&mut cx);
        if let Some(window) = self.window.as_ref() {
            window.pre_present_notify();
        }
        cx.paint_state.renderer.finish();
        self.process_update();
    }

    pub(crate) fn process_update(&mut self) {
        let mut flags = ChangeFlags::empty();
        loop {
            flags |= self.process_update_messages();
            if !self.needs_layout()
                && !self.has_deferred_update_messages()
                && !self.has_anim_update_messages()
            {
                break;
            }
            // QUESTION: why do we always request a layout?
            flags |= ChangeFlags::LAYOUT;
            self.layout();
            flags |= self.process_deferred_update_messages();
            flags |= self.process_anim_update_messages();
        }

        self.set_cursor();

        if !flags.is_empty() {
            self.request_paint();
        }
    }

    fn process_central_messages(&self) {
        CENTRAL_UPDATE_MESSAGES.with(|central_msgs| {
            if !central_msgs.borrow().is_empty() {
                UPDATE_MESSAGES.with(|msgs| {
                    let mut msgs = msgs.borrow_mut();
                    let central_msgs = std::mem::take(&mut *central_msgs.borrow_mut());
                    for (id, msg) in central_msgs {
                        if let Some(root) = id.root_id() {
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
                        if let Some(root) = id.root_id() {
                            let msgs = msgs.entry(root).or_default();
                            msgs.push((id, msg));
                        }
                    }
                });
            }
        });
    }

    fn process_update_messages(&mut self) -> ChangeFlags {
        let mut flags = ChangeFlags::empty();
        loop {
            self.process_central_messages();
            let msgs = UPDATE_MESSAGES.with(|msgs| {
                msgs.borrow_mut()
                    .remove(&self.view.id())
                    .unwrap_or_default()
            });
            if msgs.is_empty() {
                break;
            }
            for msg in msgs {
                let mut cx = UpdateCx {
                    app_state: &mut self.app_state,
                };
                match msg {
                    UpdateMessage::RequestPaint => {
                        flags |= ChangeFlags::PAINT;
                    }
                    UpdateMessage::RequestLayout { id } => {
                        cx.app_state.request_layout(id);
                    }
                    UpdateMessage::Focus(id) => {
                        if cx.app_state.focus != Some(id) {
                            let old = cx.app_state.focus;
                            cx.app_state.focus = Some(id);
                            cx.app_state.focus_changed(old, cx.app_state.focus);
                        }
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
                                cx.app_state.request_layout(old_id);
                            }
                        }

                        if cx.app_state.has_style_for_sel(id, StyleSelector::Active) {
                            cx.app_state.request_layout(id);
                        }
                    }
                    UpdateMessage::Disabled { id, is_disabled } => {
                        if is_disabled {
                            cx.app_state.disabled.insert(id);
                            cx.app_state.hovered.remove(&id);
                        } else {
                            cx.app_state.disabled.remove(&id);
                        }
                        cx.app_state.request_layout(id);
                    }
                    UpdateMessage::State { id, state } => {
                        let id_path = ID_PATHS.with(|paths| paths.borrow().get(&id).cloned());
                        if let Some(id_path) = id_path {
                            flags |= self.view.update_main(&mut cx, &id_path.0, state);
                        }
                    }
                    UpdateMessage::BaseStyle { id, style } => {
                        let state = cx.app_state.view_state(id);
                        state.base_style = Some(style);
                        cx.request_layout(id);
                    }
                    UpdateMessage::Style { id, style } => {
                        let state = cx.app_state.view_state(id);
                        state.style = style;
                        cx.request_layout(id);
                    }
                    UpdateMessage::ResponsiveStyle { id, style, size } => {
                        let state = cx.app_state.view_state(id);

                        state.add_responsive_style(size, style);
                    }
                    UpdateMessage::StyleSelector {
                        id,
                        style,
                        selector,
                    } => {
                        let state = cx.app_state.view_state(id);
                        let style = Some(style);
                        match selector {
                            StyleSelector::Hover => state.hover_style = style,
                            StyleSelector::Focus => state.focus_style = style,
                            StyleSelector::FocusVisible => state.focus_visible_style = style,
                            StyleSelector::Disabled => state.disabled_style = style,
                            StyleSelector::Active => state.active_style = style,
                            StyleSelector::Dragging => state.dragging_style = style,
                        }
                        cx.request_layout(id);
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
                            window.set_outer_position(winit::dpi::Position::Logical(
                                winit::dpi::LogicalPosition::new(pos.x, pos.y),
                            ));
                        }
                    }
                    UpdateMessage::EventListener {
                        id,
                        listener,
                        action,
                    } => {
                        let state = cx.app_state.view_state(id);
                        state.event_listeners.insert(listener, action);
                    }
                    UpdateMessage::ResizeListener { id, action } => {
                        let state = cx.app_state.view_state(id);
                        state.resize_listener = Some(ResizeListener {
                            rect: Rect::ZERO,
                            callback: action,
                        });
                    }
                    UpdateMessage::MoveListener { id, action } => {
                        let state = cx.app_state.view_state(id);
                        state.move_listener = Some(MoveListener {
                            window_origin: Point::ZERO,
                            callback: action,
                        });
                    }
                    UpdateMessage::CleanupListener { id, action } => {
                        let state = cx.app_state.view_state(id);
                        state.cleanup_listener = Some(action);
                    }
                    UpdateMessage::Animation { id, animation } => {
                        cx.app_state.animated.insert(id);
                        let view_state = cx.app_state.view_state(id);
                        view_state.animation = Some(animation);
                    }
                    UpdateMessage::WindowScale(scale) => {
                        cx.app_state.scale = scale;
                        cx.request_layout(self.view.id());
                        let scale = self.scale * cx.app_state.scale;
                        self.paint_state.set_scale(scale);
                    }
                    UpdateMessage::ContextMenu { id, menu } => {
                        let state = cx.app_state.view_state(id);
                        state.context_menu = Some(menu);
                    }
                    UpdateMessage::PopoutMenu { id, menu } => {
                        let state = cx.app_state.view_state(id);
                        state.popout_menu = Some(menu);
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
                        #[cfg(target_os = "linux")]
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
                                winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(
                                    position.x * self.app_state.scale,
                                    position.y * self.app_state.scale,
                                )),
                                winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
                                    size.width * self.app_state.scale,
                                    size.height * self.app_state.scale,
                                )),
                            );
                        }
                    }
                }
            }
        }
        flags
    }

    fn process_deferred_update_messages(&mut self) -> ChangeFlags {
        self.process_central_messages();
        let mut flags = ChangeFlags::empty();
        let msgs = DEFERRED_UPDATE_MESSAGES.with(|msgs| {
            msgs.borrow_mut()
                .remove(&self.view.id())
                .unwrap_or_default()
        });
        if msgs.is_empty() {
            return flags;
        }

        let mut cx = UpdateCx {
            app_state: &mut self.app_state,
        };
        for (id, state) in msgs {
            let id_path = ID_PATHS.with(|paths| paths.borrow().get(&id).cloned());
            if let Some(id_path) = id_path {
                flags |= self.view.update_main(&mut cx, &id_path.0, state);
            }
        }

        flags
    }

    fn process_anim_update_messages(&mut self) -> ChangeFlags {
        let mut flags = ChangeFlags::empty();
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
                    flags |= self.process_update_anim_prop(view_id, kind, val);
                }
            }
        }

        flags
    }

    fn process_update_anim_prop(
        &mut self,
        view_id: Id,
        kind: AnimPropKind,
        val: AnimValue,
    ) -> ChangeFlags {
        let layout = self.app_state.get_layout(view_id).unwrap();
        let view_state = self.app_state.view_state(view_id);
        let anim = view_state.animation.as_mut().unwrap();
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
            AnimPropKind::BorderRadius => {
                let border_radius = view_state.computed_style.border_radius;
                AnimatedProp::BorderRadius {
                    from: border_radius.0,
                    to: val.get_f64(),
                }
            }
            AnimPropKind::BorderColor => {
                let border_color = view_state.computed_style.border_color;
                AnimatedProp::BorderColor {
                    from: border_color,
                    to: val.get_color(),
                }
            }
            AnimPropKind::Background => {
                //TODO:  get from cx
                let bg = view_state
                    .computed_style
                    .background
                    .expect("Bg must be set in the styles");
                AnimatedProp::Background {
                    from: bg,
                    to: val.get_color(),
                }
            }
            AnimPropKind::Color => {
                //TODO:  get from cx
                let color = view_state
                    .computed_style
                    .color
                    .expect("Color must be set in the animated view's style");
                AnimatedProp::Color {
                    from: color,
                    to: val.get_color(),
                }
            }
        };

        // Overrides the old value
        // TODO: logic based on the old val to make the animation smoother when overriding an old
        // animation that was in progress
        anim.props_mut().insert(kind, prop);
        anim.begin();

        ChangeFlags::LAYOUT
    }

    fn needs_layout(&mut self) -> bool {
        self.app_state.view_state(self.view.id()).request_layout
    }

    fn has_deferred_update_messages(&self) -> bool {
        DEFERRED_UPDATE_MESSAGES.with(|m| {
            m.borrow()
                .get(&self.view.id())
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

    fn request_paint(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(crate) fn destroy(&mut self) {
        self.event(Event::WindowClosed);
        self.scope.dispose();
    }

    #[cfg(target_os = "macos")]
    fn show_context_menu(&self, menu: winit::menu::Menu, pos: Option<Point>) {
        if let Some(window) = self.window.as_ref() {
            {
                use winit::platform::macos::WindowExtMacOS;
                window.show_context_menu(
                    menu,
                    pos.map(|pos| {
                        winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(
                            pos.x * self.app_state.scale,
                            pos.y * self.app_state.scale,
                        ))
                    }),
                );
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn show_context_menu(&self, menu: winit::menu::Menu, pos: Option<Point>) {
        use winit::platform::windows::WindowExtWindows;

        if let Some(window) = self.window.as_ref() {
            {
                window.show_context_menu(
                    menu,
                    pos.map(|pos| {
                        winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(
                            pos.x * self.app_state.scale,
                            pos.y * self.app_state.scale,
                        ))
                    }),
                );
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn show_context_menu(&self, menu: Menu, _platform_menu: winit::menu::Menu, pos: Option<Point>) {
        let pos = pos.unwrap_or(self.cursor_position);
        let pos = Point::new(pos.x / self.app_state.scale, pos.y / self.app_state.scale);
        self.context_menu.set(Some((menu, pos)));
    }

    pub(crate) fn menu_action(&mut self, id: usize) {
        set_current_view(self.view.id());
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
}

pub(crate) fn get_current_view() -> Id {
    CURRENT_RUNNING_VIEW_HANDLE.with(|running| *running.borrow())
}
/// Set this view handle to the current running view handle
pub(crate) fn set_current_view(id: Id) {
    CURRENT_RUNNING_VIEW_HANDLE.with(|running| {
        *running.borrow_mut() = id;
    });
}

#[cfg(target_os = "linux")]
fn context_menu_view(
    cx: Scope,
    window_id: WindowId,
    context_menu: RwSignal<Option<(Menu, Point)>>,
    window_size: RwSignal<Size>,
) -> impl View {
    use floem_reactive::{create_effect, create_rw_signal};
    use peniko::Color;

    use crate::{
        app::{add_app_update_event, AppUpdateEvent},
        views::{empty, list, svg, text},
    };

    #[derive(Clone, PartialEq, Eq, Hash)]
    struct MenuDisplay {
        id: Option<u64>,
        enabled: bool,
        title: String,
        children: Option<Vec<Option<MenuDisplay>>>,
    }

    fn format_menu(menu: &Menu) -> Vec<Option<MenuDisplay>> {
        menu.children
            .iter()
            .map(|e| match e {
                crate::menu::MenuEntry::Separator => None,
                crate::menu::MenuEntry::Item(i) => Some(MenuDisplay {
                    id: Some(i.id),
                    enabled: i.enabled,
                    title: i.title.clone(),
                    children: None,
                }),
                crate::menu::MenuEntry::SubMenu(m) => Some(MenuDisplay {
                    id: None,
                    enabled: m.item.enabled,
                    title: m.item.title.clone(),
                    children: Some(format_menu(m)),
                }),
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
        menu: Option<MenuDisplay>,
        context_menu: RwSignal<Option<(Menu, Point)>>,
        focus_count: RwSignal<i32>,
        on_child_submenu_for_parent: RwSignal<bool>,
    ) -> impl View {
        if let Some(menu) = menu {
            let menu_width = create_rw_signal(0.0);
            let show_submenu = create_rw_signal(false);
            let on_submenu = create_rw_signal(false);
            let on_child_submenu = create_rw_signal(false);
            let has_submenu = menu.children.is_some();
            let submenu_svg = r#"<svg width="16" height="16" viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="currentColor"><path fill-rule="evenodd" clip-rule="evenodd" d="M10.072 8.024L5.715 3.667l.618-.62L11 7.716v.618L6.333 13l-.618-.619 4.357-4.357z"/></svg>"#;
            container_box(
                stack((
                    stack((
                        text(menu.title),
                        svg(|| submenu_svg.to_string()).style(move |s| {
                            s.size(20.0, 20.0)
                                .color(Color::rgb8(201, 201, 201))
                                .margin_right(10.0)
                                .margin_left(20.0)
                                .apply_if(!has_submenu, |s| s.hide())
                        }),
                    ))
                    .on_event(EventListener::PointerEnter, move |_| {
                        if has_submenu {
                            show_submenu.set(true);
                        }
                        true
                    })
                    .on_event(EventListener::PointerLeave, move |_| {
                        if has_submenu {
                            show_submenu.set(false);
                        }
                        true
                    })
                    .on_resize(move |rect| {
                        let width = rect.width();
                        if menu_width.get_untracked() != width {
                            menu_width.set(width);
                        }
                    })
                    .on_click(move |_| {
                        context_menu.set(None);
                        focus_count.set(0);
                        if let Some(id) = menu.id {
                            add_app_update_event(AppUpdateEvent::MenuAction {
                                window_id,
                                action_id: id as usize,
                            });
                        }
                        true
                    })
                    .on_secondary_click(move |_| {
                        context_menu.set(None);
                        focus_count.set(0);
                        if let Some(id) = menu.id {
                            add_app_update_event(AppUpdateEvent::MenuAction {
                                window_id,
                                action_id: id as usize,
                            });
                        }
                        true
                    })
                    .disabled(move || !menu.enabled)
                    .style(|s| {
                        s.width(100.pct())
                            .min_width(100.pct())
                            .padding_horiz(20.0)
                            .justify_between()
                            .items_center()
                    })
                    .hover_style(|s| s.border_radius(10.0).background(Color::rgb8(65, 65, 65)))
                    .active_style(|s| s.border_radius(10.0).background(Color::rgb8(92, 92, 92)))
                    .disabled_style(|s| s.color(Color::rgb8(92, 92, 92))),
                    list(
                        move || menu.children.clone().unwrap_or_default(),
                        move |s| s.clone(),
                        move |menu| {
                            view_fn(window_id, menu, context_menu, focus_count, on_child_submenu)
                        },
                    )
                    .keyboard_navigatable()
                    .on_event(EventListener::FocusGained, move |_| {
                        focus_count.update(|count| {
                            *count += 1;
                        });
                        true
                    })
                    .on_event(EventListener::FocusLost, move |_| {
                        let count = focus_count
                            .try_update(|count| {
                                *count -= 1;
                                *count
                            })
                            .unwrap();
                        if count < 1 {
                            context_menu.set(None);
                        }
                        true
                    })
                    .on_event(EventListener::KeyDown, move |event| {
                        if let Event::KeyDown(event) = event {
                            if event.key.logical_key == Key::Escape {
                                context_menu.set(None);
                            }
                        }
                        true
                    })
                    .on_event(EventListener::PointerDown, move |_| true)
                    .on_event(EventListener::PointerEnter, move |_| {
                        if has_submenu {
                            on_submenu.set(true);
                            on_child_submenu_for_parent.set(true);
                        }
                        true
                    })
                    .on_event(EventListener::PointerLeave, move |_| {
                        if has_submenu {
                            on_submenu.set(false);
                            on_child_submenu_for_parent.set(false);
                        }
                        true
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
                                !show_submenu.get() && !on_submenu.get() && !on_child_submenu.get(),
                                |s| s.hide(),
                            )
                    }),
                ))
                .style(|s| s.min_width(100.pct())),
            )
            .style(|s| s.min_width(100.pct()))
        } else {
            container_box(empty().style(|s| {
                s.width(100.pct())
                    .height(1.0)
                    .margin_vert(5.0)
                    .background(Color::rgb8(92, 92, 92))
            }))
            .style(|s| s.min_width(100.pct()).padding_horiz(20.0))
        }
    }

    let on_child_submenu = create_rw_signal(false);
    let view = list(
        move || context_menu_items.get().unwrap_or_default(),
        move |s| s.clone(),
        move |menu| view_fn(window_id, menu, context_menu, focus_count, on_child_submenu),
    )
    .on_resize(move |rect| {
        context_menu_size.set(rect.size());
    })
    .on_event(EventListener::PointerDown, move |_| true)
    .keyboard_navigatable()
    .on_event(EventListener::KeyDown, move |event| {
        if let Event::KeyDown(event) = event {
            if event.key.logical_key == Key::Escape {
                context_menu.set(None);
            }
        }
        true
    })
    .on_event(EventListener::FocusGained, move |_| {
        focus_count.update(|count| {
            *count += 1;
        });
        true
    })
    .on_event(EventListener::FocusLost, move |_| {
        let count = focus_count
            .try_update(|count| {
                *count -= 1;
                *count
            })
            .unwrap();
        if count < 1 {
            context_menu.set(None);
        }
        true
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
