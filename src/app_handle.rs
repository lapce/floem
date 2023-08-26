use std::cell::RefCell;
use std::time::Duration;
use std::{any::Any, collections::HashMap};

use crate::action::exec_after;
use crate::animate::AnimValue;
use crate::context::MoveListener;
use crate::id::WindowId;
use crate::view::{view_debug_tree, view_tab_navigation};
use crate::window::WINDOWS;
use floem_reactive::{with_scope, Scope};
use floem_renderer::Renderer;
use glazier::kurbo::{Affine, Point, Rect, Size, Vec2};
use glazier::{
    FileDialogOptions, FileDialogToken, FileInfo, Scale, TimerToken, WinHandler, WindowHandle,
    WindowState,
};

use crate::menu::Menu;
use crate::{
    animate::{AnimPropKind, AnimUpdateMsg, AnimatedProp, Animation, SizeUnit},
    context::{
        AppState, EventCallback, EventCx, LayoutCx, PaintCx, PaintState, ResizeCallback,
        ResizeListener, UpdateCx, VIEW_CONTEXT_STORE,
    },
    event::{Event, EventListener},
    ext_event::EXT_EVENT_HANDLER,
    id::{Id, ID_PATHS},
    responsive::ScreenSize,
    style::{CursorStyle, Style},
    view::{ChangeFlags, View},
};

thread_local! {
    /// Stores a queue of update messages for each view. This is a list of build in messages, including a built-in State message
    /// that you can use to send a state update to a view.
    pub(crate) static UPDATE_MESSAGES: RefCell<HashMap<Id, Vec<UpdateMessage>>> = Default::default();
    pub(crate) static DEFERRED_UPDATE_MESSAGES: RefCell<DeferredUpdateMessages> = Default::default();
    pub(crate) static ANIM_UPDATE_MESSAGES: RefCell<Vec<AnimUpdateMsg>> = Default::default();
    /// It stores the active view handle, so that when you dispatch an action, it knows
    /// which view handle it submitted to
    pub(crate) static CURRENT_RUNNING_VIEW_HANDLE: RefCell<Id> = RefCell::new(Id::next());
    pub(crate) static WINDOW_HANDLES: RefCell<HashMap<Id, WindowHandle>> = Default::default();
}

pub type FileDialogs = HashMap<FileDialogToken, Box<dyn Fn(Option<FileInfo>)>>;
type DeferredUpdateMessages = HashMap<Id, Vec<(Id, Box<dyn Any>)>>;

// Primarily used to mint and assign a unique ID to each view.
#[derive(Copy, Clone)]
pub struct ViewContext {
    pub id: Id,
}

impl ViewContext {
    pub fn save() {
        VIEW_CONTEXT_STORE.with(|store| {
            store.borrow_mut().save();
        })
    }

    pub fn set_current(cx: ViewContext) {
        VIEW_CONTEXT_STORE.with(|store| {
            store.borrow_mut().set_current(cx);
        })
    }

    pub fn get_current() -> ViewContext {
        VIEW_CONTEXT_STORE.with(|store| store.borrow().cx)
    }

    pub fn restore() {
        VIEW_CONTEXT_STORE.with(|store| {
            store.borrow_mut().restore();
        })
    }

    pub fn with_context<T>(cx: ViewContext, f: impl FnOnce() -> T) -> T {
        ViewContext::save();
        ViewContext::set_current(cx);
        let value = f();
        ViewContext::restore();
        value
    }

    /// Use this method if you are creating a `View` that has a child.
    ///
    /// Ensures that the child is initialized with the "correct" `ViewContext`
    /// and that the context is restored after the child (and its children) are initialized.
    /// For the child's `ViewContext` to be "correct", the child's ViewContext's `Id`  must bet set to the parent `View`'s `Id`.
    ///
    /// This method returns the `Id` that should be attached to the parent `View` along with the initialized child.
    pub fn new_id_with_child<V>(child: impl FnOnce() -> V) -> (Id, V) {
        let cx = ViewContext::get_current();
        let id = cx.new_id();
        let mut child_cx = cx;
        child_cx.id = id;
        let child = ViewContext::with_context(child_cx, child);
        (id, child)
    }

    pub fn with_id(mut self, id: Id) -> Self {
        self.id = id;
        self
    }

    pub fn new_id(&self) -> Id {
        self.id.new()
    }
}

pub enum StyleSelector {
    Hover,
    Focus,
    FocusVisible,
    Disabled,
    Active,
    Dragging,
}

