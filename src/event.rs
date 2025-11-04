use peniko::kurbo::{Affine, Point, Size, Vec2};
use ui_events::{
    ScrollDelta,
    keyboard::{Code, KeyState, KeyboardEvent},
    pointer::{
        PointerButtonEvent, PointerEvent, PointerGestureEvent, PointerScrollEvent, PointerUpdate,
    },
};
use winit::window::Theme;

use crate::dropped_file::{self, FileDragEvent};
use dpi::LogicalPosition;

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
    DroppedFiles,
}

#[derive(Debug, Clone)]
pub enum Event {
    Pointer(PointerEvent),
    FileDrag(dropped_file::FileDragEvent),
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
        matches!(self, Event::Key(_))
    }

    pub(crate) fn is_pointer(&self) -> bool {
        matches!(self, Event::Pointer(_))
    }

    #[allow(unused)]
    pub(crate) fn is_pointer_down(&self) -> bool {
        matches!(self, Event::Pointer(PointerEvent::Down { .. }))
    }

    #[allow(unused)]
    pub(crate) fn is_pointer_up(&self) -> bool {
        matches!(self, Event::Pointer(PointerEvent::Up { .. }))
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
        }
    }

    pub fn pixel_scroll_delta_vec2(&self) -> Option<Vec2> {
        if let Event::Pointer(PointerEvent::Scroll(PointerScrollEvent {
            delta: ScrollDelta::PixelDelta(delta),
            state,
            ..
        })) = self
        {
            let log = delta.to_logical(state.scale_factor);
            Some(Vec2 { x: log.x, y: log.y })
        } else {
            None
        }
    }

    pub fn point(&self) -> Option<Point> {
        match self {
            Event::Pointer(PointerEvent::Down(PointerButtonEvent { state, .. }))
            | Event::Pointer(PointerEvent::Up(PointerButtonEvent { state, .. }))
            | Event::Pointer(PointerEvent::Move(PointerUpdate { current: state, .. }))
            | Event::Pointer(PointerEvent::Scroll(PointerScrollEvent { state, .. })) => {
                Some(state.logical_point())
            }
            Event::FileDrag(
                FileDragEvent::DragEntered {
                    position,
                    scale_factor,
                    ..
                }
                | FileDragEvent::DragMoved {
                    position,
                    scale_factor,
                }
                | FileDragEvent::DragDropped {
                    position,
                    scale_factor,
                    ..
                }
                | FileDragEvent::DragLeft {
                    position: Some(position),
                    scale_factor,
                },
            ) => {
                let log_pos = position.to_logical(*scale_factor);
                Some(Point::new(log_pos.x, log_pos.y))
            }
            _ => None,
        }
    }

    pub fn offset(self, offset: (f64, f64)) -> Event {
        self.transform(Affine::translate(offset))
    }

    pub fn transform(mut self, transform: Affine) -> Event {
        match &mut self {
            Event::Pointer(
                PointerEvent::Down(PointerButtonEvent { state, .. })
                | PointerEvent::Up(PointerButtonEvent { state, .. })
                | PointerEvent::Gesture(PointerGestureEvent { state, .. })
                | PointerEvent::Move(PointerUpdate { current: state, .. })
                | PointerEvent::Scroll(PointerScrollEvent { state, .. }),
            ) => {
                let point = state.logical_point();
                let transformed_point = transform.inverse() * point;
                let phys_pos = LogicalPosition::new(transformed_point.x, transformed_point.y)
                    .to_physical(state.scale_factor);
                state.position = phys_pos;
            }
            Event::FileDrag(
                FileDragEvent::DragEntered {
                    position,
                    scale_factor,
                    ..
                }
                | FileDragEvent::DragMoved {
                    position,
                    scale_factor,
                }
                | FileDragEvent::DragDropped {
                    position,
                    scale_factor,
                    ..
                }
                | FileDragEvent::DragLeft {
                    position: Some(position),
                    scale_factor,
                },
            ) => {
                let log_pos = position.to_logical(*scale_factor);
                let point = Point::new(log_pos.x, log_pos.y);
                let transformed_point = transform.inverse() * point;
                let phys_pos = LogicalPosition::new(transformed_point.x, transformed_point.y)
                    .to_physical(*scale_factor);
                *position = phys_pos;
            }
            // Event::PinchGesture(_) => {}
            Event::Pointer(
                PointerEvent::Cancel(_) | PointerEvent::Leave(_) | PointerEvent::Enter(_),
            )
            | Event::FileDrag(FileDragEvent::DragLeft { position: None, .. })
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
            Event::Pointer(PointerEvent::Enter(_)) => None, // TODO
            Event::Pointer(PointerEvent::Cancel(_)) => None, // TODO
            Event::Pointer(PointerEvent::Gesture(_)) => None, // TODO
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
            Event::FileDrag(FileDragEvent::DragDropped { .. }) => Some(EventListener::DroppedFiles),
            _ => None, // TODO
        }
    }
}
