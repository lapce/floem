use peniko::kurbo::{Affine, Point, Size, Vec2};
use smallvec::SmallVec;
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
mod drag_state;
pub mod dropped_file;
pub(crate) mod path;
pub use drag_state::DragConfig;
pub(crate) use drag_state::DragTracker;

pub mod listener;

pub use dropped_file::FileDragEvent;
pub use path::clear_hit_test_cache;

pub use dispatch::*;

use crate::ElementId;

use std::any::Any;

// Trait for custom events
pub trait CustomEvent: Any + 'static {
    /// Get the unique key for this event type
    fn listener_key() -> listener::EventListenerKey
    where
        Self: Sized;

    /// Get the listener key for this trait object
    fn listener_key_dyn(&self) -> listener::EventListenerKey;

    fn transform(&mut self, transform: Affine) {
        let _ = transform;
    }

    fn allow_disabled(&self) -> bool {
        false
    }

    /// Clone this event into a Box without requiring Clone on the trait
    fn clone_box(&self) -> Box<dyn CustomEvent>;
    /// Downcast helper
    fn as_any(&self) -> &dyn Any;

    /// Format for Debug output. Default uses the type name.
    fn debug_fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(std::any::type_name::<Self>())
    }
}

/// Define a custom event type for use with Floem's event system.
///
/// This macro implements the [`CustomEvent`](crate::event::CustomEvent) trait and generates
/// a corresponding listener struct, allowing your type to be dispatched through the event
/// system and handled with [`on_event_stop`](crate::event::EventListener::on_event_stop)
/// and related methods.
///
/// # Variants
///
/// ## Simple (non-generic)
///
/// ```rust,ignore
/// #[derive(Clone)]
/// pub struct MyEvent {
///     pub message: String,
/// }
/// custom_event!(MyEvent);
/// ```
///
/// Generates `MyEventListener` and implements `CustomEvent` for `MyEvent`.
/// Handlers receive `&MyEvent` directly.
///
/// ## Simple (generic)
///
/// ```rust,ignore
/// #[derive(Clone)]
/// pub struct SelectionChanged<T: 'static> {
///     pub value: T,
/// }
/// custom_event!(SelectionChanged<T>);
/// ```
///
/// Generates `SelectionChangedListener<T>`. Each monomorphization (e.g.
/// `SelectionChanged<String>` vs `SelectionChanged<i32>`) gets its own unique
/// listener key, so events are dispatched to the correct typed handler.
/// Generic types must implement `Clone + 'static`.
///
/// For generic events, views should provide a typed convenience method rather
/// than requiring callers to use the listener directly:
/// ```rust,ignore
/// impl<T: Clone + 'static> MyDropdown<T> {
///     pub fn on_accept(self, handler: impl Fn(T) + 'static) -> Self {
///         self.on_event_stop(SelectionChanged::<T>::listener(), move |_cx, event| {
///             handler(event.value.clone());
///         })
///     }
/// }
/// ```
/// This ensures the type parameter is inferred correctly and gives callers
/// a clean API without users accidentally using the wrong generic type which would not downcast correctly.
///
/// ## With custom extractor (non-generic only)
///
/// ```rust,ignore
/// #[derive(Clone)]
/// pub struct ScrollEvent {
///     pub offset: f64,
///     pub viewport_size: f64,
/// }
/// custom_event!(ScrollEvent, f64, |event: &ScrollEvent| -> Option<f64> {
///     Some(&event.offset)
/// });
/// ```
///
/// Handlers receive `&f64` instead of `&ScrollEvent`. The extractor must return
/// a reference into the event — it cannot return computed values.
///
/// Custom extractors are not supported for generic events, since the handler type
/// would not be obvious from the listener name alone.
///
/// # Optional parameters
///
/// All variants accept optional trailing parameters in any combination:
///
/// - **`transform`** — Apply coordinate transforms to the event (e.g. for pointer-like
///   custom events that carry position data):
///   ```rust,ignore
///   custom_event!(MyPointerEvent, transform = |event: &mut MyPointerEvent, affine: Affine| {
///       event.pos = affine * event.pos;
///   });
///   ```
///
/// - **`allow_disabled`** — Allow this event to reach disabled views:
///   ```rust,ignore
///   custom_event!(MyEvent, allow_disabled = |_event: &MyEvent| true);
///   ```
///
/// - **`debug_fmt`** — Override the [`Debug`] representation used when printing
///   [`Event::Custom`](crate::event::Event::Custom). By default, the type name is used.
///   Types that derive `Debug` can delegate to it:
///   ```rust,ignore
///   custom_event!(MyEvent, debug_fmt = |event, f| std::fmt::Debug::fmt(event, f));
///   ```
///
/// # Usage
///
/// **Firing an event:**
/// ```rust
/// # use floem::context::Phases;
/// # use floem::event::{Event, RouteKind};
/// # use floem::prelude::dropdown::DropdownAccept;
/// # use floem::ViewId;
/// let view_id = ViewId::new();
/// let value = 1usize;
/// view_id.route_event(
///     Event::new_custom(DropdownAccept { value }),
///     RouteKind::Directed {
///         target: view_id.get_element_id(),
///         phases: Phases::TARGET,
///     },
/// );
/// ```
///
/// **Handling an event:**
/// ```rust,ignore
/// view.on_event_stop(MyEvent::listener(), |cx, event: &MyEvent| {
///     println!("{}", event.message);
/// })
/// ```
///
/// # Requirements
///
/// - The event type must implement [`Clone`].
/// - For generic events, all type parameters must be `Clone + 'static`.
/// - For custom extractors, the extractor closure must return a reference into the event.
#[macro_export]
macro_rules! custom_event {
    // Generic variant - EventData = Self, extract = identity
    (
        $name:ident < $($generic:ident),* >
        $(, transform = $transform:expr)?
        $(, allow_disabled = $allow_disabled:expr)?
        $(, debug_fmt = $debug_fmt:expr)?
    ) => {
        ::paste::paste! {
            #[doc = "Listener for `" $name "` events"]
            pub struct [<$name Listener>]<$($generic),*>(std::marker::PhantomData<$(fn() -> $generic),*>);

            impl<$($generic),*> Default for [<$name Listener>]<$($generic),*> {
                fn default() -> Self { Self(std::marker::PhantomData) }
            }
            impl<$($generic),*> Copy for [<$name Listener>]<$($generic),*> {}
            impl<$($generic),*> Clone for [<$name Listener>]<$($generic),*> {
                fn clone(&self) -> Self { *self }
            }

            impl<$($generic: Clone + 'static),*> [<$name Listener>]<$($generic),*> {
                fn info() -> &'static $crate::event::listener::EventKeyInfo {
                    static INFO: $crate::event::listener::EventKeyInfo = $crate::event::listener::EventKeyInfo {
                        name: || stringify!($name),
                        extract: |event| {
                            if let $crate::event::Event::Custom(custom) = event {
                                Some(custom.as_any() as &dyn std::any::Any)
                            } else {
                                None
                            }
                        },
                    };
                    &INFO
                }
            }

            impl<$($generic: Clone + 'static),*> $crate::event::listener::EventListenerTrait for [<$name Listener>]<$($generic),*> {
                type EventData = $name<$($generic),*>;

                fn listener_key() -> $crate::event::listener::EventListenerKey {
                    $crate::event::listener::EventListenerKey {
                        info: Self::info(),
                        type_discriminant: Some(std::any::TypeId::of::<$name<$($generic),*>>()),
                    }
                }

                fn extract(event: &$crate::event::Event) -> Option<&Self::EventData> {
                    if let $crate::event::Event::Custom(custom) = event {
                        custom.as_any().downcast_ref::<$name<$($generic),*>>()
                    } else {
                        None
                    }
                }
            }

            impl<$($generic: Clone + 'static),*> $crate::event::CustomEvent for $name<$($generic),*> {
                fn listener_key() -> $crate::event::listener::EventListenerKey {
                    <[<$name Listener>]<$($generic),*> as $crate::event::listener::EventListenerTrait>::listener_key()
                }

                fn listener_key_dyn(&self) -> $crate::event::listener::EventListenerKey {
                    Self::listener_key()
                }

                $(
                    fn transform(&mut self, transform: $crate::kurbo::Affine) {
                        ($transform)(self, transform)
                    }
                )?

                $(
                    fn allow_disabled(&self) -> bool {
                        ($allow_disabled)(self)
                    }
                )?

                fn as_any(&self) -> &dyn std::any::Any {
                    self
                }

                fn clone_box(&self) -> Box<dyn $crate::event::CustomEvent> {
                    Box::new(self.clone())
                }

                $crate::custom_event!(@debug_fmt self $($debug_fmt)?);
            }

            impl<$($generic: Clone + 'static),*> $name<$($generic),*> {
                /// Get the event listener for this custom event
                #[allow(dead_code)]
                pub fn listener() -> [<$name Listener>]<$($generic),*> {
                    [<$name Listener>](std::marker::PhantomData)
                }
            }
        }
    };

    // Non-generic with custom extractor and EventData type
    (
        $name:ty,
        $event_data:ty,
        $extract:expr
        $(, transform = $transform:expr)?
        $(, allow_disabled = $allow_disabled:expr)?
        $(, debug_fmt = $debug_fmt:expr)?
    ) => {
        ::paste::paste! {
            $crate::event_listener!(
                #[doc = "Listener for `" $name "` events"]
                pub [<$name Listener>]: $event_data,
                |event| {
                    if let $crate::event::Event::Custom(custom) = event {
                        custom.as_any().downcast_ref::<$name>()
                            .map($extract)
                            .map(|e| e as &dyn std::any::Any)
                    } else {
                        None
                    }
                }
            );
        }

        impl $crate::event::CustomEvent for $name {
            fn listener_key() -> $crate::event::listener::EventListenerKey {
                ::paste::paste! {
                    <[<$name Listener>] as $crate::event::listener::EventListenerTrait>::listener_key()
                }
            }

            fn listener_key_dyn(&self) -> $crate::event::listener::EventListenerKey {
                Self::listener_key()
            }

            $(
                fn transform(&mut self, transform: $crate::kurbo::Affine) {
                    ($transform)(self, transform)
                }
            )?

            $(
                fn allow_disabled(&self) -> bool {
                    ($allow_disabled)(self)
                }
            )?

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }

            fn clone_box(&self) -> Box<dyn $crate::event::CustomEvent> {
                Box::new(self.clone())
            }

            $crate::custom_event!(@debug_fmt self $($debug_fmt)?);
        }

        impl $name {
            /// Get the event listener for this custom event
            pub fn listener() -> ::paste::paste! { [<$name Listener>] } {
                ::paste::paste! { [<$name Listener>] }
            }

            /// Attempt to extract this event type from a generic [`Event`].
            ///
            /// Equivalent to:
            /// `Self::listener_key().extract(event)`
            pub fn extract(
                event: &$crate::event::Event,
            ) -> Option<&$event_data> {
                <::paste::paste! { [<$name Listener>] }
                    as $crate::event::listener::EventListenerTrait>::extract(event)
            }

        }
    };

    // Non-generic simple variant - EventData = Self, extract = identity
    (
        $name:ty
        $(, transform = $transform:expr)?
        $(, allow_disabled = $allow_disabled:expr)?
        $(, debug_fmt = $debug_fmt:expr)?
    ) => {
        $crate::custom_event! {
            $name,
            $name,
            |data: &$name| -> &$name { data }
            $(, transform = $transform)?
            $(, allow_disabled = $allow_disabled)?
            $(, debug_fmt = $debug_fmt)?
        }
    };

    // Internal helper: emit debug_fmt override or nothing (uses trait default)
    (@debug_fmt $self:ident $debug_fmt:expr) => {
        fn debug_fmt(&$self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            ($debug_fmt)($self, f)
        }
    };
    (@debug_fmt $self:ident) => {};
}

