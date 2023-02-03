use std::{any::Any, collections::HashSet};

use glazier::{
    kurbo::{Affine, Size},
    IdleHandle, WinHandler,
};
use leptos_reactive::Scope;
use parley::FontContext;
use taffy::style::Dimension;
use vello::SceneBuilder;

use crate::{
    context::{EventCx, LayoutCx, LayoutState, PaintCx, PaintState, UpdateCx},
    event::Event,
    ext_event::{EXT_EVENT_HANDLER, WRITE_SIGNALS},
    id::{Id, IDPATHS},
    style::Style,
    view::{ChangeFlags, View},
};

thread_local! {
    static UPDATE_MESSAGES: std::cell::RefCell<Vec<UpdateMessage>> = Default::default();
    static STYLE_MESSAGES: std::cell::RefCell<Vec<StyleMessage>> = Default::default();
}

pub struct App<V: View> {
    view: V,
    handle: glazier::WindowHandle,
    layout_state: LayoutState,
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

impl<V: View> App<V> {
    pub fn new(scope: Scope, app_logic: impl Fn(AppContext) -> V) -> Self {
        let context = AppContext {
            scope,
            id: Id::next(),
        };

        let view = app_logic(context);
        Self {
            view,
            layout_state: LayoutState::new(),
            paint_state: PaintState::new(),
            handle: Default::default(),
            font_cx: FontContext::new(),
        }
    }

    fn layout(&mut self) {
        let mut cx = LayoutCx {
            layout_state: &mut self.layout_state,
            font_cx: &mut self.font_cx,
        };
        cx.layout_state.root = Some(self.view.layout(&mut cx));
        cx.layout_state.compute_layout();
    }

    pub fn paint(&mut self) {
        let mut builder = SceneBuilder::for_fragment(&mut self.paint_state.fragment);
        let mut cx = PaintCx {
            layout_state: &mut self.layout_state,
            builder: &mut builder,
            saved_transforms: Vec::new(),
            transform: Affine::IDENTITY,
        };
        self.view.paint_main(&mut cx);
        self.paint_state.render();
    }

    pub fn process_update(&mut self) {
        let mut cx = UpdateCx {
            layout_state: &mut self.layout_state,
        };
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
                    flags |= self.view.update(&mut cx, &id_path.0, message.body);
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
            layout_state: &mut self.layout_state,
        };
        self.view.event(&mut cx, event);
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
        self.layout_state.set_root_size(size);
        if let Some(idle_handle) = handle.get_idle_handle() {
            *EXT_EVENT_HANDLER.handle.lock() = Some(idle_handle);
        }
        self.idle();
    }

    fn size(&mut self, size: glazier::kurbo::Size) {
        self.layout_state.set_root_size(size);
        self.layout();
        self.paint();
    }

    fn prepare_paint(&mut self) {}

    fn paint(&mut self, invalid: &glazier::Region) {
        self.paint();
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
