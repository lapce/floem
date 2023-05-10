use std::{any::Any, collections::HashMap};

use floem_renderer::Renderer;
use glazier::kurbo::{Affine, Point, Rect};
use glazier::{FileDialogOptions, FileDialogToken, FileInfo, TimerToken, WinHandler};
use leptos_reactive::{Scope, SignalSet};

use crate::{
    context::{
        AppContextStore, AppState, EventCallback, EventCx, LayoutCx, PaintCx, PaintState,
        ResizeCallback, ResizeListener, UpdateCx, APP_CONTEXT_STORE,
    },
    event::{Event, EventListner},
    ext_event::{EXT_EVENT_HANDLER, WRITE_SIGNALS},
    id::{Id, IDPATHS},
    responsive::ScreenSize,
    style::{CursorStyle, Style},
    view::{ChangeFlags, View},
};

thread_local! {
    pub(crate) static UPDATE_MESSAGES: std::cell::RefCell<HashMap<Id, Vec<UpdateMessage>>> = Default::default();
    pub(crate) static DEFERRED_UPDATE_MESSAGES: std::cell::RefCell<DeferredUpdateMessages> = Default::default();
}

pub type FileDialogs = HashMap<FileDialogToken, Box<dyn Fn(Option<FileInfo>)>>;
type DeferredUpdateMessages = HashMap<Id, Vec<(Id, Box<dyn Any>)>>;

pub struct AppHandle<V: View> {
    scope: Scope,
    view: V,
    handle: glazier::WindowHandle,
    app_state: AppState,
    paint_state: PaintState,

    file_dialogs: FileDialogs,
    timers: HashMap<TimerToken, Box<dyn FnOnce()>>,
}

#[derive(Copy, Clone)]
pub struct AppContext {
    pub scope: Scope,
    pub id: Id,
}

impl AppContext {
    pub fn save() {
        APP_CONTEXT_STORE.with(|store| {
            let mut store = store.borrow_mut();
            if let Some(store) = store.as_mut() {
                store.save();
            }
        })
    }

    pub fn set_current(cx: AppContext) {
        APP_CONTEXT_STORE.with(|store| {
            let mut store = store.borrow_mut();
            if let Some(store) = store.as_mut() {
                store.set_current(cx);
            } else {
                *store = Some(AppContextStore {
                    cx,
                    saved_cx: Vec::new(),
                });
            }
        })
    }

    pub fn get_current() -> AppContext {
        APP_CONTEXT_STORE.with(|store| {
            let store = store.borrow();
            store.as_ref().unwrap().cx
        })
    }

    pub fn restore() {
        APP_CONTEXT_STORE.with(|store| {
            let mut store = store.borrow_mut();
            if let Some(store) = store.as_mut() {
                store.restore();
            }
        })
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
}

pub enum UpdateMessage {
    Focus(Id),
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
    KeyboardNavigatable {
        id: Id,
    },
    CursorStyle {
        cursor: CursorStyle,
    },
    EventListener {
        id: Id,
        listener: EventListner,
        action: Box<EventCallback>,
    },
    ResizeListener {
        id: Id,
        action: Box<ResizeCallback>,
    },
    OpenFile {
        options: FileDialogOptions,
        file_info_action: Box<dyn Fn(Option<FileInfo>)>,
    },
    RequestTimer {
        deadline: std::time::Duration,
        action: Box<dyn FnOnce()>,
    },
}

impl<V: View> Drop for AppHandle<V> {
    fn drop(&mut self) {
        self.scope.dispose();
    }
}

impl<V: View> AppHandle<V> {
    pub fn new(scope: Scope, app_logic: impl FnOnce(AppContext) -> V) -> Self {
        let cx = AppContext {
            scope,
            id: Id::next(),
        };

        AppContext::set_current(cx);

        let view = app_logic(cx);
        Self {
            scope,
            view,
            app_state: AppState::new(),
            paint_state: PaintState::new(),
            handle: Default::default(),

            file_dialogs: HashMap::new(),
            timers: HashMap::new(),
        }
    }

