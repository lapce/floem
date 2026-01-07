use peniko::kurbo::{Affine, Point, Size, Vec2};
pub use ui_events::pointer::PointerId;
use ui_events::{
    ScrollDelta,
    keyboard::{Code, KeyState, KeyboardEvent},
    pointer::{
        PointerButton, PointerButtonEvent, PointerEvent, PointerGestureEvent, PointerInfo,
        PointerScrollEvent, PointerState, PointerUpdate,
    },
};
use winit::window::Theme;

use dpi::LogicalPosition;

mod dispatch;
pub mod dropped_file;
pub(crate) mod path;

pub use dropped_file::FileDragEvent;
pub use path::clear_hit_test_cache;

pub use dispatch::*;

use crate::VisualId;

/// Phases of event propagation.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Phase {
    /// Parent-to-target traversal.
    Capture,
    /// Target node.
    Target,
    /// Target-to-parent traversal.
    Bubble,
}
impl From<understory_responder::types::Phase> for Phase {
    fn from(value: understory_responder::types::Phase) -> Self {
        match value {
            understory_responder::types::Phase::Capture => Self::Capture,
            understory_responder::types::Phase::Target => Self::Target,
            understory_responder::types::Phase::Bubble => Self::Bubble,
        }
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
    /// Receives [`Event::Key`] with `KeyState::Down`
    KeyDown,
    /// Receives [`Event::Key`] with `KeyState::Up`
    KeyUp,
    /// Receives [`Event::Interaction`] `Click` variant
    Click,
    /// Receives [`Event::Interaction`] `DoubleClick` variant
    DoubleClick,
    /// Receives [`Event::Interaction`] `SecondaryClick` variant
    SecondaryClick,
    /// Receives [`Event::Drag`] `Start` variant
    DragStart,
    /// Receives [`Event::Drag`] `End` variant
    DragEnd,
    /// Receives [`Event::Drag`] `Move` variant
    DragOver,
    /// Receives [`Event::Drag`] `Enter` variant
    DragEnter,
    /// Receives [`Event::Drag`] `Leave` variant
    DragLeave,
    /// Receives [`Event::Pointer`] `Down` variant
    PointerDown,
    /// Receives [`Event::Pointer`] `Move` variant
    PointerMove,
    /// Receives [`Event::Pointer`] `Up` variant
    PointerUp,
    /// Receives [`Event::Pointer`] `Enter` variant
    PointerEnter,
    /// Receives [`Event::Pointer`] `Leave` variant
    PointerLeave,
    /// Receives [`Event::Pointer`] `Cancel` variant
    PointerCancel,
    /// Fired when a view gains pointer capture
    GotPointerCapture,
    /// Fired when a view loses pointer capture
    LostPointerCapture,
    /// Receives [`Event::Pointer`] `Gesture` variant with `PinchGesture`
    PinchGesture,
    /// Receives [`Event::Ime`] `Enabled` variant
    ImeEnabled,
    /// Receives [`Event::Ime`] `Disabled` variant
    ImeDisabled,
    /// Receives [`Event::Ime`] `Preedit` variant
    ImePreedit,
    /// Receives [`Event::ImeDeleteSurrounding`]
    ImeDeleteSurrounding,
    /// Receives [`Event::Ime`] `Commit` variant
    ImeCommit,
    /// Receives [`Event::Ime`] `DeleteSurrounding` variant
    DeleteSurrounding,
    /// Receives [`Event::Pointer`] `Scroll` variant
    PointerWheel,
    /// Receives [`Event::Focus`] `Gained` variant
    FocusGained,
    /// Receives [`Event::Focus`] `Lost` variant
    FocusLost,
    /// Receives [`Event::Window`] `ThemeChanged` variant
    ThemeChanged,
    /// Receives [`Event::Window`] `Closed` variant
    WindowClosed,
    /// Receives [`Event::Window`] `Resized` variant
    WindowResized,
    /// Receives [`Event::Window`] `Moved` variant
    WindowMoved,
    /// Receives [`Event::Window`] `FocusGained` variant
    WindowGotFocus,
    /// Receives [`Event::Window`] `FocusLost` variant
    WindowLostFocus,
    /// Receives [`Event::Window`] `MaximizeChanged` variant
    WindowMaximizeChanged,
    /// Receives [`Event::Window`] `ScaleChanged` variant
    WindowScaleChanged,
    /// Receives [`Event::FileDrag`] `DragDropped` variant
    DroppedFiles,
    /// Receives [`Event::FileDrag`] `DragEntered` variant
    FileDragEnter,
    /// Receives [`Event::FileDrag`] `DragMoved` variant
    FileDragMove,
    /// Receives [`Event::FileDrag`] `DragLeft` variant
    FileDragLeave,
    /// Receives [`Event::Focus`] `EnteredSubtree` variant
    FocusEnteredSubtree,
    /// Receives [`Event::Focus`] `LeftSubtree` variant
    FocusLeftSubtree,
    /// Receives [`Event::Window`] `ChangeUnderCursor` variant
    WindowChangeUnderCursor,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum FocusEvent {
    Lost,
    Gained,
    EnteredSubtree,
    LeftSubtree,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum PointerCaptureEvent {
    /// Fired when a view gains pointer capture.
    /// Contains the pointer ID that was captured.
    Gained(PointerId),
    /// Fired when a view loses pointer capture.
    /// Contains the pointer ID that was released.
    Lost(PointerId),
}

/// Describes [input method](https://en.wikipedia.org/wiki/Input_method) events.
///
/// The `Ime` events must be applied in the order they arrive.
///
/// This is also called a "composition event".
///
/// Most keypresses using a latin-like keyboard layout simply generate a
/// [`WindowEvent::KeyboardInput`]. However, one couldn't possibly have a key for every single
/// unicode character that the user might want to type
/// - so the solution operating systems employ is to allow the user to type these using _a sequence
///   of keypresses_ instead.
///
/// A prominent example of this is accents - many keyboard layouts allow you to first click the
/// "accent key", and then the character you want to apply the accent to. In this case, some
/// platforms will generate the following event sequence:
///
/// ```ignore
/// // Press "`" key
/// Ime::Preedit("`", Some((0, 0)))
/// // Press "E" key
/// Ime::Preedit("", None) // Synthetic event generated by winit to clear preedit.
/// Ime::Commit("é")
/// ```
///
/// Additionally, certain input devices are configured to display a candidate box that allow the
/// user to select the desired character interactively. (To properly position this box, you must use
/// [`Window::set_ime_cursor_area`].)
///
/// An example of a keyboard layout which uses candidate boxes is pinyin. On a latin keyboard the
/// following event sequence could be obtained:
///
/// ```ignore
/// // Press "A" key
/// Ime::Preedit("a", Some((1, 1)))
/// // Press "B" key
/// Ime::Preedit("a b", Some((3, 3)))
/// // Press left arrow key
/// Ime::Preedit("a b", Some((1, 1)))
/// // Press space key
/// Ime::Preedit("啊b", Some((3, 3)))
/// // Press space key
/// Ime::Preedit("", None) // Synthetic event generated by winit to clear preedit.
/// Ime::Commit("啊不")
/// ```
/// (Docs taken from winit-core, Apache2)
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub enum ImeEvent {
    /// Notifies when the IME was enabled.
    ///
    /// After getting this event you could receive [`Preedit`][Self::Preedit] and
    /// [`Commit`][Self::Commit] events. You should also start performing IME related requests
    /// like [`Window::set_ime_cursor_area`].
    Enabled,
    /// Notifies when a new composing text should be set at the cursor position.
    ///
    /// The value represents a pair of the preedit string and the cursor begin position and end
    /// position. When it's `None`, the cursor should be hidden. When `String` is an empty string
    /// this indicates that preedit was cleared.
    ///
    /// The cursor position is byte-wise indexed, assuming UTF-8.
    Preedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    /// Notifies when text should be inserted into the editor widget.
    ///
    /// Right before this event winit will send empty [`Self::Preedit`] event.
    Commit(String),
    DeleteSurrounding {
        /// Bytes to remove before the selection
        before_bytes: usize,
        /// Bytes to remove after the selection
        after_bytes: usize,
    },
    /// Notifies when the IME was disabled.
    ///
    /// After receiving this event you won't get any more [`Preedit`][Self::Preedit] or
    /// [`Commit`][Self::Commit] events until the next [`Enabled`][Self::Enabled] event. You should
    /// also stop issuing IME related requests like [`Window::set_ime_cursor_area`] and clear
    /// pending preedit text.
    Disabled,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum InteractionEvent {
    /// Single click (pointer or keyboard activated)
    Click,
    /// Double click
    DoubleClick,
    /// Right click / context menu trigger
    SecondaryClick,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WindowEvent {
    FocusGained,
    FocusLost,
    Closed,
    Resized(Size),
    Moved(Point),
    MaximizeChanged(bool),
    ScaleChanged(f64),
    ThemeChanged(Theme),
    ChangeUnderCursor,
}

#[derive(Clone, Debug)]
pub enum DragEvent {
    /// Drag operation started
    Start {
        /// The visual being dragged
        visual_id: VisualId,
        /// Pointer state when drag started
        state: PointerState,
        /// Which button initiated the drag
        button: PointerButton,
        /// Pointer identity
        pointer: PointerInfo,
    },

    /// Pointer moved during drag
    Move {
        /// The visual being dragged
        visual_id: VisualId,
        /// Current pointer state
        state: PointerState,
        /// Pointer identity
        pointer: PointerInfo,
        /// Offset from drag start position
        offset: Vec2,
    },

    /// Dragged element entered a potential drop target
    Enter {
        /// The visual being dragged
        dragged_visual: VisualId,
        /// The drop target visual
        drop_target: VisualId,
        /// Current pointer state
        state: PointerState,
        /// Pointer identity
        pointer: PointerInfo,
    },

    /// Dragged element left a potential drop target
    Leave {
        /// The visual being dragged
        dragged_visual: VisualId,
        /// The drop target visual that was left
        drop_target: VisualId,
        /// Current pointer state
        state: PointerState,
        /// Pointer identity
        pointer: PointerInfo,
    },

    /// Drag ended (pointer released)
    /// Dispatch will determine which visuals should receive this event
    End {
        /// The visual being dragged
        visual_id: VisualId,
        /// Final pointer state
        state: PointerState,
        /// Which button was released
        button: PointerButton,
        /// Pointer identity
        pointer: PointerInfo,
        /// Total offset from start
        offset: Vec2,
    },
}

#[derive(Debug, Clone)]
pub enum Event {
    Pointer(PointerEvent),
    Key(ui_events::keyboard::KeyboardEvent),
    FileDrag(dropped_file::FileDragEvent),
    PointerCapture(PointerCaptureEvent),
    Ime(ImeEvent),
    Focus(FocusEvent),
    Window(WindowEvent),
    Interaction(InteractionEvent),
    Drag(DragEvent),
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
            // Pointer leave and move must be delivered to update hover state correctly
            Event::Pointer(PointerEvent::Leave(_) | PointerEvent::Move(_)) => true,
            // Window events should always be delivered regardless of disabled state
            Event::Window(_) => true,
            // File drops should be allowed on disabled views for accessibility
            Event::FileDrag(FileDragEvent::DragDropped { .. }) => true,
            // Lost capture events should be delivered so views can clean up capture state
            Event::PointerCapture(PointerCaptureEvent::Lost(_)) => true,

            // Pointer down, up, enter, cancel, scroll, gesture should not trigger on disabled views
            Event::Pointer(_) => false,
            // Gained capture should only happen on active views
            Event::PointerCapture(PointerCaptureEvent::Gained(_)) => false,
            // Focus events should not be delivered to disabled views
            Event::Focus(_) => false,
            // IME events should not be delivered to disabled views
            Event::Ime(_) => false,
            // File drag preview events should not be delivered to disabled views
            Event::FileDrag(
                FileDragEvent::DragEntered { .. }
                | FileDragEvent::DragMoved { .. }
                | FileDragEvent::DragLeft { .. },
            ) => false,
            // Keyboard events should not trigger on disabled views
            Event::Key(_) => false,
            // Interaction events (click, double click, etc.) should not trigger on disabled views
            Event::Interaction(_) => false,
            // Drag events should not be initiated or handled by disabled views
            Event::Drag(_) => false,
        }
    }

    pub fn pixel_scroll_delta_vec2(&self) -> Option<Vec2> {
        if let Event::Pointer(PointerEvent::Scroll(PointerScrollEvent { delta, state, .. })) = self
        {
            match delta {
                ScrollDelta::PixelDelta(delta) => {
                    let log = delta.to_logical(state.scale_factor);
                    Some(Vec2 { x: log.x, y: log.y })
                }
                ScrollDelta::LineDelta(x, y) => {
                    // Convert line delta to pixel delta
                    // 20 pixels per line is a reasonable default for most UIs
                    const LINE_HEIGHT: f64 = 20.0;
                    Some(Vec2 {
                        x: (*x as f64) * LINE_HEIGHT,
                        y: (*y as f64) * LINE_HEIGHT,
                    })
                }
                ScrollDelta::PageDelta(x, y) => {
                    // Page deltas are synthetic (e.g., clicking scrollbar well)
                    // Use a larger multiplier for page scrolling
                    const PAGE_HEIGHT: f64 = 200.0;
                    Some(Vec2 {
                        x: (*x as f64) * PAGE_HEIGHT,
                        y: (*y as f64) * PAGE_HEIGHT,
                    })
                }
            }
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
            Event::Drag(
                DragEvent::Start { state, .. }
                | DragEvent::Move { state, .. }
                | DragEvent::Enter { state, .. }
                | DragEvent::Leave { state, .. }
                | DragEvent::End { state, .. },
            ) => {
                let point = state.logical_point();
                let transformed_point = transform.inverse() * point;
                let phys_pos = LogicalPosition::new(transformed_point.x, transformed_point.y)
                    .to_physical(state.scale_factor);
                state.position = phys_pos;
            }
            Event::Pointer(
                PointerEvent::Cancel(_) | PointerEvent::Leave(_) | PointerEvent::Enter(_),
            )
            | Event::FileDrag(FileDragEvent::DragLeft { position: None, .. })
            | Event::Key(_)
            | Event::PointerCapture(_)
            | Event::Focus(_)
            | Event::Ime(_)
            | Event::Window(_)
            | Event::Interaction(_) => {}
        }
        self
    }

    pub fn listener(&self) -> EventListener {
        match self {
            Event::Pointer(PointerEvent::Down { .. }) => EventListener::PointerDown,
            Event::Pointer(PointerEvent::Up { .. }) => EventListener::PointerUp,
            Event::Pointer(PointerEvent::Move(_)) => EventListener::PointerMove,
            Event::Pointer(PointerEvent::Scroll { .. }) => EventListener::PointerWheel,
            Event::Pointer(PointerEvent::Leave(_)) => EventListener::PointerLeave,
            Event::Pointer(PointerEvent::Enter(_)) => EventListener::PointerEnter,
            Event::Pointer(PointerEvent::Cancel(_)) => EventListener::PointerCancel,
            Event::Pointer(PointerEvent::Gesture(_)) => EventListener::PinchGesture,
            Event::PointerCapture(PointerCaptureEvent::Gained(_)) => {
                EventListener::GotPointerCapture
            }
            Event::PointerCapture(PointerCaptureEvent::Lost(_)) => {
                EventListener::LostPointerCapture
            }
            Event::Key(KeyboardEvent {
                state: KeyState::Down,
                ..
            }) => EventListener::KeyDown,
            Event::Key(KeyboardEvent {
                state: KeyState::Up,
                ..
            }) => EventListener::KeyUp,
            Event::Ime(ImeEvent::Enabled) => EventListener::ImeEnabled,
            Event::Ime(ImeEvent::Disabled) => EventListener::ImeDisabled,
            Event::Ime(ImeEvent::Preedit { .. }) => EventListener::ImePreedit,
            Event::Ime(ImeEvent::Commit(_)) => EventListener::ImeCommit,
            Event::Ime(ImeEvent::DeleteSurrounding { .. }) => EventListener::ImeDeleteSurrounding,
            Event::Focus(FocusEvent::Lost) => EventListener::FocusLost,
            Event::Focus(FocusEvent::Gained) => EventListener::FocusGained,
            Event::Focus(FocusEvent::EnteredSubtree) => EventListener::FocusEnteredSubtree,
            Event::Focus(FocusEvent::LeftSubtree) => EventListener::FocusLeftSubtree,
            Event::Window(WindowEvent::Closed) => EventListener::WindowClosed,
            Event::Window(WindowEvent::Resized(_)) => EventListener::WindowResized,
            Event::Window(WindowEvent::Moved(_)) => EventListener::WindowMoved,
            Event::Window(WindowEvent::MaximizeChanged(_)) => EventListener::WindowMaximizeChanged,
            Event::Window(WindowEvent::ScaleChanged(_)) => EventListener::WindowScaleChanged,
            Event::Window(WindowEvent::FocusGained) => EventListener::WindowGotFocus,
            Event::Window(WindowEvent::FocusLost) => EventListener::WindowLostFocus,
            Event::Window(WindowEvent::ThemeChanged(_)) => EventListener::ThemeChanged,
            Event::Window(WindowEvent::ChangeUnderCursor) => EventListener::WindowChangeUnderCursor,
            Event::Interaction(InteractionEvent::Click) => EventListener::Click,
            Event::Interaction(InteractionEvent::DoubleClick) => EventListener::DoubleClick,
            Event::Interaction(InteractionEvent::SecondaryClick) => EventListener::SecondaryClick,
            Event::FileDrag(FileDragEvent::DragDropped { .. }) => EventListener::DroppedFiles,
            Event::FileDrag(FileDragEvent::DragEntered { .. }) => EventListener::FileDragEnter,
            Event::FileDrag(FileDragEvent::DragMoved { .. }) => EventListener::FileDragMove,
            Event::FileDrag(FileDragEvent::DragLeft { .. }) => EventListener::FileDragLeave,
            Event::Drag(DragEvent::Start { .. }) => EventListener::DragStart,
            Event::Drag(DragEvent::Move { .. }) => EventListener::DragOver,
            Event::Drag(DragEvent::Enter { .. }) => EventListener::DragEnter,
            Event::Drag(DragEvent::Leave { .. }) => EventListener::DragLeave,
            Event::Drag(DragEvent::End { .. }) => EventListener::DragEnd,
        }
    }

    fn is_spatial(&self) -> bool {
        self.point().is_some()
    }

    fn is_move(&self) -> bool {
        matches!(self, Event::Pointer(PointerEvent::Move(_)))
    }
}

use std::time::Instant;

/// Tracks the state of a visual being dragged.
pub struct DragState {
    pub(crate) id: VisualId,
    pub(crate) start_state: PointerState,
    pub(crate) offset: Vec2,
    pub(crate) pointer: PointerInfo,
    pub(crate) button: PointerButton,
    pub(crate) released_at: Option<Instant>,
    pub(crate) release_location: Option<Point>,
}

pub struct DragTracker {
    /// Current drag state, if a drag is in progress
    pub(crate) state: Option<DragState>,
    /// Minimum distance (in logical pixels) pointer must move before drag starts
    threshold: f64,
    /// Whether the pointer has moved past the threshold distance
    /// When false, state exists but we're waiting to exceed threshold
    /// When true, drag events are being emitted
    threshold_exceeded: bool,
    /// Hover state for tracking drag enter/leave events
    hover_state: understory_event_state::hover::HoverState<VisualId>,
}

impl DragTracker {
    pub fn new() -> Self {
        Self {
            state: None,
            threshold: 3.0, // Common default: 3 logical pixels
            threshold_exceeded: false,
            hover_state: understory_event_state::hover::HoverState::new(),
        }
    }

    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold;
        self
    }

    /// Returns true if a drag has started (threshold has been exceeded)
    pub fn is_dragging(&self) -> bool {
        self.threshold_exceeded
    }

    /// Returns the current drag state, if any
    pub fn drag_state(&self) -> Option<&DragState> {
        self.state.as_ref()
    }

    /// Handle a pointer down event - potential drag start
    /// Returns None - drag doesn't actually start until threshold is exceeded
    pub fn on_pointer_down(
        &mut self,
        visual_id: VisualId,
        button_event: &PointerButtonEvent,
    ) -> Option<DragEvent> {
        // Only track left button drags by default (or customize as needed)
        if button_event.button != Some(PointerButton::Primary) {
            return None;
        }

        self.state = Some(DragState {
            id: visual_id,
            start_state: button_event.state.clone(),
            offset: Vec2::ZERO,
            pointer: button_event.pointer,
            button: button_event.button.unwrap_or(PointerButton::Primary),
            released_at: None,
            release_location: None,
        });
        self.threshold_exceeded = false;

        None // Don't emit Start event yet
    }

    /// Handle a pointer move event - check threshold and generate drag events
    ///
    /// `hover_path` should be the current hover path from the window's hover state
    pub fn on_pointer_move(
        &mut self,
        move_event: &PointerUpdate,
        hover_path: &[VisualId],
    ) -> Vec<DragEvent> {
        let state = match self.state.as_mut() {
            Some(s) => s,
            None => return Vec::new(),
        };

        let start_pos = state.start_state.logical_point();
        let current_pos = move_event.current.logical_point();
        let new_offset = Vec2::new(current_pos.x - start_pos.x, current_pos.y - start_pos.y);

        let mut events = Vec::new();

        // Check if we've exceeded the threshold to start dragging
        if !self.threshold_exceeded {
            let distance = (new_offset.x * new_offset.x + new_offset.y * new_offset.y).sqrt();
            if distance >= self.threshold {
                self.threshold_exceeded = true;
                events.push(DragEvent::Start {
                    visual_id: state.id,
                    state: state.start_state.clone(),
                    button: state.button,
                    pointer: state.pointer,
                });
            } else {
                // Haven't exceeded threshold yet
                return events;
            }
        }

        // Update offset
        state.offset = new_offset;

        // Emit Move event
        events.push(DragEvent::Move {
            visual_id: state.id,
            state: move_event.current.clone(),
            pointer: move_event.pointer,
            offset: new_offset,
        });

        // Extract values needed for hover state update before dropping the borrow
        let dragged_id = state.id;
        let pointer_info = state.pointer;
        let current_state = move_event.current.clone();

        // Drop the mutable borrow so we can update hover state
        drop(state);

        // Update hover state and generate Enter/Leave events
        let hover_events = self.hover_state.update_path(hover_path);
        for hover_event in hover_events {
            match hover_event {
                understory_event_state::hover::HoverEvent::Enter(drop_target) => {
                    // Don't send Enter if we're hovering over the dragged item itself
                    if drop_target != dragged_id {
                        events.push(DragEvent::Enter {
                            dragged_visual: dragged_id,
                            drop_target,
                            state: current_state.clone(),
                            pointer: pointer_info,
                        });
                    }
                }
                understory_event_state::hover::HoverEvent::Leave(drop_target) => {
                    events.push(DragEvent::Leave {
                        dragged_visual: dragged_id,
                        drop_target,
                        state: current_state.clone(),
                        pointer: pointer_info,
                    });
                }
            }
        }

        events
    }

    /// Handle a pointer up event - end the drag
    ///
    /// Returns both the drag end event and any hover leave events from clearing the hover state
    pub fn on_pointer_up(&mut self, button_event: &PointerButtonEvent) -> Vec<DragEvent> {
        let state = match self.state.take() {
            Some(s) => s,
            None => return Vec::new(),
        };

        // If we never started actually dragging (didn't exceed threshold), just clear state
        if !self.threshold_exceeded {
            self.threshold_exceeded = false;
            self.hover_state.clear();
            return Vec::new();
        }

        let mut events = Vec::new();

        // Clear hover state and generate Leave events first
        let hover_events = self.hover_state.clear();
        for hover_event in hover_events {
            if let understory_event_state::hover::HoverEvent::Leave(drop_target) = hover_event {
                events.push(DragEvent::Leave {
                    dragged_visual: state.id,
                    drop_target,
                    state: button_event.state.clone(),
                    pointer: state.pointer,
                });
            }
        }

        // Emit drag end event - dispatch will route it to appropriate views
        events.push(DragEvent::End {
            visual_id: state.id,
            state: button_event.state.clone(),
            button: state.button,
            pointer: button_event.pointer,
            offset: state.offset,
        });

        // Reset state
        self.threshold_exceeded = false;

        events
    }

    /// Handle pointer cancel - abort the drag
    pub fn on_pointer_cancel(&mut self) -> Vec<DragEvent> {
        let state = match self.state.take() {
            Some(s) => s,
            None => return Vec::new(),
        };

        if !self.threshold_exceeded {
            self.hover_state.clear();
            return Vec::new();
        }

        let mut events = Vec::new();

        // Clear hover state and generate Leave events
        let hover_events = self.hover_state.clear();
        for hover_event in hover_events {
            if let understory_event_state::hover::HoverEvent::Leave(drop_target) = hover_event {
                events.push(DragEvent::Leave {
                    dragged_visual: state.id,
                    drop_target,
                    state: state.start_state.clone(),
                    pointer: state.pointer,
                });
            }
        }

        // Create end event with start state as we don't have current
        events.push(DragEvent::End {
            visual_id: state.id,
            state: state.start_state.clone(),
            button: state.button,
            pointer: state.pointer,
            offset: state.offset,
        });

        self.threshold_exceeded = false;

        events
    }

    /// Reset the tracker state (useful if you need to cancel drag externally)
    pub fn reset(&mut self) {
        self.state = None;
        self.threshold_exceeded = false;
        self.hover_state.clear();
    }
}

impl Default for DragTracker {
    fn default() -> Self {
        Self::new()
    }
}
