pub use inner::{
    AnyDrag,
    AnyDragSource,
    AnyDragTarget,
    AnyFileDrag,
    AnyFocus,
    AnyIme,
    AnyKey,
    // Broad category listeners
    AnyPointer,
    AnyUpdatePhase,
    AnyWindow,
    // Click events
    Click,
    DoubleClick,
    DragCancel,

    DragEnd,
    DragMove,
    DragSourceEnter,
    DragSourceLeave,
    // Drag source events
    DragStart,
    DragTargetDrop,

    // Drag target events
    DragTargetEnter,
    DragTargetLeave,
    DragTargetMove,
    EventKeyInfo,
    // Core types
    EventListenerKey,
    EventListenerTrait,

    // Extracted
    Extracted,

    // File drag events
    FileDragDrop,
    FileDragEnter,
    FileDragLeave,

    FileDragMove,
    // Focus events
    FocusGained,
    FocusLost,

    GainedPointerCapture,
    ImeCommit,
    ImeDeleteSurrounding,

    ImeDisabled,
    // IME events
    ImeEnabled,
    ImePreedit,
    // Key events
    KeyDown,
    KeyUp,

    LostPointerCapture,
    PinchGesture,
    PointerCancel,
    // Pointer events
    PointerDown,
    PointerEnter,
    PointerLeave,
    PointerMove,
    PointerUp,
    PointerWheel,

    SecondaryClick,

    // Window events
    ThemeChanged,
    UpdatePhaseBoxTreeCommit,
    UpdatePhaseBoxTreePendingUpdates,
    UpdatePhaseBoxTreeUpdate,
    UpdatePhaseComplete,

    UpdatePhaseLayout,
    // Update phase events
    UpdatePhaseProcessingMessages,
    UpdatePhaseStyle,
    WindowChangeUnderCursor,

    WindowClosed,
    WindowGainedFocus,
    WindowLostFocus,
    WindowMaximizeChanged,
    WindowMoved,
    WindowResized,
    WindowScaleChanged,
};

mod inner {
    use std::{
        any::{Any, TypeId},
        ptr,
    };

    use peniko::kurbo::{Point, Size};
    use ui_events::{
        keyboard::KeyState,
        pointer::{
            PointerButtonEvent, PointerEvent, PointerGestureEvent, PointerId, PointerInfo,
            PointerScrollEvent, PointerUpdate,
        },
    };

    use crate::event::{
        DragCancelEvent, DragEndEvent, DragEnterEvent, DragEvent, DragLeaveEvent, DragMoveEvent,
        DragSourceEvent, DragStartEvent, DragTargetEvent, DragToken, Event, FileDragEvent,
        FocusEvent, ImeEvent, InteractionEvent, PointerCaptureEvent, UpdatePhaseEvent, WindowEvent,
    };

    // EventListener using the same pattern as StyleClass
    #[derive(Copy, Clone)]
    pub struct EventListenerKey {
        pub info: &'static EventKeyInfo,
        pub type_discriminant: Option<TypeId>,
    }

    impl PartialEq for EventListenerKey {
        fn eq(&self, other: &Self) -> bool {
            ptr::eq(self.info, other.info) && self.type_discriminant == other.type_discriminant
        }
    }

    impl Eq for EventListenerKey {}

