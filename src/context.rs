use peniko::kurbo::{Point, Rect};
use std::rc::Rc;

use crate::platform::menu::Menu;
use crate::{
    event::{Event, EventPropagation},
    view::ViewId,
};

pub type EventCallback = dyn FnMut(&Event) -> EventPropagation;
pub type ResizeCallback = dyn Fn(Rect);
pub type MenuCallback = dyn Fn() -> Menu;

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
// Re-export stacking context types from view module
pub(crate) use crate::view::stacking::collect_stacking_context_items;

// Re-export layout context types from layout module for backward compatibility
pub use crate::layout::{ComputeLayoutCx, LayoutCx};
// Re-export style context types from style module for backward compatibility
pub use crate::style::{InteractionState, StyleCx};
// Re-export paint context types from paint module for backward compatibility
pub use crate::paint::{PaintCx, PaintState};
// Re-export update context types from update module for backward compatibility
pub use crate::update::UpdateCx;
