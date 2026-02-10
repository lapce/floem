use std::{any::Any, ptr};

use peniko::kurbo::{Point, Size};
use ui_events::{
    keyboard::KeyState,
    pointer::{
        PointerButtonEvent, PointerEvent, PointerGestureEvent, PointerId, PointerInfo,
        PointerScrollEvent, PointerUpdate,
    },
};

use crate::event::{
    DragCancelEvent, DragEndEvent, DragEnterEvent, DragLeaveEvent, DragMoveEvent, DragSourceEvent,
    DragStartEvent, DragTargetEvent, Event, FileDragEvent, FocusEvent, ImeEvent, InteractionEvent,
    PointerCaptureEvent, WindowEvent,
};

// EventListener using the same pattern as StyleClass
#[derive(Copy, Clone)]
pub struct EventListenerKey {
    pub info: &'static EventKeyInfo,
}

impl PartialEq for EventListenerKey {
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(self.info, other.info)
    }
}

impl Eq for EventListenerKey {}

impl std::hash::Hash for EventListenerKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_usize(self.info as *const _ as usize)
    }
}

impl std::fmt::Debug for EventListenerKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", (self.info.name)())
    }
}

#[derive(Debug)]
pub struct EventKeyInfo {
    pub(crate) name: fn() -> &'static str,
    /// Extract the relevant event data if this listener matches the event
    pub(crate) extract: fn(&Event) -> Option<&dyn Any>,
}

impl EventListenerKey {
    /// Extract the relevant event data if this listener matches the event
    pub fn extract<'a>(&self, event: &'a Event) -> Option<&'a dyn Any> {
        (self.info.extract)(event)
    }
}

// Trait for built-in event listeners (similar to StyleClass)
pub trait EventListenerTrait: Default + Copy + 'static {
    /// The type of event data this listener extracts
    type EventData: ?Sized;

    fn listener_key() -> EventListenerKey;

    /// Extract and downcast to the specific event type
    fn extract(event: &Event) -> Option<&Self::EventData>
    where
        Self::EventData: Sized,
    {
        Self::listener_key().extract(event)?.downcast_ref()
    }
}