pub enum UpdateMessage {
    Focus(Id),
    Active(Id),
    WindowScale(f64),
    Disabled {
        id: Id,
        is_disabled: bool,
    },
    RequestPaint,
    RequestLayout {
        id: Id,
    },
    State {
        id: Id,
        state: Box<dyn Any>,
    },
    BaseStyle {
        id: Id,
        style: Style,
    },
    Style {
        id: Id,
        style: Style,
    },
    ResponsiveStyle {
        id: Id,
        style: Style,
        size: ScreenSize,
    },
    StyleSelector {
        id: Id,
        selector: StyleSelector,
        style: Style,
    },
    KeyboardNavigable {
        id: Id,
    },
    Draggable {
        id: Id,
    },
    EventListener {
        id: Id,
        listener: EventListener,
        action: Box<EventCallback>,
    },
    ResizeListener {
        id: Id,
        action: Box<ResizeCallback>,
    },
    MoveListener {
        id: Id,
        action: Box<dyn Fn(Point)>,
    },
    CleanupListener {
        id: Id,
        action: Box<dyn Fn()>,
    },
    ToggleWindowMaximized,
    HandleTitleBar(bool),
    SetWindowDelta(Vec2),
    OpenFile {
        options: FileDialogOptions,
        file_info_action: Box<dyn Fn(Option<FileInfo>)>,
    },
    SaveAs {
        options: FileDialogOptions,
        file_info_action: Box<dyn Fn(Option<FileInfo>)>,
    },
    RequestTimer {
        token: TimerToken,
        action: Box<dyn FnOnce(TimerToken)>,
    },
    Animation {
        id: Id,
        animation: Animation,
    },
    ContextMenu {
        id: Id,
        menu: Box<dyn Fn() -> Menu>,
    },
    PopoutMenu {
        id: Id,
        menu: Box<dyn Fn() -> Menu>,
    },
    ShowContextMenu {
        menu: Menu,
        pos: Point,
    },
    WindowMenu {
        menu: Menu,
    },
    SetWindowTitle {
        title: String,
    },
}

/// The top-level handle that is passed into the backend interface (e.g. `glazier`) to interact to window events.
/// Meant only for use with the root view of the application.
/// Owns the `AppState` and is responsible for
/// - processing all requests to update the AppState from the reactive system
/// - processing all requests to update the animation state from the reactive system
/// - requesting a new animation frame from the backend
pub struct AppHandle<V: View + 'static> {
    /// Reactive Scope for this AppHandle
    scope: Scope,
    pub(crate) window_id: WindowId,
    view: V,
    handle: glazier::WindowHandle,
    app_state: AppState,
    paint_state: PaintState,
    file_dialogs: FileDialogs,
    window_size: Size,
    pub(crate) window_menu: HashMap<u32, Box<dyn Fn()>>,
    closed: bool,
}

