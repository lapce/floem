use std::{any::Any, collections::HashMap};

use floem_renderer::Renderer;
use glazier::{
    kurbo::{Affine, Point, Rect},
    FileDialogOptions, FileDialogToken, FileInfo, WinHandler,
};
use leptos_reactive::{Scope, SignalSet};

use crate::{
    context::{
        AppState, EventCallback, EventCx, LayoutCx, PaintCx, PaintState, ResizeCallback,
        ResizeListener, UpdateCx,
    },
    event::{Event, EventListner},
    ext_event::{EXT_EVENT_HANDLER, WRITE_SIGNALS},
    id::{Id, IDPATHS},
    style::Style,
    view::{ChangeFlags, View},
};

thread_local! {
    static UPDATE_MESSAGES: std::cell::RefCell<Vec<UpdateMessage>> = Default::default();
    static DEFERRED_UPDATE_MESSAGES: std::cell::RefCell<Vec<(Id, Box<dyn Any>)>> = Default::default();
}

pub struct App<V: View> {
    view: V,
    handle: glazier::WindowHandle,
    app_state: AppState,
    paint_state: PaintState,

    file_dialogs: HashMap<FileDialogToken, Box<dyn Fn(Option<FileInfo>)>>,
}

#[derive(Copy, Clone)]
pub struct AppContext {
    pub scope: Scope,
    pub id: Id,
}

impl AppContext {
    pub fn update_focus(&self, id: Id) {
        UPDATE_MESSAGES.with(|msgs| msgs.borrow_mut().push(UpdateMessage::Focus(id)));
    }

    pub fn request_paint() {
        UPDATE_MESSAGES.with(|msgs| msgs.borrow_mut().push(UpdateMessage::RequestPaint));
    }

    pub fn update_state(id: Id, state: impl Any, deferred: bool) {
        if !deferred {
            UPDATE_MESSAGES.with(|msgs| {
                msgs.borrow_mut().push(UpdateMessage::State {
                    id,
                    state: Box::new(state),
                })
            });
        } else {
            DEFERRED_UPDATE_MESSAGES.with(|msgs| msgs.borrow_mut().push((id, Box::new(state))));
        }
    }

    pub fn update_style(id: Id, style: Style) {
        UPDATE_MESSAGES.with(|msgs| msgs.borrow_mut().push(UpdateMessage::Style { id, style }));
    }

    pub fn update_hover_style(id: Id, style: Style) {
        UPDATE_MESSAGES.with(|msgs| {
            msgs.borrow_mut()
                .push(UpdateMessage::HoverStyle { id, style })
        });
    }

    pub fn update_event_listner(id: Id, listener: EventListner, action: Box<EventCallback>) {
        UPDATE_MESSAGES.with(|msgs| {
            msgs.borrow_mut().push(UpdateMessage::EventListener {
                id,
                listener,
                action,
            })
        });
    }

    pub fn update_resize_listner(id: Id, action: Box<ResizeCallback>) {
        UPDATE_MESSAGES.with(|msgs| {
            msgs.borrow_mut()
                .push(UpdateMessage::ResizeListener { id, action })
        });
    }

    pub fn update_open_file(
        options: FileDialogOptions,
        file_info_action: impl Fn(Option<FileInfo>) + 'static,
    ) {
        UPDATE_MESSAGES.with(|msgs| {
            msgs.borrow_mut().push(UpdateMessage::OpenFile {
                options,
                file_info_action: Box::new(file_info_action),
            })
        });
    }

    pub fn with_id(mut self, id: Id) -> Self {
        self.id = id;
        self
    }

    pub fn new_id(&self) -> Id {
        self.id.new()
    }
}

