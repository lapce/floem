use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use floem_reactive::{with_scope, Scope};
use floem_renderer::Renderer;
use kurbo::{Affine, Point, Rect, Size, Vec2};
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta},
    keyboard::{Key, ModifiersState},
    window::CursorIcon,
};

use crate::{
    action::exec_after,
    animate::{AnimPropKind, AnimUpdateMsg, AnimValue, AnimatedProp, SizeUnit},
    context::{
        AppState, EventCx, LayoutCx, MoveListener, PaintCx, PaintState, ResizeListener, UpdateCx,
        ViewContext,
    },
    event::{Event, EventListener},
    id::{Id, ID_PATHS},
    keyboard::KeyEvent,
    menu::Menu,
    pointer::{PointerButton, PointerInputEvent, PointerMoveEvent, PointerWheelEvent},
    style::{CursorStyle, StyleSelector},
    update::{
        UpdateMessage, ANIM_UPDATE_MESSAGES, CURRENT_RUNNING_VIEW_HANDLE, DEFERRED_UPDATE_MESSAGES,
        UPDATE_MESSAGES,
    },
    view::{ChangeFlags, View},
};

/// The top-level window handle that owns the winit Window.
/// Meant only for use with the root view of the application.
/// Owns the `AppState` and is responsible for
/// - processing all requests to update the AppState from the reactive system
/// - processing all requests to update the animation state from the reactive system
/// - requesting a new animation frame from the backend
pub(crate) struct WindowHandle {
    pub(crate) window: Option<Arc<winit::window::Window>>,
    /// Reactive Scope for this WindowHandle
    scope: Scope,
    view: Box<dyn View>,
    app_state: AppState,
    paint_state: PaintState,
    size: Size,
    pub(crate) scale: f64,
    pub(crate) modifiers: ModifiersState,
    pub(crate) cursor_position: Point,
    pub(crate) last_pointer_down: Option<(u8, Instant)>,
}

impl WindowHandle {
    pub(crate) fn new(
        window: winit::window::Window,
        view_fn: impl FnOnce(winit::window::WindowId) -> Box<dyn View> + 'static,
    ) -> Self {
        let window_id = window.id();
        let id = Id::next();
        set_current_view(id);

        let scope = Scope::new();
        let cx = ViewContext { id };
        let view = ViewContext::with_context(cx, || with_scope(scope, move || view_fn(window_id)));

        let scale = window.scale_factor();
        let size: LogicalSize<f64> = window.inner_size().to_logical(scale);
        let size = Size::new(size.width, size.height);
        let paint_state = PaintState::new(&window, scale, size);
        let mut window_handle = Self {
            window: Some(Arc::new(window)),
            scope: Scope::new(),
            view,
            app_state: AppState::new(),
            paint_state,
            size,
            scale,
            modifiers: ModifiersState::default(),
            cursor_position: Point::ZERO,
            last_pointer_down: None,
        };
        window_handle.app_state.set_root_size(size);
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
                    }
                } else if let Some(listener) = event.listener() {
                    if let Some(action) = cx.get_event_listener(self.view.id(), &listener) {
                        processed |= (*action)(&event);
                    }
                }

                if !processed {
                    if let Event::KeyDown(KeyEvent { key, modifiers }) = &event {
                        if key.logical_key == Key::Tab {
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
            if let Some(old_id) = was_focused {
                // To remove the styles applied by the Focus selector
                if cx.app_state.has_style_for_sel(old_id, StyleSelector::Focus)
                    || cx
                        .app_state
                        .has_style_for_sel(old_id, StyleSelector::FocusVisible)
                {
                    cx.app_state.request_layout(old_id);
                }
                if let Some(action) = cx.get_event_listener(old_id, &EventListener::FocusLost) {
                    (*action)(&event);
                }
            }

            if let Some(id) = cx.app_state.focus {
                // To apply the styles of the Focus selector
                if cx.app_state.has_style_for_sel(id, StyleSelector::Focus)
                    || cx
                        .app_state
                        .has_style_for_sel(id, StyleSelector::FocusVisible)
                {
                    cx.app_state.request_layout(id);
                }
                if let Some(action) = cx.get_event_listener(id, &EventListener::FocusGained) {
                    (*action)(&event);
                }
            }
        }

        self.process_update();
    }

    pub(crate) fn scale(&mut self, scale: f64) {
        self.scale = scale;
        let scale = self.scale * self.app_state.scale;
        self.paint_state.set_scale(scale);
        self.request_paint();
    }

    pub(crate) fn size(&mut self, size: Size) {
        self.size = size;
        self.app_state.update_screen_size_bp(size);
        self.event(Event::WindowResized(size));
        let scale = self.scale * self.app_state.scale;
        self.paint_state.resize(scale, size);
        self.app_state.set_root_size(size);
        self.layout();
        self.process_update();
        self.request_paint();
    }

    pub(crate) fn position(&mut self, point: Point) {
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
        self.cursor_position = pos;
        self.last_pointer_down = None;
        let event = PointerMoveEvent {
            pos,
            modifiers: self.modifiers,
        };
        self.event(Event::PointerMove(event));
    }

    pub(crate) fn mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let delta = match delta {
            MouseScrollDelta::LineDelta(x, y) => Vec2::new(-x as f64, -y as f64),
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

    fn process_update_messages(&mut self) -> ChangeFlags {
        let mut flags = ChangeFlags::empty();
        loop {
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
                        let old = cx.app_state.focus;
                        cx.app_state.focus = Some(id);

                        if let Some(old_id) = old {
                            // To remove the styles applied by the Focus selector
                            if cx.app_state.has_style_for_sel(old_id, StyleSelector::Focus) {
                                cx.app_state.request_layout(old_id);
                            }
                        }

                        if cx.app_state.has_style_for_sel(id, StyleSelector::Focus) {
                            cx.app_state.request_layout(id);
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
                    UpdateMessage::HandleTitleBar(_val) => {
                        // self.handle.handle_titlebar(val);
                    }
                    UpdateMessage::ToggleWindowMaximized => {
                        // let window_state = self.handle.get_window_state();
                        // match window_state {
                        //     glazier::WindowState::Maximized => {
                        //         self.handle.set_window_state(WindowState::Restored);
                        //     }
                        //     glazier::WindowState::Minimized => {
                        //         self.handle.set_window_state(WindowState::Maximized);
                        //     }
                        //     glazier::WindowState::Restored => {
                        //         self.handle.set_window_state(WindowState::Maximized);
                        //     }
                        // }
                    }
                    UpdateMessage::SetWindowDelta(_delta) => {
                        // self.handle.set_position(self.handle.get_position() + delta);
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
                        let menu = menu.popup();
                        let platform_menu = menu.platform_menu();
                        cx.app_state.context_menu.clear();
                        cx.app_state.update_context_menu(menu);
                        self.show_context_menu(platform_menu, pos);
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
                }
            }
        }
        flags
    }

    fn process_deferred_update_messages(&mut self) -> ChangeFlags {
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
                    from: border_radius as f64,
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

    fn show_context_menu(&self, menu: winit::menu::Menu, pos: Option<Point>) {
        if let Some(window) = self.window.as_ref() {
            #[cfg(target_os = "macos")]
            {
                use winit::platform::macos::WindowExtMacOS;
                window.show_context_menu(
                    menu,
                    pos.map(|pos| {
                        winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(
                            pos.x, pos.y,
                        ))
                    }),
                );
            }
        }
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
