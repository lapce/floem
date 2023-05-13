use glazier::{
    kurbo::{Point, Size},
    KeyEvent, PointerEvent,
};

#[derive(Hash, PartialEq, Eq)]
pub enum EventListner {
    KeyDown,
    Click,
    PointerEnter,
    PointerLeave,
    DoubleClick,
    PointerWheel,
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

    pub fn listener(&self) -> Option<EventListner> {
        match self {
            Event::PointerDown(_) => None,
            Event::PointerUp(_) => None,
            Event::PointerMove(_) => None,
            Event::PointerWheel(_) => Some(EventListner::PointerWheel),
            Event::KeyDown(_) | Event::KeyUp(_) => Some(EventListner::KeyDown),
            Event::WindowClosed => Some(EventListner::WindowClosed),
            Event::WindowResized(_) => Some(EventListner::WindowResized),
            Event::WindowMoved(_) => Some(EventListner::WindowMoved),
        }
    }
}
