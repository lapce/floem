use std::{any::Any, cell::RefCell, collections::HashMap};

use floem_winit::window::ResizeDirection;
use peniko::kurbo::{Point, Rect, Size, Vec2};

use crate::{
    animate::{AnimUpdateMsg, Animation},
    id::ViewId,
    menu::Menu,
    view::View,
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
    pub(crate) static ANIM_UPDATE_MESSAGES: RefCell<Vec<AnimUpdateMsg>> = Default::default();
    /// It stores the active view handle, so that when you dispatch an action, it knows
    /// which view handle it submitted to
    pub(crate) static CURRENT_RUNNING_VIEW_HANDLE: RefCell<ViewId> = RefCell::new(ViewId::new());
}

type DeferredUpdateMessages = HashMap<ViewId, Vec<(ViewId, Box<dyn Any>)>>;

pub(crate) enum UpdateMessage {
    Focus(ViewId),
    ClearFocus(ViewId),
    Active(ViewId),
    ClearActive(ViewId),
    WindowScale(f64),
    Disabled {
        id: ViewId,
        is_disabled: bool,
    },
    RequestPaint,
    State {
        id: ViewId,
        state: Box<dyn Any>,
    },
    KeyboardNavigable {
        id: ViewId,
    },
    Draggable {
        id: ViewId,
    },
    ToggleWindowMaximized,
    SetWindowMaximized(bool),
    MinimizeWindow,
    DragWindow,
    DragResizeWindow(ResizeDirection),
    SetWindowDelta(Vec2),
    Animation {
        id: ViewId,
        animation: Animation,
    },
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
        id: ViewId,
        position: Point,
        view: Box<dyn FnOnce() -> Box<dyn View>>,
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
}

impl std::fmt::Debug for UpdateMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateMessage::Focus(id) => f.write_fmt(format_args!("Focus({:?})", id)),
            UpdateMessage::ClearFocus(id) => f.write_fmt(format_args!("ClearFocus({:?})", id)),
            UpdateMessage::Active(id) => f.write_fmt(format_args!("Active({:?})", id)),
            UpdateMessage::ClearActive(id) => f.write_fmt(format_args!("ClearActive({:?})", id)),
            UpdateMessage::WindowScale(scale) => {
                f.write_fmt(format_args!("WindowScale({})", scale))
            }
            UpdateMessage::Disabled { id, is_disabled } => {
                f.write_fmt(format_args!("Disabled({:?}:{})", id, is_disabled))
            }
            UpdateMessage::RequestPaint => f.write_str("RequestPaint"),
            UpdateMessage::State { id, state: _ } => {
                f.write_fmt(format_args!("State({:?}:???)", id))
            }
            UpdateMessage::KeyboardNavigable { id } => {
                f.write_fmt(format_args!("KeyboardNavigable({:?})", id))
            }
            UpdateMessage::Draggable { id } => f.write_fmt(format_args!("Draggable({:?})", id)),
            UpdateMessage::ToggleWindowMaximized => f.write_str("ToggleWindowMaximized"),
            UpdateMessage::SetWindowMaximized(maximized) => {
                f.write_fmt(format_args!("SetWindowMaximized({})", maximized))
            }
            UpdateMessage::MinimizeWindow => f.write_str("MinimizeWindow"),
            UpdateMessage::DragWindow => f.write_str("DragWindow"),
            UpdateMessage::DragResizeWindow(direction) => {
                f.write_fmt(format_args!("DragResizeWindow({:?})", direction))
            }
            UpdateMessage::SetWindowDelta(delta) => {
                f.write_fmt(format_args!("SetWindowDelta({}, {})", delta.x, delta.y))
            }
            UpdateMessage::Animation { id, animation: _ } => {
                f.write_fmt(format_args!("Animation({:?})", id))
            }
            UpdateMessage::ShowContextMenu { menu: _, pos } => {
                f.write_fmt(format_args!("ShowContextMenu({:?})", pos))
            }
            UpdateMessage::WindowMenu { menu: _ } => f.write_str("WindowMenu"),
            UpdateMessage::SetWindowTitle { title } => {
                f.write_fmt(format_args!("SetWindowTitle({:?})", title))
            }
            UpdateMessage::AddOverlay {
                id,
                position,
                view: _,
            } => f.write_fmt(format_args!("AddOverlay({:?} : {:?})", id, position)),
            UpdateMessage::RemoveOverlay { id } => {
                f.write_fmt(format_args!("RemoveOverlay({:?})", id))
            }
            UpdateMessage::Inspect => f.write_str("Inspect"),
            UpdateMessage::ScrollTo { id, rect } => {
                f.write_fmt(format_args!("ScrollTo({:?}:{:?})", id, rect))
            }
            UpdateMessage::FocusWindow => f.write_str("FocusWindow"),
            UpdateMessage::SetImeAllowed { allowed } => {
                f.write_fmt(format_args!("SetImeAllowed({})", allowed))
            }
            UpdateMessage::SetImeCursorArea { position, size } => {
                f.write_fmt(format_args!("SetImeCursorArea({:?}, {:?})", position, size))
            }
            UpdateMessage::WindowVisible(visible) => {
                f.write_fmt(format_args!("WindowVisible({})", visible))
            }
        }
    }
}