/// Phases of event propagation.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Phase {
    /// Parent-to-target traversal.
    Capture,
    /// Target node.
    Target,
    /// Target-to-parent traversal.
    Bubble,
    /// A phase for global and fallback events.
    /// Currently this is used for keyboard and ime events that are broadcast after failing to be handled by the focus path.
    Broadcast,
}
impl Phase {
    /// Returns `true` if the phase is [`Capture`].
    ///
    /// [`Capture`]: Phase::Capture
    #[must_use]
    pub fn is_capture(&self) -> bool {
        matches!(self, Self::Capture)
    }

    /// Returns `true` if the phase is [`Target`].
    ///
    /// [`Target`]: Phase::Target
    #[must_use]
    pub fn is_target(&self) -> bool {
        matches!(self, Self::Target)
    }

    /// Returns `true` if the phase is [`Bubble`].
    ///
    /// [`Bubble`]: Phase::Bubble
    #[must_use]
    pub fn is_bubble(&self) -> bool {
        matches!(self, Self::Bubble)
    }

    /// Returns `true` if the phase is [`Broadcast`].
    ///
    /// [`Broadcast`]: Phase::Broadcast
    #[must_use]
    pub fn is_broadcast(&self) -> bool {
        matches!(self, Self::Broadcast)
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

/// Focus-related events fired during focus transitions.
///
/// Focus events are fired when keyboard focus moves between elements in the UI.
/// Both variants participate in all three phases (capture, target, and bubble)
/// using `Phases::STANDARD`, similar to W3C's `focusin` and `focusout` events.
///
/// This allows parent elements to track focus changes within their subtree by
/// listening during the capture or bubble phases.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum FocusEvent {
    /// The element directly received focus.
    ///
    /// This event participates in all three phases: capture, target, and bubble
    /// (using `Phases::STANDARD`), similar to W3C's `focusin` event.
    ///
    /// The event is targeted at the element that became focused, meaning listeners
    /// on ancestor elements can observe it during the capture and bubble phases.
    Gained,

    /// The element directly lost focus.
    ///
    /// This event participates in all three phases: capture, target, and bubble
    /// (using `Phases::STANDARD`), similar to W3C's `focusout` event.
    ///
    /// The event is targeted at the element that lost focus, meaning listeners
    /// on ancestor elements can observe it during the capture and bubble phases.
    Lost,
}

/// Pointer capture state changes.
///
/// Dispatched to the **Target phase only** on the element gaining or losing capture.
/// Fired after the current event completes, not immediately when `request_pointer_capture` is called.
/// See [`Event::PointerCapture`] for full routing and lifecycle details.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum PointerCaptureEvent {
    /// Fired after a view successfully gains pointer capture (following a
    /// `cx.request_pointer_capture()` call). Contains the [`DragToken`] wrapping the pointer ID.
    ///
    /// All subsequent pointer events for this pointer ID are now routed directly to this element,
    /// bypassing spatial hit-testing. Call `cx.start_drag(drag_token, config, use_preview)`
    /// in this handler to initiate a drag operation.
    ///
    /// No preventable default action.
    Gained(DragToken),
    /// Fired when a view loses pointer capture (on `PointerUp`, `PointerCancel`, or explicit
    /// release). Contains the [`PointerId`] that was released.
    ///
    /// Use this event to clean up any internal state associated with pointer capture
    /// (e.g., reset drag visual state).
    ///
    /// No preventable default action. Note: disabled views can still receive this event
    /// to clean up internal state.
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

/// High-level interaction events that abstract over pointer and keyboard input.
/// These events represent user intent (clicking, double-clicking, etc.)
/// regardless of the input method used.
///
/// Dispatched via Capture → Target → Bubble to the common ancestor of the pointer down and up
/// targets, or to the focused element for keyboard-triggered clicks. See [`Event::Interaction`]
/// for full routing details.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum InteractionEvent {
    /// A single click, generated from a pointer Down+Up sequence on the same view or a keyboard
    /// trigger (`Space`/`Enter`/`NumpadEnter` on the focused element).
    ///
    /// No preventable default action. This event is itself a default action of
    /// [`Event::Pointer`] and [`Event::Key`] events.
    Click,
    /// A double click, generated when two rapid pointer clicks occur (pointer up count > 1).
    /// Dispatched immediately after the second [`InteractionEvent::Click`].
    ///
    /// No preventable default action.
    DoubleClick,
    /// A secondary (right) click, generated from a secondary button Down+Up sequence.
    ///
    /// No preventable default action. Note: context menus are handled separately via the
    /// view's context menu configuration, triggered on pointer down/up depending on the
    /// platform, not from this event.
    SecondaryClick,
}

/// Phases of the window update cycle.
///
/// These events are fired during `process_update()` to allow views to observe
/// and react to different stages of the update pipeline. This is primarily useful
/// for debugging, profiling, or coordinating effects that need to run at specific
/// points in the update cycle.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum UpdatePhaseEvent {
    /// Processing update messages from the queue.
    /// This phase handles state changes and view updates queued by the application.
    ProcessingMessages,

    /// Computing styles for views that need style resolution.
    /// This phase resolves CSS-like properties and inheritance.
    Style,

    /// Computing layout for views with changed sizes or positions.
    /// This phase uses the Taffy layout engine to determine final rectangles.
    Layout,

    /// Updating the box tree from layout results.
    /// This phase synchronizes the rendering box tree with computed layout.
    BoxTreeUpdate,

    /// Processing pending individual box tree updates.
    /// This phase handles incremental box tree changes for specific views.
    BoxTreePendingUpdates,

    /// Committing the box tree changes.
    /// This phase finalizes box tree modifications before painting.
    BoxTreeCommit,

    /// Update cycle complete.
    /// This phase signals that all updates have been processed and the window
    /// is ready for painting if needed.
    Complete,

    /// Update cycle complete.
    /// This phase signals that all updates have been processed and the window
    /// is ready for painting if needed.
    PaintPresent,
}

/// Events related to the application window state.
///
/// Unlike pointer and keyboard events which are dispatched via hit-testing,
/// window events are sent only to views that have registered a listener
/// for them (e.g., `.on_event(listener::WindowResized, ...)`). Views that
/// do not register a listener will not receive window events.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WindowEvent {
    /// The window gained input focus.
    FocusGained,

    /// The window lost input focus.
    FocusLost,

    /// The window was closed.
    Closed,

    /// The window was resized to the given dimensions.
    Resized(Size),

    /// The window was moved to the given position.
    Moved(Point),

    /// The window's maximized state changed. `true` if now maximized.
    MaximizeChanged(bool),

    /// The window's scale factor changed (e.g., moved to a different DPI display).
    ScaleChanged(f64),

    /// The system theme changed (e.g., light to dark mode).
    ThemeChanged(Theme),

    /// The element under the cursor changed.
    ChangeUnderCursor,

    /// The window update cycle entered a new phase.
    ///
    /// These events are fired during the update processing loop to signal
    /// transitions between different stages (style → layout → box tree → etc.).
    /// Views that register listeners for this event can observe the update
    /// pipeline's progress, which is useful for debugging, performance monitoring,
    /// or triggering effects that must run at specific update stages.
    ///
    /// Not all phases will run for every cycle and in a single cycle a single phase might run multiple times.
    ///
    /// Like the other window events, these are only sent to views that
    /// have explicitly registered a listener via `.on_event(listener::UpdatePhase, ...)`.
    UpdatePhase(UpdatePhaseEvent),
}

// ============================================================================
// Shared Event Data Structures
// ============================================================================
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct DragToken(PointerId);
impl DragToken {
    pub fn pointer_id(&self) -> PointerId {
        self.0
    }
}

