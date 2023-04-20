use glazier::{
    kurbo::{Point, Size},
    KeyEvent, MouseEvent,
};

#[derive(Hash, PartialEq, Eq)]
pub enum EventListner {
    KeyDown,
    MouseWheel,
    WindowClosed,
    WindowResized,
}

#[derive(Debug, Clone)]
pub enum Event {
    MouseDown(MouseEvent),
    MouseUp(MouseEvent),
    MouseMove(MouseEvent),
    MouseWheel(MouseEvent),
    KeyDown(KeyEvent),
    WindowClosed,
    WindowResized(Size),
}

impl Event {
    pub fn needs_focus(&self) -> bool {
        match self {
            Event::MouseDown(_)
            | Event::MouseUp(_)
            | Event::MouseMove(_)
            | Event::MouseWheel(_)
            | Event::WindowClosed
            | Event::WindowResized(_) => false,
            Event::KeyDown(_) => true,
        }
    }

    pub(crate) fn is_mouse(&self) -> bool {
        match self {
            Event::MouseDown(_)
            | Event::MouseUp(_)
            | Event::MouseMove(_)
            | Event::MouseWheel(_) => true,
            Event::KeyDown(_) | Event::WindowClosed | Event::WindowResized(_) => false,
        }
    }

    pub fn allows_disabled(&self) -> bool {
        match self {
            Event::MouseDown(_) | Event::MouseUp(_) | Event::MouseWheel(_) | Event::KeyDown(_) => {
                false
            }
            Event::WindowClosed | Event::WindowResized(_) | Event::MouseMove(_) => true,
        }
    }

    pub fn point(&self) -> Option<Point> {
        match self {
            Event::MouseDown(mouse_event)
            | Event::MouseUp(mouse_event)
            | Event::MouseMove(mouse_event)
            | Event::MouseWheel(mouse_event) => Some(mouse_event.pos),
            Event::KeyDown(_) | Event::WindowClosed | Event::WindowResized(_) => None,
        }
    }

    pub fn offset(mut self, offset: (f64, f64)) -> Event {
        match &mut self {
            Event::MouseDown(mouse_event)
            | Event::MouseUp(mouse_event)
            | Event::MouseMove(mouse_event)
            | Event::MouseWheel(mouse_event) => {
                mouse_event.pos -= offset;
            }
            Event::KeyDown(_) | Event::WindowClosed | Event::WindowResized(_) => {}
        }
        self
    }

    pub fn listener(&self) -> Option<EventListner> {
        match self {
            Event::MouseDown(_) => None,
            Event::MouseUp(_) => None,
            Event::MouseMove(_) => None,
            Event::MouseWheel(_) => Some(EventListner::MouseWheel),
            Event::KeyDown(_) => Some(EventListner::KeyDown),
            Event::WindowClosed => Some(EventListner::WindowClosed),
            Event::WindowResized(_) => Some(EventListner::WindowResized),
        }
    }
}
