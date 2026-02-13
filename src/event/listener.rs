use std::{any::Any, ptr};

use peniko::kurbo::{Point, Size};
use ui_events::{
    keyboard::KeyState,
    pointer::{
        PointerButtonEvent, PointerEvent, PointerGestureEvent, PointerId, PointerInfo,
        PointerScrollEvent, PointerUpdate,
    },
};

use crate::event::*;

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
    ///
    /// This is the first event in the drag lifecycle, fired after the pointer has moved beyond the drag threshold.
    /// Use this event to set drag data via `event.set_data()` that will be available to drop targets.
    pub DragStart: DragStartEvent,
    |event| {
        if let Event::Drag(DragEvent::Source(DragSourceEvent::Start(dse))) = event {
            return Some(dse as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragSourceEvent::Move`] - sent to the element being dragged as the pointer moves during the drag.
    ///
    /// This event fires continuously as the user drags. Use it to update custom drag previews or track drag position.
    pub DragMove: DragMoveEvent,
    |event| {
        if let Event::Drag(DragEvent::Source(DragSourceEvent::Move(dme))) = event {
            return Some(dme as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragSourceEvent::Enter`] - sent to the element being dragged when it enters a potential drop target.
    ///
    /// `other_element` in the event data identifies the drop target that was entered.
    /// Use this to update drag preview appearance when hovering over valid drop zones.
    pub DragSourceEnter: DragEnterEvent,
    |event| {
        if let Event::Drag(DragEvent::Source(DragSourceEvent::Enter(dee))) = event {
            return Some(dee as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragSourceEvent::Leave`] - sent to the element being dragged when it leaves a potential drop target.
    ///
    /// `other_element` in the event data identifies the drop target that was left.
    /// Use this to restore default drag preview appearance when leaving drop zones.
    pub DragSourceLeave: DragLeaveEvent,
    |event| {
        if let Event::Drag(DragEvent::Source(DragSourceEvent::Leave(dle))) = event {
            return Some(dle as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragSourceEvent::End`] - sent to the element being dragged when the drag completes successfully.
    ///
    /// `other_element` in the event data is the drop target that accepted the drop.
    /// This event is only sent if a target accepted the drop via `prevent_default()`.
    /// Use this to perform cleanup or update state after a successful drop.
    pub DragEnd: DragEndEvent,
    |event| {
        if let Event::Drag(DragEvent::Source(DragSourceEvent::End(dde))) = event {
            return Some(dde as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragSourceEvent::Cancel`] - sent to the element being dragged when the drag is cancelled.
    ///
    /// Drag cancellation occurs when:
    /// - User presses Escape key
    /// - Pointer leaves the window
    /// - No drop target accepted the drop
    /// - Pointer capture is lost unexpectedly
    ///
    /// Use this to restore original state or undo any changes made during the drag.
    pub DragCancel: DragCancelEvent,
    |event| {
        if let Event::Drag(DragEvent::Source(DragSourceEvent::Cancel(dce))) = event {
            return Some(dce as &dyn Any);
        }
        None
    }
);

// ============================================================================
// Drag Target Event Listeners
// ============================================================================

event_listener!(
    /// Receives [`DragTargetEvent::Enter`] - sent to a drop target when a dragged element enters its bounds.
    ///
    /// `other_element` in the event data identifies the element being dragged (the drag source).
    /// Use this to show visual feedback that the element can accept drops (e.g., highlight border, show drop indicator).
    pub DragTargetEnter: DragEnterEvent,
    |event| {
        if let Event::Drag(DragEvent::Target(DragTargetEvent::Enter(dee))) = event {
            return Some(dee as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragTargetEvent::Move`] - sent to a drop target as the pointer moves while a dragged element is over it.
    ///
    /// `other_element` in the event data identifies the element being dragged.
    /// Use this to update drop position indicators or highlight specific drop zones within the target.
    pub DragTargetMove: DragMoveEvent,
    |event| {
        if let Event::Drag(DragEvent::Target(DragTargetEvent::Move(dme))) = event {
            return Some(dme as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragTargetEvent::Leave`] - sent to a drop target when a dragged element leaves its bounds.
    ///
    /// `other_element` in the event data identifies the element being dragged.
    /// Use this to remove visual feedback (e.g., remove highlight, hide drop indicator).
    pub DragTargetLeave: DragLeaveEvent,
    |event| {
        if let Event::Drag(DragEvent::Target(DragTargetEvent::Leave(dle))) = event {
            return Some(dle as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives [`DragTargetEvent::Drop`] - sent to a drop target when a dragged element is dropped on it.
    ///
    /// `other_element` in the event data identifies the element being dragged.
    /// Access drag data via `event.data.downcast_ref::<YourType>()`.
    ///
    /// **Important**: Call `cx.prevent_default()` to accept the drop. If not called, the drag will be cancelled
    /// and the source will receive `DragCancel` instead of `DragEnd`.
    pub DragTargetDrop: DragEndEvent,
    |event| {
        if let Event::Drag(DragEvent::Target(DragTargetEvent::Drop(dde))) = event {
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
    pub GainedPointerCapture: DragToken,
    |event| {
        if let Event::PointerCapture(PointerCaptureEvent::Gained(token)) = event {
            return Some(token as &dyn Any);
        }
        None
    }
);
/// Web-standard name for pointer capture gained event.
#[deprecated(
    note = "Use `GainedPointerCapture` instead for consistency with other Floem event names. This alias matches the web's `gotpointercapture` event name."
)]
#[expect(non_upper_case_globals)]
pub const GotPointerCapture: GainedPointerCapture = GainedPointerCapture;

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
    /// Listens for when an element or its descendants gain focus.
    ///
    /// This event propagates through all three phases (capture, target, bubble), similar to
    /// standard DOM events. **By default, event listeners using `on_event` only listen to
    /// target and bubble phases.**
    ///
    /// # Filtering by Phase
    ///
    /// To only respond when this specific element gains focus (not descendants):
    /// ```rust
    /// view.on_event_stop(FocusGained, |cx, _| {
    ///     if cx.phase.is_target() {
    ///         // Only this element gained focus
    ///     }
    /// })
    /// ```
    ///
    /// Or configure which phases to listen to:
    /// ```rust
    /// view.on_event_stop_with_config(
    ///     FocusGained,
    ///     EventCallbackConfig { phases: Phases::TARGET },
    ///     |cx, _| {
    ///         // Only fires when this element itself gains focus
    ///     }
    /// )
    /// ```
    ///
    /// To listen during capture phase (fires before descendants):
    /// ```rust
    /// view.on_event_stop_with_config(
    ///     FocusGained,
    ///     EventCallbackConfig { phases: Phases::CAPTURE },
    ///     |cx, _| {
    ///         // Fires before any descendant receives the event
    ///     }
    /// )
    /// ```
    pub FocusGained: (),
    |event| {
        if let Event::Focus(FocusEvent::Gained) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);
#[deprecated(note = "Use `FocusGained` instead.")]
#[expect(non_upper_case_globals)]
pub const GotFocus: FocusGained = FocusGained;

event_listener!(
    /// Listens for when an element or its descendants lose focus.
    ///
    /// This event propagates through all three phases (capture, target, bubble), similar to
    /// standard DOM events. **By default, event listeners using `on_event` only listen to
    /// target and bubble phases.**
    ///
    /// # Filtering by Phase
    ///
    /// To only respond when this specific element loses focus (not descendants):
    /// ```rust
    /// view.on_event_stop(FocusLost, |cx, _| {
    ///     if cx.phase.is_target() {
    ///         // Only this element lost focus
    ///     }
    /// })
    /// ```
    ///
    /// Or configure which phases to listen to:
    /// ```rust
    /// view.on_event_stop_with_config(
    ///     FocusLost,
    ///     EventCallbackConfig { phases: Phases::TARGET },
    ///     |cx, _| {
    ///         // Only fires when this element itself loses focus
    ///     }
    /// )
    /// ```
    ///
    /// To listen during capture phase (fires before descendants):
    /// ```rust
    /// view.on_event_stop_with_config(
    ///     FocusLost,
    ///     EventCallbackConfig { phases: Phases::CAPTURE },
    ///     |cx, _| {
    ///         // Fires before any descendant receives the event
    ///     }
    /// )
    /// ```
    pub FocusLost: (),
    |event| {
        if let Event::Focus(FocusEvent::Lost) = event {
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
    pub WindowGainedFocus: (),
    |event| {
        if let Event::Window(WindowEvent::FocusGained) = event {
            return Some(&() as &dyn Any);
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
    /// Receives `Event::Window` `UpdatePhase(ProcessingMessages)` variant
    pub UpdatePhaseProcessingMessages: (),
    |event| {
        if let Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::ProcessingMessages)) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);
event_listener!(
    /// Receives `Event::Window` `UpdatePhase(Style)` variant
    pub UpdatePhaseStyle: (),
    |event| {
        if let Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Style)) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);
event_listener!(
    /// Receives `Event::Window` `UpdatePhase(Layout)` variant
    pub UpdatePhaseLayout: (),
    |event| {
        if let Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Layout)) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);
event_listener!(
    /// Receives `Event::Window` `UpdatePhase(BoxTreeUpdate)` variant
    pub UpdatePhaseBoxTreeUpdate: (),
    |event| {
        if let Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::BoxTreeUpdate)) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);
event_listener!(
    /// Receives `Event::Window` `UpdatePhase(BoxTreePendingUpdates)` variant
    pub UpdatePhaseBoxTreePendingUpdates: (),
    |event| {
        if let Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::BoxTreePendingUpdates)) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);
event_listener!(
    /// Receives `Event::Window` `UpdatePhase(BoxTreeCommit)` variant
    pub UpdatePhaseBoxTreeCommit: (),
    |event| {
        if let Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::BoxTreeCommit)) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);
event_listener!(
    /// Receives `Event::Window` `UpdatePhase(Complete)` variant
    pub UpdatePhaseComplete: (),
    |event| {
        if let Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Complete)) = event {
            return Some(&() as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives `Event::FileDrag` `Dropped` variant
    pub FileDragDrop: crate::event::dropped_file::FileDragDropped,
    |event| {
        if let Event::FileDrag(FileDragEvent::Drop(data)) = event {
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

// ============================================================================
// Broad Category Event Listeners
// ============================================================================

event_listener!(
    /// Receives any `Event::Pointer` variant.
    pub AnyPointer: PointerEvent,
    |event| {
        if let Event::Pointer(pe) = event {
            return Some(pe as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives any `Event::Key` variant.
    pub AnyKey: ui_events::keyboard::KeyboardEvent,
    |event| {
        if let Event::Key(ke) = event {
            return Some(ke as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives any `Event::Window` variant.
    pub AnyWindow: WindowEvent,
    |event| {
        if let Event::Window(we) = event {
            return Some(we as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives any `Event::Focus` variant.
    pub AnyFocus: FocusEvent,
    |event| {
        if let Event::Focus(fe) = event {
            return Some(fe as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives any `Event::Ime` variant.
    pub AnyIme: ImeEvent,
    |event| {
        if let Event::Ime(ie) = event {
            return Some(ie as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives any `Event::Drag(DragEvent::Source(...))` variant.
    ///
    /// Use this when you want to handle all drag source events in one place rather than
    /// registering separate listeners for each event type. Pattern match on the inner
    /// `DragSourceEvent` to handle specific events:
    ///
    /// ```rust,ignore
    /// view.on_event_stop(AnyDragSource, |cx, event: &DragSourceEvent| {
    ///     match event {
    ///         DragSourceEvent::Start(e) => { /* ... */ }
    ///         DragSourceEvent::Move(e) => { /* ... */ }
    ///         DragSourceEvent::End(e) => { /* ... */ }
    ///         _ => {}
    ///     }
    /// })
    /// ```
    pub AnyDragSource: DragSourceEvent,
    |event| {
        if let Event::Drag(DragEvent::Source(dse)) = event {
            return Some(dse as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives any `Event::Drag(DragEvent::Target(...))` variant.
    ///
    /// Use this when you want to handle all drag target events in one place rather than
    /// registering separate listeners for each event type. Pattern match on the inner
    /// `DragTargetEvent` to handle specific events:
    ///
    /// ```rust,ignore
    /// view.on_event_stop(AnyDragTarget, |cx, event: &DragTargetEvent| {
    ///     match event {
    ///         DragTargetEvent::Enter(e) => { /* ... */ }
    ///         DragTargetEvent::Move(e) => { /* ... */ }
    ///         DragTargetEvent::Drop(e) => { /* ... */ }
    ///         _ => {}
    ///     }
    /// })
    /// ```
    pub AnyDragTarget: DragTargetEvent,
    |event| {
        if let Event::Drag(DragEvent::Target(dte)) = event {
            return Some(dte as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives any `Event::Drag(...)` variant (both source and target events).
    ///
    /// Use this when you want to handle all drag events regardless of whether they're
    /// source or target events. Pattern match on the `DragEvent` to distinguish:
    ///
    /// ```rust,ignore
    /// view.on_event_stop(AnyDrag, |cx, event: &DragEvent| {
    ///     match event {
    ///         DragEvent::Source(DragSourceEvent::Start(e)) => { /* ... */ }
    ///         DragEvent::Target(DragTargetEvent::Drop(e)) => { /* ... */ }
    ///         _ => {}
    ///     }
    /// })
    /// ```
    pub AnyDrag: DragEvent,
    |event| {
        if let Event::Drag(de) = event {
            return Some(de as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives any `Event::FileDrag` variant.
    pub AnyFileDrag: FileDragEvent,
    |event| {
        if let Event::FileDrag(fde) = event {
            return Some(fde as &dyn Any);
        }
        None
    }
);

event_listener!(
    /// Receives any `Event::Window(WindowEvent::UpdatePhase)` variant.
    pub AnyUpdatePhase: UpdatePhaseEvent,
    |event| {
        if let Event::Window(WindowEvent::UpdatePhase(phase)) = event {
            return Some(phase as &dyn Any);
        }
        None
    }
);
