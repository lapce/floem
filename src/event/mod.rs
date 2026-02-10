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
/// self.id.dispatch_event(
///     Event::new_custom(DropdownAccept { value: *val }),
///     DispatchKind::Directed {
///         target: self.id.get_element_id(),
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
/// - This macro depends on the [`paste`](https://docs.rs/paste) crate.
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
                    trait HasInfo {
                        fn get() -> &'static $crate::event::listener::EventKeyInfo;
                    }

                    struct InfoHolder<$($generic),*>(std::marker::PhantomData<$(fn() -> $generic),*>);

                    impl<$($generic: Clone + 'static),*> HasInfo for InfoHolder<$($generic),*> {
                        fn get() -> &'static $crate::event::listener::EventKeyInfo {
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

                    <InfoHolder<$($generic),*> as HasInfo>::get()
                }
            }

            impl<$($generic: Clone + 'static),*> $crate::event::listener::EventListenerTrait for [<$name Listener>]<$($generic),*> {
                type EventData = $name<$($generic),*>;

                fn listener_key() -> $crate::event::listener::EventListenerKey {
                    $crate::event::listener::EventListenerKey { info: Self::info() }
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
                            .and_then($extract)
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
            |data: &$name| -> Option<&$name> { Some(data) }
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

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum FocusEvent {
    Lost,
    Got,
    EnteredSubtree,
    LeftSubtree,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum PointerCaptureEvent {
    /// Fired when a view gains pointer capture.
    /// Contains the pointer ID that was captured.
    Got(DragToken),
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

/// High-level interaction events that abstract over pointer and keyboard input.
/// These events represent user intent (clicking, double-clicking, etc.)
/// regardless of the input method used.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum InteractionEvent {
    /// Single click (pointer or keyboard activated)
    Click,
    /// Double click
    DoubleClick,
    /// Right click / context menu trigger
    SecondaryClick,
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

    /// Drag was ended - sent when the pointer is released.
    /// `other_element` is the drop target if one accepted the drop, `None` otherwise.
    End(DragEndEvent),

    /// Drag was cancelled - sent when the drag is aborted
    /// (e.g., Escape key pressed).
    Cancel(DragCancelEvent),
}

/// Events sent to potential drop targets during a drag operation.
///
/// These events allow drop targets to:
/// - Know when a dragged element is hovering over them (Enter/Leave/Move)
/// - Accept or reject drops by calling `prevent_default()` on the Drop event
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

    /// A dragged element was dropped on this target - sent when the pointer is released
    /// while over this target. `other_element` is the element being dragged.
    ///
    /// Call `prevent_default()` to accept the drop. If no target accepts, the dragged
    /// element will receive a `Cancel` event instead.
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

// Update Event enum
pub enum Event {
    Pointer(PointerEvent),
    Key(ui_events::keyboard::KeyboardEvent),
    FileDrag(dropped_file::FileDragEvent),
    PointerCapture(PointerCaptureEvent),
    Ime(ImeEvent),
    Focus(FocusEvent),
    Window(WindowEvent),
    /// High-level interaction events that abstract over pointer and keyboard input.
    /// These events represent user intent (clicking, double-clicking, etc.)
    /// regardless of the input method used.
    /// These are emitted after the cooresponding events that cause them.
    Interaction(InteractionEvent),
    /// Drag target events - sent to potential drop targets when a dragged element
    /// interacts with them (enters, leaves, or is dropped on them).
    DragTarget(DragTargetEvent),
    /// Drag source events - sent to the element being dragged throughout the
    /// drag operation lifecycle (start, move, enter/leave targets, drop, cancel).
    DragSource(DragSourceEvent),
    Custom(Box<dyn CustomEvent>),
    /// Sentinel value used internally when temporarily moving an event out of EventCx.
    /// This allows event handlers to receive both &mut EventCx and typed event data without
    /// borrow conflicts. If you see this variant in your event handler, you're likely accessing
    /// cx.event directly - use the typed event data parameter passed to your handler instead.
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
            Event::DragTarget(e) => f.debug_tuple("DragTarget").field(e).finish(),
            Event::DragSource(e) => f.debug_tuple("DragSource").field(e).finish(),
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
            Self::DragTarget(arg0) => Self::DragTarget(arg0.clone()),
            Self::DragSource(arg0) => Self::DragSource(arg0.clone()),
            Self::Custom(arg0) => Self::Custom(arg0.clone_box()),
            Self::Extracted => Self::Extracted,
        }
    }
}

impl Event {
    pub fn needs_focus(&self) -> bool {
        matches!(self, Event::Key(_))
    }

    pub fn is_pointer(&self) -> bool {
        matches!(self, Event::Pointer(_))
    }

    pub fn is_pointer_down(&self) -> bool {
        matches!(self, Event::Pointer(PointerEvent::Down { .. }))
    }

    pub fn is_pointer_up(&self) -> bool {
        matches!(self, Event::Pointer(PointerEvent::Up { .. }))
    }

    /// Enter, numpad enter and space cause a view to be activated with the keyboard
    pub(crate) fn is_keyboard_trigger(&self) -> bool {
        match self {
            Event::Key(key) => {
                matches!(key.code, Code::NumpadEnter | Code::Enter | Code::Space)
                    && (key.state == KeyState::Up || (key.state == KeyState::Down && key.repeat))
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
            Event::FileDrag(FileDragEvent::Dropped(_)) => true,
            // Lost capture events should be delivered so views can clean up capture state
            Event::PointerCapture(PointerCaptureEvent::Lost(_)) => true,

            // Pointer down, up, enter, cancel, scroll, gesture should not trigger on disabled views
            Event::Pointer(_) => false,
            // Gained capture should only happen on active views
            Event::PointerCapture(PointerCaptureEvent::Got(_)) => false,
            // Focus events should not be delivered to disabled views
            Event::Focus(_) => false,
            // IME events should not be delivered to disabled views
            Event::Ime(_) => false,
            // File drag preview events should not be delivered to disabled views
            Event::FileDrag(
                FileDragEvent::Enter(_) | FileDragEvent::Move(_) | FileDragEvent::Leave(_),
            ) => false,
            // Keyboard events should not trigger on disabled views
            Event::Key(_) => false,
            // Interaction events (click, double click, etc.) should not trigger on disabled views
            Event::Interaction(_) => false,
            // Drag events should not be initiated or handled by disabled views
            Event::DragTarget(_) | Event::DragSource(_) => false,
            // allow custom events to decide
            Event::Custom(custom) => custom.allow_disabled(),
            Event::Extracted => {
                unreachable!("this event should never be dispatched")
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
    /// let world_transform = box_tree.world_transform(node_id)?; // local → world
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
                | FileDragEvent::Dropped(dropped_file::FileDragDropped { position, .. }),
            ) => {
                let transformed_point = transform * *position;
                *position = transformed_point;
            }
            Self::DragTarget(de) => {
                // Transform current state
                let state = de.current_state_mut();
                let point = state.logical_point();
                let transformed_point = transform * point;
                let phys_pos = LogicalPosition::new(transformed_point.x, transformed_point.y)
                    .to_physical(state.scale_factor);
                state.position = phys_pos;

                // Transform start state
                let start_state = de.start_state_mut();
                let start_point = start_state.logical_point();
                let transformed_start_point = transform * start_point;
                let phys_start_pos =
                    LogicalPosition::new(transformed_start_point.x, transformed_start_point.y)
                        .to_physical(start_state.scale_factor);
                start_state.position = phys_start_pos;
            }
            Self::DragSource(de) => {
                // Transform current state
                let state = de.current_state_mut();
                let point = state.logical_point();
                let transformed_point = transform * point;
                let phys_pos = LogicalPosition::new(transformed_point.x, transformed_point.y)
                    .to_physical(state.scale_factor);
                state.position = phys_pos;

                // Transform start state
                let start_state = de.start_state_mut();
                let start_point = start_state.logical_point();
                let transformed_start_point = transform * start_point;
                let phys_start_pos =
                    LogicalPosition::new(transformed_start_point.x, transformed_start_point.y)
                        .to_physical(start_state.scale_factor);
                start_state.position = phys_start_pos;
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
            // TODO: make dyn custom event impl clone then use transform method
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
    pub fn listener_keys(&self) -> SmallVec<[listener::EventListenerKey; 2]> {
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
            Self::PointerCapture(PointerCaptureEvent::Got(_)) => GotPointerCapture::listener_key(),
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
            Self::Focus(FocusEvent::Lost) => FocusLost::listener_key(),
            Self::Focus(FocusEvent::Got) => FocusGot::listener_key(),
            Self::Focus(FocusEvent::EnteredSubtree) => FocusEnteredSubtree::listener_key(),
            Self::Focus(FocusEvent::LeftSubtree) => FocusLeftSubtree::listener_key(),
            Self::Window(WindowEvent::Closed) => WindowClosed::listener_key(),
            Self::Window(WindowEvent::Resized(_)) => WindowResized::listener_key(),
            Self::Window(WindowEvent::Moved(_)) => WindowMoved::listener_key(),
            Self::Window(WindowEvent::MaximizeChanged(_)) => WindowMaximizeChanged::listener_key(),
            Self::Window(WindowEvent::ScaleChanged(_)) => WindowScaleChanged::listener_key(),
            Self::Window(WindowEvent::FocusGained) => WindowGainedFocus::listener_key(),
            Self::Window(WindowEvent::FocusLost) => WindowLostFocus::listener_key(),
            Self::Window(WindowEvent::ThemeChanged(_)) => ThemeChanged::listener_key(),
            Self::Window(WindowEvent::ChangeUnderCursor) => WindowChangeUnderCursor::listener_key(),
            Self::Interaction(InteractionEvent::Click) => Click::listener_key(),
            Self::Interaction(InteractionEvent::DoubleClick) => DoubleClick::listener_key(),
            Self::Interaction(InteractionEvent::SecondaryClick) => SecondaryClick::listener_key(),
            Self::FileDrag(FileDragEvent::Dropped(_)) => FileDragDrop::listener_key(),
            Self::FileDrag(FileDragEvent::Enter(_)) => FileDragEnter::listener_key(),
            Self::FileDrag(FileDragEvent::Move(_)) => FileDragMove::listener_key(),
            Self::FileDrag(FileDragEvent::Leave(_)) => FileDragLeave::listener_key(),
            Self::DragSource(DragSourceEvent::Start(..)) => DragStart::listener_key(),
            Self::DragSource(DragSourceEvent::Move(..)) => DragMove::listener_key(),
            Self::DragSource(DragSourceEvent::Enter(..)) => DragSourceEnter::listener_key(),
            Self::DragSource(DragSourceEvent::Leave(..)) => DragSourceLeave::listener_key(),
            Self::DragSource(DragSourceEvent::End(..)) => DragEnd::listener_key(),
            Self::DragSource(DragSourceEvent::Cancel(..)) => DragCancel::listener_key(),
            Self::DragTarget(DragTargetEvent::Enter(..)) => DragTargetEnter::listener_key(),
            Self::DragTarget(DragTargetEvent::Move(..)) => DragTargetMove::listener_key(),
            Self::DragTarget(DragTargetEvent::Leave(..)) => DragTargetLeave::listener_key(),
            Self::DragTarget(DragTargetEvent::Drop(..)) => DragTargetDrop::listener_key(),
            Self::Extracted => Extracted::listener_key(),
            Self::Custom(custom) => custom.listener_key_dyn(),
        };
        keys.push(specific);

        // Add broad category listeners
        match self {
            Self::Pointer(_) => keys.push(AnyPointer::listener_key()),
            Self::Key(_) => keys.push(AnyKey::listener_key()),
            Self::Window(_) => keys.push(AnyWindow::listener_key()),
            Self::Focus(_) => keys.push(AnyFocus::listener_key()),
            Self::Ime(_) => keys.push(AnyIme::listener_key()),
            Self::DragSource(_) => keys.push(AnyDragSource::listener_key()),
            Self::DragTarget(_) => keys.push(AnyDragTarget::listener_key()),
            Self::FileDrag(_) => keys.push(AnyFileDrag::listener_key()),
            _ => {}
        }

        keys
    }

    fn is_spatial(&self) -> bool {
        self.point().is_some()
    }

    fn is_move(&self) -> bool {
        matches!(self, Event::Pointer(PointerEvent::Move(_)))
    }

    pub fn new_custom(custom: impl CustomEvent) -> Self {
        Self::Custom(Box::new(custom))
    }
}

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