/// Event data for when a drag operation starts.
///
/// Only sent to the element being dragged via [`DragSourceEvent::Start`].
#[derive(Clone, Debug)]
pub struct DragStartEvent {
    /// Pointer state when drag started
    pub start_state: PointerState,
    /// Current pointer state which may differ from the start state if the threshold is greater than 0
    pub current_state: PointerState,
    /// Which button initiated the drag
    pub button: Option<PointerButton>,
    /// Pointer identity
    pub pointer: PointerInfo,
    /// Custom data passed from the drag source
    pub custom_data: Option<std::rc::Rc<dyn std::any::Any>>,
}

/// Event data for when the pointer moves during a drag operation.
///
/// Sent to the dragged element via [`DragSourceEvent::Move`] and to drop targets via [`DragTargetEvent::Move`].
#[derive(Clone, Debug)]
pub struct DragMoveEvent {
    /// The "other" element involved in this move event.
    ///
    /// When sent to the dragged element (via [`DragSourceEvent::Move`]):
    /// This is `None` as the source doesn't need to track itself.
    ///
    /// When sent to a drop target (via [`DragTargetEvent::Move`]):
    /// This is `Some(dragged_element)` - the element being dragged over this target.
    pub other_element: Option<ElementId>,
    /// The starting state of the drag
    pub start_state: PointerState,
    /// The current state of the pointer
    pub current_state: PointerState,
    /// Which button initiated the drag
    pub button: Option<PointerButton>,
    /// Pointer identity
    pub pointer: PointerInfo,
    /// Custom data passed from the drag source
    pub custom_data: Option<std::rc::Rc<dyn std::any::Any>>,
}

/// Event data for when a dragged element enters a drop target, or when a drop target
/// is entered by a dragged element.
///
/// This struct is shared between:
/// - [`DragSourceEvent::Enter`] - sent to the element being dragged
/// - [`DragTargetEvent::Enter`] - sent to the drop target being entered
#[derive(Clone, Debug)]
pub struct DragEnterEvent {
    /// The "other" element involved in this enter event.
    ///
    /// When sent to the dragged element (via [`DragSourceEvent::Enter`]):
    /// This is the drop target element that was just entered.
    ///
    /// When sent to a drop target (via [`DragTargetEvent::Enter`]):
    /// This is the dragged element that just entered.
    pub other_element: ElementId,
    /// The pointer state from when the drag started
    pub start_state: PointerState,
    /// The current state of the pointer
    pub current_state: PointerState,
    /// Which button initiated the drag
    pub button: Option<PointerButton>,
    /// Pointer identity
    pub pointer: PointerInfo,
    /// Custom data passed from the drag source
    pub custom_data: Option<std::rc::Rc<dyn std::any::Any>>,
}

/// Event data for when a dragged element leaves a drop target, or when a drop target
/// is left by a dragged element.
///
/// This struct is shared between:
/// - [`DragSourceEvent::Leave`] - sent to the element being dragged
/// - [`DragTargetEvent::Leave`] - sent to the drop target being left
#[derive(Clone, Debug)]
pub struct DragLeaveEvent {
    /// The "other" element involved in this leave event.
    ///
    /// When sent to the dragged element (via [`DragSourceEvent::Leave`]):
    /// This is the drop target element that was just left.
    ///
    /// When sent to a drop target (via [`DragTargetEvent::Leave`]):
    /// This is the dragged element that just left.
    pub other_element: ElementId,
    /// The pointer state from when the drag started
    pub start_state: PointerState,
    /// Current pointer state
    pub current_state: PointerState,
    /// Which button initiated the drag
    pub button: Option<PointerButton>,
    /// Pointer identity
    pub pointer: PointerInfo,
    /// Custom data passed from the drag source
    pub custom_data: Option<std::rc::Rc<dyn std::any::Any>>,
}

/// Event data for when a drag operation completes with a drop.
///
/// This struct is shared between:
/// - [`DragSourceEvent::Drop`] - sent to the element being dragged
/// - [`DragTargetEvent::Drop`] - sent to the drop target (which can call `prevent_default()` to accept)
#[derive(Clone, Debug)]
pub struct DragEndEvent {
    /// The "other" element involved in this drop event.
    ///
    /// When sent to the dragged element (via [`DragSourceEvent::Drop`]):
    /// This is the drop target element where the drop occurred, or `None` if dropped
    /// outside any valid drop target.
    ///
    /// When sent to a drop target (via [`DragTargetEvent::Drop`]):
    /// This is always `Some` containing the dragged element that was dropped.
    pub other_element: Option<ElementId>,
    /// The pointer state from when the drag started
    pub start_state: PointerState,
    /// Final pointer state
    pub current_state: PointerState,
    /// Which button initiated the drag and has now been released
    pub button: Option<PointerButton>,
    /// Pointer identity
    pub pointer: PointerInfo,
    /// Custom data passed from the drag source
    pub custom_data: Option<std::rc::Rc<dyn std::any::Any>>,
}

/// Event data for when a drag operation is cancelled.
///
/// Only sent to the element being dragged via [`DragSourceEvent::Cancel`].
/// Occurs when the drag is aborted (e.g., Escape key pressed, pointer left window,
/// or pointer released without a valid drop target accepting the drop).
#[derive(Clone, Debug)]
pub struct DragCancelEvent {
    /// The pointer state from when the drag started
    pub start_state: PointerState,
    /// Final pointer state
    pub current_state: PointerState,
    /// Which button initiated the drag
    pub button: Option<PointerButton>,
    /// Pointer identity
    pub pointer: PointerInfo,
    /// Custom data passed from the drag source
    pub custom_data: Option<std::rc::Rc<dyn std::any::Any>>,
}

// ============================================================================
// Event Enums
// ============================================================================

/// Events sent to the element being dragged during a drag operation.
///
/// These events track the lifecycle of the drag from the perspective of the source element:
/// - Starting and moving
/// - Entering and leaving potential drop targets
/// - Completing with either a drop or cancellation
///
/// The dragged element receives all these events and can use them to provide visual feedback,
/// track what targets are being hovered over, and respond to the outcome of the drag.
#[derive(Clone, Debug)]
pub enum DragSourceEvent {
    /// Drag operation started - sent when the pointer has moved beyond the drag threshold
    /// while the button is held down.
    Start(DragStartEvent),

    /// Pointer moved during drag - sent as the pointer moves while dragging.
    Move(DragMoveEvent),

    /// The dragged element entered a potential drop target.
    /// `other_element` is the target that was entered.
    Enter(DragEnterEvent),

    /// The dragged element left a potential drop target.
    /// `other_element` is the target that was left.
    Leave(DragLeaveEvent),

    /// Drag ended — sent when the pointer is released.
    ///
    /// `other_element` is `Some(target_id)` if the pointer was released over a drop target,
    /// or `None` if released without a drop target under the pointer.
    ///
    /// Note: `DragTargetEvent::Drop` is dispatched simultaneously when `other_element` is `Some`.
    End(DragEndEvent),

    /// Drag was cancelled — sent when the drag is aborted by a `PointerCancel` event
    /// (e.g., touch interrupted, window lost focus during touch drag).
    Cancel(DragCancelEvent),
}

/// Events sent to potential drop targets during a drag operation.
///
/// These events allow drop targets to:
/// - Know when a dragged element is hovering over them (Enter/Leave/Move)
/// - React to drops and perform the drop action (Drop)
/// - Provide visual feedback during drag hover
/// - Access information about the dragged element via `other_element`
///
/// Drop targets only receive events when a dragged element is directly over them.
#[derive(Clone, Debug)]
pub enum DragTargetEvent {
    /// A dragged element entered this drop target.
    /// `other_element` is the element being dragged.
    Enter(DragEnterEvent),

    /// The pointer moved while a dragged element is over this drop target.
    /// `other_element` is the element being dragged.
    Move(DragMoveEvent),

    /// A dragged element left this drop target.
    /// `other_element` is the element being dragged.
    Leave(DragLeaveEvent),

    /// A dragged element was dropped on this target — sent when the pointer is released
    /// while over this target. `other_element` is the element being dragged.
    ///
    /// The drag source always receives [`DragSourceEvent::End`] simultaneously (with
    /// `other_element` set to this target's ID). The source only receives
    /// [`DragSourceEvent::Cancel`] if the drag was aborted by a `PointerCancel` event,
    /// not based on whether a drop target handles this event.
    ///
    /// No preventable default action.
    Drop(DragEndEvent),
}

impl DragSourceEvent {
    /// Get the current pointer state for this drag event.
    pub fn current_state(&self) -> &PointerState {
        match self {
            Self::Start(e) => &e.current_state,
            Self::Move(e) => &e.current_state,
            Self::Enter(e) => &e.current_state,
            Self::Leave(e) => &e.current_state,
            Self::End(e) => &e.current_state,
            Self::Cancel(e) => &e.current_state,
        }
    }

    /// Get a mutable reference to the current pointer state for this drag event.
    pub fn current_state_mut(&mut self) -> &mut PointerState {
        match self {
            Self::Start(e) => &mut e.current_state,
            Self::Move(e) => &mut e.current_state,
            Self::Enter(e) => &mut e.current_state,
            Self::Leave(e) => &mut e.current_state,
            Self::End(e) => &mut e.current_state,
            Self::Cancel(e) => &mut e.current_state,
        }
    }

    /// Get the pointer state from when the drag started.
    pub fn start_state(&self) -> &PointerState {
        match self {
            Self::Start(e) => &e.start_state,
            Self::Move(e) => &e.start_state,
            Self::Enter(e) => &e.start_state,
            Self::Leave(e) => &e.start_state,
            Self::End(e) => &e.start_state,
            Self::Cancel(e) => &e.start_state,
        }
    }

