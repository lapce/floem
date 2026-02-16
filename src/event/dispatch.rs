//! Event dispatch logic for handling events through the view tree.

use std::{rc::Rc, sync::LazyLock, time::Instant};

use peniko::kurbo::{Affine, Point};
use smallvec::SmallVec;
use ui_events::{
    keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey},
    pointer::{PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo},
};
use understory_box_tree::NodeFlags;
use understory_event_state::{
    click::ClickResult, focus::FocusEvent as UnderFocusEvent, hover::HoverEvent,
};

use crate::{
    BoxTree, ElementId, ViewId,
    action::show_context_menu,
    context::*,
    event::{
        DragEvent, DragToken, Event, FocusEvent, InteractionEvent, Phase, PointerCaptureEvent,
        WindowEvent, drag_state::DragEventDispatch, dropped_file::FileDragEvent, path::hit_test,
    },
    view::{VIEW_STORAGE, View},
    window::WindowState,
};

static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

/// A single step in a capture/target/bubble dispatch sequence.
#[derive(Clone, Copy, Debug)]
pub struct Dispatch {
    pub phase: Phase,
    /// The element this step is addressed to.
    pub target_element_id: ElementId,
    /// The view that owns `target_element_id`.
    pub owning_id: ViewId,
}

impl Dispatch {
    #[inline]
    fn capture(target: ElementId) -> Self {
        Self {
            phase: Phase::Capture,
            target_element_id: target,
            owning_id: target.owning_id(),
        }
    }
    #[inline]
    fn target(target: ElementId) -> Self {
        Self {
            phase: Phase::Target,
            target_element_id: target,
            owning_id: target.owning_id(),
        }
    }
    #[inline]
    fn bubble(target: ElementId) -> Self {
        Self {
            phase: Phase::Bubble,
            target_element_id: target,
            owning_id: target.owning_id(),
        }
    }
    #[inline]
    pub fn broadcast(target: ElementId) -> Self {
        Self {
            phase: Phase::Broadcast,
            target_element_id: target,
            owning_id: target.owning_id(),
        }
    }
}

/// Propagation control returned by per-node handlers.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Continue,
    Stop,
}

impl Outcome {
    #[inline]
    fn is_stop(self) -> bool {
        matches!(self, Self::Stop)
    }
}

/// Walk a dispatch sequence, calling `handler` for each step.
/// Returns the first `Dispatch` where `handler` returned `Stop`, or `None` if all completed.
#[inline]
fn run_dispatch(
    seq: impl IntoIterator<Item = Dispatch>,
    mut handler: impl FnMut(Dispatch) -> Outcome,
) -> Option<Dispatch> {
    seq.into_iter().find(|&d| handler(d).is_stop())
}

/// Builds the ancestor chain from target to root by walking up the box tree.
///
/// Returns a vector of VisualIds ordered from target to root: [target, parent1, parent2, ..., root]
///
/// # Arguments
/// * `target` - The starting VisualId (deepest node in the tree)
/// * `root_view_id` - The root ViewId for constructing VisualIds from box tree NodeIds
/// * `box_tree` - The box tree to traverse for parent relationships
///
/// # Returns
/// Vector of VisualIds from target to root, or just [target] if no parents found
fn build_ancestor_chain(target: ElementId, box_tree: &BoxTree) -> SmallVec<[ElementId; 64]> {
    let mut path = SmallVec::new();
    let mut current = target;
    let mut visited = std::collections::HashSet::new();
    const MAX_DEPTH: usize = 1000; // Prevent runaway loops

    visited.insert(current.0);
    path.push(current);

    while let Some(parent_node) = box_tree.parent_of(current.0) {
        current = box_tree.meta(parent_node).flatten().unwrap();

        // Cycle detection
        if !visited.insert(current.0) || path.len() >= MAX_DEPTH {
            eprintln!("Warning: Detected cycle or excessive depth in box tree parent chain");
            break;
        }

        path.push(current);
    }

    path
}

/// Iterator that yields `Dispatch` items for capture/target/bubble phases.
///
/// Given an ancestor chain [target, parent1, parent2, ..., root], yields:
/// - Capture phase: root -> parent2 -> parent1 (excluding target)
/// - Target phase: target
/// - Bubble phase: parent1 -> parent2 (excluding target and root)
struct DispatchSequenceIter {
    ancestor_chain: SmallVec<[ElementId; 64]>,
    phase: DispatchPhase,
    index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DispatchPhase {
    Capture,
    Target,
    Bubble,
    Done,
}

impl DispatchSequenceIter {
    fn new(ancestor_chain: SmallVec<[ElementId; 64]>) -> Self {
        Self {
            ancestor_chain,
            phase: DispatchPhase::Capture,
            index: 0,
        }
    }
}

impl Iterator for DispatchSequenceIter {
    type Item = Dispatch;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ancestor_chain.is_empty() {
            return None;
        }

        match self.phase {
            DispatchPhase::Capture => {
                // ancestor_chain = [target, parent1, parent2, ..., root]
                // Capture emits: root, parent_{n-1}, ..., parent1 (excluding target)
                let capture_count = self.ancestor_chain.len().saturating_sub(1);
                if self.index < capture_count {
                    let idx = self.ancestor_chain.len() - 1 - self.index;
                    let element_id = self.ancestor_chain[idx];
                    self.index += 1;
                    Some(Dispatch::capture(element_id))
                } else {
                    self.phase = DispatchPhase::Target;
                    self.index = 0;
                    self.next()
                }
            }
            DispatchPhase::Target => {
                self.phase = DispatchPhase::Bubble;
                let target = self.ancestor_chain[0];
                Some(Dispatch::target(target))
            }
            DispatchPhase::Bubble => {
                // Bubble: parent1 to root (excluding target and root)
                let bubble_count = self.ancestor_chain.len().saturating_sub(2);
                if self.index < bubble_count {
                    let idx = self.index + 1; // skip target at index 0
                    let element_id = self.ancestor_chain[idx];
                    self.index += 1;
                    Some(Dispatch::bubble(element_id))
                } else {
                    self.phase = DispatchPhase::Done;
                    None
                }
            }
            DispatchPhase::Done => None,
        }
    }
}

/// Builds a capture/target/bubble dispatch iterator for a target.
///
/// Returns an iterator that yields FloemDispatch items without intermediate allocations.
///
/// # Arguments
/// * `target` - The target VisualId
/// * `root_view_id` - The root ViewId
/// * `box_tree` - The box tree to traverse
///
/// # Returns
/// Iterator over dispatch sequence with all phases
fn build_capture_bubble_path(target: ElementId, box_tree: &BoxTree) -> DispatchSequenceIter {
    let ancestor_chain = build_ancestor_chain(target, box_tree);
    DispatchSequenceIter::new(ancestor_chain)
}