    impl std::hash::Hash for EventListenerKey {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            state.write_usize(self.info as *const _ as usize);
            self.type_discriminant.hash(state);
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
                $crate::event::listener::EventListenerKey {
                    info: &INFO,
                    type_discriminant: None,
                }
            }
        }
    };
}

    // Built-in event listener unit structs
    event_listener!(
        /// Receives [`Event::Key`] with `KeyState::Down` — fired when a key is pressed.
        ///
        /// # Routing
        /// Key events are classified as shortcut-like or typing keys:
        /// - **Shortcut-like** (Ctrl/Cmd/Alt combos, F-keys, Escape, Tab, etc.): Dispatched to the
        ///   focused element via Capture → Target → Bubble first. If unconsumed (or no view has
        ///   focus), falls back to all views that registered a key listener via the listener registry.
        /// - **Typing keys** (unmodified character input, etc.): Dispatched only to the focused
        ///   element via Capture → Target → Bubble. Dropped if no view has focus.
        ///
        /// # Default Actions (preventable with `cx.prevent_default()`)
        /// - `Tab` (no modifiers): Moves focus to the next focusable element. `Shift+Tab` moves backwards.
        /// - `Alt+ArrowUp/Down/Left/Right`: Directional focus navigation.
        /// - `Space`, `Enter`, `NumpadEnter` (on key-up or repeat): Generates [`InteractionEvent::Click`]
        ///   on the focused element.
        pub KeyDown: ui_events::keyboard::KeyboardEvent,
        |event| {
            if let Event::Key(kb_event) = event
                && kb_event.state == KeyState::Down {
                    return Some(kb_event as &dyn Any);
                }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Key`] with `KeyState::Up` — fired when a key is released.
        ///
        /// # Routing
        /// Same routing as [`KeyDown`]: shortcut-like keys try the focused path first, then fall
        /// back to the listener registry; typing keys are dispatched only to the focused element.
        ///
        /// # Default Actions (preventable with `cx.prevent_default()`)
        /// - `Space`, `Enter`, `NumpadEnter` (key-up): Generates [`InteractionEvent::Click`] on
        ///   the focused element. (All other keys on key-up have no preventable default action.)
        pub KeyUp: ui_events::keyboard::KeyboardEvent,
        |event| {
            if let Event::Key(kb_event) = event
                && kb_event.state == KeyState::Up {
                    return Some(kb_event as &dyn Any);
                }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Interaction`] `Click` variant — a synthetic click generated from a
        /// pointer Down+Up sequence or a keyboard trigger (`Space`/`Enter` on the focused element).
        ///
        /// # Routing
        /// Dispatched via Capture → Target → Bubble to the common ancestor of the pointer down and
        /// up targets, or to the focused element for keyboard-triggered clicks.
        ///
        /// # Default Actions
        /// No preventable default action. This event is itself a default action of [`PointerUp`]
        /// and [`KeyDown`]/[`KeyUp`] events.
        pub Click: (),
        |event| {
            if let Event::Interaction(InteractionEvent::Click) = event {
                return Some(&() as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Interaction`] `DoubleClick` variant — fired when two rapid pointer
        /// clicks occur (pointer up count > 1). Dispatched immediately after the second [`Click`].
        ///
        /// # Routing
        /// Dispatched via Capture → Target → Bubble to the common ancestor of the pointer targets.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub DoubleClick: (),
        |event| {
            if let Event::Interaction(InteractionEvent::DoubleClick) = event {
                return Some(&() as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Interaction`] `SecondaryClick` variant — fired when a secondary
        /// (right) pointer button click is detected.
        ///
        /// # Routing
        /// Dispatched via Capture → Target → Bubble to the common ancestor of the pointer targets.
        ///
        /// # Default Actions
        /// No preventable default action. Note: context menus are handled separately via the
        /// view's context menu configuration, not from this event.
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
        /// Receives [`DragSourceEvent::Start`] — sent to the element being dragged when the drag
        /// begins (pointer has moved beyond the drag threshold while a button is held down).
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on the dragged element.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub DragStart: DragStartEvent,
        |event| {
            if let Event::Drag(DragEvent::Source(DragSourceEvent::Start(dse))) = event {
                return Some(dse as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`DragSourceEvent::Move`] — sent to the element being dragged as the pointer
        /// moves during the drag.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on the dragged element.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub DragMove: DragMoveEvent,
        |event| {
            if let Event::Drag(DragEvent::Source(DragSourceEvent::Move(dme))) = event {
                return Some(dme as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`DragSourceEvent::Enter`] — sent to the element being dragged when it enters
        /// a potential drop target. `other_element` identifies the drop target that was entered.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on the dragged element.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub DragSourceEnter: DragEnterEvent,
        |event| {
            if let Event::Drag(DragEvent::Source(DragSourceEvent::Enter(dee))) = event {
                return Some(dee as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`DragSourceEvent::Leave`] — sent to the element being dragged when it leaves
        /// a potential drop target. `other_element` identifies the drop target that was left.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on the dragged element.
        ///
        /// # Default Actions
        /// No preventable default action. Note: disabled views can still receive this event to
        /// clean up internal drag state if the view was disabled mid-drag.
        pub DragSourceLeave: DragLeaveEvent,
        |event| {
            if let Event::Drag(DragEvent::Source(DragSourceEvent::Leave(dle))) = event {
                return Some(dle as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`DragSourceEvent::End`] — sent to the element being dragged when the pointer
        /// is released. `other_element` is `Some(target_id)` if released over a drop target,
        /// or `None` if released without a target under the pointer.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on the dragged element.
        ///
        /// # Default Actions
        /// No preventable default action. Note: disabled views can still receive this event to
        /// clean up internal drag state if the view was disabled mid-drag.
        pub DragEnd: DragEndEvent,
        |event| {
            if let Event::Drag(DragEvent::Source(DragSourceEvent::End(dde))) = event {
                return Some(dde as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`DragSourceEvent::Cancel`] — sent to the element being dragged when the drag
        /// is cancelled. Cancellation occurs when:
        /// - The pointer is released but no drop target called `cx.prevent_default()` to accept
        /// - The pointer leaves the window
        /// - `PointerCancel` fires (e.g., touch interrupted)
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on the dragged element.
        ///
        /// # Default Actions
        /// No preventable default action. Note: disabled views can still receive this event to
        /// clean up internal drag state if the view was disabled mid-drag.
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
        /// Receives [`DragTargetEvent::Enter`] — sent to a drop target when a dragged element enters
        /// its bounds. `other_element` identifies the element being dragged.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on the drop target element.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub DragTargetEnter: DragEnterEvent,
        |event| {
            if let Event::Drag(DragEvent::Target(DragTargetEvent::Enter(dee))) = event {
                return Some(dee as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`DragTargetEvent::Move`] — sent to a drop target as the pointer moves while
        /// a dragged element is over it. `other_element` identifies the element being dragged.
        ///
        /// # Routing
        /// Dispatched via Capture → Target → Bubble on the drop target element (the only drag
        /// target event that uses standard phases).
        ///
        /// # Default Actions
        /// No preventable default action.
        pub DragTargetMove: DragMoveEvent,
        |event| {
            if let Event::Drag(DragEvent::Target(DragTargetEvent::Move(dme))) = event {
                return Some(dme as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`DragTargetEvent::Leave`] — sent to a drop target when a dragged element
        /// leaves its bounds. `other_element` identifies the element being dragged.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on the drop target element.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub DragTargetLeave: DragLeaveEvent,
        |event| {
            if let Event::Drag(DragEvent::Target(DragTargetEvent::Leave(dle))) = event {
                return Some(dle as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`DragTargetEvent::Drop`] — sent to a drop target when a dragged element is
        /// released over it. `other_element` identifies the element being dragged.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on the drop target element.
        ///
        /// The drag source simultaneously receives [`DragSourceEvent::End`] with `other_element`
        /// set to this target's ID. The source only receives [`DragSourceEvent::Cancel`] if the
        /// drag was aborted by a `PointerCancel` event (e.g., touch interrupted), not based on
        /// whether this event is handled.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub DragTargetDrop: DragEndEvent,
        |event| {
            if let Event::Drag(DragEvent::Target(DragTargetEvent::Drop(dde))) = event {
                return Some(dde as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Pointer`] `Down` variant — fired when a pointer button is pressed.
        ///
        /// # Routing
        /// Spatially hit-tested at the pointer location, then dispatched via Capture → Target →
        /// Bubble. If the pointer is captured by an element, routed directly to that element instead.
        ///
        /// # Default Actions (preventable with `cx.prevent_default()`)
        /// - Moves keyboard focus to the hit element (fires synthetic `FocusLost`/`FocusGained`).
        /// - On macOS: shows context menu on secondary button press.
        pub PointerDown: PointerButtonEvent,
        |event| {
            if let Event::Pointer(PointerEvent::Down(pbe)) = event {
                return Some(pbe as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Pointer`] `Move` variant — fired when the pointer moves.
        ///
        /// # Routing
        /// Spatially hit-tested at the pointer location, then dispatched via Capture → Target →
        /// Bubble. If the pointer is captured by an element, routed directly to that element instead.
        ///
        /// # Default Actions (preventable with `cx.prevent_default()`)
        /// - If the pointer has moved beyond the drag threshold while a button is held, begins a
        ///   drag operation (`DragSourceEvent::Start` fires).
        /// - While a drag is active, fires drag source/target move and enter/leave events.
        pub PointerMove: PointerUpdate,
        |event| {
            if let Event::Pointer(PointerEvent::Move(pu)) = event {
                return Some(pu as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Pointer`] `Up` variant — fired when a pointer button is released.
        ///
        /// # Routing
        /// Spatially hit-tested at the pointer location, then dispatched via Capture → Target →
        /// Bubble. If the pointer is captured by an element, routed directly to that element instead.
        ///
        /// # Default Actions (preventable with `cx.prevent_default()`)
        /// - Ends any active drag (fires `DragSourceEvent::End` or `DragSourceEvent::Cancel`
        ///   depending on whether a target accepted).
        /// - Releases pointer capture unconditionally.
        /// - On non-macOS platforms: shows context menu on secondary button release.
        pub PointerUp: PointerButtonEvent,
        |event| {
            if let Event::Pointer(PointerEvent::Up(pbe)) = event {
                return Some(pbe as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Pointer`] `Enter` variant — fired when the pointer enters a view's bounds.
        ///
        /// # Routing
        /// Dispatched to the **Target phase only** (no capture/bubble). Generated synthetically by
        /// the hover state machine when a view enters the hover path. Ancestor views do not receive
        /// this event — use [`AnyPointer`] or [`PointerMove`] and inspect the hit path if needed.
        ///
        /// # Default Actions
        /// No preventable default action. This event is itself generated as a side effect of
        /// [`PointerMove`] and pointer capture routing.
        pub PointerEnter: PointerInfo,
        |event| {
            if let Event::Pointer(PointerEvent::Enter(info)) = event {
                return Some(info as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Pointer`] `Leave` variant — fired when the pointer leaves a view's bounds.
        ///
        /// # Routing
        /// Dispatched to the **Target phase only** (no capture/bubble). Generated synthetically by
        /// the hover state machine when a view leaves the hover path.
        ///
        /// # Default Actions
        /// No preventable default action. This event is itself generated as a side effect of
        /// [`PointerMove`] and [`PointerLeave`] (window-level) default actions. Note: disabled
        /// views can still receive this event to update their internal hover state.
        pub PointerLeave: PointerInfo,
        |event| {
            if let Event::Pointer(PointerEvent::Leave(info)) = event {
                return Some(info as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Pointer`] `Cancel` variant — fired when a pointer gesture is cancelled
        /// by the system (e.g., window loses focus, touch is interrupted).
        ///
        /// # Routing
        /// Spatially hit-tested or directed to the capturing element. Dispatched via Capture →
        /// Target → Bubble.
        ///
        /// # Default Actions (preventable with `cx.prevent_default()`)
        /// - Aborts any active drag (`DragSourceEvent::Cancel` fires).
        /// - Releases pointer capture unconditionally.
        pub PointerCancel: PointerInfo,
        |event| {
            if let Event::Pointer(PointerEvent::Cancel(info)) = event {
                return Some(info as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`PointerCaptureEvent::Gained`] — fired after a view successfully gains pointer
        /// capture (following a `cx.request_pointer_capture()` call in a `PointerDown` handler).
        ///
        /// # Routing
        /// Dispatched to the **Target phase only** on the capturing element. Fired after the current
        /// event completes, not immediately when `request_pointer_capture` is called.
        ///
        /// # Default Actions
        /// No preventable default action. Call `cx.start_drag(drag_token, config, use_preview)` in
        /// this handler to initiate a drag operation.
        #[doc(alias = "GotPointerCapture")]
        pub GainedPointerCapture: DragToken,
        |event| {
            if let Event::PointerCapture(PointerCaptureEvent::Gained(token)) = event {
                return Some(token as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`PointerCaptureEvent::Lost`] — fired when a view loses pointer capture (on
        /// `PointerUp`, `PointerCancel`, or explicit release).
        ///
        /// # Routing
        /// Dispatched to the **Target phase only** on the element that had capture.
        ///
        /// # Default Actions
        /// No preventable default action. Use this event to clean up internal state associated with
        /// pointer capture. Note: disabled views can still receive this event to clean up state.
        pub LostPointerCapture: PointerId,
        |event| {
            if let Event::PointerCapture(PointerCaptureEvent::Lost(id)) = event {
                return Some(id as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Pointer`] `Gesture` variant — fired for touchpad pinch/zoom gestures.
        ///
        /// # Routing
        /// Spatially hit-tested at the pointer location, then dispatched via Capture → Target → Bubble.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub PinchGesture: PointerGestureEvent,
        |event| {
            if let Event::Pointer(PointerEvent::Gesture(pge)) = event {
                return Some(pge as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Ime`] `Enabled` variant — fired when IME composition is activated.
        ///
        /// # Routing
        /// Dispatched to the focused element via Capture → Target → Bubble.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub ImeEnabled: (),
        |event| {
            if let Event::Ime(ImeEvent::Enabled) = event {
                return Some(&() as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Ime`] `Disabled` variant — fired when IME composition ends.
        ///
        /// # Routing
        /// Dispatched to the focused element via Capture → Target → Bubble.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub ImeDisabled: (),
        |event| {
            if let Event::Ime(ImeEvent::Disabled) = event {
                return Some(&() as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Ime`] `Preedit` variant — fired when the IME composition text updates.
        ///
        /// # Routing
        /// Dispatched to the focused element via Capture → Target → Bubble.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub ImePreedit: ImeEvent,
        |event| {
            if let Event::Ime(e@ ImeEvent::Preedit { .. }) = event {
                return Some(e as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Ime`] `Commit` variant — fired when IME composition produces final text.
        ///
        /// # Routing
        /// Dispatched to the focused element via Capture → Target → Bubble.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub ImeCommit: String,
        |event| {
            if let Event::Ime(ImeEvent::Commit(text)) = event {
                return Some(text as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Ime`] `DeleteSurrounding` variant — fired when the IME requests that
        /// text surrounding the cursor be deleted (before/after selection).
        ///
        /// # Routing
        /// Dispatched to the focused element via Capture → Target → Bubble.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub ImeDeleteSurrounding: ImeEvent,
        |event| {
            if let Event::Ime(e@ ImeEvent::DeleteSurrounding { .. }) = event {
                return Some(e as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Pointer`] `Scroll` variant — fired for scroll wheel or touchpad scroll.
        ///
        /// # Routing
        /// Spatially hit-tested at the pointer location, then dispatched via Capture → Target → Bubble.
        ///
        /// # Default Actions
        /// No preventable default action. Use [`PointerScrollEventExt::resolve_to_points`] to
        /// convert scroll deltas to pixel values accounting for line and page sizes.
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
        #[doc(alias = "GotFocus")]
        pub FocusGained: (),
        |event| {
            if let Event::Focus(FocusEvent::Gained) = event {
                return Some(&() as &dyn Any);
            }
            None
        }
    );

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
        /// Receives [`Event::Window`] `ThemeChanged` variant — fired when the system theme changes
        /// (e.g., light to dark mode).
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        /// Views that do not register a listener do not receive window events.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub ThemeChanged: winit::window::Theme,
        |event| {
            if let Event::Window(WindowEvent::ThemeChanged(theme)) = event {
                return Some(theme as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Window`] `Closed` variant — fired when the window is closed.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub WindowClosed: (),
        |event| {
            if let Event::Window(WindowEvent::Closed) = event {
                return Some(&() as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Window`] `Resized` variant — fired when the window size changes.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub WindowResized: Size,
        |event| {
            if let Event::Window(WindowEvent::Resized(size)) = event {
                return Some(size as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Window`] `Moved` variant — fired when the window is moved.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub WindowMoved: Point,
        |event| {
            if let Event::Window(WindowEvent::Moved(pos)) = event {
                return Some(pos as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Window`] `FocusGained` variant — fired when the window gains OS-level
        /// input focus (distinct from view-level focus tracked by [`FocusGained`]).
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub WindowGainedFocus: (),
        |event| {
            if let Event::Window(WindowEvent::FocusGained) = event {
                return Some(&() as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Window`] `FocusLost` variant — fired when the window loses OS-level
        /// input focus (distinct from view-level focus tracked by [`FocusLost`]).
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub WindowLostFocus: (),
        |event| {
            if let Event::Window(WindowEvent::FocusLost) = event {
                return Some(&() as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Window`] `MaximizeChanged` variant — fired when the window's maximized
        /// state changes. The boolean is `true` if now maximized.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub WindowMaximizeChanged: bool,
        |event| {
            if let Event::Window(WindowEvent::MaximizeChanged(maximized)) = event {
                return Some(maximized as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Window`] `ScaleChanged` variant — fired when the window's DPI scale
        /// factor changes (e.g., when moved to a different display).
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub WindowScaleChanged: f64,
        |event| {
            if let Event::Window(WindowEvent::ScaleChanged(scale)) = event {
                return Some(scale as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives `Event::Window(UpdatePhase(ProcessingMessages))` — fired at the start of the
        /// update cycle's message processing phase.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        /// Not all phases run every cycle; a single phase may run multiple times per cycle.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub UpdatePhaseProcessingMessages: (),
        |event| {
            if let Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::ProcessingMessages)) = event {
                return Some(&() as &dyn Any);
            }
            None
        }
    );
    event_listener!(
        /// Receives `Event::Window(UpdatePhase(Style))` — fired during the style resolution phase.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        /// Not all phases run every cycle; a single phase may run multiple times per cycle.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub UpdatePhaseStyle: (),
        |event| {
            if let Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Style)) = event {
                return Some(&() as &dyn Any);
            }
            None
        }
    );
    event_listener!(
        /// Receives `Event::Window(UpdatePhase(Layout))` — fired during the layout computation phase.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        /// Not all phases run every cycle; a single phase may run multiple times per cycle.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub UpdatePhaseLayout: (),
        |event| {
            if let Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Layout)) = event {
                return Some(&() as &dyn Any);
            }
            None
        }
    );
    event_listener!(
        /// Receives `Event::Window(UpdatePhase(BoxTreeUpdate))` — fired when the box tree is being
        /// updated from layout results.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        /// Not all phases run every cycle; a single phase may run multiple times per cycle.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub UpdatePhaseBoxTreeUpdate: (),
        |event| {
            if let Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::BoxTreeUpdate)) = event {
                return Some(&() as &dyn Any);
            }
            None
        }
    );
    event_listener!(
        /// Receives `Event::Window(UpdatePhase(BoxTreePendingUpdates))` — fired when incremental
        /// box tree changes for specific views are being processed.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        /// Not all phases run every cycle; a single phase may run multiple times per cycle.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub UpdatePhaseBoxTreePendingUpdates: (),
        |event| {
            if let Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::BoxTreePendingUpdates)) = event {
                return Some(&() as &dyn Any);
            }
            None
        }
    );
    event_listener!(
        /// Receives `Event::Window(UpdatePhase(BoxTreeCommit))` — fired when box tree changes are
        /// being committed (finalized before painting).
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        /// Not all phases run every cycle; a single phase may run multiple times per cycle.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub UpdatePhaseBoxTreeCommit: (),
        |event| {
            if let Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::BoxTreeCommit)) = event {
                return Some(&() as &dyn Any);
            }
            None
        }
    );
    event_listener!(
        /// Receives `Event::Window(UpdatePhase(Complete))` — fired when the entire update cycle
        /// is complete and the window is ready for painting if needed.
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        /// Not all phases run every cycle; a single phase may run multiple times per cycle.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub UpdatePhaseComplete: (),
        |event| {
            if let Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Complete)) = event {
                return Some(&() as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`FileDragEvent::Drop`] — fired when files are dropped onto a view.
        ///
        /// # Routing
        /// Spatially hit-tested at the drop location. Dispatched to **Target phase only**.
        ///
        /// # Default Actions
        /// No preventable default action. The application must handle the dropped files. Note:
        /// disabled views can still receive this event for accessibility reasons.
        pub FileDragDrop: crate::event::dropped_file::FileDragDropped,
        |event| {
            if let Event::FileDrag(FileDragEvent::Drop(data)) = event {
                return Some(data as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`FileDragEvent::Enter`] — fired when files being dragged enter a view's bounds.
        ///
        /// # Routing
        /// Dispatched to **Target phase only**. Generated synthetically by the file-drag hover state
        /// machine when files enter the view's bounds.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub FileDragEnter: crate::event::dropped_file::FileDragEnter,
        |event| {
            if let Event::FileDrag(FileDragEvent::Enter(data)) = event {
                return Some(data as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`FileDragEvent::Move`] — fired as files are dragged over a view.
        ///
        /// # Routing
        /// Spatially hit-tested at the drag location. Dispatched to **Target phase only**.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub FileDragMove: crate::event::dropped_file::FileDragMove,
        |event| {
            if let Event::FileDrag(FileDragEvent::Move(data)) = event {
                return Some(data as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`FileDragEvent::Leave`] — fired when files being dragged leave a view's bounds.
        ///
        /// # Routing
        /// Dispatched to **Target phase only**. Generated synthetically by the file-drag hover state
        /// machine when files leave the view's bounds.
        ///
        /// # Default Actions
        /// No preventable default action.
        pub FileDragLeave: crate::event::dropped_file::FileDragLeave,
        |event| {
            if let Event::FileDrag(FileDragEvent::Leave(data)) = event {
                return Some(data as &dyn Any);
            }
            None
        }
    );

    event_listener!(
        /// Receives [`Event::Window`] `ChangeUnderCursor` variant — fired when the element under the
        /// cursor changes (e.g., after a layout update or view tree change at the current pointer position).
        ///
        /// # Routing
        /// Dispatched to **Target phase only** on all views that have registered this listener.
        /// Also triggers a hover state update from the last known pointer position, which may
        /// generate synthetic `PointerEnter`/`PointerLeave` events.
        ///
        /// # Default Actions
        /// No preventable default action.
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
}
