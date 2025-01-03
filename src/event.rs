use floem_winit::{
    keyboard::{KeyCode, PhysicalKey},
    window::Theme,
};
use peniko::kurbo::{Affine, Point, Size};
#[cfg(feature = "log")]
use slotmap::Key;

use crate::{
    dropped_file::DroppedFileEvent,
    keyboard::KeyEvent,
    pointer::{PointerInputEvent, PointerMoveEvent, PointerWheelEvent},
    touchpad::TouchpadMagnifyEvent,
    ViewId,
};

pub struct EventResult(pub EventPropagation);

impl EventResult {
    pub fn event_continue() -> Self {
        Self(EventPropagation::Continue)
    }

    pub fn event_stop(_view_id: ViewId, _event: &Event) -> Self {
        #[cfg(feature = "log")]
        match _event {
            Event::PointerDown(_) => {
                log::debug!("PointerDown processed by {}", _view_id.data().as_ffi());
            }
            Event::PointerUp(_) => {
                log::debug!("PointerUp processed by {}", _view_id.data().as_ffi());
            }
            Event::PointerMove(_) => {
                log::debug!("PointerMove processed by {}", _view_id.data().as_ffi());
            }
            Event::PointerWheel(_) => {
                log::debug!("PointerWheel processed by {}", _view_id.data().as_ffi());
            }
            Event::PointerLeave => {
                log::debug!("PointerLeave processed by {}", _view_id.data().as_ffi());
            }
            Event::DroppedFile(_) => {
                log::debug!("DroppedFile processed by {}", _view_id.data().as_ffi());
            }
            Event::KeyDown(_) => {
                log::debug!("KeyDown processed by {}", _view_id.data().as_ffi());
            }
            Event::KeyUp(_) => {
                log::debug!("KeyUp processed by {}", _view_id.data().as_ffi());
            }
            Event::ImeEnabled => {
                log::debug!("ImeEnabled processed by {}", _view_id.data().as_ffi());
            }
            Event::ImeDisabled => {
                log::debug!("ImeDisabled processed by {}", _view_id.data().as_ffi());
            }
            Event::ImePreedit { .. } => {
                log::debug!("ImePreedit processed by {}", _view_id.data().as_ffi());
            }
            Event::ImeCommit(_) => {
                log::debug!("ImeCommit processed by {}", _view_id.data().as_ffi());
            }
            Event::WindowGotFocus => {
                log::debug!("WindowGotFocus processed by {}", _view_id.data().as_ffi());
            }
            Event::WindowLostFocus => {
                log::debug!("WindowLostFocus processed by {}", _view_id.data().as_ffi());
            }
            Event::WindowClosed => {
                log::debug!("WindowClosed processed by {}", _view_id.data().as_ffi());
            }
            Event::WindowResized(_) => {
                log::debug!("WindowResized processed by {}", _view_id.data().as_ffi());
            }
            Event::WindowMoved(_) => {
                log::debug!("WindowMoved processed by {}", _view_id.data().as_ffi());
            }
            Event::WindowMaximizeChanged(_) => {
                log::debug!(
                    "WindowMaximizeChanged processed by {}",
                    _view_id.data().as_ffi()
                );
            }
            Event::ThemeChanged(_) => {
                log::debug!("ThemeChanged processed by {}", _view_id.data().as_ffi());
            }
            Event::FocusGained => {
                log::debug!("FocusGained processed by {}", _view_id.data().as_ffi());
            }
            Event::FocusLost => {
                log::debug!("FocusLost processed by {}", _view_id.data().as_ffi());
            }
            Event::TouchpadMagnify(_) => {}
        }
        Self(EventPropagation::Stop)
    }
    pub fn is_processed(&self) -> bool {
        self.0.is_processed()
    }
}

/// Control whether an event will continue propagating or whether it should stop.
pub enum EventPropagation {
    /// Stop event propagation and mark the event as processed
    Stop,
    /// Let event propagation continue
    Continue,
}