/// Walk up the box tree from `target`, yielding each ancestor filtered by `predicate`,
/// then reverse so the result is root→target order.
fn build_focus_path(
    target: ElementId,
    box_tree: &BoxTree,
    keyboard_navigation: bool,
) -> SmallVec<[ElementId; 64]> {
    let mut path: SmallVec<[ElementId; 64]> = std::iter::successors(Some(target), |&cur| {
        box_tree
            .parent_of(cur.0)
            .map(|p| box_tree.meta(p).flatten().unwrap())
    })
    .filter(|id| {
        box_tree
            .flags(id.0)
            .map(|f| {
                f.contains(NodeFlags::VISIBLE)
                    && (if keyboard_navigation {
                        // For keyboard navigation, node must be keyboard navigable
                        // (which (at least in Floem) implies focusable)
                        f.contains(NodeFlags::KEYBOARD_NAVIGABLE)
                    } else {
                        // For non-keyboard navigation, node must be focusable
                        f.contains(NodeFlags::FOCUSABLE)
                    })
            })
            .unwrap_or(false)
    })
    .collect();
    path.reverse();
    path
}

/// Defines the routing strategy for dispatching events through the view tree.
///
/// Different event types require different routing behaviors. This enum encapsulates
/// the routing strategies used in Floem's event system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RouteKind {
    /// Route to a specific target with customizable phases.
    Directed {
        target: ElementId,
        phases: crate::context::Phases,
    },

    /// Route to the currently focused view with specified phases.
    /// If no view has focus, the event is not delivered.
    /// This is a like the [Self::Directed] but is a convenience variant that automatically resolves to the focused target.
    Focused { phases: crate::context::Phases },

    /// Route based on spatial hit testing at a point (pointer events).
    /// The target is determined by performing a hit test at the given point,
    /// then the event is dispatched using the specified phases.
    ///
    /// If no point is supplied the event's point method will be invoked.
    /// If the event also does not have a point the event will not dispatch to any view.
    Spatial {
        point: Option<Point>,
        phases: crate::context::Phases,
    },

    /// Route to a target and all its descendants.
    /// The event is delivered to the target and every view in its subtree.
    /// If `respect_propagation` is true, propagation can be stopped by event handlers.
    /// If false, all views in the subtree will receive the event regardless.
    Subtree {
        target: ElementId,
        respect_propagation: bool,
    },

    /// Broadcast to all views in DOM order (global broadcast).
    /// If `respect_propagation` is true, propagation can be stopped by event handlers.
    /// If false, all views will receive the event regardless. Use for window-wide events
    /// that all views should receive.
    /// This is like subtree but automatically targets the window root
    Broadcast { respect_propagation: bool },
}

/// Event routing data containing routing strategy and event source.
///
/// This struct wraps a `RouteKind` along with the source `ElementId`,
/// allowing the event system to track where events originate from.
#[derive(Debug, Clone)]
pub struct RouteData {
    /// The routing strategy to use for this event
    pub kind: RouteKind,
    /// The visual ID of the view that originated/sent this event.
    /// For window events, this is typically the root view.
    /// For user-emitted events, this is the view that called emit.
    pub source: ElementId,
}

pub(crate) struct GlobalEventCx<'a> {
    pub window_state: &'a mut WindowState,
    // TODO: dispatch is per event / synthetic so not global
    dispatch: Option<Rc<[Dispatch]>>,
    event: Event,
    hit_path: Option<Rc<[ElementId]>>,
    /// The visual ID that is the source of the current event being dispatched
    source: ElementId,
    pending_events: SmallVec<[(RouteKind, Event); 8]>,
}

#[expect(clippy::large_enum_variant)]
pub(crate) enum OverrideKind<'a> {
    Normal { triggered_by: Option<&'a Event> },
    Synthetic { event: Event },
}

pub struct EventCx<'a> {
    pub window_state: &'a mut WindowState,
    /// An event that has been transformed to the local coordinate space of the target node.
    ///
    /// Do not use this inside of event listeners, the data has been extracted and the variant of this event will always be `Extracted`.
    pub event: Event,
    /// The transform from window space to the local space of the target element that was used to transform the event.
    pub world_transform: Affine,
    /// In the case that `event` is a synthetic event like `Click`, the caused by may contain information about the triggering event.
    ///
    /// This event is not transformed. If you want it in coordinates local to your element, use the `cx.world_transform`.
    pub triggered_by: Option<&'a Event>,
    /// If the event is an event with a point (the [Event::point] method for the event returned `Some(point)`), this contains the full set of visual ids that were under the pointer.
    pub hit_path: Option<Rc<[ElementId]>>,
    /// The event phase for this local event
    pub phase: Phase,
    /// The target of this event
    pub target: ElementId,
    /// The visual ID that is the source/origin of this event
    pub source: ElementId,
    /// The full dispatch for this event. This contains the full set of targets and their respective phases for the event.
    pub dispatch: Option<Rc<[Dispatch]>>,
    /// The element caused the event to be dispatched. Window events this will be the `ViewId` of the window handle.
    pub source_id: ElementId,
    /// Whether preventDefault() was called (shared across all phases of this event)
    default_prevented: &'a mut bool,
    /// Whether stopImmediatePropagation() was called
    stop_immediate: bool,
}
impl<'a> EventCx<'a> {
    /// Stop propagation to other listeners on this target AND to other nodes in the path.
    /// This is the web's stopImmediatePropagation().
    pub fn stop_immediate_propagation(&mut self) {
        self.stop_immediate = true;
    }

    /// Prevent the default action for this event.
    /// This is the web's preventDefault().
    pub fn prevent_default(&mut self) {
        *self.default_prevented = true;
    }

    /// Check if preventDefault() was called on this event.
    pub fn is_default_prevented(&self) -> bool {
        *self.default_prevented
    }

    /// Request pointer capture for this element.
    ///
    /// This should typically be called in response to a pointer down event.
    /// Once capture is gained, the element will receive a `PointerCaptureEvent::Gained`,
    /// at which point it can call `request_drag()` to begin tracking a potential drag.
    ///
    /// # Example
    /// ```rust
    /// Event::Pointer(PointerEvent::Down(pe)) => {
    ///     if let Some(pointer_id) = pe.pointer.pointer_id {
    ///         cx.request_pointer_capture(pointer_id);
    ///     }
    /// }
    /// Event::PointerCapture(PointerCaptureEvent::Gained(_)) => {
    ///     cx.request_drag(cx.view_id, 3.0);
    /// }
    /// ```
    pub fn request_pointer_capture(&mut self, pointer_id: PointerId) -> bool {
        self.window_state
            .set_pointer_capture(pointer_id, self.target)
    }