    /// Get the pointer button that initiated this drag.
    pub fn button(&self) -> Option<PointerButton> {
        match self {
            Self::Start(e) => e.button,
            Self::Move(e) => e.button,
            Self::Enter(e) => e.button,
            Self::Leave(e) => e.button,
            Self::End(e) => e.button,
            Self::Cancel(e) => e.button,
        }
    }

    /// Get the pointer identity information.
    pub fn pointer(&self) -> &PointerInfo {
        match self {
            Self::Start(e) => &e.pointer,
            Self::Move(e) => &e.pointer,
            Self::Enter(e) => &e.pointer,
            Self::Leave(e) => &e.pointer,
            Self::End(e) => &e.pointer,
            Self::Cancel(e) => &e.pointer,
        }
    }

    /// Get a mutable reference to the pointer state from when the drag started.
    pub fn start_state_mut(&mut self) -> &mut PointerState {
        match self {
            Self::Start(e) => &mut e.start_state,
            Self::Move(e) => &mut e.start_state,
            Self::Enter(e) => &mut e.start_state,
            Self::Leave(e) => &mut e.start_state,
            Self::End(e) => &mut e.start_state,
            Self::Cancel(e) => &mut e.start_state,
        }
    }
}

impl DragTargetEvent {
    /// Get the current pointer state for this drag event.
    pub fn current_state(&self) -> &PointerState {
        match self {
            Self::Enter(e) => &e.current_state,
            Self::Move(e) => &e.current_state,
            Self::Leave(e) => &e.current_state,
            Self::Drop(e) => &e.current_state,
        }
    }

    /// Get a mutable reference to the current pointer state for this drag event.
    pub fn current_state_mut(&mut self) -> &mut PointerState {
        match self {
            Self::Enter(e) => &mut e.current_state,
            Self::Move(e) => &mut e.current_state,
            Self::Leave(e) => &mut e.current_state,
            Self::Drop(e) => &mut e.current_state,
        }
    }

    /// Get the pointer state from when the drag started.
    pub fn start_state(&self) -> &PointerState {
        match self {
            Self::Enter(e) => &e.start_state,
            Self::Move(e) => &e.start_state,
            Self::Leave(e) => &e.start_state,
            Self::Drop(e) => &e.start_state,
        }
    }

    /// Get the pointer button that initiated this drag.
    pub fn button(&self) -> Option<PointerButton> {
        match self {
            Self::Enter(e) => e.button,
            Self::Move(e) => e.button,
            Self::Leave(e) => e.button,
            Self::Drop(e) => e.button,
        }
    }

    /// Get the pointer identity information.
    pub fn pointer(&self) -> &PointerInfo {
        match self {
            Self::Enter(e) => &e.pointer,
            Self::Move(e) => &e.pointer,
            Self::Leave(e) => &e.pointer,
            Self::Drop(e) => &e.pointer,
        }
    }

    /// Get the ID of the element being dragged (the source element).
    pub fn dragged_element(&self) -> Option<ElementId> {
        match self {
            Self::Enter(e) => Some(e.other_element),
            Self::Move(e) => e.other_element,
            Self::Leave(e) => Some(e.other_element),
            Self::Drop(e) => e.other_element,
        }
    }

    /// Get a mutable reference to the pointer state from when the drag started.
    pub fn start_state_mut(&mut self) -> &mut PointerState {
        match self {
            Self::Enter(e) => &mut e.start_state,
            Self::Move(e) => &mut e.start_state,
            Self::Leave(e) => &mut e.start_state,
            Self::Drop(e) => &mut e.start_state,
        }
    }

    /// Returns `true` if the drag target event is [`Enter`].
    ///
    /// [`Enter`]: DragTargetEvent::Enter
    #[must_use]
    pub fn is_enter(&self) -> bool {
        matches!(self, Self::Enter(..))
    }

    /// Returns `true` if the drag target event is [`Move`].
    ///
    /// [`Move`]: DragTargetEvent::Move
    #[must_use]
    pub fn is_move(&self) -> bool {
        matches!(self, Self::Move(..))
    }

    /// Returns `true` if the drag target event is [`Leave`].
    ///
    /// [`Leave`]: DragTargetEvent::Leave
    #[must_use]
    pub fn is_leave(&self) -> bool {
        matches!(self, Self::Leave(..))
    }

    /// Returns `true` if the drag target event is [`Drop`].
    ///
    /// [`Drop`]: DragTargetEvent::Drop
    #[must_use]
    pub fn is_drop(&self) -> bool {
        matches!(self, Self::Drop(..))
    }
}

/// Drag and drop event discriminating between source and target events.
#[derive(Clone, Debug)]
pub enum DragEvent {
    /// Events sent to the element being dragged.
    Source(DragSourceEvent),
    /// Events sent to potential drop targets.
    Target(DragTargetEvent),
}

/// The Floem Events.
///
/// # Event System Overview
///
/// Floem's event system is inspired by the DOM event model, with events flowing through
/// the view tree in multiple phases:
///
/// ## Event Phases
///
/// Most events support three phases of propagation:
/// - **Capture**: Events travel from root to target, allowing ancestors to intercept early
/// - **Target**: The event reaches the actual target element
/// - **Bubble**: Events travel from target back to root, allowing ancestors to handle after target
///
/// There is also a broadcast phases where events propagate recursively depth first through the element tree.
/// This is used internally only for keyboard events. This way your keyboard event listeners can run even without the view having focus.
///
/// Not all events propagate through all phases - see individual variants for details.
///
/// ## Event Routing
///
/// Events are routed through the view tree using different strategies:
/// - **Directed**: Route to a specific target with customizable phases (keyboard events to focused view)
/// - **Spatial**: Route based on hit-testing at a point to find the top hit and then route directed to the hit (pointer events)
/// - **Broadcast**: Route to all views or a subtree (window resize events) that have registered a listener recursively depth first.
///
/// ## Propagation Control
///
/// Event handlers can control propagation using [`EventCx`]:
/// - `cx.stop_immediate_propagation()`: Stop all further propagation, including other listeners on the same target
/// - Returning `EventPropagation::Stop`: Stop propagation to next phase (bubble/capture), but allow other listeners on same target
/// - `cx.prevent_default()`: Prevent default browser-like behaviors (tab navigation, clicks, etc.)
///
/// ## Event Lifecycle
///
/// 1. External events arrive from the window system (pointer, keyboard, window events)
/// 2. Events are routed through the view tree based on their type
/// 3. Synthetic events may be generated (e.g., Click from PointerDown+Up)
/// 4. Default behaviors execute if not prevented (drag thresholds, tab navigation)
/// 5. Pending events (synthetic or user-emitted) are processed
pub enum Event {
    /// Pointer events from mice, pens, and touch input.
    ///
    /// # Routing
    /// - **Spatial routing**: Hit-tested at the pointer location
    /// - **Phases**: Capture, Target, Bubble
    /// - **Exception**: `PointerEnter` and `PointerLeave` use Target phase only (no bubbling)
    ///
    /// # Pointer Capture
    /// While a pointer is down, an element can request pointer capture via
    /// `cx.request_pointer_capture(pointer_id)`. While captured, all pointer events
    /// for that pointer are routed directly to the capturing element, bypassing hit-testing.
    ///
    /// # Events
    /// - `Down`: Button pressed (triggers focus update and click tracking)
    /// - `Up`: Button released (may generate Click/DoubleClick events)
    /// - `Move`: Pointer moved (updates hover state, may start drag if threshold exceeded)
    /// - `Cancel`: Gesture cancelled (releases capture, cancels pending clicks)
    /// - `Enter`: Pointer entered this element (Target phase only, fired when hover state changes)
    /// - `Leave`: Pointer left this element (Target phase only, fired when hover state changes)
    /// - `Scroll`: Scroll wheel/touchpad input
    /// - `Gesture`: Touchpad gestures (pinch, rotate)
    ///
    /// # Default Actions
    /// Call `cx.prevent_default()` to suppress these behaviors.
    ///
    /// - [`PointerEvent::Down`]: Moves keyboard focus to the hit element (fires synthetic
    ///   `FocusLost`/`FocusGained` events). On macOS, shows the context menu on secondary button press.
    /// - [`PointerEvent::Move`]: If the pointer has moved beyond the drag threshold while a button
    ///   is held, begins a drag operation (`DragSourceEvent::Start` fires). While a drag is active,
    ///   fires drag source/target move and enter/leave events as the pointer moves.
    /// - [`PointerEvent::Up`]: Ends any active drag (fires `DragSourceEvent::End` or
    ///   `DragSourceEvent::Cancel` depending on whether a target accepted). Releases pointer capture
    ///   unconditionally. On non-macOS platforms, shows the context menu on secondary button release.
    /// - [`PointerEvent::Leave`]: Clears all hover state, firing synthetic `PointerLeave` events
    ///   for all currently-hovered elements.
    /// - [`PointerEvent::Cancel`]: Aborts any active drag (`DragSourceEvent::Cancel` fires).
    ///   Releases pointer capture unconditionally.
    /// - [`PointerEvent::Enter`], [`PointerEvent::Scroll`], [`PointerEvent::Gesture`]: No
    ///   preventable default action.
    ///
    /// # Example
    /// ```rust
    /// # use floem::event::{Event, EventCx};
    /// # use ui_events::pointer::PointerEvent;
    /// fn handle(event: Event, cx: &mut EventCx) {
    ///     match event {
    ///         Event::Pointer(PointerEvent::Down(pe)) => {
    ///             if let Some(pointer_id) = pe.pointer.pointer_id {
    ///                 cx.request_pointer_capture(pointer_id);
    ///             }
    ///         }
    ///         _ => {}
    ///     }
    /// }
    /// ```
    Pointer(PointerEvent),

