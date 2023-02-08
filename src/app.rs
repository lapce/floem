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
    pub fn update_focus(&self, id: Id) {
        UPDATE_MESSAGES.with(|msgs| msgs.borrow_mut().push(UpdateMessage::Focus(id)));
    }

    pub fn update_state(id: Id, state: impl Any) {
        UPDATE_MESSAGES.with(|msgs| {
            msgs.borrow_mut().push(UpdateMessage::State {
                id,
                state: Box::new(state),
            })
        });
    }

    pub fn update_style(id: Id, style: Style) {
        UPDATE_MESSAGES.with(|msgs| msgs.borrow_mut().push(UpdateMessage::Style { id, style }));
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
    State {
        id: Id,
        state: Box<dyn Any>,
    },
    Style {
        id: Id,
        style: Style,
    },
    EventListener {
        id: Id,
        listener: EventListner,
        action: Box<EventCallback>,
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
            app_state: &mut self.app_state,
        };

        let mut flags = ChangeFlags::empty();
        if !cx.app_state.layout_changed.is_empty() {
            flags |= ChangeFlags::LAYOUT;
        }

        IDPATHS.with(|paths| {
            UPDATE_MESSAGES.with(|msgs| {
                let msgs = msgs.take();
                for msg in msgs {
                    match msg {
                        UpdateMessage::Focus(id) => {
                            cx.app_state.focus = Some(id);
                        }
                        UpdateMessage::State { id, state } => {
                            if let Some(id_path) = paths.borrow().get(&id) {
                                flags |= self.view.update_main(&mut cx, &id_path.0, state);
                            }
                        }
                        UpdateMessage::Style { id, style } => {
                            flags |= ChangeFlags::LAYOUT;
                            let state = cx.app_state.view_states.entry(id).or_default();
                            state.style = style;
                            cx.request_layout(id);
                        }
                        UpdateMessage::EventListener {
                            id,
                            listener,
                            action,
                        } => {
                            let state = cx.app_state.view_states.entry(id).or_default();
                            state.event_listeners.insert(listener, action);
                        }
                    }
                }
            });
        });

        cx.app_state.process_layout_changed();

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
        self.event(Event::MouseDown(event.clone()));
    }

    fn mouse_up(&mut self, event: &glazier::MouseEvent) {
        self.event(Event::MouseUp(event.clone()));
    }

    fn mouse_move(&mut self, event: &glazier::MouseEvent) {
        self.event(Event::MouseMove(event.clone()));
    }

    fn wheel(&mut self, event: &glazier::MouseEvent) {
        self.event(Event::MouseWheel(event.clone()));
    }

    fn idle(&mut self, token: glazier::IdleToken) {
        self.idle();
    }

    fn as_any(&mut self) -> &mut dyn Any {
        todo!()
    }
}