impl<V: View> AppHandle<V> {
    pub fn new(window_id: WindowId, app_logic: impl FnOnce() -> V + 'static) -> Self {
        let id = Id::next();
        set_current_view(id);
        {
            *EXT_EVENT_HANDLER.active.lock() = id;
        }

        let scope = Scope::new();
        let cx = ViewContext { id };
        let view = ViewContext::with_context(cx, || with_scope(scope, app_logic));
        Self {
            scope,
            view,
            window_id,
            app_state: AppState::new(),
            paint_state: PaintState::new(),
            handle: Default::default(),
            file_dialogs: HashMap::new(),
            window_size: Size::ZERO,
            window_menu: HashMap::new(),
            closed: false,
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
        cx.paint_state.renderer.as_mut().unwrap().begin();
        self.view.paint_main(&mut cx);
        cx.paint_state.renderer.as_mut().unwrap().finish();
        self.process_update();
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
                    UpdateMessage::HandleTitleBar(val) => {
                        self.handle.handle_titlebar(val);
                    }
                    UpdateMessage::ToggleWindowMaximized => {
                        let window_state = self.handle.get_window_state();
                        match window_state {
                            glazier::WindowState::Maximized => {
                                self.handle.set_window_state(WindowState::Restored);
                            }
                            glazier::WindowState::Minimized => {
                                self.handle.set_window_state(WindowState::Maximized);
                            }
                            glazier::WindowState::Restored => {
                                self.handle.set_window_state(WindowState::Maximized);
                            }
                        }
                    }
                    UpdateMessage::SetWindowDelta(delta) => {
                        self.handle.set_position(self.handle.get_position() + delta);
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
                    UpdateMessage::OpenFile {
                        options,
                        file_info_action,
                    } => {
                        let token = self.handle.open_file(options);
                        if let Some(token) = token {
                            self.file_dialogs.insert(token, file_info_action);
                        }
                    }
                    UpdateMessage::SaveAs {
                        options,
                        file_info_action,
                    } => {
                        let token = self.handle.save_as(options);
                        if let Some(token) = token {
                            self.file_dialogs.insert(token, file_info_action);
                        }
                    }
                    UpdateMessage::RequestTimer { token, action } => {
                        cx.app_state.request_timer(token, action);
                    }
                    UpdateMessage::Animation { id, animation } => {
                        cx.app_state.animated.insert(id);
                        let view_state = cx.app_state.view_state(id);
                        view_state.animation = Some(animation);
                    }
                    UpdateMessage::WindowScale(scale) => {
                        cx.app_state.scale = scale;
                        cx.request_layout(self.view.id());
                        let scale = self.handle.get_scale().unwrap_or_default();
                        let scale = Scale::new(
                            scale.x() * cx.app_state.scale,
                            scale.y() * cx.app_state.scale,
                        );
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
                        self.handle.show_context_menu(platform_menu, pos);
                    }
                    UpdateMessage::WindowMenu { menu } => {
                        let platform_menu = menu.platform_menu();
                        self.update_window_menu(menu);
                        self.handle.set_menu(platform_menu);
                    }
                    UpdateMessage::SetWindowTitle { title } => {
                        self.handle.set_title(&title);
                    }
                }
            }
        }
        flags
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