    /// Keyboard events for key presses and releases.
    ///
    /// # Routing
    /// Key events are classified as either **shortcut-like** or **typing** keys,
    /// which determines their routing strategy. See [`KeyEventExt::is_shortcut_like`].
    ///
    /// ## Typing keys (unmodified character input, arrows, etc.)
    /// - **Directed to focused element**: Capture → Target → Bubble
    /// - If no view has focus, the event is dropped
    ///
    /// ## Shortcut-like keys (Ctrl/Cmd/Alt combos, F-keys, Escape, Tab, etc.)
    /// - **Directed to focused element first**: Capture → Target → Bubble
    /// - **Fallback**: If unconsumed (or no view has focus), dispatched via the
    ///   listener registry to all views that registered a key event listener.
    ///   No ordering or propagation is respected in this fallback.
    ///
    /// # Default Actions
    /// Call `cx.prevent_default()` to suppress these behaviors.
    ///
    /// - `Tab` (KeyDown, no modifiers): Moves focus to the next focusable element.
    ///   `Shift+Tab` moves focus backwards.
    /// - `Alt+ArrowUp/Down/Left/Right` (KeyDown): Directional focus navigation.
    /// - `Space`, `Enter`, `NumpadEnter` (on key-up or repeat): Generates an
    ///   [`InteractionEvent::Click`] on the currently focused element.
    ///   See [`Event::is_keyboard_trigger`].
    ///
    /// # Example
    /// ```rust
    /// # use floem::event::{Event, EventCx};
    /// # use ui_events::keyboard::{Key, KeyboardEvent, KeyState};
    /// fn handle(event: Event, cx: &mut EventCx) {
    ///     match event {
    ///         Event::Key(KeyboardEvent { key: Key::Character(c), state: KeyState::Down, .. }) => {
    ///             if c == "s" {
    ///                 // Handle a "save" shortcut.
    ///                 cx.prevent_default();
    ///             }
    ///         }
    ///         _ => {}
    ///     }
    /// }
    /// ```
    Key(ui_events::keyboard::KeyboardEvent),

    /// File drag and drop events.
    ///
    /// # Routing
    /// - **Spatial routing**: Hit-tested at the drag location
    /// - **Phases**: Target phase only (no capture/bubble)
    ///
    /// # Hover State
    /// FileDrag events maintain separate hover state from pointer events. When a file
    /// drag enters the window, pointer hover state is cleared and file drag hover state
    /// takes over. On drop or leave, it reverts to pointer hover state.
    ///
    /// # Events
    /// - `Enter`: Files dragged over this view (hover enter)
    /// - `Over`: Files moved while over this view
    /// - `Leave`: Files dragged away from this view (hover leave)
    /// - `Drop`: Files dropped on this view
    ///
    /// # Default Actions
    /// No preventable default action for any `FileDrag` variant.
    ///
    /// # Example
    /// ```rust
    /// # use floem::event::{Event, FileDragEvent};
    /// fn handle(event: Event) {
    ///     if let Event::FileDrag(FileDragEvent::Drop(drop)) = event {
    ///         for path in drop.paths.iter() {
    ///             println!("Dropped file: {:?}", path);
    ///         }
    ///     }
    /// }
    /// ```
    FileDrag(dropped_file::FileDragEvent),

    /// Pointer capture state changes.
    ///
    /// # Routing
    /// - **Directed to capture target**: Sent only to the view gaining/losing capture
    /// - **Phases**: Target phase only
    ///
    /// # Capture Lifecycle
    /// 1. View calls `cx.request_pointer_capture(pointer_id)` (typically in PointerDown)
    /// 2. After current event completes, `Gained` is sent to the target
    /// 3. All pointer events for that pointer_id are now routed to this view
    /// 4. Capture is released on PointerUp, PointerCancel, or explicit release
    /// 5. `Lost` is sent to the view that had capture
    ///
    /// # Use Cases
    /// - Implementing draggable elements
    /// - Tracking gestures that extend beyond view boundaries
    /// - Ensuring pointer up events are received even if pointer moves off element
    ///
    /// # Default Actions
    /// No preventable default action for any `PointerCapture` variant.
    ///
    /// # Example
    /// ```rust
    /// # use floem::event::{DragConfig, Event, EventCx, PointerCaptureEvent};
    /// fn handle(event: Event, cx: &mut EventCx) {
    ///     if let Event::PointerCapture(PointerCaptureEvent::Gained(drag_token)) = event {
    ///         // Now we have capture, start tracking the drag
    ///         cx.start_drag(drag_token, DragConfig::default(), true);
    ///     }
    /// }
    /// ```
    PointerCapture(PointerCaptureEvent),

    /// Input Method Editor (IME) events for composing text in languages like Chinese, Japanese, Korean.
    ///
    /// # Routing
    /// - **Directed to focused view**: Sent to the currently focused text input view
    /// - **Phases**: Capture, Target, Bubble
    ///
    /// # IME Composition
    /// IME allows users to compose complex characters through multiple keystrokes.
    /// This is used for:
    /// - Complex language input (Chinese, Japanese, Korean, etc.)
    /// - Emoji pickers on some platforms
    /// - Dead key combinations (accented characters)
    ///
    /// Composition lifecycle:
    /// 1. `Enabled`: IME composition started
    /// 2. `Preedit`: Composition text updated (user is still typing)
    /// 3. `Commit`: Final text committed (composition complete)
    /// 4. `Disabled`: IME composition ended
    ///
    /// # Default Actions
    /// No preventable default action for any `Ime` variant.
    ///
    /// # Example
    /// ```rust
    /// # use floem::event::{Event, ImeEvent};
    /// # fn insert_text(_text: &str) {}
    /// # fn show_preedit(_text: &str, _cursor: Option<(usize, usize)>) {}
    /// fn handle(event: Event) {
    ///     match event {
    ///         Event::Ime(ImeEvent::Commit(text)) => {
    ///             insert_text(&text);
    ///         }
    ///         Event::Ime(ImeEvent::Preedit { text, cursor }) => {
    ///             show_preedit(&text, cursor);
    ///         }
    ///         _ => {}
    ///     }
    /// }
    /// ```
    Ime(ImeEvent),

    /// Focus-related events fired when keyboard focus changes.
    ///
    /// # Routing
    /// - **Directed through focus path**: Sent to views in the focus ancestry chain
    /// - **Phases**: Capture, Target, Bubble
    ///
    /// # Focus Model
    /// Focus follows an ancestry chain from the focused view up to the root.
    /// When focus changes, the old and new paths are compared:
    /// - Views that were in the old path but not the new path receive `FocusLost`
    /// - Views that are in the new path but not the old path receive `FocusGained`
    ///
    /// # Focus Methods
    /// Focus can be changed through:
    /// - Pointer down (spatial focus)
    /// - Tab/Shift+Tab (sequential navigation)
    /// - Alt+Arrow (directional navigation)
    /// - Programmatic: `view_id.request_focus()`
    ///
    /// # Keyboard Navigation
    /// Only views with `keyboard_navigable()` set can receive focus via keyboard.
    /// The `:focus` and `:focus-visible` style selectors update when focus changes.
    ///
    /// # Default Actions
    /// No preventable default action. Style re-resolution (`:focus`, `:focus-visible`) is
    /// triggered before these events are dispatched and cannot be suppressed.
    ///
    /// # Example
    /// ```rust
    /// # use floem::event::{Event, FocusEvent};
    /// fn handle(event: Event) {
    ///     let mut cursor_visible = false;
    ///     match event {
    ///         Event::Focus(FocusEvent::Gained) => {
    ///             cursor_visible = true;
    ///         }
    ///         Event::Focus(FocusEvent::Lost) => {
    ///             cursor_visible = false;
    ///         }
    ///         _ => {}
    ///     }
    ///     let _ = cursor_visible;
    /// }
    /// ```
    Focus(FocusEvent),

    /// Window-level events like resize, close, theme changes, update phases.
    ///
    /// # Routing
    /// - **Broadcast to registered listeners**: Only views that have registered window event
    ///   listeners receive these events
    /// - **Phases**: Target phase only
    ///
    /// # Registration
    /// Views register interest in window events through the event listener system.
    /// This avoids broadcasting to all views for events most don't care about.
    ///
    /// # Events
    /// - `Resized`: Window size changed (triggers responsive style updates)
    /// - `CloseRequested`: User requested window close
    /// - `Destroyed`: Window is being destroyed
    /// - `ThemeChanged`: System theme changed (light/dark mode)
    /// - `RescaleRequested`: DPI scale factor changed
    ///
    /// # Default Actions
    /// No preventable default action for any `Window` variant.
    ///
    /// # Example
    /// ```rust
    /// # use floem::event::{Event, EventPropagation, WindowEvent};
    /// fn on_window_event(event: &Event) -> EventPropagation {
    ///     if let Event::Window(WindowEvent::Resized(size)) = event {
    ///         println!("Window resized to {}x{}", size.width, size.height);
    ///     }
    ///     EventPropagation::Continue
    /// }
    /// ```
    Window(WindowEvent),

