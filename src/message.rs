use std::{any::Any, cell::RefCell, collections::HashMap};

use floem_reactive::Scope;
use peniko::kurbo::{Point, Rect, Size, Vec2};
use ui_events::pointer::PointerId;
use winit::window::{ResizeDirection, Theme};

use crate::{
    element_id::ElementId,
    event::{Event, RouteKind, listener},
    platform::menu::Menu,
    style::recalc::StyleReason,
    view::{AnyView, View, ViewId},
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
    pub(crate) static CURRENT_RUNNING_VIEW_HANDLE: RefCell<Option<ViewId>> = const { RefCell::new(None) };
}

type DeferredUpdateMessages = HashMap<ViewId, Vec<(ViewId, Box<dyn Any>)>>;

pub enum UpdateMessage {
    ///
    /// This will build a focus path from the root to the target element,
    /// including all focusable ancestors. The path respects the current
    /// keyboard navigation mode:
    /// - In keyboard navigation mode: only includes keyboard-navigable elements
    /// - In non-keyboard mode: includes all focusable elements
    ///
    /// Elements that gain or lose focus will receive `FocusEvent::Gained` or
    /// `FocusEvent::Lost` events respectively.
    Focus(ElementId),

    /// Clear all focus state.
    ///
    /// This removes focus from all elements in the current focus path.
    /// Each element in the path will receive a `FocusEvent::Lost` event
    /// in order from deepest to shallowest.
    ClearFocus,

    /// Set pointer capture for a view (W3C Pointer Events API).
    SetPointerCapture {
        element_id: ElementId,
        pointer_id: PointerId,
    },
    /// Release pointer capture from an element.
    ReleasePointerCapture {
        element_id: ElementId,
        pointer_id: PointerId,
    },
    WindowScale(f64),
    RequestPaint,
    RequestLayout,
    /// Request that the box tree be updated from the layout tree (full walk) and committed
    RequestBoxTreeUpdate,
    /// Request that a specific view's box tree node be updated and committed
    RequestBoxTreeUpdateForView(ViewId),
    /// Request that the box tree be committed without updating from layout
    RequestBoxTreeCommit,
    State {
        id: ViewId,
        state: Box<dyn Any>,
    },
    RequestStyle(ElementId, StyleReason),
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
    #[cfg(not(target_arch = "wasm32"))]
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
        id: ElementId,
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
    CheckPointerHover,
    WindowVisible(bool),
    ViewTransitionAnimComplete(ViewId),
    SetTheme(Option<Theme>),
    /// Remove views from the tree (used by keyed children).
    /// Each view and its children will be properly cleaned up.
    RemoveViews(Vec<ViewId>),
    /// Add a child to a parent view. The child is constructed lazily
    /// when the message is processed, inside the parent's scope if provided.
    AddChild {
        parent_id: ViewId,
        child: DeferredChild,
    },
    /// Add multiple children to a parent view. The children are constructed lazily
    /// when the message is processed, inside the parent's scope if provided.
    AddChildren {
        parent_id: ViewId,
        children: DeferredChildren,
    },
    /// Set up reactive children (derived_children, derived_child, keyed_children).
    /// The setup is deferred to ensure it runs inside the correct scope.
    SetupReactiveChildren {
        setup: DeferredReactiveSetup,
    },
    RegisterListener(listener::EventListenerKey, ViewId),
    RemoveListener(listener::EventListenerKey, ViewId),
    RouteEvent {
        id: ViewId,
        event: Box<Event>, // boxed because of large size
        route_kind: RouteKind,
        triggered_by: Option<Box<Event>>,
    },
    MarkViewLayoutDirty(ViewId),
}

/// Context passed during the update phase of the view lifecycle.
pub struct UpdateCx<'a> {
    pub window_state: &'a mut WindowState,
}

/// A deferred child view that will be constructed when the message is processed.
///
/// Uses `Option` + `take()` pattern to allow `FnOnce` to be called from storage.
/// The scope is resolved at build time by looking up the parent's context scope.
pub struct DeferredChild {
    builder: Option<Box<dyn FnOnce() -> AnyView>>,
}

impl DeferredChild {
    /// Create a new deferred child with a builder function.
    ///
    /// The scope is not captured here - it will be resolved when `build()` is called
    /// by looking up the parent's context scope in the view hierarchy.
    pub fn new(builder: impl FnOnce() -> AnyView + 'static) -> Self {
        Self {
            builder: Some(Box::new(builder)),
        }
    }

    /// Build the child view inside the given scope.
    ///
    /// The scope should be resolved by the caller (typically by looking up
    /// the parent's context scope in the view hierarchy).
    ///
    /// # Panics
    /// Panics if called more than once.
    pub fn build(&mut self, scope: Scope) -> AnyView {
        let builder = self
            .builder
            .take()
            .expect("DeferredChild::build called twice");
        scope.enter(builder)
    }
}

/// Multiple deferred children that will be constructed when the message is processed.
pub struct DeferredChildren {
    builder: Option<Box<dyn FnOnce() -> Vec<AnyView>>>,
}

impl DeferredChildren {
    /// Create new deferred children with a builder function.
    ///
    /// The scope is not captured here - it will be resolved when `build()` is called
    /// by looking up the parent's context scope in the view hierarchy.
    pub fn new(builder: impl FnOnce() -> Vec<AnyView> + 'static) -> Self {
        Self {
            builder: Some(Box::new(builder)),
        }
    }

    /// Build all children inside the given scope.
    ///
    /// The scope should be resolved by the caller (typically by looking up
    /// the parent's context scope in the view hierarchy).
    ///
    /// # Panics
    /// Panics if called more than once.
    pub fn build(&mut self, scope: Scope) -> Vec<AnyView> {
        let builder = self
            .builder
            .take()
            .expect("DeferredChildren::build called twice");
        scope.enter(builder)
    }
}

/// Deferred setup for reactive children (derived_children, derived_child, keyed_children).
///
/// This captures all the setup logic in a closure that will be executed when the
/// message is processed. The scope is resolved at run time by looking up the parent's
/// scope in the view hierarchy.
pub struct DeferredReactiveSetup {
    view_id: ViewId,
    setup: Option<Box<dyn FnOnce()>>,
}

impl DeferredReactiveSetup {
    /// Create a new deferred reactive setup.
    ///
    /// The setup function will be called inside the view's scope (resolved via `find_scope()`)
    /// when `run()` is invoked. It should set up the reactive effect and initial children.
    pub fn new(view_id: ViewId, setup: impl FnOnce() + 'static) -> Self {
        Self {
            view_id,
            setup: Some(Box::new(setup)),
        }
    }

    /// Run the setup inside the view's scope.
    ///
    /// The scope is resolved by walking up the view hierarchy to find the nearest
    /// ancestor with a scope. If none is found, uses the current scope.
    ///
    /// # Panics
    /// Panics if called more than once.
    pub fn run(&mut self) {
        let setup = self
            .setup
            .take()
            .expect("DeferredReactiveSetup::run called twice");
        let scope = self.view_id.find_scope().unwrap_or_else(Scope::current);
        scope.enter(setup)
    }
}
