use std::{any::Any, cell::RefCell, collections::HashMap};

use floem_winit::window::ResizeDirection;
use kurbo::{Point, Rect, Size, Vec2};

use crate::{
    animate::{AnimUpdateMsg, Animation},
    context::{EventCallback, ResizeCallback},
    event::EventListener,
    id::Id,
    menu::Menu,
    style::{Style, StyleClassRef, StyleSelector},
    view::Widget,
    view_data::{ChangeFlags, StackOffset},
};

thread_local! {
    pub(crate) static CENTRAL_UPDATE_MESSAGES: RefCell<Vec<(Id, UpdateMessage)>> = Default::default();
    /// Stores a queue of update messages for each view. This is a list of build in messages, including a built-in State message
    /// that you can use to send a state update to a view.
    pub(crate) static UPDATE_MESSAGES: RefCell<HashMap<Id, Vec<UpdateMessage>>> = Default::default();
    pub(crate) static CENTRAL_DEFERRED_UPDATE_MESSAGES: RefCell<Vec<(Id, Box<dyn Any>)>> = Default::default();
    pub(crate) static DEFERRED_UPDATE_MESSAGES: RefCell<DeferredUpdateMessages> = Default::default();
    pub(crate) static ANIM_UPDATE_MESSAGES: RefCell<Vec<AnimUpdateMsg>> = Default::default();
    /// It stores the active view handle, so that when you dispatch an action, it knows
    /// which view handle it submitted to
    pub(crate) static CURRENT_RUNNING_VIEW_HANDLE: RefCell<Id> = RefCell::new(Id::next());
}

// pub type FileDialogs = HashMap<FileDialogToken, Box<dyn Fn(Option<FileInfo>)>>;
type DeferredUpdateMessages = HashMap<Id, Vec<(Id, Box<dyn Any>)>>;

pub(crate) enum UpdateMessage {
    Focus(Id),
    ClearFocus(Id),
    Active(Id),
    WindowScale(f64),
    Disabled {
        id: Id,
        is_disabled: bool,
    },
    RequestChange {
        id: Id,
        flags: ChangeFlags,
    },
    RequestPaint,
    State {
        id: Id,
        state: Box<dyn Any>,
    },
    Style {
        id: Id,
        style: Style,
        offset: StackOffset<Style>,
    },
    AddClass {
        id: Id,
        class: StyleClassRef,
    },
    StyleSelector {
        id: Id,
        selector: StyleSelector,
        style: Style,
    },
    KeyboardNavigable {
        id: Id,
    },
    Draggable {
        id: Id,
    },
    EventListener {
        id: Id,
        listener: EventListener,
        action: Box<EventCallback>,
    },
    ResizeListener {
        id: Id,
        action: Box<ResizeCallback>,
    },
    MoveListener {
        id: Id,
        action: Box<dyn Fn(Point)>,
    },
    CleanupListener {
        id: Id,
        action: Box<dyn Fn()>,
    },
    ToggleWindowMaximized,
    SetWindowMaximized(bool),
    MinimizeWindow,
    DragWindow,
    DragResizeWindow(ResizeDirection),
    SetWindowDelta(Vec2),
    Animation {
        id: Id,
        animation: Animation,
    },
    ContextMenu {
        id: Id,
        menu: Box<dyn Fn() -> Menu>,
    },
    PopoutMenu {
        id: Id,
        menu: Box<dyn Fn() -> Menu>,
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
        id: Id,
        position: Point,
        view: Box<dyn FnOnce() -> Box<dyn Widget>>,
    },
    RemoveOverlay {
        id: Id,
    },
    Inspect,
    ScrollTo {
        id: Id,
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
}