    /// Request that this element start tracking a drag.
    ///
    /// When `use_default_preview` is true, floem will set the `dragging_preview` to this element which will make it apprear above all other content and under the cursor.
    ///
    /// This should be called in response to `PointerCaptureEvent::Gained`.
    /// The element must have pointer capture before requesting drag.
    /// The drag won't actually start until the pointer moves beyond the threshold.
    pub fn start_drag(
        &mut self,
        drag_token: DragToken,
        config: crate::event::DragConfig,
        use_default_preview: bool,
    ) -> bool {
        let Some(Event::Pointer(PointerEvent::Down(pbe))) = self.triggered_by.as_ref() else {
            return false;
        };

        let pointer_id = drag_token.pointer_id();

        let element_id = self.target;

        if self.window_state.get_pointer_capture_target(pointer_id) != Some(element_id) {
            return false;
        };

        self.window_state.drag_tracker.request_drag(
            self.target,
            pointer_id,
            pbe.state.clone(),
            pbe.button,
            pbe.pointer,
            config,
            use_default_preview,
        )
    }

    fn dispatch_one(&mut self) -> Outcome {
        if self.target.is_view() & self.target.owning_id().is_disabled()
            && !self.event.allow_disabled()
        {
            return Outcome::Continue;
        }

        // CRITICAL: No other borrows of view_storage, states, or views must be held here
        VIEW_STORAGE.with(|s| {
            assert!(
                s.try_borrow_mut().is_ok(),
                "VIEW_STORAGE is already borrowed when calling view.event()"
            );
        });
        assert!(
            self.target.owning_id().state().try_borrow_mut().is_ok(),
            "ViewState is already borrowed when calling view.event()"
        );
        let view = self.target.owning_id().view();
        assert!(
            view.try_borrow_mut().is_ok(),
            "View is already borrowed when calling view.event()"
        );

        // Track if propagation was stopped
        let mut stop_propagation = false;

        // Call view's event handler
        let view_result = view.borrow_mut().event(self);

        // Break early if stopImmediatePropagation was called
        if self.stop_immediate {
            return Outcome::Stop;
        }

        if view_result.is_stop() {
            stop_propagation = true;
        }

        // Run registered event listeners if the target is a view
        if self.target.is_view() {
            let listener_keys = self.event.listener_keys();
            for key in &listener_keys {
                let handlers = self
                    .target
                    .owning_id()
                    .state()
                    .borrow()
                    .event_listeners
                    .get(key)
                    .cloned();
                if let Some(handlers) = handlers {
                    for (handler, config) in handlers {
                        if !config.phases.matches(&self.phase) {
                            continue;
                        }
                        let result = (handler.borrow_mut())(self);
                        if result.is_stop() {
                            stop_propagation = true;
                        }
                        if self.stop_immediate {
                            return Outcome::Stop;
                        }
                    }
                }
            }
        }

        // After all listeners on this target have run, check if we should continue
        if stop_propagation {
            Outcome::Stop
        } else {
            Outcome::Continue
        }
    }

    // /// Set a drag preview that will be rendered under the pointer.
    // ///
    // /// The preview will be rendered above all other content and will follow the pointer.
    // ///
    // /// # Parameters
    // /// - `element_id`: The element to render as the drag preview (could be the dragged element itself,
    // ///   or a custom preview visual)
    // /// - `pointer_offset`: Where the pointer is relative to the preview, as percentages (0.0-100.0).
    // ///   E.g., (50., 50.) means the pointer grabbed the center.
    // ///
    // /// # Example
    // /// ```rust
    // /// Event::DragSource(DragSourceEvent::Start(e)) => {
    // ///     // Show a semi-transparent preview of this element following the cursor
    // ///     cx.set_drag_preview(cx.view_id, (0.5, 0.5));
    // /// }
    // /// ```
    // pub fn set_drag_preview(&mut self, visual_id: impl Into<VisualId>, pointer_offset: (Pct, Pct)) {
    //     self.window_state.dragging_preview = Some(DraggingPreview {
    //         element_id: element_id.into(),
    //         pointer_offset,
    //     });
    // }
}

// ============================================================================
// Construction & Context Creation
// ============================================================================

/// Construction and event context utilities.
///
/// Provides methods for creating a `GlobalEventCx` and converting dispatches
/// into localized `EventCx` instances with proper coordinate transformations.
impl<'a> GlobalEventCx<'a> {
    pub fn new(window_state: &'a mut WindowState, source: ElementId, event: Event) -> Self {
        Self {
            window_state,
            event,
            hit_path: None,
            dispatch: None,
            pending_events: SmallVec::new(),
            source,
        }
    }

    /// Create an `EventCx` for `dispatch`, run `dispatch_one`, and return the outcome.
    pub fn dispatch_event_cx(
        &mut self,
        dispatch: &Dispatch,
        triggered_by: Option<&Event>,
        default_prevented: &mut bool,
    ) -> Outcome {
        if self.event.is_pointer()
            && dispatch.target_element_id.is_view()
            && dispatch.target_element_id.owning_id().pointer_events_none()
        {
            return Outcome::Continue;
        }

        let world_transform = match self
            .window_state
            .box_tree
            .borrow()
            .world_transform(dispatch.target_element_id.0)
        {
            Ok(transform) => transform,
            Err(transform) => transform.value().unwrap(),
        }
        .inverse();

        let mut cx = EventCx {
            window_state: self.window_state,
            event: self.event.clone().transform(world_transform),
            world_transform,
            triggered_by,
            hit_path: self.hit_path.clone(),
            phase: dispatch.phase,
            target: dispatch.target_element_id,
            source: self.source,
            dispatch: self.dispatch.clone(),
            source_id: self.source,
            default_prevented,
            stop_immediate: false,
        };
        cx.dispatch_one()
    }
}

// ============================================================================
// Event Routing - Entry Points
// ============================================================================

