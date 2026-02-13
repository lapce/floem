use peniko::kurbo::{Affine, Point, Rect};
use smallvec::SmallVec;
use std::{cell::RefCell, rc::Rc};

use crate::custom_event;
use crate::event::Phase;
use crate::platform::menu::Menu;
use crate::{event::EventPropagation, view::ViewId};

pub type EventCallback = dyn FnMut(&mut EventCx) -> EventPropagation;
pub type ResizeCallback = dyn Fn(Rect);
pub type MenuCallback = dyn Fn() -> Menu;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Phases: u8 {
        const CAPTURE = 0b1;
        const TARGET = 0b10;
        const BUBBLE = 0b100;
        const BROADCAST = 0b1000;
    }
}
impl Phases {
    /// Target and Bubble phases
    pub const TARGET_AND_BUBBLE: Phases =
        Phases::from_bits_truncate(Phases::TARGET.bits() | Phases::BUBBLE.bits());

    /// Capture and Target phases
    pub const CAPTURE_AND_TARGET: Phases =
        Phases::from_bits_truncate(Phases::CAPTURE.bits() | Phases::TARGET.bits());

    /// Capture and Bubble phases
    pub const CAPTURE_AND_BUBBLE: Phases =
        Phases::from_bits_truncate(Phases::CAPTURE.bits() | Phases::BUBBLE.bits());

    /// Broadcast phase only (for global keyboard shortcuts, etc.)
    pub const BROADCAST_ONLY: Phases = Phases::BROADCAST;
    /// All standard phases (capture/target/bubble) - common for pointer events
    pub const STANDARD: Phases = Phases::from_bits_truncate(
        Phases::CAPTURE.bits() | Phases::TARGET.bits() | Phases::BUBBLE.bits(),
    );

    /// Check if this phase set includes the given phase.
    pub fn matches(&self, phase: &Phase) -> bool {
        match phase {
            Phase::Capture => self.contains(Phases::CAPTURE),
            Phase::Target => self.contains(Phases::TARGET),
            Phase::Bubble => self.contains(Phases::BUBBLE),
            Phase::Broadcast => self.contains(Phases::BROADCAST), // <- Add this
        }
    }
}

/// Configuration for event callbacks that determines when they should be invoked during event propagation.
///
/// Event listeners in Floem are called during different phases of the event propagation pipeline,
/// similar to the DOM event model. This config allows you to specify which phases your callback
/// should respond to.
///
/// # Examples
///
/// Listen during the target and bubble phases (default):
/// ```ignore
/// view
///     .on_event_with_config(
///         EventListener::Click,
///         EventCallbackConfig::default(),
///         |cx, _| EventPropagation::Continue,
///     )
/// ```
///
/// Listen only during the capture phase (useful for intercepting before child handlers):
/// ```ignore
/// view
///     .on_event_with_config(
///         EventListener::KeyDown,
///         EventCallbackConfig {
///             phases: Phases::CAPTURE,
///         },
///         |cx, event| {
///             // Handle key down during capture phase
///             EventPropagation::Continue
///         },
///     )
/// ```
///
/// Listen during all phases:
/// ```ignore
/// view
///     .on_event_with_config(
///         EventListener::Focus,
///         EventCallbackConfig {
///             phases: Phases::CAPTURE | Phases::TARGET | Phases::BUBBLE,
///         },
///         |cx, _| EventPropagation::Continue,
///     )
/// ```
#[derive(Clone, Copy, PartialEq)]
pub struct EventCallbackConfig {
    /// Determines which event propagation phases should trigger this callback.
    pub phases: Phases,
}
impl Default for EventCallbackConfig {
    fn default() -> Self {
        Self {
            phases: Phases::TARGET | Phases::BUBBLE,
        }
    }
}

/// Vector of event listeners, optimized for the common case of 0-1 listeners per event type.
/// Uses SmallVec to avoid heap allocation when there's only one listener.
/// Inspired by Chromium's HeapVector<..., 1> pattern for event listener storage.
pub type EventListenerVec = SmallVec<[(Rc<RefCell<EventCallback>>, EventCallbackConfig); 1]>;

/// Event fired when a view's layout changes
///
/// This is fired when the view's size or position in the layout changes.
/// It does not fire for visual-only changes from transforms (translation, scale, rotation).
///
/// # Important: Layout vs Visual Coordinates
///
/// **WARNING**: The window coordinates provided by this event (`new_window_origin`, `box_window()`,
/// `content_box_window()`) represent the position in the **box layout tree**, NOT the final visual
/// position after transforms are applied. If transforms like translation, scale, or rotation are
/// applied to this view or its ancestors, the actual rendered position will differ from these coordinates.
///
/// **Use `VisualChanged` instead if you need the actual rendered position in the window after all
/// transforms have been applied.**
///
/// ## When to use LayoutChanged vs VisualChanged
///
/// - Use `LayoutChanged`: When you need layout box sizes or relative positioning within the layout tree
/// - Use `VisualChanged`: When you need to know where the view actually appears on screen, or to
///   position elements relative to the view's visual appearance
///
/// ## Coordinate Spaces in Floem
///
/// **Note**: All event handling and painting in Floem happen in the view's **local coordinate space**.
/// Floem automatically handles transformations to and from local coordinates. You typically don't need
/// window coordinates unless you're positioning elements relative to other views' visual positions or
/// interacting with platform APIs that expect window coordinates.
///
/// Use `box_local()` and `content_box_local()` to get coordinates in the view's local space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutChanged {
    /// The new layout box relative to the parent layout
    pub new_box: Rect,
    /// The new content box relative to the parent layout
    pub new_content_box: Rect,
    /// The position of the layout box's origin in window coordinates (box layout position, NOT visual position)
    ///
    /// **WARNING**: This does not include transforms. Use `VisualChanged` for actual rendered position.
    pub new_window_origin: Point,
}
custom_event!(LayoutChanged, allow_disabled = |_event| true);