    /// High-level interaction events that abstract over pointer and keyboard input.
    ///
    /// # Routing
    /// - **Directed to interaction target**: Sent to the view that was clicked/interacted with
    /// - **Phases**: Capture, Target, Bubble
    ///
    /// # Event Generation
    /// These events are synthetic - generated by Floem after analyzing lower-level
    /// pointer and keyboard events:
    ///
    /// - `Click`: Generated when pointer down+up occur on the same view within threshold,
    ///   OR when Space/Enter pressed on focused view
    /// - `DoubleClick`: Generated when two clicks occur rapidly (count > 1)
    /// - `SecondaryClick`: Generated from right-click (secondary button)
    ///
    /// # Click Detection
    /// A click is detected when:
    /// 1. Pointer down occurs on a view
    /// 2. Pointer doesn't move beyond threshold distance
    /// 3. Pointer up occurs within timeout
    /// 4. Common ancestor between down and up targets receives the click
    ///
    /// # Triggered By
    /// Interaction events have `cx.triggered_by` set to the original pointer/keyboard
    /// event that caused them. Use this to access original event details like modifiers.
    ///
    /// # Default Actions
    /// No preventable default action. These events are themselves generated as default
    /// actions of lower-level pointer and keyboard events.
    ///
    /// # Example
    /// ```rust
    /// # use floem::event::{Event, InteractionEvent};
    /// let event = Event::Interaction(InteractionEvent::Click);
    /// let triggered_by: Option<Event> = None;
    /// if matches!(event, Event::Interaction(InteractionEvent::Click)) {
    ///     // Handle click regardless of whether it came from mouse or keyboard.
    ///     if let Some(Event::Pointer(_)) = triggered_by {
    ///         // Access pointer-specific details
    ///     }
    /// }
    /// ```
    Interaction(InteractionEvent),

    /// Drag and drop events for implementing draggable elements and drop targets.
    ///
    /// # Routing
    /// - **Source events**: Directed to the view being dragged (Target phase only)
    /// - **Target events**: Spatial routing via hit-testing (Target phase only, except Move which uses STANDARD phases)
    ///
    /// # Drag Lifecycle
    ///
    /// ## Starting a Drag
    /// Call `draggable` or `.draggable_with_config()` on a view to make it draggable:
    /// ```rust,ignore
    /// my_view.draggable_with_config(|| {
    ///     DragConfig::default()
    ///         .with_custom_data(my_item_id)
    /// })
    /// ```
    ///
    /// ## During Drag
    /// As the dragged element moves:
    /// - **Source receives**: `Move` events while dragging
    /// - **Targets receive**: `Enter` when drag enters their bounds, `Move` while hovering, `Leave` when drag exits
    /// - **Source receives**: `Enter`/`Leave` when entering/leaving valid drop targets
    ///
    /// ## Ending Drag
    /// The drag ends with one of:
    /// - **Released over a target**: Target receives `Drop` and source receives `End` with
    ///   `other_element: Some(target_id)` simultaneously.
    /// - **Released without a target**: Source receives `End` with `other_element: None`.
    /// - **System cancel**: Source receives `Cancel` when the drag is aborted by a
    ///   `PointerCancel` event (e.g., touch interrupted).
    ///
    /// # Example: Sortable List
    ///
    /// ```rust,ignore
    /// // Make a view draggable with custom data
    /// my_view
    ///     .on_event_stop(listener::DragTargetEnter, move |_, drag_enter| {
    ///         if let Some(custom_data) = &drag_enter.custom_data
    ///             && let Some(dragged_id) = custom_data.downcast_ref::<usize>()
    ///         {
    ///             // Reorder items based on drag position
    ///             handle_reorder(*dragged_id, this_item_id);
    ///         }
    ///     })
    ///     .draggable_with_config(move || {
    ///         DragConfig::default()
    ///             .with_custom_data(item_id)
    ///     })
    /// ```
    ///
    /// # Default Actions
    /// No preventable default action for any `Drag` variant.
    Drag(DragEvent),

    /// Custom user-defined events.
    ///
    /// # Routing
    /// - **User-controlled**: Routing determined by how the event is dispatched
    /// - **Phases**: Specified when dispatching the event
    ///
    /// # Defining Custom Events
    ///
    /// Use the `custom_event!` macro to define your event type:
    ///
    /// ```rust,ignore
    /// #[derive(Clone)]
    /// struct DataChanged {
    ///     new_value: String,
    /// }
    /// custom_event!(DataChanged);
    /// ```
    ///
    /// This generates a `DataChangedListener` and implements the `CustomEvent` trait.
    ///
    /// # Dispatching Custom Events
    ///
    /// ```rust,ignore
    /// // Dispatch to a specific target
    /// view_id.dispatch_event(
    ///     Event::new_custom(DataChanged {
    ///         new_value: "updated".to_string()
    ///     }),
    ///     RouteKind::Directed {
    ///         target: view_id.get_element_id(),
    ///         phases: Phases::TARGET,
    ///     },
    /// );
    ///
    /// // Or dispatch spatially (hit-test based)
    /// view_id.dispatch_event(
    ///     Event::new_custom(MyPointerEvent { pos: point }),
    ///     RouteKind::Spatial {
    ///         point: Some(point),
    ///         phases: Phases::STANDARD,
    ///     },
    /// );
    /// ```
    ///
    /// # Default Actions
    /// No preventable default action. Routing and any behaviors are fully determined by
    /// the application when dispatching the event.
    ///
    /// # Handling Custom Events
    ///
    /// ```rust,ignore
    /// view.on_event_stop(DataChangedListener, |cx, event: &DataChanged| {
    ///     println!("Data changed to: {}", event.new_value);
    /// })
    /// ```
    ///
    /// # Generic Custom Events
    ///
    /// For generic events, each monomorphization gets its own listener:
    ///
    /// ```rust,ignore
    /// #[derive(Clone)]
    /// struct SelectionChanged<T: 'static> {
    ///     value: T,
    /// }
    /// custom_event!(SelectionChanged<T>);
    ///
    /// // String and i32 versions are separate event types
    /// dropdown.on_event_stop(SelectionChangedListener::<String>, |cx, event| {
    ///     // Receives SelectionChanged<String>
    /// });
    /// ```
    Custom(Box<dyn CustomEvent>),

    /// Sentinel value used internally when temporarily moving an event out of `EventCx`.
    ///
    /// # Internal Use Only
    ///
    /// This variant allows event handlers to receive both `&mut EventCx` and typed event
    /// data without borrow conflicts and without cloning. The event is temporarily replaced with `Extracted`
    /// while being passed to handlers.
    ///
    /// **If you see this variant in your event handler**, you're likely accessing `cx.event`
    /// directly. Use the typed event data parameter passed to your handler instead:
    ///
    /// ```rust
    /// # enum Event { Pointer, Extracted }
    /// # struct Cx { event: Event }
    /// // ❌ Don't do this:
    /// let bad = |cx: &Cx| {
    ///     matches!(cx.event, Event::Pointer) // may observe temporary extraction state
    /// };
    ///
    /// // ✅ Do this instead:
    /// let good = |event: &Event| {
    ///     matches!(event, Event::Pointer)
    /// };
    /// # let _ = (bad, good);
    /// ```
    Extracted,
}
impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::Pointer(e) => f.debug_tuple("Pointer").field(e).finish(),
            Event::Key(e) => f.debug_tuple("Key").field(e).finish(),
            Event::FileDrag(e) => f.debug_tuple("FileDrag").field(e).finish(),
            Event::PointerCapture(e) => f.debug_tuple("PointerCapture").field(e).finish(),
            Event::Ime(e) => f.debug_tuple("Ime").field(e).finish(),
            Event::Focus(e) => f.debug_tuple("Focus").field(e).finish(),
            Event::Window(e) => f.debug_tuple("Window").field(e).finish(),
            Event::Interaction(e) => f.debug_tuple("Interaction").field(e).finish(),
            Event::Drag(e) => f.debug_tuple("Drag").field(e).finish(),
            Event::Custom(e) => {
                f.write_str("Custom(")?;
                e.debug_fmt(f)?;
                f.write_str(")")
            }
            Event::Extracted => f.write_str("Extracted"),
        }
    }
}
impl Clone for Event {
    fn clone(&self) -> Self {
        match self {
            Self::Pointer(arg0) => Self::Pointer(arg0.clone()),
            Self::Key(arg0) => Self::Key(arg0.clone()),
            Self::FileDrag(arg0) => Self::FileDrag(arg0.clone()),
            Self::PointerCapture(arg0) => Self::PointerCapture(*arg0),
            Self::Ime(arg0) => Self::Ime(arg0.clone()),
            Self::Focus(arg0) => Self::Focus(*arg0),
            Self::Window(arg0) => Self::Window(*arg0),
            Self::Interaction(arg0) => Self::Interaction(*arg0),
            Self::Drag(arg0) => Self::Drag(arg0.clone()),
            Self::Custom(arg0) => Self::Custom(arg0.clone_box()),
            Self::Extracted => Self::Extracted,
        }
    }
}

impl Event {
    pub fn is_pointer(&self) -> bool {
        matches!(self, Event::Pointer(_))
    }

    pub fn is_pointer_down(&self) -> bool {
        matches!(self, Event::Pointer(PointerEvent::Down { .. }))
    }

    pub fn is_pointer_up(&self) -> bool {
        matches!(self, Event::Pointer(PointerEvent::Up { .. }))
    }
    pub fn is_key_up(&self) -> bool {
        matches!(
            self,
            Event::Key(KeyboardEvent {
                state: KeyState::Up,
                ..
            })
        )
    }
    pub fn is_key_down(&self) -> bool {
        matches!(
            self,
            Event::Key(KeyboardEvent {
                state: KeyState::Down,
                ..
            })
        )
    }

    /// Enter, numpad enter and space cause a view to be activated with the keyboard
    pub fn is_keyboard_trigger(&self) -> bool {
        match self {
            Event::Key(key) => {
                matches!(key.code, Code::NumpadEnter | Code::Enter | Code::Space)
                    && (key.state == KeyState::Up || (key.state == KeyState::Down && key.repeat))
            }
            _ => false,
        }
    }

    /// Enter, numpad enter and space cause a view to be activated with the keyboard
    pub fn is_keyboard_trigger_start(&self) -> bool {
        match self {
            Event::Key(key) => {
                matches!(key.code, Code::NumpadEnter | Code::Enter | Code::Space)
                    && (key.state == KeyState::Down || key.repeat)
            }
            _ => false,
        }
    }