/// Primary event routing entry points.
///
/// These methods serve as the main interfaces for routing events into the view tree.
/// `route_window_event` handles external window events, while `route` provides
/// a unified interface for different routing strategies.
impl<'a> GlobalEventCx<'a> {
    pub fn route_window_event(&mut self) {
        // Handle pointer capture (explicit routing during drags/gestures)
        match &self.event {
            Event::Pointer(pointer_event) => {
                // Check if this pointer has capture
                let pointer_id = pointer_event.pointer_info().pointer_id;
                let capture_target =
                    pointer_id.and_then(|id| self.window_state.get_pointer_capture_target(id));

                if let Some(capture_target) = capture_target {
                    let route_kind = RouteKind::Directed {
                        target: capture_target,
                        phases: Phases::STANDARD,
                    };
                    self.route(route_kind, OverrideKind::Normal { triggered_by: None });
                } else if let Some(point) = pointer_event.logical_point() {
                    let route_kind = RouteKind::Spatial {
                        point: Some(point),
                        phases: Phases::STANDARD,
                    };
                    self.route(route_kind, OverrideKind::Normal { triggered_by: None });
                } else {
                    // Pointer Enter / Leave / Cancel not routed without capture target but handled in default behaviors
                    self.handle_default_behaviors();
                }
            }
            Event::Key(_) => {
                let route_kind = RouteKind::Focused {
                    phases: Phases::all(),
                };
                // TODO: clarify semantics here of when to fallback to global. need shared default_prevented?
                self.route(route_kind, OverrideKind::Normal { triggered_by: None });
            }
            Event::Ime(_) => {
                let route_kind = RouteKind::Focused {
                    phases: Phases::STANDARD,
                };
                self.route(route_kind, OverrideKind::Normal { triggered_by: None });
            }
            Event::FileDrag(fde) => {
                let point = fde.logical_point();
                let route_kind = RouteKind::Spatial {
                    point: Some(point),
                    phases: Phases::TARGET,
                };
                self.route(route_kind, OverrideKind::Normal { triggered_by: None });
            }
            Event::Window(we) => {
                if matches!(we, WindowEvent::ChangeUnderCursor) {
                    self.update_hover_from_point(self.window_state.last_pointer.0);
                }
                let listener_keys = self.event.listener_keys();
                let mut interested: SmallVec<[ViewId; 64]> = SmallVec::new();
                for key in &listener_keys {
                    if let Some(ids) = self.window_state.listeners.get(key) {
                        for &id in ids {
                            if !interested.contains(&id) {
                                interested.push(id);
                            }
                        }
                    }
                }

                for id in interested {
                    let route_kind = RouteKind::Directed {
                        target: id.get_element_id(),
                        phases: Phases::TARGET,
                    };
                    self.route(route_kind, OverrideKind::Normal { triggered_by: None });
                }
            }
            Event::PointerCapture(_)
            | Event::Focus(_)
            | Event::Interaction(_)
            | Event::Drag(_)
            | Event::Custom(_)
            | Event::Extracted => {
                panic!(
                    "received {:?}, which is not an external event.",
                    &self.event
                );
            }
        }
    }

    /// Route a synthetic event (one generated internally, not from the OS).
    /// Synthetic events are routed with OverrideKind::Synthetic.
    pub fn route_synthetic(&mut self, kind: RouteKind, event: Event) {
        self.route(kind, OverrideKind::Synthetic { event });
    }

    /// Route a normal event (from the OS).
    /// Normal events are routed with OverrideKind::Normal.
    pub fn route_normal(&mut self, kind: RouteKind, triggered_by: Option<&Event>) {
        self.route(kind, OverrideKind::Normal { triggered_by });
    }

    pub fn route(&mut self, kind: RouteKind, mut override_kind: OverrideKind) -> Option<Dispatch> {
        // save state because route can be called recursively
        let (triggered_by, saved_hit_path) = match override_kind {
            OverrideKind::Normal { triggered_by } => (triggered_by, None),
            OverrideKind::Synthetic { ref mut event } => {
                std::mem::swap(&mut self.event, event);
                (Some(&*event), self.hit_path.clone())
            }
        };

        // set the visual hit path if the event has a point.
        // this might be overriden by route spatial but that's fine because hit tests are cached RC.
        if let Some(point) = self.event.point() {
            let path = hit_test(self.window_state.root_view_id, point);
            self.hit_path = path.clone();
        }

        let remaining_dispatch = match kind {
            RouteKind::Directed { target, phases } => {
                self.route_directed(target, triggered_by, phases)
            }
            RouteKind::Focused { phases } => {
                if let Some(focus) = self.window_state.focus_state.current_path().last() {
                    self.route_directed(*focus, triggered_by, phases)
                } else {
                    self.route_global(triggered_by, true);
                    None
                }
            }

            RouteKind::Spatial { point, phases } => {
                let point = point.or_else(|| self.event.point());
                if let Some(point) = point {
                    self.route_spatial(point, triggered_by, phases)
                } else {
                    None
                }
            }

            RouteKind::Subtree {
                target,
                respect_propagation,
            } => {
                self.route_subtree(target, triggered_by, respect_propagation);
                None
            }

            RouteKind::Broadcast {
                respect_propagation,
            } => {
                self.route_global(triggered_by, respect_propagation);
                None
            }
        };

        // Only handle default behaviors if preventDefault was not called (not currently checked)
        self.handle_default_behaviors();

        if let Event::Pointer(pe) = &self.event {
            if let Some(pointer_id) = pe.pointer_info().pointer_id {
                self.process_pending_pointer_capture(pointer_id);
            }
        }

        self.send_pending_events();
        assert!(
            self.pending_events.is_empty(),
            "all pending events should have been sent"
        );

        // Restore original event and hit_path if synthetic
        if let OverrideKind::Synthetic { event } = override_kind {
            self.event = event;
            self.hit_path = saved_hit_path;
        }

        remaining_dispatch
    }
}

// ============================================================================
// Routing Strategies
// ============================================================================

/// Event routing strategy implementations.
///
/// Each method implements a different routing strategy for delivering events to views:
/// - `route_directed`: Capture/bubble phases through ancestor chain to a specific target
/// - `route_to_target`: Direct delivery to a single target without propagation
/// - `route_spatial`: Hit-testing based routing for pointer events at a point
/// - `route_dom`: Broadcast to all views in tree order (no propagation respected)
impl<'a> GlobalEventCx<'a> {
    /// Route directed events (keyboard to focused view)
    pub(crate) fn route_directed(
        &mut self,
        target: ElementId,
        triggered_by: Option<&Event>,
        phases: crate::context::Phases,
    ) -> Option<Dispatch> {
        use crate::context::Phases;
        let default_prevented = &mut false;

        // handle pointer defaults
        if self.event.is_keyboard_trigger() {
            if let Some(focus) = self.window_state.focus_state.current_path().last() {
                // Keyboard trigger creates its own synthetic Click event
                self.pending_events.push((
                    RouteKind::Directed {
                        target: *focus,
                        phases: Phases::STANDARD,
                    },
                    Event::Interaction(InteractionEvent::Click),
                ));
            }
        }

        let extra_dispatch = if phases == Phases::TARGET {
            let d = Dispatch::target(target);
            let outcome = self.dispatch_event_cx(&d, triggered_by, default_prevented);
            if outcome.is_stop() { Some(d) } else { None }
        } else {
            let box_tree = self.window_state.box_tree.borrow();
            let iter =
                build_capture_bubble_path(target, &box_tree).filter(|d| phases.matches(&d.phase));
            drop(box_tree);

            run_dispatch(iter, |d| {
                self.dispatch_event_cx(&d, triggered_by, default_prevented)
            })
        };

        // the keyboard event was not handled
        if extra_dispatch.is_none() && phases.contains(Phases::BROADCAST) {
            self.route_global(triggered_by, true);
            None
        } else {
            extra_dispatch
        }
    }

