use std::any::Any;

use glazier::{kurbo::Affine, WinHandler};
use leptos_reactive::Scope;
use parley::FontContext;
use vello::SceneBuilder;

use crate::{
    context::{AppState, EventCallback, EventCx, LayoutCx, PaintCx, PaintState, UpdateCx},
    event::{Event, EventListner},
    ext_event::{EXT_EVENT_HANDLER, WRITE_SIGNALS},
    id::{Id, IDPATHS},
    style::Style,
    view::{ChangeFlags, View},
};

thread_local! {
    static UPDATE_MESSAGES: std::cell::RefCell<Vec<UpdateMessage>> = Default::default();
    static STYLE_MESSAGES: std::cell::RefCell<Vec<StyleMessage>> = Default::default();
    static EVENT_LISTNER_MESSAGES: std::cell::RefCell<Vec<EventListnerMessage>> = Default::default();
}

pub struct App<V: View> {
    view: V,
    handle: glazier::WindowHandle,
    app_state: AppState,
    paint_state: PaintState,
    font_cx: FontContext,
}

#[derive(Copy, Clone)]
pub struct AppContext {
    pub scope: Scope,
    pub id: Id,
}

impl AppContext {
    pub fn add_update(message: UpdateMessage) {
        UPDATE_MESSAGES.with(|messages| messages.borrow_mut().push(message));
    }

    pub fn add_style(id: Id, style: Style) {
        STYLE_MESSAGES.with(|messages| messages.borrow_mut().push(StyleMessage::new(id, style)));
    }

    pub fn add_event_listner(id: Id, listener: EventListner, action: Box<EventCallback>) {
        EVENT_LISTNER_MESSAGES.with(|messages| {
            messages.borrow_mut().push(EventListnerMessage {
                id,
                listener,
                action,
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

pub struct UpdateMessage {
    pub id: Id,
    pub body: Box<dyn Any>,
}

impl UpdateMessage {
    pub fn new(id: Id, event: impl Any) -> UpdateMessage {
        UpdateMessage {
            id,
            body: Box::new(event),
        }
    }
}

pub struct StyleMessage {
    pub id: Id,
    pub style: Style,
}

impl StyleMessage {
    pub fn new(id: Id, style: Style) -> StyleMessage {
        StyleMessage { id, style }
    }
}

pub struct EventListnerMessage {
    pub id: Id,
    pub listener: EventListner,
    pub action: Box<EventCallback>,
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
            font_cx: FontContext::new(),
        }
    }

    fn layout(&mut self) {
        let mut cx = LayoutCx {
            layout_state: &mut self.app_state,
            font_cx: &mut self.font_cx,
        };
        cx.layout_state.root = Some(self.view.layout(&mut cx));
        cx.layout_state.compute_layout();
    }

    pub fn paint(&mut self) {
        let mut builder = SceneBuilder::for_fragment(&mut self.paint_state.fragment);
        let mut cx = PaintCx {
            layout_state: &mut self.app_state,
            builder: &mut builder,
            saved_transforms: Vec::new(),
            transform: Affine::IDENTITY,
        };
        self.view.paint_main(&mut cx);
        self.paint_state.render();
    }

    pub fn process_update(&mut self) {
        let mut cx = UpdateCx {
            layout_state: &mut self.app_state,
        };
        EVENT_LISTNER_MESSAGES.with(|messages| {
            let messages = messages.take();
            for msg in messages {
                let state = cx.layout_state.view_states.entry(msg.id).or_default();
                state.event_listeners.insert(msg.listener, msg.action);
            }
        });

        let style_messages = STYLE_MESSAGES.with(|messages| messages.take());
        let mut flags = if style_messages.is_empty() {
            ChangeFlags::empty()
        } else {
            ChangeFlags::LAYOUT
        };
        for msg in style_messages {
            let state = cx.layout_state.view_states.entry(msg.id).or_default();
            state.style = msg.style;
            cx.request_layout(msg.id);
        }

        let messages = UPDATE_MESSAGES.with(|messages| messages.take());
        for message in messages {
            IDPATHS.with(|paths| {
                if let Some(id_path) = paths.borrow().get(&message.id) {
                    flags |= self.view.update_main(&mut cx, &id_path.0, message.body);
                }
            });
        }

        cx.layout_state.process_layout_changed();

        if flags.contains(ChangeFlags::LAYOUT) {
            self.layout();
            self.paint();
        } else if flags.contains(ChangeFlags::PAINT) {
            self.paint();
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
        } else {
            self.view.event_main(&mut cx, None, event);
        }
        self.process_update();
    }

    fn idle(&mut self) {
        while let Some(id) = EXT_EVENT_HANDLER.queue.lock().pop_front() {
            WRITE_SIGNALS.with(|signals| {
                let signals = signals.borrow_mut();
                if let Some(write) = signals.get(&id) {
                    write.set(Some(()));
                }
            });
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
        self.app_state.set_root_size(size);
        self.layout();
        self.paint();
    }

    fn prepare_paint(&mut self) {}

    fn paint(&mut self, invalid: &glazier::Region) {
        self.paint();
    }

    fn key_down(&mut self, event: glazier::KeyEvent) -> bool {
        self.event(Event::KeyDown(event));
        true
    }

    fn mouse_down(&mut self, event: &glazier::MouseEvent) {
        self.event(Event::MouseDown(event.into()));
    }

    fn mouse_up(&mut self, event: &glazier::MouseEvent) {
        self.event(Event::MouseUp(event.into()));
    }

    fn mouse_move(&mut self, event: &glazier::MouseEvent) {
        self.event(Event::MouseMove(event.into()));
    }

    fn wheel(&mut self, event: &glazier::MouseEvent) {
        self.event(Event::MouseWheel(event.into()));
    }

    fn idle(&mut self, token: glazier::IdleToken) {
        self.idle();
    }

    fn as_any(&mut self) -> &mut dyn Any {
        todo!()
    }
}