    /// Returns whether this event should be delivered to disabled views.
    ///
    /// Disabled views (marked via `.disabled()`) generally don't receive interactive events,
    /// but some events must still be delivered to maintain correct per-view internal state
    /// or complete ongoing operations.
    ///
    /// Note: This only affects whether individual views receive events in their event handlers.
    /// Global state tracking (hover paths, focus paths, etc.) is updated independently of
    /// whether views are disabled.
    ///
    /// # Events Allowed on Disabled Views
    ///
    /// - **Hover state tracking**: `PointerLeave` allow disabled views to
    ///   update their internal hover state
    /// - **Capture cleanup**: `PointerCapture::Lost` allows views to clean up internal state
    ///   when they lose capture (e.g., if disabled mid-drag)
    /// - **Drag lifecycle completion**: `DragSource` Leave/End/Cancel events allow drags
    ///   to complete properly if a view becomes disabled mid-drag. The drag was initiated when
    ///   enabled, so the source view needs lifecycle events to clean up its internal drag state.
    ///   Start and Move events are blocked since disabled views don't need to initiate or track drags.
    /// - **Window events**: Window state changes (resize, theme change, etc.) may require
    ///   internal updates even in disabled views
    /// - **Accessibility drops**: `FileDrag::Drop` can be received by disabled views for
    ///   accessibility reasons
    ///
    /// # Events Blocked on Disabled Views
    ///
    /// - **Interactive pointer events**: Down, Up, Move, Scroll, Gesture, Cancel
    /// - **Capture initiation**: `PointerCapture::Gained` (views shouldn't gain new capture when disabled)
    /// - **Focus changes**: Focus events don't fire for disabled views
    /// - **Keyboard input**: Key and IME events
    /// - **User interactions**: Click, DoubleClick, SecondaryClick
    /// - **Drag targets**: Disabled views cannot receive dragged elements
    /// - **Drag source updates**: Start and Move events (disabled views don't initiate or track drags)
    /// - **File drag preview**: File drag hover events (Enter, Move, Leave)
    ///
    /// # Custom Events
    ///
    /// Custom events implement their own `allow_disabled()` logic via the `CustomEvent` trait.
    pub fn allow_disabled(&self) -> bool {
        match self {
            // Hover state tracking - views need leave to update internal hover state if they had an enter
            Event::Pointer(PointerEvent::Leave(_)) => true,

            // Capture cleanup - views need to clean up internal capture state
            Event::PointerCapture(PointerCaptureEvent::Lost(_)) => true,

            // Drag source lifecycle completion - allow cleanup events only
            Event::Drag(DragEvent::Source(
                DragSourceEvent::Leave(_) | DragSourceEvent::End(_) | DragSourceEvent::Cancel(_),
            )) => true,

            // Window-level events may require internal updates
            Event::Window(_) => true,

            // Accessibility - allow file drops on disabled views
            Event::FileDrag(FileDragEvent::Drop(_)) => true,

            // Block all other pointer events
            Event::Pointer(_) => false,

            // Block new capture
            Event::PointerCapture(_) => false,

            // Block focus changes
            Event::Focus(_) => false,

            // Block keyboard input
            Event::Key(_) | Event::Ime(_) => false,

            // Block user interactions
            Event::Interaction(_) => false,

            // Block all drag target events and drag source start/move
            Event::Drag(_) => false,

            // Block file drag preview events
            Event::FileDrag(_) => false,

            // Custom events decide their own disabled behavior
            Event::Custom(custom) => custom.allow_disabled(),

            Event::Extracted => {
                // this probably shouldn't happen
                false
            }
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
            Event::FileDrag(file_drag_event) => Some(file_drag_event.logical_point()),
            _ => None,
        }
    }

    /// Transform this event from one coordinate space to another.
    ///
    /// This method applies an affine transformation to all position-related data in the event,
    /// including pointer positions, drag positions, and file drag positions.
    ///
    /// # Parameters
    ///
    /// * `transform` - An affine transform that maps from the **source coordinate space to the
    ///   target coordinate space**. This transform is applied directly to event positions without
    ///   inversion.
    ///
    /// # Coordinate Space Mapping
    ///
    /// If you want to transform an event from world/window coordinates to a view's local
    /// coordinate space, you must pass the **inverse** of the view's world transform:
    ///
    /// ```ignore
    /// let world_transform = box_tree.get_or_compute_world_transform(node_id)?; // local → world
    /// let local_transform = world_transform.inverse();           // world → local
    /// let local_event = event.transform(local_transform);
    /// ```
    ///
    /// Common use cases:
    /// - **World to local**: Pass `world_transform.inverse()` to convert window coordinates to view-local coordinates
    /// - **Local to world**: Pass `world_transform` to convert view-local coordinates to window coordinates
    /// - **Between views**: Compose transforms as needed: `target_world.inverse() * source_world`
    ///
    /// # Event Types Transformed
    ///
    /// This method transforms position data for:
    /// - `PointerEvent` variants (Down, Up, Move, Scroll, Gesture)
    /// - `FileDragEvent` variants (Enter, Move, Leave, Dropped)
    /// - `DragTarget` and `DragSource` events (both current and start positions)
    /// - Custom events (via their `transform` method)
    ///
    /// Other event types (Key, Focus, Window, etc.) are returned unchanged.
    pub fn transform(mut self, transform: Affine) -> Event {
        match &mut self {
            Self::Pointer(
                PointerEvent::Down(PointerButtonEvent { state, .. })
                | PointerEvent::Up(PointerButtonEvent { state, .. })
                | PointerEvent::Gesture(PointerGestureEvent { state, .. })
                | PointerEvent::Move(PointerUpdate { current: state, .. })
                | PointerEvent::Scroll(PointerScrollEvent { state, .. }),
            ) => {
                let point = state.logical_point();
                let transformed_point = transform * point;
                let phys_pos = LogicalPosition::new(transformed_point.x, transformed_point.y)
                    .to_physical(state.scale_factor);
                state.position = phys_pos;
            }
            Self::FileDrag(
                FileDragEvent::Enter(dropped_file::FileDragEnter { position, .. })
                | FileDragEvent::Move(dropped_file::FileDragMove { position, .. })
                | FileDragEvent::Leave(dropped_file::FileDragLeave { position })
                | FileDragEvent::Drop(dropped_file::FileDragDropped { position, .. }),
            ) => {
                let transformed_point = transform * *position;
                *position = transformed_point;
            }
            Self::Drag(de) => {
                match de {
                    DragEvent::Target(dte) => {
                        // Transform current state
                        let state = dte.current_state_mut();
                        let point = state.logical_point();
                        let transformed_point = transform * point;
                        let phys_pos =
                            LogicalPosition::new(transformed_point.x, transformed_point.y)
                                .to_physical(state.scale_factor);
                        state.position = phys_pos;
                        // Transform start state
                        let start_state = dte.start_state_mut();
                        let start_point = start_state.logical_point();
                        let transformed_start_point = transform * start_point;
                        let phys_start_pos = LogicalPosition::new(
                            transformed_start_point.x,
                            transformed_start_point.y,
                        )
                        .to_physical(start_state.scale_factor);
                        start_state.position = phys_start_pos;
                    }
                    DragEvent::Source(dse) => {
                        // Transform current state
                        let state = dse.current_state_mut();
                        let point = state.logical_point();
                        let transformed_point = transform * point;
                        let phys_pos =
                            LogicalPosition::new(transformed_point.x, transformed_point.y)
                                .to_physical(state.scale_factor);
                        state.position = phys_pos;
                        // Transform start state
                        let start_state = dse.start_state_mut();
                        let start_point = start_state.logical_point();
                        let transformed_start_point = transform * start_point;
                        let phys_start_pos = LogicalPosition::new(
                            transformed_start_point.x,
                            transformed_start_point.y,
                        )
                        .to_physical(start_state.scale_factor);
                        start_state.position = phys_start_pos;
                    }
                }
            }

            Self::Pointer(
                PointerEvent::Cancel(_) | PointerEvent::Leave(_) | PointerEvent::Enter(_),
            )
            | Self::Key(_)
            | Self::PointerCapture(_)
            | Self::Focus(_)
            | Self::Ime(_)
            | Self::Window(_)
            | Self::Interaction(_)
            | Self::Extracted => {}
            Self::Custom(custom) => {
                custom.transform(transform);
            }
        }
        self
    }

