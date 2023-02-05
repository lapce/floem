use glazier::{
    keyboard_types::KeyboardEvent,
    kurbo::{Point, Vec2},
    Modifiers, MouseButton, MouseButtons,
};

#[derive(Hash, PartialEq, Eq)]
pub enum EventListner {
    KeyDown,
}

#[derive(Debug, Clone)]
pub enum Event {
    MouseDown(MouseEvent),
    MouseUp(MouseEvent),
    MouseMove(MouseEvent),
    MouseWheel(MouseEvent),
    KeyDown(KeyboardEvent),
}

impl Event {
    pub fn point(&self) -> Option<Point> {
        match self {
            Event::MouseDown(mouse_event)
            | Event::MouseUp(mouse_event)
            | Event::MouseMove(mouse_event)
            | Event::MouseWheel(mouse_event) => Some(mouse_event.pos),
            Event::KeyDown(_) => None,
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
            Event::KeyDown(_) => {}
        }
        self
    }

    pub fn listener(&self) -> Option<EventListner> {
        match self {
            Event::MouseDown(_) => None,
            Event::MouseUp(_) => None,
            Event::MouseMove(_) => None,
            Event::MouseWheel(_) => None,
            Event::KeyDown(_) => Some(EventListner::KeyDown),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MouseEvent {
    /// The position of the mouse in the coordinate space of the receiver.
    pub pos: Point,
    /// The position of the mose in the window coordinate space.
    pub window_pos: Point,
    pub buttons: MouseButtons,
    pub mods: Modifiers,
    pub count: u8,
    pub focus: bool,
    pub button: MouseButton,
    pub wheel_delta: Vec2,
}

impl<'a> From<&'a glazier::MouseEvent> for MouseEvent {
    fn from(src: &glazier::MouseEvent) -> MouseEvent {
        let glazier::MouseEvent {
            pos,
            buttons,
            mods,
            count,
            focus,
            button,
            wheel_delta,
        } = src;
        MouseEvent {
            pos: *pos,
            window_pos: *pos,
            buttons: *buttons,
            mods: *mods,
            count: *count,
            focus: *focus,
            button: *button,
            wheel_delta: *wheel_delta,
        }
    }
}