    fn layout(&mut self) {
        let mut cx = LayoutCx {
            app_state: &mut self.app_state,
            viewport: None,
            font_size: None,
            font_family: None,
            font_weight: None,
            font_style: None,
            line_height: None,
            window_origin: Point::ZERO,
            saved_viewports: Vec::new(),
            saved_font_sizes: Vec::new(),
            saved_font_families: Vec::new(),
            saved_font_weights: Vec::new(),
            saved_font_styles: Vec::new(),
            saved_line_heights: Vec::new(),
            saved_window_origins: Vec::new(),
        };
        cx.app_state.root = Some(self.view.layout_main(&mut cx));
        cx.app_state.compute_layout();

        cx.clear();
        self.view.compute_layout_main(&mut cx);
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
            saved_transforms: Vec::new(),
            saved_clips: Vec::new(),
            saved_colors: Vec::new(),
            saved_font_sizes: Vec::new(),
            saved_font_families: Vec::new(),
            saved_font_weights: Vec::new(),
            saved_font_styles: Vec::new(),
            saved_line_heights: Vec::new(),
        };
        cx.paint_state.renderer.as_mut().unwrap().begin();
        self.view.paint_main(&mut cx);
        cx.paint_state.renderer.as_mut().unwrap().finish();
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
            let id_path = IDPATHS.with(|paths| paths.borrow().get(&id).cloned());
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
            let mut cx = UpdateCx {
                app_state: &mut self.app_state,
            };
            for msg in msgs {
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
                        let id_path = IDPATHS.with(|paths| paths.borrow().get(&id).cloned());
                        if let Some(id_path) = id_path {
                            flags |= self.view.update_main(&mut cx, &id_path.0, state);
                        }
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
                        }
                        cx.request_layout(id);
                    }
                    UpdateMessage::KeyboardNavigatable { id } => {
                        cx.app_state.keyboard_navigatable.insert(id);
                    }
                    UpdateMessage::CursorStyle { cursor } => {
                        cx.app_state.cursor = cursor;
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
                            window_origin: Point::ZERO,
                            rect: Rect::ZERO,
                            callback: action,
                        });
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
                    UpdateMessage::RequestTimer { deadline, action } => {
                        let token = self.handle.request_timer(deadline);
                        self.timers.insert(token, action);
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
        DEFERRED_UPDATE_MESSAGES.with(|m| !m.borrow().is_empty())
    }

    pub fn process_update(&mut self) {
        let mut flags = ChangeFlags::empty();
        loop {
            flags |= self.process_update_messages();
            if !self.needs_layout() && !self.has_deferred_update_messages() {
                break;
            }
            flags |= ChangeFlags::LAYOUT;
            self.layout();
            flags |= self.process_deferred_update_messages();
        }

        let glazier_cursor = match self.app_state.cursor {
            CursorStyle::Default => glazier::Cursor::Arrow,
            CursorStyle::Pointer => glazier::Cursor::Pointer,
            CursorStyle::Text => glazier::Cursor::IBeam,
        };
        self.handle.set_cursor(&glazier_cursor);

        if !flags.is_empty() {
            self.handle.invalidate();
        }
    }

    pub fn event(&mut self, event: Event) {
        let mut cx = EventCx {
            app_state: &mut self.app_state,
        };

        let is_pointer_move = matches!(&event, Event::PointerMove(_));
        let was_hovered = if is_pointer_move {
            cx.app_state.cursor = CursorStyle::Default;
            let was_hovered = cx.app_state.hovered.clone();
            cx.app_state.hovered.clear();
            Some(was_hovered)
        } else {
            None
        };

        if event.needs_focus() {
            let mut processed = false;
            if let Some(listener) = event.listener() {
                if let Some(action) = cx.get_event_listener(self.view.id(), &listener) {
                    processed |= (*action)(&event);
                }
            }

            if !processed {
                if let Some(id) = cx.app_state.focus {
                    IDPATHS.with(|paths| {
                        if let Some(id_path) = paths.borrow().get(&id) {
                            processed |=
                                self.view
                                    .event_main(&mut cx, Some(&id_path.0), event.clone());
                        }
                    });
                }

                if !processed {
                    if let Event::KeyDown(glazier::KeyEvent { key, mods, .. }) = &event {
                        if key == &glazier::KbKey::Tab {
                            let backwards = mods.contains(glazier::Modifiers::SHIFT);
                            self.view.tab_navigation(cx.app_state, backwards);
                        } else if let glazier::KbKey::Character(character) = key {
                            // 'I' displays some debug information
                            if character.eq_ignore_ascii_case("i") {
                                self.view.debug_tree();
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
            let id = cx.app_state.active.unwrap();
            IDPATHS.with(|paths| {
                if let Some(id_path) = paths.borrow().get(&id) {
                    self.view
                        .event_main(&mut cx, Some(&id_path.0), event.clone());
                }
            });
            if let Event::PointerUp(_) = &event {
                // To remove the styles applied by the Active selector
                if cx.app_state.has_style_for_sel(id, StyleSelector::Active) {
                    cx.app_state.request_layout(id);
                }

                cx.app_state.active = None;
            }
        } else {
            self.view.event_main(&mut cx, None, event);
        }

        if is_pointer_move {
            for id in was_hovered
                .unwrap()
                .symmetric_difference(&cx.app_state.hovered.clone())
            {
                let view_state = cx.app_state.view_state(*id);
                if view_state.hover_style.is_some() || view_state.active_style.is_some() {
                    cx.app_state.request_layout(*id);
                }
            }
        }

        self.process_update();
    }

    fn idle(&mut self) {
        while let Some(id) = EXT_EVENT_HANDLER.queue.lock().pop_front() {
            let write = WRITE_SIGNALS.with(|signals| signals.borrow_mut().get(&id).cloned());
            if let Some(write) = write {
                write.set(Some(()));
            }
        }
        self.process_update();
    }
}

impl<V: View> WinHandler for AppHandle<V> {
    fn connect(&mut self, handle: &glazier::WindowHandle) {
        self.paint_state.connect(handle);
        self.handle = handle.clone();
        let size = handle.get_size();
        self.app_state.set_root_size(size);
        if let Some(idle_handle) = handle.get_idle_handle() {
            *EXT_EVENT_HANDLER.handle.lock() = Some(idle_handle);
        }
        self.idle();
    }

    fn size(&mut self, size: glazier::kurbo::Size) {
        self.app_state.update_scr_size_breakpt(size);
        self.event(Event::WindowResized(size));
        let scale = self.handle.get_scale().unwrap_or_default();
        self.paint_state.resize(scale, size);
        self.app_state.set_root_size(size);
        self.layout();
        self.process_update();
        self.handle.invalidate();
    }

    fn position(&mut self, point: Point) {
        self.event(Event::WindowMoved(point));
    }

    fn prepare_paint(&mut self) {}

    fn paint(&mut self, _invalid: &glazier::Region) {
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

    fn pointer_down(&mut self, event: &glazier::PointerEvent) {
        self.event(Event::PointerDown(event.clone()));
    }

    fn pointer_up(&mut self, event: &glazier::PointerEvent) {
        self.event(Event::PointerUp(event.clone()));
    }

    fn pointer_move(&mut self, event: &glazier::PointerEvent) {
        self.event(Event::PointerMove(event.clone()));
    }

    fn wheel(&mut self, event: &glazier::PointerEvent) {
        self.event(Event::PointerWheel(event.clone()));
    }

    fn idle(&mut self, _token: glazier::IdleToken) {
        self.idle();
    }

    fn as_any(&mut self) -> &mut dyn Any {
        todo!()
    }

    fn open_file(&mut self, token: FileDialogToken, file: Option<FileInfo>) {
        if let Some(action) = self.file_dialogs.remove(&token) {
            action(file);
        }
    }

    fn timer(&mut self, token: TimerToken) {
        if let Some(action) = self.timers.remove(&token) {
            action();
        }
        self.process_update();
    }

    fn request_close(&mut self) {
        self.handle.close();
    }

    fn destroy(&mut self) {
        self.event(Event::WindowClosed);
        glazier::Application::global().quit();
    }
}