// Macro to define event listener unit structs
#[macro_export]
macro_rules! event_listener {
    ($(#[$meta:meta])* $v:vis $name:ident : $event_ty:ty, $extract:expr) => {
        $(#[$meta])*
        #[derive(Default, Copy, Clone)]
        $v struct $name;

        impl $crate::event::listener::EventListenerTrait for $name {
            type EventData = $event_ty;

            fn listener_key() -> $crate::event::listener::EventListenerKey {
                static INFO: $crate::event::listener::EventKeyInfo = $crate::event::listener::EventKeyInfo {
                    name: || std::any::type_name::<$name>(),
                    extract: $extract,
                };
                $crate::event::listener::EventListenerKey { info: &INFO }
            }
        }
    };
}

// Built-in event listener unit structs
event_listener!(
    /// Receives [`Event::Key`] with `KeyState::Down`
    pub KeyDown: ui_events::keyboard::KeyboardEvent,
    |event| {
        if let Event::Key(kb_event) = event {
            if kb_event.state == KeyState::Down {
                return Some(kb_event as &dyn Any);
            }
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Key`] with `KeyState::Up`
    pub KeyUp: ui_events::keyboard::KeyboardEvent,
    |event| {
        if let Event::Key(kb_event) = event {
            if kb_event.state == KeyState::Up {
                return Some(kb_event as &dyn Any);
            }
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Interaction`] `Click` variant
    pub Click: (),
    |event| {
        if let Event::Interaction(InteractionEvent::Click) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Interaction`] `DoubleClick` variant
    pub DoubleClick: (),
    |event| {
        if let Event::Interaction(InteractionEvent::DoubleClick) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);

event_listener!(
/// Receives [`Event::Interaction`] `SecondaryClick` variant
pub SecondaryClick: (),
|event| {
    if let Event::Interaction(InteractionEvent::SecondaryClick) = event {
        return Some(&() as &dyn Any);
    }
    None
}
);

// ============================================================================
// Drag Source Event Listeners
// ============================================================================

event_listener!(
    /// Receives [`DragSourceEvent::Start`] - sent to the element being dragged when the drag operation begins.
    pub DragStart: DragStartEvent,
    |event| {
        if let Event::DragSource(DragSourceEvent::Start(dse)) = event {
            return Some(dse as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragSourceEvent::Move`] - sent to the element being dragged as the pointer moves during the drag.
    pub DragMove: DragMoveEvent,
    |event| {
        if let Event::DragSource(DragSourceEvent::Move(dme)) = event {
            return Some(dme as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragSourceEvent::Enter`] - sent to the element being dragged when it enters a potential drop target.
    /// `other_element` in the event data is the drop target that was entered.
    pub DragSourceEnter: DragEnterEvent,
    |event| {
        if let Event::DragSource(DragSourceEvent::Enter(dee)) = event {
            return Some(dee as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragSourceEvent::Leave`] - sent to the element being dragged when it leaves a potential drop target.
    /// `other_element` in the event data is the drop target that was left.
    pub DragSourceLeave: DragLeaveEvent,
    |event| {
        if let Event::DragSource(DragSourceEvent::Leave(dle)) = event {
            return Some(dle as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragSourceEvent::End`] - sent to the element being dragged when the pointer is released.
    /// `other_element` in the event data is the drop target if one accepted the drop, `None` otherwise.
    pub DragEnd: DragEndEvent,
    |event| {
        if let Event::DragSource(DragSourceEvent::End(dde)) = event {
            return Some(dde as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragSourceEvent::Cancel`] - sent to the element being dragged when the drag is cancelled
    /// (e.g., Escape key pressed, pointer left window, or no target accepted the drop).
    pub DragCancel: DragCancelEvent,
    |event| {
        if let Event::DragSource(DragSourceEvent::Cancel(dce)) = event {
            return Some(dce as &dyn Any);
        }
        None
    }
);

// ============================================================================
// Drag Target Event Listeners
// ============================================================================

event_listener!(
    /// Receives [`DragTargetEvent::Enter`] - sent to a drop target when a dragged element enters it.
    /// `other_element` in the event data is the element being dragged.
    pub DragTargetEnter: DragEnterEvent,
    |event| {
        if let Event::DragTarget(DragTargetEvent::Enter(dee)) = event {
            return Some(dee as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragTargetEvent::Move`] - sent to a drop target when the pointer moves while a dragged element is over it.
    /// `other_element` in the event data is the element being dragged.
    pub DragTargetMove: DragMoveEvent,
    |event| {
        if let Event::DragTarget(DragTargetEvent::Move(dme)) = event {
            return Some(dme as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragTargetEvent::Leave`] - sent to a drop target when a dragged element leaves it.
    /// `other_element` in the event data is the element being dragged.
    pub DragTargetLeave: DragLeaveEvent,
    |event| {
        if let Event::DragTarget(DragTargetEvent::Leave(dle)) = event {
            return Some(dle as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragTargetEvent::Drop`] - sent to a drop target when a dragged element is dropped on it.
    /// `other_element` in the event data is the element being dragged.
    /// Call `prevent_default()` to accept the drop.
    pub DragTargetDrop: DragEndEvent,
    |event| {
        if let Event::DragTarget(DragTargetEvent::Drop(dde)) = event {
            return Some(dde as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Pointer`] `Down` variant
    pub PointerDown: PointerButtonEvent,
    |event| {
        if let Event::Pointer(PointerEvent::Down(pbe)) = event {
            return Some(pbe as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Pointer`] `Move` variant
    pub PointerMove: PointerUpdate,
    |event| {
        if let Event::Pointer(PointerEvent::Move(pu)) = event {
            return Some(pu as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Pointer`] `Up` variant
    pub PointerUp: PointerButtonEvent,
    |event| {
        if let Event::Pointer(PointerEvent::Up(pbe)) = event {
            return Some(pbe as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Pointer`] `Enter` variant
    pub PointerEnter: PointerInfo,
    |event| {
        if let Event::Pointer(PointerEvent::Enter(info)) = event {
            return Some(info as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Pointer`] `Leave` variant
    pub PointerLeave: PointerInfo,
    |event| {
        if let Event::Pointer(PointerEvent::Leave(info)) = event {
            return Some(info as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Pointer`] `Cancel` variant
    pub PointerCancel: PointerInfo,
    |event| {
        if let Event::Pointer(PointerEvent::Cancel(info)) = event {
            return Some(info as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Fired when a view gains pointer capture
    pub GainedPointerCapture: PointerId,
    |event| {
        if let Event::PointerCapture(PointerCaptureEvent::Gained(id)) = event {
            return Some(id as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Fired when a view loses pointer capture
    pub LostPointerCapture: PointerId,
    |event| {
        if let Event::PointerCapture(PointerCaptureEvent::Lost(id)) = event {
            return Some(id as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Pointer`] `Gesture` variant with `PinchGesture`
    pub PinchGesture: PointerGestureEvent,
    |event| {
        if let Event::Pointer(PointerEvent::Gesture(pge)) = event {
            return Some(pge as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Ime`] `Enabled` variant
    pub ImeEnabled: (),
    |event| {
        if let Event::Ime(ImeEvent::Enabled) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Ime`] `Disabled` variant
    pub ImeDisabled: (),
    |event| {
        if let Event::Ime(ImeEvent::Disabled) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Ime`] `Preedit` variant
    pub ImePreedit: ImeEvent,
    |event| {
        if let Event::Ime(e@ ImeEvent::Preedit { .. }) = event {
            return Some(e as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Ime`] `Commit` variant
    pub ImeCommit: String,
    |event| {
        if let Event::Ime(ImeEvent::Commit(text)) = event {
            return Some(text as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Ime`] `DeleteSurrounding` variant
    pub ImeDeleteSurrounding: ImeEvent,
    |event| {
        if let Event::Ime(e@ ImeEvent::DeleteSurrounding { .. }) = event {
            return Some(e as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Pointer`] `Scroll` variant
    pub PointerWheel: PointerScrollEvent,
    |event| {
        if let Event::Pointer(PointerEvent::Scroll(pse)) = event {
            return Some(pse as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Focus`] `Gained` variant
    pub FocusGained: (),
    |event| {
        if let Event::Focus(FocusEvent::Gained) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Focus`] `Lost` variant
    pub FocusLost: (),
    |event| {
        if let Event::Focus(FocusEvent::Lost) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Focus`] `EnteredSubtree` variant
    pub FocusEnteredSubtree: (),
    |event| {
        if let Event::Focus(FocusEvent::EnteredSubtree) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Focus`] `LeftSubtree` variant
    pub FocusLeftSubtree: (),
    |event| {
        if let Event::Focus(FocusEvent::LeftSubtree) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Window`] `ThemeChanged` variant
    pub ThemeChanged: winit::window::Theme,
    |event| {
        if let Event::Window(WindowEvent::ThemeChanged(theme)) = event {
            return Some(theme as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Window`] `Closed` variant
    pub WindowClosed: (),
    |event| {
        if let Event::Window(WindowEvent::Closed) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Window`] `Resized` variant
    pub WindowResized: Size,
    |event| {
        if let Event::Window(WindowEvent::Resized(size)) = event {
            return Some(size as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Window`] `Moved` variant
    pub WindowMoved: Point,
    |event| {
        if let Event::Window(WindowEvent::Moved(pos)) = event {
            return Some(pos as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Window`] `FocusGained` variant
    pub WindowGainedFocus: WindowEvent,
    |event| {
        if let Event::Window(e @ WindowEvent::FocusGained) = event {
            return Some(e as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Window`] `FocusLost` variant
    pub WindowLostFocus: (),
    |event| {
        if let Event::Window(WindowEvent::FocusLost) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Window`] `MaximizeChanged` variant
    pub WindowMaximizeChanged: bool,
    |event| {
        if let Event::Window(WindowEvent::MaximizeChanged(maximized)) = event {
            return Some(maximized as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Window`] `ScaleChanged` variant
    pub WindowScaleChanged: f64,
    |event| {
        if let Event::Window(WindowEvent::ScaleChanged(scale)) = event {
            return Some(scale as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives `Event::FileDrag` `Dropped` variant
    pub FileDragDrop: crate::event::dropped_file::FileDragDropped,
    |event| {
        if let Event::FileDrag(FileDragEvent::Dropped(data)) = event {
            return Some(data as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives `Event::FileDrag` `Enter` variant
    pub FileDragEnter: crate::event::dropped_file::FileDragEnter,
    |event| {
        if let Event::FileDrag(FileDragEvent::Enter(data)) = event {
            return Some(data as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives `Event::FileDrag` `Move` variant
    pub FileDragMove: crate::event::dropped_file::FileDragMove,
    |event| {
        if let Event::FileDrag(FileDragEvent::Move(data)) = event {
            return Some(data as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives `Event::FileDrag` `Leave` variant
    pub FileDragLeave: crate::event::dropped_file::FileDragLeave,
    |event| {
        if let Event::FileDrag(FileDragEvent::Leave(data)) = event {
            return Some(data as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Window`] `ChangeUnderCursor` variant
    pub WindowChangeUnderCursor: (),
    |event| {
        if let Event::Window(WindowEvent::ChangeUnderCursor) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`Event::Extracted`]. This isn't useful. Don't use this.
    pub Extracted: (),
    |event| {
        if let Event::Extracted = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);
