use glazier::{
    kurbo::{Point, Size},
    KeyEvent, PointerEvent,
};

#[derive(Hash, PartialEq, Eq)]
pub enum EventListener {
    KeyDown,
    KeyUp,
    Click,
    DoubleClick,
    SecondaryClick,
    DragStart,
    DragEnd,
    DragOver,
    DragEnter,
    DragLeave,
    Drop,
    PointerDown,
    PointerMove,
    PointerUp,
    PointerEnter,
    PointerLeave,
    PointerWheel,
    FocusGained,
    FocusLost,
    WindowClosed,
    WindowResized,
    WindowMoved,
}

#[derive(Debug, Clone)]
pub enum Event {
    PointerDown(PointerEvent),
    PointerUp(PointerEvent),
    PointerMove(PointerEvent),
    PointerWheel(PointerEvent),
    KeyDown(KeyEvent),
    KeyUp(KeyEvent),
    WindowClosed,
    WindowResized(Size),
    WindowMoved(Point),
}

impl Event {
    pub fn needs_focus(&self) -> bool {
        match self {
            Event::PointerDown(_)
            | Event::PointerUp(_)
            | Event::PointerMove(_)
            | Event::PointerWheel(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_) => false,
            Event::KeyDown(_) | Event::KeyUp(_) => true,
        }
    }

    pub(crate) fn is_pointer(&self) -> bool {
        match self {
            Event::PointerDown(_)
            | Event::PointerUp(_)
            | Event::PointerMove(_)
            | Event::PointerWheel(_) => true,
            Event::KeyDown(_)
            | Event::KeyUp(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_) => false,
        }
    }

    /// Enter, numpad enter and space cause a view to be activated with the keyboard
    pub(crate) fn is_keyboard_trigger(&self) -> bool {
        match self {
            Event::KeyDown(key) | Event::KeyUp(key) => matches!(
                key.code,
                glazier::Code::NumpadEnter | glazier::Code::Enter | glazier::Code::Space,
            ),
            _ => false,
        }
    }

    pub fn allow_disabled(&self) -> bool {
        match self {
            Event::PointerDown(_)
            | Event::PointerUp(_)
            | Event::PointerWheel(_)
            | Event::KeyDown(_)
            | Event::KeyUp(_) => false,
            Event::PointerMove(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_) => true,
        }
    }

    pub fn point(&self) -> Option<Point> {
        match self {
            Event::PointerDown(pointer_event)
            | Event::PointerUp(pointer_event)
            | Event::PointerMove(pointer_event)
            | Event::PointerWheel(pointer_event) => Some(pointer_event.pos),
            Event::KeyDown(_)
            | Event::KeyUp(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_) => None,
        }
    }

    pub fn scale(mut self, scale: f64) -> Event {
        match &mut self {
            Event::PointerDown(pointer_event)
            | Event::PointerUp(pointer_event)
            | Event::PointerMove(pointer_event)
            | Event::PointerWheel(pointer_event) => {
                pointer_event.pos.x /= scale;
                pointer_event.pos.y /= scale;
            }
            Event::KeyDown(_)
            | Event::KeyUp(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_) => {}
        }
        self
    }

    pub fn offset(mut self, offset: (f64, f64)) -> Event {
        match &mut self {
            Event::PointerDown(pointer_event)
            | Event::PointerUp(pointer_event)
            | Event::PointerMove(pointer_event)
            | Event::PointerWheel(pointer_event) => {
                pointer_event.pos -= offset;
            }
            Event::KeyDown(_)
            | Event::KeyUp(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_) => {}
        }
        self
    }

    pub fn listener(&self) -> Option<EventListener> {
        match self {
            Event::PointerDown(_) => Some(EventListener::PointerDown),
            Event::PointerUp(_) => Some(EventListener::PointerUp),
            Event::PointerMove(_) => Some(EventListener::PointerMove),
            Event::PointerWheel(_) => Some(EventListener::PointerWheel),
            Event::KeyDown(_) => Some(EventListener::KeyDown),
            Event::KeyUp(_) => Some(EventListener::KeyDown),
            Event::WindowClosed => Some(EventListener::WindowClosed),
            Event::WindowResized(_) => Some(EventListener::WindowResized),
            Event::WindowMoved(_) => Some(EventListener::WindowMoved),
        }
    }
}