impl EventPropagation {
    pub fn is_continue(&self) -> bool {
        matches!(self, EventPropagation::Continue)
    }

    pub fn is_stop(&self) -> bool {
        matches!(self, EventPropagation::Stop)
    }

    pub fn is_processed(&self) -> bool {
        matches!(self, EventPropagation::Stop)
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Copy, Clone)]
pub enum EventListener {
    /// Receives [`Event::KeyDown`]
    KeyDown,
    /// Receives [`Event::KeyUp`]
    KeyUp,
    /// Receives [`Event::PointerUp`] or [`Event::KeyDown`]
    /// `KeyDown` occurs when using enter on a focused element, such as a button.
    Click,
    /// Receives [`Event::PointerUp`]
    DoubleClick,
    /// Receives [`Event::PointerUp`]
    SecondaryClick,
    /// Receives [`Event::PointerMove`]
    DragStart,
    /// Receives [`Event::PointerUp`]
    DragEnd,
    /// Receives [`Event::PointerMove`]
    DragOver,
    /// Receives [`Event::PointerMove`]
    DragEnter,
    /// Receives [`Event::PointerMove`]
    DragLeave,
    /// Receives [`Event::PointerUp`]
    Drop,
    /// Receives [`Event::PointerDown`]
    PointerDown,
    /// Receives [`Event::PointerMove`]
    PointerMove,
    /// Receives [`Event::PointerUp`]
    PointerUp,
    /// Receives [`Event::PointerMove`]
    PointerEnter,
    /// Receives [`Event::PointerLeave`]
    PointerLeave,
    /// Receives [`Event::TouchpadMagnify`]
    TouchpadMagnify,
    /// Receives [`Event::ImeEnabled`]
    ImeEnabled,
    /// Receives [`Event::ImeDisabled`]
    ImeDisabled,
    /// Receives [`Event::ImePreedit`]
    ImePreedit,
    /// Receives [`Event::ImeCommit`]
    ImeCommit,
    /// Receives [`Event::PointerWheel`]
    PointerWheel,
    /// Receives [`Event::FocusGained`]
    FocusGained,
    /// Receives [`Event::FocusLost`]
    FocusLost,
    /// Receives [`Event::ThemeChanged`]
    ThemeChanged,
    /// Receives [`Event::WindowClosed`]
    WindowClosed,
    /// Receives [`Event::WindowResized`]
    WindowResized,
    /// Receives [`Event::WindowMoved`]
    WindowMoved,
    /// Receives [`Event::WindowGotFocus`]
    WindowGotFocus,
    /// Receives [`Event::WindowLostFocus`]
    WindowLostFocus,
    /// Receives [`Event::WindowMaximizeChanged`]
    WindowMaximizeChanged,
    /// Receives [`Event::DroppedFile`]
    DroppedFile,
}