    fn route_spatial(
        &mut self,
        point: Point,
        triggered_by: Option<&Event>,
        phases: crate::context::Phases,
    ) -> Option<Dispatch> {
        // clear keyboard navigation on pointer down
        if self.event.is_pointer_down() {
            self.window_state.keyboard_navigation = false;
        }

        // override the hit path because the spatial point might not be the event point
        let path = hit_test(self.window_state.root_view_id, point);
        self.hit_path = path.clone();

        // if hit off the window, use default which is empty path.
        let path = self.hit_path.clone().unwrap_or_default();
        if self.event.is_pointer() || self.event.is_file_drag() {
            // update hover with any path even empty
            self.update_hover_from_path(&path);
        }
        // on any pointer down update focus if something was hit
        if self.event.is_pointer_down() {
            // update focus
            if let Some(hit) = path.last().copied() {
                self.update_focus(hit, false);
            }
        }

        self.handle_pointer_state_updates();

        // now actually route spatial
        if let Some(target) = path.last() {
            self.route(
                RouteKind::Directed {
                    target: *target,
                    phases,
                },
                OverrideKind::Normal { triggered_by },
            )
        } else {
            None
        }
    }

    /// Route to a target and all its descendants (subtree)
    fn route_subtree(
        &mut self,
        target: ElementId,
        triggered_by: Option<&Event>,
        respect_propagation: bool,
    ) {
        let default_prevented = &mut false;
        let target_view_id = target.owning_id();
        self.route_tree_recursive(
            target_view_id,
            triggered_by,
            respect_propagation,
            default_prevented,
        );
    }

    /// Route events to all views in DOM order (global broadcast)
    fn route_global(&mut self, triggered_by: Option<&Event>, respect_propagation: bool) {
        let root = self.window_state.root_view_id.get_element_id();
        self.route_subtree(root, triggered_by, respect_propagation);
    }

    /// Recursively walk the tree and dispatch events.
    /// Used by both route_subtree and route_global.
    fn route_tree_recursive(
        &mut self,
        view_id: ViewId,
        triggered_by: Option<&Event>,
        respect_propagation: bool,
        default_prevented: &mut bool,
    ) {
        let d = Dispatch::target(view_id.get_element_id());
        let outcome = self.dispatch_event_cx(&d, triggered_by, default_prevented);

        if respect_propagation && outcome.is_stop() {
            return;
        }

        for child_id in view_id.children() {
            self.route_tree_recursive(
                child_id,
                triggered_by,
                respect_propagation,
                default_prevented,
            );
        }
    }
}

// ============================================================================
// Event Processing Lifecycle
// ============================================================================