impl LayoutChanged {
    /// Get the layout box in the view's local coordinate space (relative to its own origin which is `Point::ZERO`)
    pub fn box_local(&self) -> Rect {
        self.new_box.with_origin(Point::ZERO)
    }

    /// Get the content box in the view's local coordinate space (relative to the box origin)
    pub fn content_box_local(&self) -> Rect {
        self.new_content_box
            .with_origin(Point::ZERO - self.new_box.origin().to_vec2())
    }

    /// Get the layout box in window coordinates (box layout position, NOT visual position)
    ///
    /// **WARNING**: This does not include transforms. The actual rendered position may differ
    /// if transforms are applied. Use `VisualChanged` for the actual visual position.
    pub fn box_window(&self) -> Rect {
        self.new_box.with_origin(self.new_window_origin)
    }

    /// Get the content box in window coordinates (box layout position, NOT visual position)
    ///
    /// **WARNING**: This does not include transforms. The actual rendered position may differ
    /// if transforms are applied. Use `VisualChanged` for the actual visual position.
    pub fn content_box_window(&self) -> Rect {
        let content_offset = self.new_content_box.origin() - self.new_box.origin();
        self.new_content_box
            .with_origin(self.new_window_origin + content_offset)
    }
}

/// Event fired when a view's visual representation in the window changes.
///
/// This is fired when the view's final rendered position or transform changes,
/// including changes from CSS-like transforms (translation, scale, rotation).
/// This represents the view's actual visual appearance in the window after all
/// transforms have been applied.
///
/// # Important: Visual vs Layout Coordinates
///
/// **Use this event when you need to know where the view actually appears on screen.**
/// The coordinates and transform provided here reflect the final rendered state after
/// all transforms (translation, scale, rotation) from this view and all ancestors have
/// been applied.
///
/// ## When to use VisualChanged vs LayoutChanged
///
/// - Use `VisualChanged`: When you need the actual rendered position on screen, or to position
///   elements relative to where a view visually appears (e.g., tooltips, popovers, overlays)
/// - Use `LayoutChanged`: When you only care about layout box sizes or relative positioning
///   within the layout tree, ignoring transforms
///
/// ## Coordinate Spaces in Floem
///
/// **Note**: All event handling and painting in Floem happen in the view's **local coordinate space**.
/// Floem automatically handles transformations to and from local coordinates. You typically only need
/// visual window coordinates when positioning separate elements (like overlays or platform windows)
/// relative to a view's visual appearance, or when interacting with platform APIs that expect
/// window coordinates.
///
/// ## Use Cases for VisualChanged
///
/// - Positioning tooltips or popovers relative to a transformed view
/// - Implementing drag-and-drop with visual feedback
/// - Coordinating with platform overlays or native windows
/// - Calculating whether two views visually overlap on screen
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualChanged {
    /// The new axis-aligned bounding box in window coordinates after all transforms
    ///
    /// This is the smallest rectangle that contains the view after all transforms are applied.
    /// For rotated views, this will be larger than the view's layout box.
    pub new_visual_aabb: Rect,
    /// The new world transform matrix combining all parent and local transforms
    ///
    /// This transform maps from the view's local coordinate space to window coordinates.
    pub new_world_transform: Affine,
}
custom_event!(VisualChanged, allow_disabled = |_event| true);

impl VisualChanged {
    /// Get the visual origin of the view in window coordinates
    ///
    /// This returns the top-left corner of the visual bounding box after all transforms.
    /// Note that for rotated or scaled views, this may not correspond directly to the
    /// layout box origin transformed by `new_world_transform`.
    pub fn visual_window_origin(&self) -> Point {
        self.new_visual_aabb.origin()
    }

    /// Transform a point from the view's local coordinate space to window coordinates
    ///
    /// This applies the full world transform to convert a point in the view's local
    /// coordinate space to its position in the window.
    pub fn local_to_window(&self, local_point: Point) -> Point {
        self.new_world_transform * local_point
    }

    /// Transform a point from window coordinates to the view's local coordinate space
    ///
    /// This applies the inverse world transform to convert a point in window coordinates
    /// to the view's local coordinate space. Returns `None` if the transform is not invertible.
    pub fn window_to_local(&self, window_point: Point) -> Point {
        self.new_world_transform.inverse() * window_point
    }
}

pub(crate) type CleanupListeners = Vec<Rc<dyn Fn()>>;

pub(crate) enum FrameUpdate {
    Style(ViewId),
    Layout,
    BoxTreeCommit,
    Paint(ViewId),
}

// Re-export EventCx from event module for backward compatibility
pub use crate::event::EventCx;

// Re-export layout context types from layout module for backward compatibility
pub use crate::layout::LayoutCx;
// Re-export style context types from style module for backward compatibility
pub use crate::style::{InteractionState, StyleCx};
// Re-export paint context types from paint module for backward compatibility
pub use crate::paint::{PaintCx, PaintState};
// Re-export update context types from message module for backward compatibility
pub use crate::message::UpdateCx;