pub enum UpdateMessage {
    Focus(Id),
    RequestPaint,
    State {
        id: Id,
        state: Box<dyn Any>,
    },
    Style {
        id: Id,
        style: Style,
    },
    HoverStyle {
        id: Id,
        style: Style,
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
}

impl<V: View> App<V> {
    pub fn new(scope: Scope, app_logic: impl Fn(AppContext) -> V) -> Self {
        let context = AppContext {
            scope,
            id: Id::next(),
        };

        let view = app_logic(context);
        Self {
            view,
            app_state: AppState::new(),
            paint_state: PaintState::new(),
            handle: Default::default(),

            file_dialogs: HashMap::new(),
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
            window_origin: Point::ZERO,
            saved_viewports: Vec::new(),
            saved_font_sizes: Vec::new(),
            saved_font_families: Vec::new(),
            saved_font_weights: Vec::new(),
            saved_font_styles: Vec::new(),
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
            saved_transforms: Vec::new(),
            saved_clips: Vec::new(),
            saved_colors: Vec::new(),
            saved_font_sizes: Vec::new(),
            saved_font_families: Vec::new(),
            saved_font_weights: Vec::new(),
            saved_font_styles: Vec::new(),
        };
        cx.paint_state.renderer.as_mut().unwrap().begin();
        self.view.paint_main(&mut cx);
        cx.paint_state.renderer.as_mut().unwrap().finish();
    }

    fn process_deferred_update_messages(&mut self) -> ChangeFlags {
        let mut flags = ChangeFlags::empty();

        let msgs = DEFERRED_UPDATE_MESSAGES.with(|msgs| msgs.take());
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
            let msgs = UPDATE_MESSAGES.with(|msgs| msgs.take());
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
                    UpdateMessage::Focus(id) => {
                        cx.app_state.focus = Some(id);
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
                    UpdateMessage::HoverStyle { id, style } => {
                        let state = cx.app_state.view_state(id);
                        state.hover_style = Some(style);
                        cx.request_layout(id);
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

        if !flags.is_empty() {
            self.handle.invalidate();
        }
    }

    pub fn event(&mut self, event: Event) {
        let mut cx = EventCx {
            app_state: &mut self.app_state,
        };
        if event.needs_focus() {
            if let Some(id) = cx.app_state.focus {
                IDPATHS.with(|paths| {
                    if let Some(id_path) = paths.borrow().get(&id) {
                        self.view.event_main(&mut cx, Some(&id_path.0), event);
                    }
                });
            }
        } else if cx.app_state.active.is_some() && event.is_mouse() {
            let id = cx.app_state.active.unwrap();
            IDPATHS.with(|paths| {
                if let Some(id_path) = paths.borrow().get(&id) {
                    self.view
                        .event_main(&mut cx, Some(&id_path.0), event.clone());
                }
            });
            if let Event::MouseUp(_) = &event {
                self.app_state.active = None;
            }
        } else {
            self.view.event_main(&mut cx, None, event);
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

impl<V: View> WinHandler for App<V> {
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
        self.event(Event::WindowResized(size));
        let scale = self.handle.get_scale().unwrap_or_default();
        self.paint_state.resize(scale, size);
        self.app_state.set_root_size(size);
        self.layout();
        self.process_update();
        self.handle.invalidate();
    }

    fn prepare_paint(&mut self) {}

    fn paint(&mut self, _invalid: &glazier::Region) {
        self.paint();
    }

    fn key_down(&mut self, event: glazier::KeyEvent) -> bool {
        self.event(Event::KeyDown(event));
        true
    }

    fn mouse_down(&mut self, event: &glazier::MouseEvent) {
        self.event(Event::MouseDown(event.clone()));
    }

    fn mouse_up(&mut self, event: &glazier::MouseEvent) {
        self.event(Event::MouseUp(event.clone()));
    }

    fn mouse_move(&mut self, event: &glazier::MouseEvent) {
        self.event(Event::MouseMove(event.clone()));
    }

    fn mouse_wheel(&mut self, event: &glazier::MouseEvent) {
        self.event(Event::MouseWheel(event.clone()));
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

    fn destroy(&mut self) {
        self.event(Event::WindowClosed);
    }
}