/// Event processing lifecycle hooks.
///
/// Methods that run before and after event routing to handle state updates,
/// interaction tracking, and default browser-like behaviors (click detection,
/// drag thresholds, context menus, etc.).
impl<'a> GlobalEventCx<'a> {
    /// Handle default behaviors (drag, tab navigation, etc.)
    ///
    /// (Preventable things)
    fn handle_default_behaviors(&mut self) {
        // Pointer move - check threshold and handle active drag
        let pointer_move = match &self.event {
            Event::Pointer(PointerEvent::Move(pu)) => Some(pu.clone()),
            _ => None,
        };

        if let Some(pu) = pointer_move {
            let box_tree = self.window_state.box_tree.clone();
            // Check if pending drag exceeded threshold
            if let Some(drag_dispatch) = self
                .window_state
                .drag_tracker
                .check_threshold(&pu, &box_tree.borrow())
            {
                self.window_state.needs_box_tree_commit = true;
                match drag_dispatch {
                    DragEventDispatch::Source(source_id, drag_source_event) => {
                        self.route_synthetic(
                            RouteKind::Directed {
                                target: source_id,
                                phases: Phases::TARGET,
                            },
                            Event::Drag(DragEvent::Source(drag_source_event)),
                        );
                    }
                    DragEventDispatch::Target(target_id, drag_target_event) => {
                        // Use STANDARD phases for Move to allow bubbling
                        let phases = if drag_target_event.is_move() {
                            Phases::STANDARD
                        } else {
                            Phases::TARGET
                        };
                        self.route_synthetic(
                            RouteKind::Directed {
                                target: target_id,
                                phases,
                            },
                            Event::Drag(DragEvent::Target(drag_target_event)),
                        );
                    }
                }
            }
            // Handle move events for active drag
            if let Some(_active) = &self.window_state.drag_tracker.active_drag {
                self.window_state.needs_box_tree_from_layout = true;
                let hover_path = self
                    .hit_path
                    .as_ref()
                    .map(|p| p.iter().as_slice())
                    .unwrap_or(&[]);
                let drag_events = self
                    .window_state
                    .drag_tracker
                    .on_pointer_move(&pu, hover_path);
                for drag_event in drag_events {
                    match drag_event {
                        DragEventDispatch::Source(source_id, drag_source_event) => {
                            self.route_synthetic(
                                RouteKind::Directed {
                                    target: source_id,
                                    phases: Phases::TARGET,
                                },
                                Event::Drag(DragEvent::Source(drag_source_event)),
                            );
                        }
                        DragEventDispatch::Target(target_id, drag_target_event) => {
                            // Use STANDARD phases for Move to allow bubbling
                            let phases = if drag_target_event.is_move() {
                                Phases::STANDARD
                            } else {
                                Phases::TARGET
                            };
                            self.route_synthetic(
                                RouteKind::Directed {
                                    target: target_id,
                                    phases,
                                },
                                Event::Drag(DragEvent::Target(drag_target_event)),
                            );
                        }
                    }
                }
            }
        }

        // Pointer up - end drag and release capture
        let pe = match &self.event {
            Event::Pointer(PointerEvent::Up(pe)) => Some(pe.clone()),
            _ => None,
        };
        if let Some(pe) = pe {
            let drag_events = self.window_state.drag_tracker.on_pointer_up(&pe);
            for drag_event in drag_events {
                match drag_event {
                    DragEventDispatch::Source(source_id, drag_source_event) => {
                        self.route_synthetic(
                            RouteKind::Directed {
                                target: source_id,
                                phases: Phases::TARGET,
                            },
                            Event::Drag(DragEvent::Source(drag_source_event)),
                        );
                    }
                    DragEventDispatch::Target(target_id, drag_target_event) => {
                        // Use STANDARD phases for Move to allow bubbling
                        let phases = if drag_target_event.is_move() {
                            Phases::STANDARD
                        } else {
                            Phases::TARGET
                        };
                        self.route_synthetic(
                            RouteKind::Directed {
                                target: target_id,
                                phases,
                            },
                            Event::Drag(DragEvent::Target(drag_target_event)),
                        );
                    }
                }
            }
            // Auto-release pointer capture
            if let Some(pointer_id) = pe.pointer.pointer_id {
                self.window_state
                    .release_pointer_capture_unconditional(pointer_id);
            }
        }

        if let Event::Pointer(PointerEvent::Leave(_)) = &self.event {
            self.update_hover_from_path(&[]);
        }

        // Pointer cancel - abort drag
        let pi = match &self.event {
            Event::Pointer(PointerEvent::Cancel(pi)) => Some(*pi),
            _ => None,
        };
        if let Some(pi) = pi {
            let drag_events = self.window_state.drag_tracker.on_pointer_cancel(pi);
            for drag_event in drag_events {
                match drag_event {
                    DragEventDispatch::Source(target_id, drag_event) => {
                        self.route_synthetic(
                            RouteKind::Directed {
                                target: target_id,
                                phases: Phases::TARGET,
                            },
                            Event::Drag(DragEvent::Source(drag_event)),
                        );
                    }
                    DragEventDispatch::Target(target_id, drag_event) => {
                        self.route_synthetic(
                            RouteKind::Directed {
                                target: target_id,
                                phases: Phases::TARGET,
                            },
                            Event::Drag(DragEvent::Target(drag_event)),
                        );
                    }
                }
            }
            // Release capture on cancel
            if let Some(pointer_id) = pi.pointer_id {
                self.window_state
                    .release_pointer_capture_unconditional(pointer_id);
            }
        }

        // Tab navigation
        if let Event::Key(KeyboardEvent {
            key: Key::Named(NamedKey::Tab),
            modifiers,
            state: KeyState::Down,
            ..
        }) = &self.event
        {
            if modifiers.is_empty() || *modifiers == Modifiers::SHIFT {
                let backwards = modifiers.contains(Modifiers::SHIFT);
                self.view_tab_navigation(backwards);
            }
        }

        // Arrow navigation
        let arrow_key = match &self.event {
            Event::Key(KeyboardEvent {
                key:
                    Key::Named(
                        name @ (NamedKey::ArrowUp
                        | NamedKey::ArrowDown
                        | NamedKey::ArrowLeft
                        | NamedKey::ArrowRight),
                    ),
                modifiers,
                state: KeyState::Down,
                ..
            }) if *modifiers == Modifiers::ALT => Some(*name),
            _ => None,
        };
        if let Some(name) = arrow_key {
            self.view_arrow_navigation(&name);
        }

        // Window resized - mark responsive styles dirty
        if let Event::Window(WindowEvent::Resized(_)) = &self.event {
            // VIEW_STORAGE.with_borrow(|storage| {
            //     for view_id in storage.view_ids.keys() {
            //         self.window_state.style_dirty.insert(view_id);
            //     }
            // });
        }

        // Context/popout menus (platform-specific timing)
        let pbe = match &self.event {
            Event::Pointer(PointerEvent::Down(pbe)) if cfg!(target_os = "macos") => Some(pbe),
            Event::Pointer(PointerEvent::Up(pbe)) if !cfg!(target_os = "macos") => Some(pbe),
            _ => None,
        };
        if let Some(pbe) = pbe {
            self.handle_menu_events(&pbe.clone());
        }
    }

    fn handle_menu_events(&mut self, pbe: &PointerButtonEvent) {
        let Some(button) = pbe.button else { return };
        let Some(hit) = self
            .hit_path
            .as_ref()
            .and_then(|p| p.last().copied())
            .filter(|id| id.is_view())
        else {
            return;
        };

        let view_state = hit.owning_id().state();

        // Context menu on secondary button
        if button == PointerButton::Secondary {
            let context_menu = view_state.borrow().context_menu.clone();
            if let Some(menu) = context_menu {
                let position = pbe.state.logical_point();
                show_context_menu(menu(), Some(position));
                // we need to clear the click state after menus because winit can lose the on up event while the menu is active
                self.window_state.click_state.clear();
            }
        }

        // Popout menu on primary button
        if button == PointerButton::Primary {
            let popout_menu = view_state.borrow().popout_menu.clone();
            if let Some(menu) = popout_menu {
                let bounds = self
                    .window_state
                    .box_tree
                    .borrow()
                    .world_bounds(hit.owning_id().get_element_id().0)
                    .ok()
                    .unwrap_or_default();
                let bottom_left = Point::new(bounds.x0, bounds.y1);
                show_context_menu(menu(), Some(bottom_left));
                // we need to clear the click state after menus because winit can lose the on up event while the menu is active
                self.window_state.click_state.clear();
            }
        }
    }
}

/// =========================================================================
/// Pointer Capture Processing (inspired by Chromium's ProcessPendingPointerCapture)
/// =========================================================================
impl<'a> GlobalEventCx<'a> {
    /// Process pending pointer capture changes for a specific pointer.
    ///
    /// This implements Chromium's two-phase capture model:
    /// 1. Compare pending vs current capture state for the pointer
    /// 2. Fire `LostPointerCapture` to the old target (if any)
    /// 3. Move pending to active capture map
    /// 4. Fire `GainedPointerCapture` to the new target (if any)
    ///
    /// This ensures proper event ordering: lost fires before got.
    #[inline]
    pub(crate) fn process_pending_pointer_capture(&mut self, pointer_id: PointerId) {
        let current_target = self.window_state.get_pointer_capture_target(pointer_id);
        let pending_target = self.window_state.get_pending_capture_target(pointer_id);

        // No change in capture state
        if current_target == pending_target {
            return;
        }

        // Fire LostPointerCapture to the old target
        if let Some(old_target) = current_target {
            self.window_state.remove_active_capture(pointer_id);
            let event = Event::PointerCapture(PointerCaptureEvent::Lost(pointer_id));
            self.route_synthetic(
                RouteKind::Directed {
                    target: old_target,
                    phases: Phases::TARGET,
                },
                event,
            );
        }

        // Fire GainedPointerCapture to the new target
        if let Some(new_target) = pending_target {
            if !new_target.owning_id().is_hidden() {
                self.window_state.set_active_capture(pointer_id, new_target);
                let event = Event::PointerCapture(PointerCaptureEvent::Gained(super::DragToken(
                    pointer_id,
                )));
                self.route_synthetic(
                    RouteKind::Directed {
                        target: new_target,
                        phases: Phases::TARGET,
                    },
                    event,
                );

                // If the view was removed during the event handler, clean up
                if new_target.owning_id().is_hidden() {
                    self.window_state.remove_active_capture(pointer_id);
                    let event = Event::PointerCapture(PointerCaptureEvent::Lost(pointer_id));
                    self.route_synthetic(
                        RouteKind::Directed {
                            target: new_target,
                            phases: Phases::TARGET,
                        },
                        event,
                    );
                }
            }
        }
    }
}