#[derive(Debug, Clone)]
pub enum Event {
    PointerDown(PointerInputEvent),
    PointerUp(PointerInputEvent),
    PointerMove(PointerMoveEvent),
    PointerWheel(PointerWheelEvent),
    PointerLeave,
    TouchpadMagnify(TouchpadMagnifyEvent),
    DroppedFile(DroppedFileEvent),
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
    ThemeChanged(Theme),
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
            | Event::PointerLeave
            | Event::TouchpadMagnify(..)
            | Event::FocusGained
            | Event::FocusLost
            | Event::ImeEnabled
            | Event::ImeDisabled
            | Event::ImePreedit { .. }
            | Event::ImeCommit(_)
            | Event::ThemeChanged(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_)
            | Event::WindowMaximizeChanged(_)
            | Event::WindowGotFocus
            | Event::WindowLostFocus
            | Event::DroppedFile(_) => false,
            Event::KeyDown(_) | Event::KeyUp(_) => true,
        }
    }

    pub(crate) fn is_pointer(&self) -> bool {
        match self {
            Event::PointerDown(_)
            | Event::PointerUp(_)
            | Event::PointerMove(_)
            | Event::PointerWheel(_)
            | Event::PointerLeave => true,
            Event::TouchpadMagnify(_)
            | Event::KeyDown(_)
            | Event::KeyUp(_)
            | Event::FocusGained
            | Event::FocusLost
            | Event::ImeEnabled
            | Event::ImeDisabled
            | Event::ImePreedit { .. }
            | Event::ImeCommit(_)
            | Event::ThemeChanged(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_)
            | Event::WindowMaximizeChanged(_)
            | Event::WindowGotFocus
            | Event::WindowLostFocus
            | Event::DroppedFile(_) => false,
        }
    }

    /// Enter, numpad enter and space cause a view to be activated with the keyboard
    pub(crate) fn is_keyboard_trigger(&self) -> bool {
        match self {
            Event::KeyDown(key) | Event::KeyUp(key) => {
                matches!(
                    key.key.physical_key,
                    PhysicalKey::Code(KeyCode::NumpadEnter)
                        | PhysicalKey::Code(KeyCode::Enter)
                        | PhysicalKey::Code(KeyCode::Space),
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
            Event::TouchpadMagnify(_)
            | Event::PointerLeave
            | Event::PointerMove(_)
            | Event::ThemeChanged(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_)
            | Event::WindowGotFocus
            | Event::WindowMaximizeChanged(_)
            | Event::WindowLostFocus
            | Event::DroppedFile(_) => true,
        }
    }

    pub fn point(&self) -> Option<Point> {
        match self {
            Event::PointerDown(pointer_event) | Event::PointerUp(pointer_event) => {
                Some(pointer_event.pos)
            }
            Event::PointerMove(pointer_event) => Some(pointer_event.pos),
            Event::PointerWheel(pointer_event) => Some(pointer_event.pos),
            Event::DroppedFile(event) => Some(event.pos),
            Event::TouchpadMagnify(_)
            | Event::PointerLeave
            | Event::KeyDown(_)
            | Event::KeyUp(_)
            | Event::FocusGained
            | Event::FocusLost
            | Event::ImeEnabled
            | Event::ImeDisabled
            | Event::ImePreedit { .. }
            | Event::ThemeChanged(_)
            | Event::ImeCommit(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_)
            | Event::WindowMaximizeChanged(_)
            | Event::WindowGotFocus
            | Event::WindowLostFocus => None,
        }
    }

    pub fn transform(mut self, transform: Affine) -> Event {
        match &mut self {
            Event::PointerDown(pointer_event) | Event::PointerUp(pointer_event) => {
                pointer_event.pos = transform.inverse() * pointer_event.pos;
            }
            Event::PointerMove(pointer_event) => {
                pointer_event.pos = transform.inverse() * pointer_event.pos;
            }
            Event::PointerWheel(pointer_event) => {
                pointer_event.pos = transform.inverse() * pointer_event.pos;
            }
            Event::DroppedFile(event) => {
                event.pos = transform.inverse() * event.pos;
            }
            Event::TouchpadMagnify(_)
            | Event::PointerLeave
            | Event::KeyDown(_)
            | Event::KeyUp(_)
            | Event::FocusGained
            | Event::FocusLost
            | Event::ImeEnabled
            | Event::ImeDisabled
            | Event::ImePreedit { .. }
            | Event::ThemeChanged(_)
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
            Event::PointerLeave => Some(EventListener::PointerLeave),
            Event::TouchpadMagnify(_) => Some(EventListener::TouchpadMagnify),
            Event::KeyDown(_) => Some(EventListener::KeyDown),
            Event::KeyUp(_) => Some(EventListener::KeyUp),
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
            Event::ThemeChanged(_) => Some(EventListener::ThemeChanged),
            Event::DroppedFile(_) => Some(EventListener::DroppedFile),
        }
    }
}
