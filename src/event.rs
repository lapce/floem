use peniko::kurbo::{Affine, Point, Size};
use ui_events::{
    keyboard::{Code, KeyState, KeyboardEvent},
    pointer::PointerUpdate,
};
use winit::window::Theme;

use crate::dropped_file::FileDragEvent;

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
    /// Receives [`Event::PinchGesture`]
    PinchGesture,
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
    /// Receives [`Event::WindowScaleChanged`]
    WindowScaleChanged,
    /// Receives [`Event::DroppedFile`]
    DroppedFile,
}

pub type PointerEvent = ui_events::pointer::PointerEvent<Point>;

#[derive(Debug, Clone)]
pub enum Event {
    Pointer(PointerEvent),
    FileDrag(FileDragEvent),
    Key(ui_events::keyboard::KeyboardEvent),
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
    WindowScaleChanged(f64),
}

impl Event {
    pub fn needs_focus(&self) -> bool {
        match self {
            Event::Key(_) => true,
            _ => false,
        }
    }

    pub(crate) fn is_pointer(&self) -> bool {
        match self {
            Event::Pointer(_) => true,
            _ => false,
        }
    }

    #[allow(unused)]
    pub(crate) fn is_pointer_down(&self) -> bool {
        match self {
            Event::Pointer(PointerEvent::Down { .. }) => true,
            _ => false,
        }
    }

    #[allow(unused)]
    pub(crate) fn is_pointer_up(&self) -> bool {
        match self {
            Event::Pointer(PointerEvent::Up { .. }) => true,
            _ => false,
        }
    }

    /// Enter, numpad enter and space cause a view to be activated with the keyboard
    pub(crate) fn is_keyboard_trigger(&self) -> bool {
        match self {
            Event::Key(key) => {
                matches!(key.code, Code::NumpadEnter | Code::Enter | Code::Space)
            }
            _ => false,
        }
    }

    pub fn allow_disabled(&self) -> bool {
        match self {
            Event::Pointer(PointerEvent::Leave(_) | PointerEvent::Move(_))
            | Event::ThemeChanged(_)
            | Event::WindowClosed
            | Event::WindowResized(_)
            | Event::WindowMoved(_)
            | Event::WindowGotFocus
            | Event::WindowMaximizeChanged(_)
            | Event::WindowScaleChanged(_)
            | Event::WindowLostFocus
            | Event::FileDrag(FileDragEvent::DragDropped { .. }) => true,
            Event::Pointer(_)
            | Event::FocusGained
            | Event::FocusLost
            | Event::ImeEnabled
            | Event::ImeDisabled
            | Event::ImePreedit { .. }
            | Event::ImeCommit(_)
            | Event::FileDrag(
                FileDragEvent::DragEntered { .. }
                | FileDragEvent::DragMoved { .. }
                | FileDragEvent::DragLeft { .. },
            )
            | Event::Key(_) => false,
            // Event::PinchGesture(_)
        }
    }

    pub fn point(&self) -> Option<Point> {
        match self {
            Event::Pointer(PointerEvent::Down { state, .. })
            | Event::Pointer(PointerEvent::Up { state, .. })
            | Event::Pointer(PointerEvent::Move(PointerUpdate { current: state, .. }))
            | Event::Pointer(PointerEvent::Scroll { state, .. }) => Some(state.position),
            Event::FileDrag(
                FileDragEvent::DragEntered { position, .. }
                | FileDragEvent::DragMoved { position }
                | FileDragEvent::DragDropped { position, .. }
                | FileDragEvent::DragLeft {
                    position: Some(position),
                },
            ) => Some(*position),
            _ => None,
        }
    }

    pub fn offset(self, offset: (f64, f64)) -> Event {
        self.transform(Affine::translate(offset))
    }

    pub fn transform(mut self, transform: Affine) -> Event {
        match &mut self {
            Event::Pointer(PointerEvent::Down { state, .. })
            | Event::Pointer(PointerEvent::Up { state, .. })
            | Event::Pointer(PointerEvent::Move(PointerUpdate { current: state, .. }))
            | Event::Pointer(PointerEvent::Scroll { state, .. }) => {
                state.position = transform.inverse() * state.position;
            }
            Event::FileDrag(
                FileDragEvent::DragEntered { position, .. }
                | FileDragEvent::DragMoved { position }
                | FileDragEvent::DragDropped { position, .. }
                | FileDragEvent::DragLeft {
                    position: Some(position),
                },
            ) => {
                *position = transform.inverse() * *position;
            }
            // Event::PinchGesture(_) => {}
            Event::Pointer(PointerEvent::Cancel(_))
            | Event::Pointer(PointerEvent::Leave(_))
            | Event::Pointer(PointerEvent::Enter(_))
            | Event::FileDrag(FileDragEvent::DragLeft { position: None })
            | Event::Key(_)
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
            | Event::WindowScaleChanged(_)
            | Event::WindowGotFocus
            | Event::WindowLostFocus => {}
        }
        self
    }

    pub fn listener(&self) -> Option<EventListener> {
        match self {
            Event::Pointer(PointerEvent::Down { .. }) => Some(EventListener::PointerDown),
            Event::Pointer(PointerEvent::Up { .. }) => Some(EventListener::PointerUp),
            Event::Pointer(PointerEvent::Move(_)) => Some(EventListener::PointerMove),
            Event::Pointer(PointerEvent::Scroll { .. }) => Some(EventListener::PointerWheel),
            Event::Pointer(PointerEvent::Leave(_)) => Some(EventListener::PointerLeave),
            Event::Pointer(PointerEvent::Enter(_)) => None,
            Event::Pointer(PointerEvent::Cancel(_)) => None,
            // Event::PinchGesture(_) => Some(EventListener::PinchGesture),
            Event::Key(KeyboardEvent {
                state: KeyState::Down,
                ..
            }) => Some(EventListener::KeyDown),
            Event::Key(KeyboardEvent {
                state: KeyState::Up,
                ..
            }) => Some(EventListener::KeyUp),
            Event::ImeEnabled => Some(EventListener::ImeEnabled),
            Event::ImeDisabled => Some(EventListener::ImeDisabled),
            Event::ImePreedit { .. } => Some(EventListener::ImePreedit),
            Event::ImeCommit(_) => Some(EventListener::ImeCommit),
            Event::WindowClosed => Some(EventListener::WindowClosed),
            Event::WindowResized(_) => Some(EventListener::WindowResized),
            Event::WindowMoved(_) => Some(EventListener::WindowMoved),
            Event::WindowMaximizeChanged(_) => Some(EventListener::WindowMaximizeChanged),
            Event::WindowScaleChanged(_) => Some(EventListener::WindowScaleChanged),
            Event::WindowGotFocus => Some(EventListener::WindowGotFocus),
            Event::WindowLostFocus => Some(EventListener::WindowLostFocus),
            Event::FocusLost => Some(EventListener::FocusLost),
            Event::FocusGained => Some(EventListener::FocusGained),
            Event::ThemeChanged(_) => Some(EventListener::ThemeChanged),
            Event::FileDrag(_) => Some(EventListener::DroppedFile),
        }
    }
}