// ============================================================================
// Focus Management
// ============================================================================

/// Focus state management and focus event dispatching.
///
/// Handles updating the focused element, building focus paths through the view tree,
/// and dispatching FocusGained/FocusLost/EnteredSubtree/LeftSubtree events.
/// Integrates with the style system to update :focus and :focus-visible selectors.
impl<'a> GlobalEventCx<'a> {
    /// Update focus to a new view, firing focus enter/leave events
    pub fn update_focus(&mut self, element_id: ElementId, keyboard_navigation: bool) {
        // Build path using router
        let path = {
            let box_tree = self.window_state.box_tree.borrow();
            build_focus_path(element_id, &box_tree, keyboard_navigation)
        };
        self.update_focus_from_path(&path, keyboard_navigation);
    }

    /// call this using a dom order path, not a visual path from hit testing.
    ///
    /// build a dom order path using lookup after hit testing in order to build a path
    pub fn update_focus_from_path(&mut self, path: &[ElementId], keyboard_navigation: bool) {
        // Update focus state and get enter/leave events
        let focus_events = self.window_state.focus_state.update_path(path);

        // Fire focus events
        for focus_event in focus_events {
            match focus_event {
                UnderFocusEvent::Enter(id) => {
                    if id.is_view() {
                        self.window_state.style_dirty.insert(id.owning_id());
                    }
                    self.pending_events.push((
                        RouteKind::Directed {
                            target: id,
                            phases: Phases::STANDARD,
                        },
                        Event::Focus(FocusEvent::Gained),
                    ));
                }
                UnderFocusEvent::Leave(id) => {
                    if id.is_view() {
                        self.window_state.style_dirty.insert(id.owning_id());
                    }
                    self.pending_events.push((
                        RouteKind::Directed {
                            target: id,
                            phases: Phases::STANDARD,
                        },
                        Event::Focus(FocusEvent::Lost),
                    ));
                }
            }
        }

        self.window_state.keyboard_navigation = keyboard_navigation;
    }
}

// ============================================================================
// Keyboard Navigation
// ============================================================================

/// Keyboard-driven focus navigation.
///
/// Implements Tab and Arrow key navigation using the understory_focus system
/// for spatial and sequential focus traversal. Handles both forward/backward
/// tab navigation and directional arrow key navigation.
impl<'a> GlobalEventCx<'a> {
    /// Tab navigation using understory_focus for spatial awareness
    pub(crate) fn view_tab_navigation(&mut self, backwards: bool) {
        // Get the focus scope root (could be enhanced to find actual scope boundaries)
        let scope_root = self.window_state.root_view_id.get_element_id();

        let current_focus = self
            .window_state
            .focus_state
            .current_path()
            .last()
            .cloned()
            // .unwrap_or_else(|| {
            //     self.window_state
            //         .box_tree
            //         .borrow()
            //         .hit_test_point(
            //             self.window_state.last_pointer.0,
            //             QueryFilter::new().visible(),
            //         )
            //         .map(|hit| dbg!(VisualId(hit.node, self.window_state.root_view_id)))
            // });
            .unwrap_or(scope_root);

        // TODO: replace with non-build_focus_space traversal
        let _ = (scope_root, current_focus, backwards);
    }

    pub(crate) fn view_arrow_navigation(&mut self, key: &NamedKey) {
        // TODO: replace with non-build_focus_space traversal
        let _ = key;
    }
}

// ============================================================================
// Hover State Management
// ============================================================================

/// Hover state tracking and pointer enter/leave event dispatching.
///
/// Manages which elements are currently under the pointer, dispatching
/// PointerEnter/PointerLeave events as the pointer moves and updating
/// the :hover style selector state.
impl<'a> GlobalEventCx<'a> {
    pub(crate) fn update_hover_from_point(&mut self, point: Point) {
        let path = hit_test(self.window_state.root_view_id, point);
        if let Some(path) = path {
            self.update_hover_from_path(&path);
        }
    }

    pub(crate) fn update_hover_from_path(&mut self, path: &[ElementId]) {
        if matches!(&self.event, Event::FileDrag(FileDragEvent::Drop(..))) {
            // Drop: clear hover state and dirty styles, but don't emit file-drag leave events
            let leave_events = self.window_state.hover_state.update_path(&[]);
            for event in leave_events {
                #[expect(irrefutable_let_patterns)]
                if let HoverEvent::Leave(target) | HoverEvent::Enter(target) = &event {
                    if target.is_view() {
                        self.window_state.style_dirty.insert(target.owning_id());
                    }
                }
            }
            // Re-enter with pointer enters
            let enter_events = self.window_state.hover_state.update_path(path);
            Self::push_hover_events(
                &mut self.pending_events,
                enter_events,
                false,
                self.window_state,
            );
        } else if matches!(&self.event, Event::FileDrag(FileDragEvent::Enter(..))) {
            // Drag start: emit pointer leaves then file-drag enters
            let leave_events = self.window_state.hover_state.update_path(&[]);
            Self::push_hover_events(
                &mut self.pending_events,
                leave_events,
                false, // pointer leaves
                self.window_state,
            );
            let enter_events = self.window_state.hover_state.update_path(path);
            Self::push_hover_events(
                &mut self.pending_events,
                enter_events,
                true, // file-drag enters
                self.window_state,
            );
        } else {
            let events = self.window_state.hover_state.update_path(path);
            let use_file_drag = self.window_state.file_drag_paths.is_some();
            Self::push_hover_events(
                &mut self.pending_events,
                events,
                use_file_drag,
                self.window_state,
            );
        }
        self.window_state.needs_cursor_resolution = true;
    }