    pub fn process_update(&mut self) {
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

    fn set_cursor(&mut self) {
        let glazier_cursor = match self.app_state.cursor {
            Some(CursorStyle::Default) => glazier::Cursor::Arrow,
            Some(CursorStyle::Pointer) => glazier::Cursor::Pointer,
            Some(CursorStyle::Text) => glazier::Cursor::IBeam,
            None => glazier::Cursor::Arrow,
        };
        if glazier_cursor != self.app_state.last_cursor {
            self.handle.set_cursor(&glazier_cursor);
            self.app_state.last_cursor = glazier_cursor;
            self.request_paint();
        }
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
                    if let Event::KeyDown(glazier::KeyEvent { key, mods, .. }) = &event {
                        if key == &glazier::KbKey::Tab {
                            let backwards = mods.contains(glazier::Modifiers::SHIFT);
                            view_tab_navigation(&self.view, cx.app_state, backwards);
                            view_debug_tree(&self.view);
                        } else if let glazier::KbKey::Character(character) = key {
                            // 'I' displays some debug information
                            if character.eq_ignore_ascii_case("i") {
                                view_debug_tree(&self.view);
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

    fn idle(&mut self) {
        set_current_view(self.view.id());
        if let Some(triggers) = { EXT_EVENT_HANDLER.queue.lock().remove(&self.view.id()) } {
            for trigger in triggers {
                trigger.notify();
            }
        }
        self.process_update();
    }

    fn request_paint(&self) {
        self.handle.invalidate();
    }

    fn update_window_menu(&mut self, mut menu: Menu) {
        if let Some(action) = menu.item.action.take() {
            self.window_menu.insert(menu.item.id as u32, action);
        }
        for child in menu.children {
            match child {
                crate::menu::MenuEntry::Separator => {}
                crate::menu::MenuEntry::Item(mut item) => {
                    if let Some(action) = item.action.take() {
                        self.window_menu.insert(item.id as u32, action);
                    }
                }
                crate::menu::MenuEntry::SubMenu(m) => {
                    self.update_window_menu(m);
                }
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

pub(crate) fn get_current_window_handle() -> Option<WindowHandle> {
    let view_id = get_current_view();
    WINDOW_HANDLES.with(|window_handles| window_handles.borrow().get(&view_id).cloned())
}

impl<V: View> WinHandler for AppHandle<V> {
    fn connect(&mut self, handle: &glazier::WindowHandle) {
        WINDOWS.with(|windows| {
            windows.borrow_mut().insert(self.window_id, handle.clone());
        });
        WINDOW_HANDLES.with(|window_handles| {
            window_handles
                .borrow_mut()
                .insert(self.view.id(), handle.clone());
        });
        self.app_state.handle = handle.clone();
        self.paint_state.connect(handle);
        self.handle = handle.clone();
        let size = handle.get_size();
        self.app_state.set_root_size(size);
        if let Some(idle_handle) = handle.get_idle_handle() {
            EXT_EVENT_HANDLER
                .handle
                .lock()
                .insert(self.view.id(), idle_handle);
        }
        self.idle();
    }

    fn scale(&mut self, scale: Scale) {
        let scale = Scale::new(
            scale.x() * self.app_state.scale,
            scale.y() * self.app_state.scale,
        );
        self.paint_state.set_scale(scale);
        self.request_paint();
    }

    fn size(&mut self, size: glazier::kurbo::Size) {
        self.window_size = size;
        self.app_state.update_screen_size_bp(size);
        self.event(Event::WindowResized(size));
        let scale = self.handle.get_scale().unwrap_or_default();
        let scale = Scale::new(
            scale.x() * self.app_state.scale,
            scale.y() * self.app_state.scale,
        );
        self.paint_state.resize(scale, size / self.app_state.scale);
        self.app_state.set_root_size(size);
        self.layout();
        self.process_update();
        self.request_paint();
    }

    fn position(&mut self, point: Point) {
        self.event(Event::WindowMoved(point));
    }

    fn prepare_paint(&mut self) {}

    fn paint(&mut self, _invalid: &glazier::Region) {
        if self.closed {
            return;
        }
        set_current_view(self.view.id());
        self.paint();
    }

    fn key_down(&mut self, event: glazier::KeyEvent) -> bool {
        assert_eq!(event.state, glazier::KeyState::Down);
        self.event(Event::KeyDown(event));
        true
    }

    fn key_up(&mut self, event: glazier::KeyEvent) {
        assert_eq!(event.state, glazier::KeyState::Up);
        self.event(Event::KeyUp(event));
    }

    fn pointer_down(&mut self, event: glazier::PointerEvent) {
        self.event(Event::PointerDown(event.clone()));
    }

    fn pointer_up(&mut self, event: glazier::PointerEvent) {
        self.event(Event::PointerUp(event.clone()));
    }

    fn pointer_move(&mut self, event: glazier::PointerEvent) {
        self.event(Event::PointerMove(event.clone()));
    }

    fn wheel(&mut self, event: glazier::PointerEvent) {
        self.event(Event::PointerWheel(event.clone()));
    }

    fn idle(&mut self, _token: glazier::IdleToken) {
        self.idle();
    }

    fn command(&mut self, id: u32) {
        set_current_view(self.view.id());
        if let Some(action) = self.window_menu.get(&id) {
            (*action)();
            self.process_update();
        } else if let Some(action) = self.app_state.context_menu.get(&id) {
            (*action)();
            self.process_update();
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        &mut self.app_state
    }

    fn open_file(&mut self, token: FileDialogToken, file: Option<FileInfo>) {
        set_current_view(self.view.id());
        if let Some(action) = self.file_dialogs.remove(&token) {
            action(file);
        }
    }

    fn save_as(&mut self, token: FileDialogToken, file: Option<FileInfo>) {
        set_current_view(self.view.id());
        if let Some(action) = self.file_dialogs.remove(&token) {
            action(file);
        }
    }

    fn timer(&mut self, token: TimerToken) {
        set_current_view(self.view.id());
        if let Some(action) = self.app_state.timers.remove(&token) {
            action(token);
        }
        self.process_update();
    }

    fn request_close(&mut self) {
        self.handle.close();
    }

    fn got_focus(&mut self) {
        {
            *EXT_EVENT_HANDLER.active.lock() = self.view.id();
        }
        self.event(Event::WindowGotFocus);
    }

    fn lost_focus(&mut self) {
        self.event(Event::WindowLostFocus);
    }

    fn destroy(&mut self) {
        self.closed = true;
        self.event(Event::WindowClosed);
        let windows_len = WINDOWS.with(|windows| {
            let mut windows = windows.borrow_mut();
            windows.remove(&self.window_id);
            windows.len()
        });
        WINDOW_HANDLES.with(|window_handles| {
            window_handles.borrow_mut().remove(&self.view.id());
        });
        self.scope.dispose();
        EXT_EVENT_HANDLER.handle.lock().remove(&self.view.id());
        EXT_EVENT_HANDLER.queue.lock().remove(&self.view.id());
        if windows_len == 0 {
            #[cfg(not(target_os = "macos"))]
            glazier::Application::global().quit();
        }
    }
}
