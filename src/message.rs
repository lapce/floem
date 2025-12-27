use std::{any::Any, cell::RefCell, collections::HashMap};

use peniko::kurbo::{Point, Rect, Size, Vec2};
use ui_events::pointer::PointerId;
use winit::window::{ResizeDirection, Theme};

use crate::{
    platform::menu::Menu,
    view::{View, ViewId},
    window::state::WindowState,
};

thread_local! {
    /// Stores all the update message with their original `ViewId`
    /// When a view sends a update message, we need to store them in `CENTRAL_UPDATE_MESSAGES`,
    /// because when the view was built, it probably hasn't got a parent yet,
    /// so we didn't know which window root view it belonged to.
    /// In `process_update_messages`, it will parse all the entries in `CENTRAL_UPDATE_MESSAGES`,
    /// and put the messages to `UPDATE_MESSAGES` according to their root `ViewId`.
    pub(crate) static CENTRAL_UPDATE_MESSAGES: RefCell<Vec<(ViewId, UpdateMessage)>> = Default::default();
    /// Stores a queue of update messages for each view. This is a list of build in messages, including a built-in State message
    /// that you can use to send a state update to a view.
    pub(crate) static UPDATE_MESSAGES: RefCell<HashMap<ViewId, Vec<UpdateMessage>>> = Default::default();
    /// Similar to `CENTRAL_UPDATE_MESSAGES` but for `DEFERRED_UPDATE_MESSAGES`
    pub(crate) static CENTRAL_DEFERRED_UPDATE_MESSAGES: RefCell<Vec<(ViewId, Box<dyn Any>)>> = Default::default();
    pub(crate) static DEFERRED_UPDATE_MESSAGES: RefCell<DeferredUpdateMessages> = Default::default();
    /// It stores the active view handle, so that when you dispatch an action, it knows
    /// which view handle it submitted to
    pub(crate) static CURRENT_RUNNING_VIEW_HANDLE: RefCell<ViewId> = RefCell::new(ViewId::new());
}

type DeferredUpdateMessages = HashMap<ViewId, Vec<(ViewId, Box<dyn Any>)>>;

pub enum UpdateMessage {
    Focus(ViewId),
    ClearFocus(ViewId),
    ClearAppFocus,
    Active(ViewId),
    ClearActive(ViewId),
    /// Set pointer capture for a view (W3C Pointer Events API).
    SetPointerCapture {
        view_id: ViewId,
        pointer_id: PointerId,
    },
    /// Release pointer capture from a view.
    ReleasePointerCapture {
        view_id: ViewId,
        pointer_id: PointerId,
    },
    WindowScale(f64),
    RequestPaint,
    State {
        id: ViewId,
        state: Box<dyn Any>,
    },
    RequestStyle(ViewId),
    RequestViewStyle(ViewId),
    ToggleWindowMaximized,
    SetWindowMaximized(bool),
    MinimizeWindow,
    DragWindow,
    DragResizeWindow(ResizeDirection),
    SetWindowDelta(Vec2),
    ShowContextMenu {
        menu: Menu,
        pos: Option<Point>,
    },
    WindowMenu {
        menu: Menu,
    },
    SetWindowTitle {
        title: String,
    },
    AddOverlay {
        view: Box<dyn View>,
    },
    RemoveOverlay {
        id: ViewId,
    },
    Inspect,
    ScrollTo {
        id: ViewId,
        rect: Option<Rect>,
    },
    FocusWindow,
    SetImeAllowed {
        allowed: bool,
    },
    SetImeCursorArea {
        position: Point,
        size: Size,
    },
    WindowVisible(bool),
    ViewTransitionAnimComplete(ViewId),
    SetTheme(Option<Theme>),
}

/// Context passed during the update phase of the view lifecycle.
pub struct UpdateCx<'a> {
    pub window_state: &'a mut WindowState,
}