    fn push_hover_events(
        pending: &mut SmallVec<[(RouteKind, Event); 8]>,
        events: impl IntoIterator<Item = HoverEvent<ElementId>>,
        use_file_drag: bool,
        window_state: &mut WindowState,
    ) {
        for event in events {
            match event {
                HoverEvent::Enter(target) => {
                    if target.is_view() {
                        window_state.style_dirty.insert(target.owning_id());
                    }
                    let event = if use_file_drag {
                        if let Some(paths) = &window_state.file_drag_paths {
                            Event::FileDrag(FileDragEvent::Enter(
                                crate::dropped_file::FileDragEnter {
                                    paths: paths.clone(),
                                    position: window_state.last_pointer.0,
                                },
                            ))
                        } else {
                            Event::Pointer(PointerEvent::Enter(window_state.last_pointer.1))
                        }
                    } else {
                        Event::Pointer(PointerEvent::Enter(window_state.last_pointer.1))
                    };
                    pending.push((
                        RouteKind::Directed {
                            target,
                            phases: Phases::TARGET,
                        },
                        event,
                    ));
                }
                HoverEvent::Leave(target) => {
                    if target.is_view() {
                        window_state.style_dirty.insert(target.owning_id());
                    }
                    let event = if use_file_drag {
                        Event::FileDrag(FileDragEvent::Leave(crate::dropped_file::FileDragLeave {
                            position: window_state.last_pointer.0,
                        }))
                    } else {
                        Event::Pointer(PointerEvent::Leave(window_state.last_pointer.1))
                    };
                    pending.push((
                        RouteKind::Directed {
                            target,
                            phases: Phases::TARGET,
                        },
                        event,
                    ));
                }
            }
        }
    }

    fn handle_pointer_state_updates(&mut self) {
        static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

        let Event::Pointer(pe) = &self.event else {
            return;
        };

        // for all other pointer events it must have a point and a path
        let Some((point, path)) = pe.logical_point().zip(self.hit_path.clone()) else {
            // cancel, enter or leave pointer events, no additional state to handle
            return;
        };

        match pe {
            PointerEvent::Down(PointerButtonEvent {
                button, pointer, ..
            }) => {
                // on pointer down, request style for the full hit path
                // TODO: make this only request style if the view has active selector
                for hit in path
                    .iter()
                    .filter(|id| id.is_view())
                    .map(|id| id.owning_id())
                {
                    self.window_state.style_dirty.insert(hit);
                }
                // TODO: click state should track count.
                self.window_state.click_state.on_down(
                    pointer.pointer_id.map(|p| p.get_inner()),
                    button.map(|b| b as u8),
                    path.clone(),
                    point,
                    Instant::now().duration_since(*START_TIME).as_millis() as u64,
                );
            }
            PointerEvent::Up(pbe @ PointerButtonEvent { button, state, .. }) => {
                self.handle_click_events(
                    &path,
                    point,
                    pbe.pointer.pointer_id,
                    *button,
                    state.count,
                );
            }
            PointerEvent::Cancel(PointerInfo { pointer_id, .. }) => {
                if let Some(canceled) = self
                    .window_state
                    .click_state
                    .cancel(pointer_id.map(|p| p.get_inner()))
                {
                    for target in canceled.target.iter() {
                        self.window_state.style_dirty.insert(target.owning_id());
                    }
                }
            }
            PointerEvent::Move(pu) => {
                self.window_state.last_pointer = (pu.current.logical_point(), pu.pointer);
                let exceeded_nodes = self.window_state.click_state.on_move(
                    pu.pointer.pointer_id.map(|p| p.get_inner()),
                    pu.current.logical_point(),
                );
                if let Some(element_ids) = exceeded_nodes {
                    for id in element_ids
                        .iter()
                        .filter(|id| id.is_view())
                        .map(|id| id.owning_id())
                    {
                        self.window_state.style_dirty.insert(id);
                    }
                }
            }

            _ => {}
        }
    }

    fn handle_click_events(
        &mut self,
        path: &Rc<[ElementId]>,
        point: Point,
        pointer_id: Option<PointerId>,
        button: Option<PointerButton>,
        count: u8,
    ) {
        let hit_path_len = path.len();
        let res = self.window_state.click_state.on_up(
            pointer_id.map(|p| p.get_inner()),
            button.map(|b| b as u8),
            path,
            point,
            Instant::now().duration_since(*START_TIME).as_millis() as u64,
        );

        for hit in path.iter() {
            hit.owning_id().request_style();
        }

        match res {
            ClickResult::Click(click_hit) => {
                self.push_interaction_events(
                    click_hit.last().copied(),
                    count,
                    button == Some(PointerButton::Secondary),
                );
            }
            ClickResult::Suppressed(Some(og_target)) => {
                let common_ancestor_idx = og_target
                    .iter()
                    .zip(path.iter())
                    .position(|(a, b)| a != b)
                    .unwrap_or(og_target.len().min(hit_path_len));
                if common_ancestor_idx > 0 {
                    let common_path = &path[..common_ancestor_idx];
                    self.push_interaction_events(
                        common_path.last().copied(),
                        count,
                        button == Some(PointerButton::Secondary),
                    );
                } else {
                    for target in og_target.iter() {
                        self.window_state.style_dirty.insert(target.owning_id());
                    }
                }
            }
            ClickResult::Suppressed(None) => {}
        }
    }

    fn push_interaction_events(&mut self, target: Option<ElementId>, count: u8, secondary: bool) {
        if let Some(id) = target {
            if id.is_view() {
                self.window_state.style_dirty.insert(id.owning_id());
            }
            let route_kind = RouteKind::Directed {
                target: id,
                phases: Phases::STANDARD,
            };
            if secondary {
                self.pending_events.push((
                    route_kind,
                    Event::Interaction(InteractionEvent::SecondaryClick),
                ));
            } else {
                self.pending_events
                    .push((route_kind, Event::Interaction(InteractionEvent::Click)));
                if count > 1 {
                    self.pending_events.push((
                        route_kind,
                        Event::Interaction(InteractionEvent::DoubleClick),
                    ));
                }
            }
        }
    }

    fn send_pending_events(&mut self) {
        while !self.pending_events.is_empty() {
            let pending = std::mem::take(&mut self.pending_events);

            for (route, event) in pending {
                self.route(route, OverrideKind::Synthetic { event });
                // New pending events will be processed in next iteration
            }
        }
    }
}
impl Drop for GlobalEventCx<'_> {
    fn drop(&mut self) {
        self.send_pending_events();
    }
}