    /// Returns all listener keys that this event should trigger.
    ///
    /// Each event returns its specific listener key (e.g., `PointerDown`) plus any
    /// broad category keys it belongs to (e.g., `AnyPointer`). This allows views
    /// to listen for either specific events or entire event categories.
    pub fn listener_keys(&self) -> SmallVec<[listener::EventListenerKey; 4]> {
        use listener::*;
        let mut keys = SmallVec::new();

        // Add the specific listener key
        let specific = match self {
            Self::Pointer(PointerEvent::Down { .. }) => PointerDown::listener_key(),
            Self::Pointer(PointerEvent::Up { .. }) => PointerUp::listener_key(),
            Self::Pointer(PointerEvent::Move(_)) => PointerMove::listener_key(),
            Self::Pointer(PointerEvent::Scroll { .. }) => PointerWheel::listener_key(),
            Self::Pointer(PointerEvent::Leave(_)) => PointerLeave::listener_key(),
            Self::Pointer(PointerEvent::Enter(_)) => PointerEnter::listener_key(),
            Self::Pointer(PointerEvent::Cancel(_)) => PointerCancel::listener_key(),
            Self::Pointer(PointerEvent::Gesture(_)) => PinchGesture::listener_key(),
            Self::PointerCapture(PointerCaptureEvent::Gained(_)) => {
                GainedPointerCapture::listener_key()
            }
            Self::PointerCapture(PointerCaptureEvent::Lost(_)) => {
                LostPointerCapture::listener_key()
            }
            Self::Key(KeyboardEvent {
                state: KeyState::Down,
                ..
            }) => KeyDown::listener_key(),
            Self::Key(KeyboardEvent {
                state: KeyState::Up,
                ..
            }) => KeyUp::listener_key(),
            Self::Ime(ImeEvent::Enabled) => ImeEnabled::listener_key(),
            Self::Ime(ImeEvent::Disabled) => ImeDisabled::listener_key(),
            Self::Ime(ImeEvent::Preedit { .. }) => ImePreedit::listener_key(),
            Self::Ime(ImeEvent::Commit(_)) => ImeCommit::listener_key(),
            Self::Ime(ImeEvent::DeleteSurrounding { .. }) => ImeDeleteSurrounding::listener_key(),
            Self::Focus(FocusEvent::Gained) => FocusGained::listener_key(),
            Self::Focus(FocusEvent::Lost) => FocusLost::listener_key(),
            Self::Window(WindowEvent::Closed) => WindowClosed::listener_key(),
            Self::Window(WindowEvent::Resized(_)) => WindowResized::listener_key(),
            Self::Window(WindowEvent::Moved(_)) => WindowMoved::listener_key(),
            Self::Window(WindowEvent::MaximizeChanged(_)) => WindowMaximizeChanged::listener_key(),
            Self::Window(WindowEvent::ScaleChanged(_)) => WindowScaleChanged::listener_key(),
            Self::Window(WindowEvent::FocusGained) => WindowGainedFocus::listener_key(),
            Self::Window(WindowEvent::FocusLost) => WindowLostFocus::listener_key(),
            Self::Window(WindowEvent::ThemeChanged(_)) => ThemeChanged::listener_key(),
            Self::Window(WindowEvent::ChangeUnderCursor) => WindowChangeUnderCursor::listener_key(),
            Self::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::ProcessingMessages)) => {
                UpdatePhaseProcessingMessages::listener_key()
            }
            Self::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Style)) => {
                UpdatePhaseStyle::listener_key()
            }
            Self::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Layout)) => {
                UpdatePhaseLayout::listener_key()
            }
            Self::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::BoxTreeUpdate)) => {
                UpdatePhaseBoxTreeUpdate::listener_key()
            }
            Self::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::BoxTreePendingUpdates)) => {
                UpdatePhaseBoxTreePendingUpdates::listener_key()
            }
            Self::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::BoxTreeCommit)) => {
                UpdatePhaseBoxTreeCommit::listener_key()
            }
            Self::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Complete)) => {
                UpdatePhaseComplete::listener_key()
            }
            Self::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::PaintPresent)) => {
                UpdatePhasePaintPresent::listener_key()
            }
            Self::Interaction(InteractionEvent::Click) => Click::listener_key(),
            Self::Interaction(InteractionEvent::DoubleClick) => DoubleClick::listener_key(),
            Self::Interaction(InteractionEvent::SecondaryClick) => SecondaryClick::listener_key(),
            Self::FileDrag(FileDragEvent::Drop(_)) => FileDragDrop::listener_key(),
            Self::FileDrag(FileDragEvent::Enter(_)) => FileDragEnter::listener_key(),
            Self::FileDrag(FileDragEvent::Move(_)) => FileDragMove::listener_key(),
            Self::FileDrag(FileDragEvent::Leave(_)) => FileDragLeave::listener_key(),
            Self::Drag(DragEvent::Source(DragSourceEvent::Start(..))) => DragStart::listener_key(),
            Self::Drag(DragEvent::Source(DragSourceEvent::Move(..))) => DragMove::listener_key(),
            Self::Drag(DragEvent::Source(DragSourceEvent::Enter(..))) => {
                DragSourceEnter::listener_key()
            }
            Self::Drag(DragEvent::Source(DragSourceEvent::Leave(..))) => {
                DragSourceLeave::listener_key()
            }
            Self::Drag(DragEvent::Source(DragSourceEvent::End(..))) => DragEnd::listener_key(),
            Self::Drag(DragEvent::Source(DragSourceEvent::Cancel(..))) => {
                DragCancel::listener_key()
            }
            Self::Drag(DragEvent::Target(DragTargetEvent::Enter(..))) => {
                DragTargetEnter::listener_key()
            }
            Self::Drag(DragEvent::Target(DragTargetEvent::Move(..))) => {
                DragTargetMove::listener_key()
            }
            Self::Drag(DragEvent::Target(DragTargetEvent::Leave(..))) => {
                DragTargetLeave::listener_key()
            }
            Self::Drag(DragEvent::Target(DragTargetEvent::Drop(..))) => {
                DragTargetDrop::listener_key()
            }
            Self::Extracted => Extracted::listener_key(),
            Self::Custom(custom) => custom.listener_key_dyn(),
        };
        keys.push(specific);

        // Add broad category listeners
        match self {
            Self::Pointer(_) => keys.push(AnyPointer::listener_key()),
            Self::Key(_) => keys.push(AnyKey::listener_key()),
            Self::Window(w) => {
                keys.push(AnyWindow::listener_key());
                if matches!(w, WindowEvent::UpdatePhase(_)) {
                    keys.push(AnyUpdatePhase::listener_key());
                }
            }
            Self::Focus(_) => keys.push(AnyFocus::listener_key()),
            Self::Ime(_) => keys.push(AnyIme::listener_key()),
            Self::Drag(de) => {
                keys.push(AnyDragSource::listener_key());
                match de {
                    DragEvent::Source(_) => keys.push(AnyDragSource::listener_key()),
                    DragEvent::Target(_) => keys.push(AnyDragTarget::listener_key()),
                }
            }
            Self::FileDrag(_) => keys.push(AnyFileDrag::listener_key()),
            _ => {}
        }

        keys
    }

    pub fn new_custom(custom: impl CustomEvent) -> Self {
        Self::Custom(Box::new(custom))
    }

    /// Returns `true` if the event is [`FileDrag`].
    ///
    /// [`FileDrag`]: Event::FileDrag
    #[must_use]
    pub fn is_file_drag(&self) -> bool {
        matches!(self, Self::FileDrag(..))
    }
}

/// A custom event requesting that ancestor scroll containers adjust their
/// viewport to reveal a target element or a specific region within it.
///
/// This event is intended to propagate upward through the element hierarchy
/// so that scrollable ancestors can respond. It does not represent a broadcast,
/// spatial, focused, or subtree operation.
///
/// # Routing Requirements
///
/// This event must be routed using `RouteKind::bubble_from`.
///
/// No other routing strategy is intended to be valid:
///
/// - Capture phases are not applicable.
/// - Target phase invocation is not required.
/// - Subtree and broadcast routing are semantically incorrect.
/// - Spatial and focused routing do not apply.
///
/// The expected routing pattern is:
///
/// ```rust,ignore
/// GlobalEventCx::new(...)
///     .route_normal(RouteKind::bubble_from(id), None);
/// ```
///
/// Routing this event with any other `RouteKind` results in undefined
/// framework-level semantics.
#[derive(Debug, Clone, Copy)]
pub struct ScrollTo {
    /// The element requesting to be scrolled into view.
    pub id: ElementId,

    /// The region within the element that should be made visible.
    ///
    /// If `None`, the entire element is considered the scroll target.
    pub rect: Option<peniko::kurbo::Rect>,
}

custom_event!(ScrollTo);

pub trait PointerScrollEventExt {
    /// Resolve scroll delta to points/pixels.
    ///
    /// Converts the scroll delta to a pixel-based Vec2, using the provided line and page sizes
    /// when available. Falls back to reasonable defaults if not provided.
    ///
    /// # Arguments
    /// * `line_size` - Optional size to use for line-based scrolling (e.g., mouse wheel clicks)
    /// * `page_size` - Optional size to use for page-based scrolling (e.g., page up/down)
    fn resolve_to_points(&self, line_size: Option<Size>, page_size: Option<Size>) -> Vec2;
}

impl PointerScrollEventExt for PointerScrollEvent {
    fn resolve_to_points(&self, line_size: Option<Size>, page_size: Option<Size>) -> Vec2 {
        match &self.delta {
            ScrollDelta::PixelDelta(delta) => {
                let log = delta.to_logical(self.state.scale_factor);
                Vec2 { x: log.x, y: log.y }
            }
            ScrollDelta::LineDelta(x, y) => {
                // Convert line delta to pixel delta
                // Use provided line size or fall back to reasonable default
                let line_height = line_size.map(|s| s.height).unwrap_or(20.0);
                let line_width = line_size.map(|s| s.width).unwrap_or(20.0);
                Vec2 {
                    x: (*x as f64) * line_width,
                    y: (*y as f64) * line_height,
                }
            }
            ScrollDelta::PageDelta(x, y) => {
                // Page deltas are synthetic (e.g., clicking scrollbar well)
                // Use provided page size or fall back to larger multiplier
                let page_height = page_size.map(|s| s.height).unwrap_or(200.0);
                let page_width = page_size.map(|s| s.width).unwrap_or(200.0);
                Vec2 {
                    x: (*x as f64) * page_width,
                    y: (*y as f64) * page_height,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    pub struct GenericKeyCheck<T: Clone + 'static> {
        _value: T,
    }

    custom_event!(GenericKeyCheck<T>);

    #[test]
    fn generic_custom_event_listener_keys_are_distinct() {
        let string_key = GenericKeyCheck::<String>::listener_key();
        let int_key = GenericKeyCheck::<i32>::listener_key();
        assert_ne!(string_key, int_key);
    }

    #[test]
    fn generic_custom_event_same_type_has_stable_key() {
        let first = GenericKeyCheck::<String>::listener_key();
        let second = GenericKeyCheck::<String>::listener_key();
        assert_eq!(first, second);
    }
}
