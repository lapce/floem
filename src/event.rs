use kurbo::{Point, Size};
use winit::keyboard::KeyCode;

use crate::{
    keyboard::KeyEvent,
    pointer::{PointerInputEvent, PointerMoveEvent, PointerWheelEvent},
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
    ImeEnabled,
    ImeDisabled,
    ImePreedit,
    ImeCommit,
    PointerWheel,
    FocusGained,
    FocusLost,
    WindowClosed,
    WindowResized,
    WindowMoved,
    WindowGotFocus,
    WindowLostFocus,
    WindowMaximizeChanged,
}

#[derive(Debug, Clone)]
pub enum Event {
    PointerDown(PointerInputEvent),
    PointerUp(PointerInputEvent),
    PointerMove(PointerMoveEvent),
    PointerWheel(PointerWheelEvent),
    KeyDown(KeyEvent),
    KeyUp(KeyEvent),
    ImeEnabled,
    ImeDisabled,
    ImePreedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    ImeCommit(String),
    WindowGotFocus,
    WindowLostFocus,
    WindowClosed,
    WindowResized(Size),
    WindowMoved(Point),
    WindowMaximizeChanged(bool),
    FocusGained,
    FocusLost,
}

impl Event {
    pub fn needs_focus(&self) -> bool {
        match self {
            Event::PointerDown(_)
            | Event::PointerUp(_)
            | Event::PointerMove(_)
            | Event::PointerWheel(_)
            | Event::FocusGained
            | Event::FocusLost
            | Event::ImeEnabled
            | Event::ImeDisabled
            | Event::ImePreedit { .. }
            | Event::ImeCommit(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_)
            | Event::WindowMaximizeChanged(_)
            | Event::WindowGotFocus
            | Event::WindowLostFocus => false,
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
            | Event::FocusGained
            | Event::FocusLost
            | Event::ImeEnabled
            | Event::ImeDisabled
            | Event::ImePreedit { .. }
            | Event::ImeCommit(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_)
            | Event::WindowMaximizeChanged(_)
            | Event::WindowGotFocus
            | Event::WindowLostFocus => false,
        }
    }

    /// Enter, numpad enter and space cause a view to be activated with the keyboard
    pub(crate) fn is_keyboard_trigger(&self) -> bool {
        match self {
            Event::KeyDown(key) | Event::KeyUp(key) => {
                matches!(
                    key.key.physical_key,
                    KeyCode::NumpadEnter | KeyCode::Enter | KeyCode::Space,
                )
            }
            _ => false,
        }
    }

    pub fn allow_disabled(&self) -> bool {
        match self {
            Event::PointerDown(_)
            | Event::PointerUp(_)
            | Event::PointerWheel(_)
            | Event::FocusGained
            | Event::FocusLost
            | Event::ImeEnabled
            | Event::ImeDisabled
            | Event::ImePreedit { .. }
            | Event::ImeCommit(_)
            | Event::KeyDown(_)
            | Event::KeyUp(_) => false,
            Event::PointerMove(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_)
            | Event::WindowGotFocus
            | Event::WindowMaximizeChanged(_)
            | Event::WindowLostFocus => true,
        }
    }

    pub fn point(&self) -> Option<Point> {
        match self {
            Event::PointerDown(pointer_event) | Event::PointerUp(pointer_event) => {
                Some(pointer_event.pos)
            }
            Event::PointerMove(pointer_event) => Some(pointer_event.pos),
            Event::PointerWheel(pointer_event) => Some(pointer_event.pos),
            Event::KeyDown(_)
            | Event::KeyUp(_)
            | Event::FocusGained
            | Event::FocusLost
            | Event::ImeEnabled
            | Event::ImeDisabled
            | Event::ImePreedit { .. }
            | Event::ImeCommit(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_)
            | Event::WindowMaximizeChanged(_)
            | Event::WindowGotFocus
            | Event::WindowLostFocus => None,
        }
    }

    pub fn scale(mut self, scale: f64) -> Event {
        match &mut self {
            Event::PointerDown(pointer_event) | Event::PointerUp(pointer_event) => {
                pointer_event.pos.x /= scale;
                pointer_event.pos.y /= scale;
            }
            Event::PointerMove(pointer_event) => {
                pointer_event.pos.x /= scale;
                pointer_event.pos.y /= scale;
            }
            Event::PointerWheel(pointer_event) => {
                pointer_event.pos.x /= scale;
                pointer_event.pos.y /= scale;
            }
            Event::KeyDown(_)
            | Event::KeyUp(_)
            | Event::FocusGained
            | Event::FocusLost
            | Event::ImeEnabled
            | Event::ImeDisabled
            | Event::ImePreedit { .. }
            | Event::ImeCommit(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_)
            | Event::WindowMaximizeChanged(_)
            | Event::WindowGotFocus
            | Event::WindowLostFocus => {}
        }
        self
    }

    pub fn offset(mut self, offset: (f64, f64)) -> Event {
        match &mut self {
            Event::PointerDown(pointer_event) | Event::PointerUp(pointer_event) => {
                pointer_event.pos -= offset;
            }
            Event::PointerMove(pointer_event) => {
                pointer_event.pos -= offset;
            }
            Event::PointerWheel(pointer_event) => {
                pointer_event.pos -= offset;
            }
            Event::KeyDown(_)
            | Event::KeyUp(_)
            | Event::FocusGained
            | Event::FocusLost
            | Event::ImeEnabled
            | Event::ImeDisabled
            | Event::ImePreedit { .. }
            | Event::ImeCommit(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_)
            | Event::WindowMaximizeChanged(_)
            | Event::WindowGotFocus
            | Event::WindowLostFocus => {}
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
            Event::ImeEnabled => Some(EventListener::ImeEnabled),
            Event::ImeDisabled => Some(EventListener::ImeDisabled),
            Event::ImePreedit { .. } => Some(EventListener::ImePreedit),
            Event::ImeCommit(_) => Some(EventListener::ImeCommit),
            Event::WindowClosed => Some(EventListener::WindowClosed),
            Event::WindowResized(_) => Some(EventListener::WindowResized),
            Event::WindowMoved(_) => Some(EventListener::WindowMoved),
            Event::WindowMaximizeChanged(_) => Some(EventListener::WindowMaximizeChanged),
            Event::WindowGotFocus => Some(EventListener::WindowGotFocus),
            Event::WindowLostFocus => Some(EventListener::WindowLostFocus),
            Event::FocusLost => Some(EventListener::FocusLost),
            Event::FocusGained => Some(EventListener::FocusGained),
        }
    }
}
