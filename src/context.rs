use peniko::kurbo::{Point, Rect};
use smallvec::SmallVec;
use std::{cell::RefCell, rc::Rc};

use crate::platform::menu::Menu;
use crate::{
    event::{Event, EventPropagation},
    view::ViewId,
};

pub type EventCallback = dyn FnMut(&Event) -> EventPropagation;
pub type ResizeCallback = dyn Fn(Rect);
pub type MenuCallback = dyn Fn() -> Menu;

/// Vector of event listeners, optimized for the common case of 0-1 listeners per event type.
/// Uses SmallVec to avoid heap allocation when there's only one listener.
/// Inspired by Chromium's HeapVector<..., 1> pattern for event listener storage.
pub type EventListenerVec = SmallVec<[Rc<RefCell<EventCallback>>; 1]>;

#[derive(Default)]
pub(crate) struct ResizeListeners {
    pub(crate) rect: Rect,
    pub(crate) callbacks: Vec<Rc<ResizeCallback>>,
}

/// Listeners for when the view moves to a different position in the window
#[derive(Default)]
pub(crate) struct MoveListeners {
    pub(crate) window_origin: Point,
    pub(crate) callbacks: Vec<Rc<dyn Fn(Point)>>,
}

pub(crate) type CleanupListeners = Vec<Rc<dyn Fn()>>;

pub(crate) enum FrameUpdate {
    Style(ViewId),
    Layout(ViewId),
    Paint(ViewId),
}

// Re-export EventCx from event module for backward compatibility
pub use crate::event::EventCx;
// Re-export DragState from window_state
pub use crate::window::state::DragState;

// Re-export layout context types from layout module for backward compatibility
pub use crate::layout::{ComputeLayoutCx, LayoutCx};
// Re-export style context types from style module for backward compatibility
pub use crate::style::{InteractionState, StyleCx};
// Re-export paint context types from paint module for backward compatibility
pub use crate::paint::{PaintCx, PaintState};
// Re-export update context types from message module for backward compatibility
pub use crate::message::UpdateCx;
