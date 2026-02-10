//! Event dispatch logic for handling events through the view tree.

#![allow(unused_imports)]
#![allow(unused)]
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    slice,
    sync::LazyLock,
    time::Instant,
};

use peniko::kurbo::Point;
use smallvec::SmallVec;
use ui_events::{
    keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey},
    pointer::{
        PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerState,
        PointerUpdate,
    },
};
use understory_box_tree::{NodeFlags, NodeId, QueryFilter};
use understory_event_state::{
    click::ClickResult,
    focus::FocusEvent as UnderFocusEvent,
    hover::{self, HoverEvent},
};
use understory_focus::{
    FocusPolicy, FocusProps, FocusSpace, FocusSymbol, adapters::box_tree::FocusPropsLookup,
};
use understory_responder::{
    dispatcher,
    router::{self, Router, path_from_dispatch},
    types::{
        DepthKey, Dispatch, Localizer, Outcome, ParentLookup, Phase as UnderPhase, ResolvedHit,
        ResolvedHitCow, ResolvedHitRef, WidgetLookup,
    },
};

use crate::{
    BoxTree, ElementId, ViewId,
    action::show_context_menu,
    context::*,
    event::{
        DragToken, Event, FocusEvent, ImeEvent, InteractionEvent, Phase, WindowEvent,
        drag_state::{DragEventDispatch, DraggingPreview},
        dropped_file::FileDragEvent,
        path::hit_test,
    },
    prelude::EventListenerTrait,
    style::{Focusable, PointerEvents, PointerEventsProp, StyleSelector},
    unit::Pct,
    view::{VIEW_STORAGE, View},
    window::{WindowState, tracking::is_known_root},
};

use super::PointerCaptureEvent;

pub(crate) type FloemDispatch = Dispatch<ElementId, ViewId, Option<()>>;

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
fn build_ancestor_chain(
    target: ElementId,
    root_view_id: ViewId,
    box_tree: &BoxTree,
) -> Vec<ElementId> {
    let mut path = Vec::new();
    let mut current = target;
    let mut visited = std::collections::HashSet::new();
    const MAX_DEPTH: usize = 1000; // Prevent runaway loops

    visited.insert(current.0);
    path.push(current);

    while let Some(parent_node) = box_tree.parent_of(current.0) {
        current = ElementId(parent_node, box_tree.meta(parent_node).flatten().unwrap());

        // Cycle detection
        if !visited.insert(current.0) || path.len() >= MAX_DEPTH {
            eprintln!("Warning: Detected cycle or excessive depth in box tree parent chain");
            break;
        }

        path.push(current);
    }

    path
}

/// Iterator that yields FloemDispatch items for capture/target/bubble phases.
///
/// Given an ancestor chain [target, parent1, parent2, ..., root], yields:
/// - Capture phase: root -> parent2 -> parent1 (excluding target)
/// - Target phase: target
/// - Bubble phase: parent1 -> parent2 (excluding target and root)
struct DispatchSequenceIter {
    ancestor_chain: Vec<ElementId>,
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
    fn new(ancestor_chain: Vec<ElementId>) -> Self {
        Self {
            ancestor_chain,
            phase: DispatchPhase::Capture,
            index: 0,
        }
    }
}

impl Iterator for DispatchSequenceIter {
    type Item = FloemDispatch;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ancestor_chain.is_empty() {
            return None;
        }

        match self.phase {
            DispatchPhase::Capture => {
                // Capture phase: root to parent (excluding target)
                // ancestor_chain = [target, parent1, parent2, ..., root]
                // We want: root, parent2, parent1
                let capture_count = self.ancestor_chain.len().saturating_sub(1);
                if self.index < capture_count {
                    let idx = self.ancestor_chain.len() - 1 - self.index;
                    let element_id = self.ancestor_chain[idx];
                    self.index += 1;
                    Some(FloemDispatch::capture(element_id).with_widget(element_id.owning_id()))
                } else {
                    // Move to target phase
                    self.phase = DispatchPhase::Target;
                    self.index = 0;
                    self.next()
                }
            }
            DispatchPhase::Target => {
                // Target phase: just the target
                self.phase = DispatchPhase::Bubble;
                let target = self.ancestor_chain[0];
                Some(FloemDispatch::target(target).with_widget(target.owning_id()))
            }
            DispatchPhase::Bubble => {
                // Bubble phase: parent1 to parent2 (excluding target and root)
                let bubble_count = self.ancestor_chain.len().saturating_sub(2);
                if self.index < bubble_count {
                    let idx = self.index + 1; // Skip target at index 0
                    let element_id = self.ancestor_chain[idx];
                    self.index += 1;
                    Some(FloemDispatch::bubble(element_id).with_widget(element_id.owning_id()))
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
fn build_capture_bubble_path(
    target: ElementId,
    root_view_id: ViewId,
    box_tree: &BoxTree,
) -> DispatchSequenceIter {
    let ancestor_chain = build_ancestor_chain(target, root_view_id, box_tree);
    DispatchSequenceIter::new(ancestor_chain)
}

// Keep these for now in case they're used elsewhere, but they're no longer needed for routing
pub(crate) struct BoxNodeLookup;
impl WidgetLookup<ElementId> for BoxNodeLookup {
    type WidgetId = ViewId;
    fn widget_of(&self, node: &ElementId) -> Option<Self::WidgetId> {
        Some(node.owning_id())
    }
}

pub(crate) struct BoxNodeParentLookup {
    root_view_id: ViewId,
    box_tree: Rc<RefCell<BoxTree>>,
}
impl ParentLookup<ElementId> for BoxNodeParentLookup {
    fn parent_of(&self, node: &ElementId) -> Option<ElementId> {
        let box_tree = self.box_tree.borrow();
        box_tree
            .parent_of(node.0)
            .map(|node| ElementId(node, box_tree.meta(node).flatten().unwrap()))
    }
}

// impl FocusPropsLookup<VisualId> for &mut WindowState {
//     fn props(&self, id: &VisualId) -> FocusProps {
//         FocusProps {
//             enabled: true,
//             order: None,
//             group: None,
//             autofocus: false,
//             policy_hint: None,
//         }
//     }
// }

/// Defines the routing strategy for dispatching events through the view tree.
///
/// Different event types require different routing behaviors. This enum encapsulates
/// the routing strategies used in Floem's event system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DispatchKind {
    /// Route to a specific target with customizable phases.
    /// - `Phases::CAPTURE | TARGET | BUBBLE` = full capture/bubble (browser-like)
    /// - `Phases::TARGET` = direct to target only
    /// - `Phases::BUBBLE` = bubble up through ancestors
    /// - `Phases::CAPTURE | TARGET` = capture down to target
    Directed {
        target: ElementId,
        phases: crate::context::Phases,
    },

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

    /// Route to the currently focused view with specified phases.
    /// If no view has focus, the event is not delivered.
    /// This is a convenience variant that automatically resolves to the focused target.
    Focused { phases: crate::context::Phases },

    /// Broadcast to all views in DOM order (global broadcast).
    /// If `respect_propagation` is true, propagation can be stopped by event handlers.
    /// If false, all views will receive the event regardless. Use for window-wide events
    /// that all views should receive.
    Global { respect_propagation: bool },
}

/// Event dispatch data containing routing strategy and event source.
///
/// This struct wraps a `DispatchKind` along with the source visual ID,
/// allowing the event system to track where events originate from.
#[derive(Debug, Clone)]
pub struct DispatchData {
    /// The routing strategy to use for this event
    pub kind: DispatchKind,
    /// The visual ID of the view that originated/sent this event.
    /// For window events, this is typically the root view.
    /// For user-emitted events, this is the view that called emit.
    pub source: ElementId,
}

pub(crate) struct GlobalEventCx<'a> {
    pub window_state: &'a mut WindowState,
    dispatch: Option<Rc<[FloemDispatch]>>,
    source_event: Option<Event>,
    hit_path: Option<Rc<[ElementId]>>,
    /// The visual ID that is the source of the current event being dispatched
    source: ElementId,
}

pub struct EventCx<'a> {
    pub window_state: &'a mut WindowState,
    pub untransformed_caused_by: Option<&'a Event>,
    /// An event that has been transformed to the local coordinate space of the target node
    pub event: Event,
    /// In the case that `event` is a synthetic event like `Click`, the caused by may contain information about the triggering event.
    pub caused_by: Option<Event>,
    /// If the event is a pointer event with a point, this contains the full set of visual ids that were under the pointer.
    pub hit_path: Option<Rc<[ElementId]>>,
    /// The event phase for this local event
    pub phase: Phase,
    /// The target of this event
    pub target: ElementId,
    /// The visual ID that is the source/origin of this event
    pub source: ElementId,
    pub dispatch: Option<Rc<[FloemDispatch]>>,
    pub view_id: ViewId,
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
    /// This should be called in response to `PointerCaptureEvent::Got`.
    /// The element must have pointer capture before requesting drag.
    /// The drag won't actually start until the pointer moves beyond the threshold.
    pub fn start_drag(
        &mut self,
        drag_token: DragToken,
        config: crate::event::DragConfig,
        use_default_preview: bool,
    ) -> bool {
        let Some(Event::Pointer(PointerEvent::Down(pbe))) = self.untransformed_caused_by else {
            return false;
        };

        let pointer_id = drag_token.pointer_id();

        let element_id = self.target;

        if self.window_state.get_pointer_capture_target(pointer_id) != Some(element_id) {
            return false;
        };

        self.window_state.drag_tracker.request_drag(
            self.view_id.get_element_id(),
            pointer_id,
            pbe.state.clone(),
            pbe.button,
            pbe.pointer,
            config,
            use_default_preview,
        )
    }

    fn dispatch_one(&mut self) -> Outcome {
        if self.view_id.is_disabled() && !self.event.allow_disabled() {
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
            self.view_id.state().try_borrow_mut().is_ok(),
            "ViewState is already borrowed when calling view.event()"
        );
        let view = self.view_id.view();
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

        // Run registered event listeners
        let handlers = self
            .view_id
            .state()
            .borrow()
            .event_listeners
            .get(&self.event.listener_key())
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

                // Break early if stopImmediatePropagation was called
                if self.stop_immediate {
                    return Outcome::Stop;
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
    pub fn new(window_state: &'a mut WindowState, source: ElementId) -> Self {
        Self {
            window_state,
            source_event: None,
            hit_path: None,
            dispatch: None,
            source,
        }
    }

    /// Set the caused_by event that will be available in EventCx
    pub fn set_caused_by(&mut self, event: Event) {
        self.source_event = Some(event);
    }

    pub fn event_cx<'b>(
        &'b mut self,
        dispatch: &FloemDispatch,
        event: &'b Event,
        default_prevented: &'b mut bool,
    ) -> EventCx<'b> {
        let view_id = dispatch
            .widget
            .expect("a widget must be associated with the dispatch or the event cannot be routed");
        let transform = match self
            .window_state
            .box_tree
            .borrow()
            .world_transform(dispatch.node.0)
        {
            Ok(transform) => transform,
            Err(transform) => transform.value().unwrap(),
        }
        .inverse();

        EventCx {
            window_state: self.window_state,
            untransformed_caused_by: self.source_event.as_ref(),
            event: event.clone().transform(transform),
            caused_by: self.source_event.clone().map(|c| c.transform(transform)),
            hit_path: self.hit_path.clone(),
            phase: dispatch.phase.into(),
            target: dispatch.node,
            source: self.source,
            dispatch: self.dispatch.clone(),
            view_id,
            default_prevented,
            stop_immediate: false,
        }
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
    pub fn route_window_event(&mut self, event: Event) {
        self.source_event = Some(event.clone());

        // One preventDefault flag per window event, shared across all phases
        let mut default_prevented = false;

        self.pre_route_external(&event, &mut default_prevented);

        // Handle pointer capture (explicit routing during drags/gestures)
        let event: &Event = &event;
        match event {
            Event::Pointer(pointer_event) => {
                // Check if this pointer has capture
                let pointer_id = pointer_event.pointer_info().pointer_id;
                let capture_target =
                    pointer_id.and_then(|id| self.window_state.get_pointer_capture_target(id));

                if let Some(capture_target) = capture_target {
                    use crate::context::Phases;
                    self.route_directed(
                        capture_target,
                        event,
                        Phases::all(),
                        &mut default_prevented,
                    );
                } else if let Some(point) = pointer_event.logical_point() {
                    use crate::context::Phases;
                    self.route_spatial(event, point, Phases::all(), &mut default_prevented)
                } else {
                    self.route_global(event, false, &mut default_prevented)
                }
            }
            Event::Key(_) | Event::Ime(_) => {
                if let Some(focus) = self.window_state.focus_state.current_path().last() {
                    use crate::context::Phases;
                    self.route_directed(*focus, event, Phases::all(), &mut default_prevented);
                } else {
                    self.route_global(event, false, &mut default_prevented);
                }
            }
            Event::FileDrag(fde) => {
                let point = fde.logical_point();
                use crate::context::Phases;
                self.route_spatial(event, point, Phases::all(), &mut default_prevented)
            }
            Event::Window(_) => self.route_global(event, false, &mut default_prevented),
            Event::PointerCapture(_)
            | Event::Focus(_)
            | Event::Interaction(_)
            | Event::DragTarget(_)
            | Event::DragSource(_)
            | Event::Custom(_)
            | Event::Extracted => {
                unreachable!("pointer capture is an internal event and doesn't have a route target")
            }
        }

        if let Event::Pointer(pe) = &event {
            if let Some(pointer_id) = pe.pointer_info().pointer_id {
                self.process_pending_pointer_capture(pointer_id);
            }
        }

        // Only handle default behaviors if preventDefault was not called
        if !default_prevented {
            self.handle_default_behaviors(event);
        }
    }

    pub fn route(&mut self, kind: DispatchKind, event: &Event) -> Option<FloemDispatch> {
        let mut default_prevented = false;

        match kind {
            DispatchKind::Directed { target, phases } => {
                self.route_directed(target, event, phases, &mut default_prevented)
            }

            DispatchKind::Spatial { point, phases } => {
                let point = point.or_else(|| event.point());
                if let Some(point) = point {
                    self.route_spatial(event, point, phases, &mut default_prevented);
                }
                None
            }

            DispatchKind::Subtree {
                target,
                respect_propagation,
            } => {
                self.route_subtree(target, event, respect_propagation, &mut default_prevented);
                None
            }

            DispatchKind::Focused { phases } => {
                if let Some(focus) = self.window_state.focus_state.current_path().last() {
                    self.route_directed(*focus, event, phases, &mut default_prevented)
                } else {
                    None
                }
            }

            DispatchKind::Global {
                respect_propagation,
            } => {
                self.route_global(event, respect_propagation, &mut default_prevented);
                None
            }
        }
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
        event: &Event,
        phases: crate::context::Phases,
        default_prevented: &mut bool,
    ) -> Option<FloemDispatch> {
        use crate::context::Phases;

        // Build dispatch sequence using custom path walking
        if phases == Phases::TARGET {
            // Direct to target only
            let seq = vec![FloemDispatch::target(target).with_widget(target.owning_id())];
            dispatcher::run(seq, self, |dispatch, event_cx| {
                event_cx
                    .event_cx(dispatch, event, default_prevented)
                    .dispatch_one()
            })
        } else {
            // Full capture/bubble path by walking up the box tree
            let box_tree = self.window_state.box_tree.borrow();
            let iter = build_capture_bubble_path(target, self.window_state.root_view_id, &box_tree);
            drop(box_tree);

            // No filtering needed for directed events
            dispatcher::run(iter, self, |dispatch, event_cx| {
                event_cx
                    .event_cx(dispatch, event, default_prevented)
                    .dispatch_one()
            })
        }
    }

    fn route_spatial(
        &mut self,
        event: &Event,
        point: Point,
        phases: crate::context::Phases,
        default_prevented: &mut bool,
    ) {
        let Some(path) = self.hit_path.clone() else {
            // No hit - clear hover state
            self.update_focus_from_path(&[], false);
            return;
        };

        // Hit path contains all elements at the point (including siblings).
        // The last element is the top-most target.
        let target = *path.last().unwrap();

        // Build dispatch sequence by walking up the parent chain from target to root
        let box_tree = self.window_state.box_tree.borrow();
        let iter = build_capture_bubble_path(target, self.window_state.root_view_id, &box_tree);
        drop(box_tree);

        // Filter out hidden views and views with pointer-events: none
        let filtered = iter.filter(|dispatch| {
            if let Some(widget) = dispatch.widget {
                !widget.is_hidden() && !widget.pointer_events_none()
            } else {
                true
            }
        });

        // Dispatch events
        dispatcher::run(filtered, self, |dispatch, event_cx| {
            let event = event.clone();
            event_cx
                .event_cx(dispatch, &event, default_prevented)
                .dispatch_one()
        });
    }

    /// Route to a target and all its descendants (subtree)
    fn route_subtree(
        &mut self,
        target: ElementId,
        event: &Event,
        respect_propagation: bool,
        default_prevented: &mut bool,
    ) {
        let target_view_id = target.owning_id();
        self.route_tree_recursive(
            target_view_id,
            event,
            respect_propagation,
            default_prevented,
        );
    }

    /// Route events to all views in DOM order (global broadcast)
    fn route_global(
        &mut self,
        event: &Event,
        respect_propagation: bool,
        default_prevented: &mut bool,
    ) {
        let root = self.window_state.root_view_id;
        self.route_tree_recursive(root, event, respect_propagation, default_prevented);
    }

    /// Recursively walk the tree and dispatch events - zero allocations
    /// Used by both route_subtree and route_global
    fn route_tree_recursive(
        &mut self,
        view_id: ViewId,
        event: &Event,
        respect_propagation: bool,
        default_prevented: &mut bool,
    ) {
        // Dispatch to current node (each view is the target, no phases)
        let dispatch = FloemDispatch {
            node: view_id.get_element_id(),
            widget: Some(view_id),
            phase: UnderPhase::Target,
            localizer: Localizer {},
            meta: None,
        };

        let outcome = self
            .event_cx(&dispatch, event, default_prevented)
            .dispatch_one();

        // If respecting propagation and event was stopped, don't continue to children
        if respect_propagation && matches!(outcome, Outcome::Stop) {
            return;
        }

        // Visit child VIEWS (not visual rectangles) to avoid infinite recursion
        // when a view has multiple visual rectangles
        for child_id in view_id.children() {
            self.route_tree_recursive(child_id, event, respect_propagation, default_prevented);
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
    /// Capture state before routing and clear sets for population during routing
    fn pre_route_external(&mut self, event: &Event, default_prevented: &mut bool) {
        let event = event.clone();
        let is_pointer_down = event.is_pointer_down();
        let is_pointer_up = event.is_pointer_up();
        let is_keyboard_trigger = event.is_keyboard_trigger();
        static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

        if let Event::Pointer(PointerEvent::Leave(info)) = event {
            self.update_hover_from_path(&[], self.window_state.last_pointer.0, info, &event);
        }

        let make_interaction_events = |count: u8, secondary: bool| -> &[Event] {
            if secondary {
                &[Event::Interaction(InteractionEvent::SecondaryClick)]
            } else if count > 1 {
                &[
                    Event::Interaction(InteractionEvent::Click),
                    Event::Interaction(InteractionEvent::DoubleClick),
                ]
            } else {
                &[Event::Interaction(InteractionEvent::Click)]
            }
        };

        if let Some(point) = event.point() {
            let path = hit_test(self.window_state.root_view_id, point).map(|v| v.1);
            self.hit_path = path;
            if self.hit_path.is_none() {
                // No hit - clear hover state
                self.update_focus_from_path(&[], false);
                self.window_state.click_state.clear();
                return;
            }
        }
        if let Event::Pointer(pe) = &event {
            let file_hover_events = self.window_state.file_hover_state.clear();
            for file_hover_event in file_hover_events {
                if let HoverEvent::Leave(element_id) = file_hover_event {
                    self.window_state.style_dirty.insert(element_id.owning_id());
                }
            }
            let Some(point) = pe.logical_point() else {
                return;
            };
            let Some(path) = self.hit_path.clone() else {
                return;
            };
            self.update_hover_from_path(
                &path,
                point,
                pe.pointer_info(),
                &Event::Pointer(pe.clone()),
            );
            match pe {
                PointerEvent::Down(PointerButtonEvent {
                    button, pointer, ..
                }) => {
                    // clear active on start of event handling pointer down
                    for hit in path.iter() {
                        hit.owning_id().request_style();
                    }
                    self.window_state.click_state.on_down(
                        pointer.pointer_id.map(|p| p.get_inner()),
                        button.map(|b| b as u8),
                        path.clone(),
                        point,
                        Instant::now().duration_since(*START_TIME).as_millis() as u64,
                    );
                }
                PointerEvent::Up(PointerButtonEvent {
                    button,
                    pointer,
                    state,
                }) => {
                    let hit_path_len = path.len();
                    let res = self.window_state.click_state.on_up(
                        pointer.pointer_id.map(|p| p.get_inner()),
                        button.map(|b| b as u8),
                        &path,
                        point,
                        Instant::now().duration_since(*START_TIME).as_millis() as u64,
                    );
                    for hit in path.iter() {
                        hit.owning_id().request_style();
                    }
                    match res {
                        ClickResult::Click(click_hit) => {
                            let events = make_interaction_events(
                                state.count,
                                *button == Some(PointerButton::Secondary),
                            );
                            if let Some(hit) = click_hit.last().copied() {
                                // Each synthetic event gets its own preventDefault flag
                                for event in events {
                                    use crate::context::Phases;
                                    let mut synthetic_prevented = false;
                                    self.route_directed(
                                        hit,
                                        event,
                                        Phases::all(),
                                        &mut synthetic_prevented,
                                    );
                                }
                            }
                        }
                        ClickResult::Suppressed(Some(og_target)) => {
                            let common_ancestor_idx = og_target
                                .iter()
                                .zip(path.iter())
                                .position(|(a, b)| a != b)
                                .unwrap_or(og_target.len().min(hit_path_len));
                            if common_ancestor_idx > 0 {
                                let common_path = &path[..common_ancestor_idx];
                                let events = make_interaction_events(
                                    state.count,
                                    *button == Some(PointerButton::Secondary),
                                );
                                if let Some(hit) = common_path.last().copied() {
                                    // Each synthetic event gets its own preventDefault flag
                                    for event in events {
                                        use crate::context::Phases;
                                        let mut synthetic_prevented = false;
                                        self.route_directed(
                                            hit,
                                            event,
                                            Phases::all(),
                                            &mut synthetic_prevented,
                                        );
                                    }
                                }
                            } else if let Some(target) = og_target.last() {
                                target.owning_id().request_style_recursive();
                            }
                        }
                        ClickResult::Suppressed(None) => {}
                    }
                }
                PointerEvent::Cancel(PointerInfo { pointer_id, .. }) => {
                    self.window_state
                        .click_state
                        .cancel(pointer_id.map(|p| p.get_inner()));
                }
                PointerEvent::Move(pu) => {
                    self.window_state.last_pointer = (pu.current.logical_point(), pu.pointer);
                    let exceeded_nodes = self.window_state.click_state.on_move(
                        pu.pointer.pointer_id.map(|p| p.get_inner()),
                        pu.current.logical_point(),
                    );
                    if let Some(element_ids) = exceeded_nodes {
                        for element_id in element_ids.iter() {
                            element_id.owning_id().request_style();
                        }
                    }
                }

                _ => {}
            }
        }
        // File drag hover tracking - emit Enter/Leave events to individual elements
        if let Event::FileDrag(FileDragEvent::Move(file_drag_move)) = event {
            if let Some(path) = &self.hit_path {
                let hover_events = self.window_state.file_hover_state.update_path(path);
                for hover_event in hover_events {
                    match hover_event {
                        HoverEvent::Enter(element_id) => {
                            self.window_state.style_dirty.insert(element_id.owning_id());
                            // Emit Enter event to the element
                            let enter_event = Event::FileDrag(FileDragEvent::Enter(
                                crate::event::dropped_file::FileDragEnter {
                                    paths: file_drag_move.paths.clone(),
                                    position: file_drag_move.position,
                                },
                            ));
                            self.route_directed(
                                element_id,
                                &enter_event,
                                Phases::all(),
                                &mut false,
                            );
                        }
                        HoverEvent::Leave(element_id) => {
                            self.window_state.style_dirty.insert(element_id.owning_id());
                            // Emit Leave event to the element
                            let leave_event = Event::FileDrag(FileDragEvent::Leave(
                                crate::event::dropped_file::FileDragLeave {
                                    position: file_drag_move.position,
                                },
                            ));
                            self.route_directed(
                                element_id,
                                &leave_event,
                                Phases::all(),
                                &mut false,
                            );
                        }
                    }
                }
            }
        }

        if is_keyboard_trigger {
            if let Some(focus) = self.window_state.focus_state.current_path().last() {
                // Keyboard trigger creates its own synthetic Click event
                use crate::context::Phases;
                self.route_directed(
                    *focus,
                    &Event::Interaction(InteractionEvent::Click),
                    Phases::all(),
                    &mut false,
                );
            }
        }

        if is_pointer_down {
            self.window_state.keyboard_navigation = false;
            if let Some(focus) = self.window_state.focus_state.current_path().last() {
                if let Some(view_id) = focus.exact_view_id() {
                    self.window_state.style_dirty.insert(view_id);
                }
            }
        }
    }

    /// Handle default behaviors (focus, click, drag, etc.)
    fn handle_default_behaviors(&mut self, event: &Event) {
        if let Event::Pointer(PointerEvent::Down(_)) = event {
            if let Some(hit) = self.hit_path.as_ref().and_then(|p| p.last().copied()) {
                self.update_focus(hit, false);
            }
        }

        // Pointer move - check threshold and handle active drag
        if let Event::Pointer(PointerEvent::Move(pu)) = event {
            let box_tree = self.window_state.box_tree.clone();
            // Check if pending drag exceeded threshold
            if let Some(drag_dispatch) = self
                .window_state
                .drag_tracker
                .check_threshold(pu, &box_tree.borrow())
            {
                self.window_state.needs_box_tree_commit = true;
                match drag_dispatch {
                    DragEventDispatch::Source(source_id, drag_source_event) => {
                        self.event_cx(
                            &FloemDispatch::target(source_id).with_widget(source_id.owning_id()),
                            &Event::DragSource(drag_source_event),
                            &mut false,
                        )
                        .dispatch_one();
                    }
                    DragEventDispatch::Target(target_id, drag_target_event) => {
                        self.event_cx(
                            &FloemDispatch::target(target_id).with_widget(target_id.owning_id()),
                            &Event::DragTarget(drag_target_event),
                            &mut false,
                        )
                        .dispatch_one();
                    }
                }
            }

            // Handle move events for active drag
            if let Some(active) = &self.window_state.drag_tracker.active_drag {
                self.window_state.needs_box_tree_from_layout = true;
                let hover_path = self
                    .hit_path
                    .as_ref()
                    .map(|p| p.iter().as_slice())
                    .unwrap_or(&[]);

                // Split at the dragged element

                let drag_events = self
                    .window_state
                    .drag_tracker
                    .on_pointer_move(pu, hover_path);
                for drag_event in drag_events {
                    match drag_event {
                        DragEventDispatch::Source(source_id, drag_source_event) => {
                            self.event_cx(
                                &FloemDispatch::target(source_id)
                                    .with_widget(source_id.owning_id()),
                                &Event::DragSource(drag_source_event),
                                &mut false,
                            )
                            .dispatch_one();
                        }
                        DragEventDispatch::Target(target_id, drag_target_event) => {
                            let view_id = target_id.owning_id();
                            self.event_cx(
                                &FloemDispatch::target(target_id).with_widget(view_id),
                                &Event::DragTarget(drag_target_event),
                                &mut false,
                            )
                            .dispatch_one();
                        }
                    }
                }
            }
        }

        // Pointer up - end drag and release capture
        if let Event::Pointer(PointerEvent::Up(pe)) = event {
            let drag_events = self.window_state.drag_tracker.on_pointer_up(pe);

            for drag_event in drag_events {
                match drag_event {
                    DragEventDispatch::Source(source_id, drag_source_event) => {
                        self.event_cx(
                            &FloemDispatch::target(source_id).with_widget(source_id.owning_id()),
                            &Event::DragSource(drag_source_event),
                            &mut false,
                        )
                        .dispatch_one();
                    }
                    DragEventDispatch::Target(target_id, drag_target_event) => {
                        self.event_cx(
                            &FloemDispatch::target(target_id).with_widget(target_id.owning_id()),
                            &Event::DragTarget(drag_target_event),
                            &mut false,
                        )
                        .dispatch_one();
                    }
                }
            }

            // Auto-release pointer capture
            if let Some(pointer_id) = pe.pointer.pointer_id {
                self.window_state
                    .release_pointer_capture_unconditional(pointer_id);
            }
        }

        // Pointer cancel - abort drag
        if let Event::Pointer(PointerEvent::Cancel(pi)) = event {
            let drag_events = self.window_state.drag_tracker.on_pointer_cancel(*pi);

            for drag_event in drag_events {
                match drag_event {
                    DragEventDispatch::Source(target_id, drag_event) => {
                        self.event_cx(
                            &FloemDispatch::target(target_id).with_widget(target_id.owning_id()),
                            &Event::DragSource(drag_event),
                            &mut false,
                        )
                        .dispatch_one();
                    }
                    DragEventDispatch::Target(target_id, drag_event) => {
                        self.event_cx(
                            &FloemDispatch::target(target_id).with_widget(target_id.owning_id()),
                            &Event::DragTarget(drag_event),
                            &mut false,
                        )
                        .dispatch_one();
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
        }) = event
        {
            if modifiers.is_empty() || *modifiers == Modifiers::SHIFT {
                let backwards = modifiers.contains(Modifiers::SHIFT);
                self.view_tab_navigation(backwards);
            }
        }

        // Arrow navigation
        if let Event::Key(KeyboardEvent {
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
        }) = event
        {
            if *modifiers == Modifiers::ALT {
                self.view_arrow_navigation(name);
            }
        }

        // Window resized - mark responsive styles dirty
        if let Event::Window(WindowEvent::Resized(_)) = event {
            VIEW_STORAGE.with_borrow(|storage| {
                for view_id in storage.view_ids.keys() {
                    self.window_state.style_dirty.insert(view_id);
                }
            });
        }

        // Context/popout menus (platform-specific timing)
        if cfg!(target_os = "macos") {
            if let Event::Pointer(PointerEvent::Down(pbe)) = event {
                self.handle_menu_events(pbe);
            }
        }
        if !cfg!(target_os = "macos") {
            if let Event::Pointer(PointerEvent::Up(pbe)) = event {
                self.handle_menu_events(pbe);
            }
        }
    }

    fn handle_menu_events(&mut self, pbe: &PointerButtonEvent) {
        let Some(button) = pbe.button else { return };
        let Some(hit) = self.hit_path.as_ref().and_then(|p| p.last().copied()) else {
            return;
        };
        let Some(view_id) = hit.exact_view_id() else {
            return;
        };

        let view_state = view_id.state();

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
                    .world_bounds(view_id.get_element_id().0)
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
    /// 4. Fire `GotPointerCapture` to the new target (if any)
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
            self.event_cx(
                &FloemDispatch::target(old_target).with_widget(old_target.owning_id()),
                &event,
                &mut false,
            )
            .dispatch_one();
        }

        // Fire GotPointerCapture to the new target
        if let Some(new_target) = pending_target {
            // Only set capture if the view is still connected
            if !new_target.owning_id().is_hidden() {
                self.window_state.set_active_capture(pointer_id, new_target);
                let event =
                    Event::PointerCapture(PointerCaptureEvent::Got(super::DragToken(pointer_id)));
                self.event_cx(
                    &FloemDispatch::target(new_target).with_widget(new_target.owning_id()),
                    &event,
                    &mut false,
                )
                .dispatch_one();

                // If the view was removed during the event handler, clean up
                if new_target.owning_id().is_hidden() {
                    self.window_state.remove_active_capture(pointer_id);
                    let event = Event::PointerCapture(PointerCaptureEvent::Lost(pointer_id));
                    self.event_cx(
                        &FloemDispatch::target(new_target).with_widget(new_target.owning_id()),
                        &event,
                        &mut false,
                    )
                    .dispatch_one();
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
        let box_tree = self.window_state.box_tree.clone();
        let mut router = Router::with_parent(
            BoxNodeLookup,
            BoxNodeParentLookup {
                root_view_id: self.window_state.root_view_id,
                box_tree: box_tree.clone(),
            },
        );
        router.set_scope(Some(Box::new({
            let box_tree = box_tree.clone();
            move |id| {
                box_tree
                    .borrow()
                    .flags(id.0)
                    .map(|f| {
                        (f.contains(NodeFlags::FOCUSABLE | NodeFlags::VISIBLE) || *id == element_id)
                            && !id.owning_id().is_disabled()
                    })
                    .unwrap_or(false)
            }
        })));
        let seq = router.dispatch_for::<()>(element_id);
        let path = router::path_from_dispatch(&seq);
        self.update_focus_from_path(&path, keyboard_navigation);
    }

    /// call this using a dom order path, not a visual path from hit testing.
    ///
    /// build a dom order path using lookup after hit testing in order to build a path
    pub fn update_focus_from_path(&mut self, path: &[ElementId], keyboard_navigation: bool) {
        self.window_state
            .focus_state
            .current_path()
            .last()
            .map(|id| id.owning_id())
            .map(|id| self.window_state.style_dirty.insert(id))
            .iter()
            .count();

        // Update focus state and get enter/leave events
        let old_target = self.window_state.focus_state.current_path().last().copied();
        let new_target = path.last().copied();
        let focus_events = self.window_state.focus_state.update_path(path);

        // Fire focus events
        for focus_event in focus_events {
            match focus_event {
                UnderFocusEvent::Enter(id) => {
                    if let Some(view_id) = id.exact_view_id() {
                        view_id.request_style_for_selector_recursive(StyleSelector::Focus);
                        if self
                            .window_state
                            .has_style_for_sel(view_id, StyleSelector::FocusVisible)
                        {
                            view_id
                                .request_style_for_selector_recursive(StyleSelector::FocusVisible);
                        }
                    }
                    if Some(id) == new_target {
                        // This is the actual focus target
                        let mut focus_prevented = false;
                        self.event_cx(
                            &FloemDispatch::target(id).with_widget(id.owning_id()),
                            &Event::Focus(FocusEvent::Got),
                            &mut focus_prevented,
                        )
                        .dispatch_one();
                    } else {
                        // This is an ancestor - subtree notification
                        let mut focus_prevented = false;
                        self.event_cx(
                            &FloemDispatch::target(id).with_widget(id.owning_id()),
                            &Event::Focus(FocusEvent::EnteredSubtree),
                            &mut focus_prevented,
                        )
                        .dispatch_one();
                    }
                }
                UnderFocusEvent::Leave(id) => {
                    if let Some(view_id) = id.exact_view_id() {
                        if self
                            .window_state
                            .has_style_for_sel(view_id, StyleSelector::Focus)
                        {
                            view_id.request_style_for_selector_recursive(StyleSelector::Focus);
                        }
                        if self
                            .window_state
                            .has_style_for_sel(view_id, StyleSelector::FocusVisible)
                        {
                            view_id
                                .request_style_for_selector_recursive(StyleSelector::FocusVisible);
                        }
                    }
                    if Some(id) == old_target {
                        // This is the element losing focus
                        let mut focus_prevented = false;
                        self.event_cx(
                            &FloemDispatch::target(id).with_widget(id.owning_id()),
                            &Event::Focus(FocusEvent::Lost),
                            &mut focus_prevented,
                        )
                        .dispatch_one();
                    } else {
                        // This is an ancestor - subtree notification
                        let mut focus_prevented = false;
                        self.event_cx(
                            &FloemDispatch::target(id).with_widget(id.owning_id()),
                            &Event::Focus(FocusEvent::LeftSubtree),
                            &mut focus_prevented,
                        )
                        .dispatch_one();
                    }
                }
            }
        }

        self.window_state
            .focus_state
            .current_path()
            .last()
            .and_then(|id| id.exact_view_id())
            .map(|n| self.window_state.style_dirty.insert(n));

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

        // Build focus space
        // TODO: retain this? if there are benefits to doing so
        let mut focus_entries = Vec::new();
        let box_tree = self.window_state.box_tree.clone();
        let box_tree = box_tree.borrow();

        let focus_space = understory_focus::adapters::box_tree::build_focus_space_for_scope(
            &box_tree,
            scope_root.0,
            &(),
            &mut focus_entries,
        );

        // Use the default policy for tab navigation
        let policy = understory_focus::DefaultPolicy {
            wrap: understory_focus::WrapMode::Scope,
        };

        let navigation = if backwards {
            understory_focus::Navigation::Prev
        } else {
            understory_focus::Navigation::Next
        };

        if let Some(new_focus) = policy.next(current_focus.0, navigation, &focus_space) {
            self.update_focus(
                ElementId(new_focus, box_tree.meta(new_focus).flatten().unwrap()),
                true,
            );
        }
    }

    pub(crate) fn view_arrow_navigation(&mut self, key: &NamedKey) {
        let scope_root = self.window_state.root_view_id.get_element_id();
        let current_focus = match self.window_state.focus_state.current_path().last().cloned() {
            Some(id) => id,
            None => {
                // No current focus, do tab navigation instead
                let backwards = matches!(key, NamedKey::ArrowUp | NamedKey::ArrowLeft);
                self.view_tab_navigation(backwards);
                return;
            }
        };

        let mut focus_entries = Vec::new();
        let box_tree = self.window_state.box_tree.clone();
        let box_tree = box_tree.borrow();
        let focus_space = understory_focus::adapters::box_tree::build_focus_space_for_scope(
            &box_tree,
            scope_root.0,
            &(),
            &mut focus_entries,
        );

        let policy = understory_focus::DefaultPolicy {
            wrap: understory_focus::WrapMode::Never, // Don't wrap on arrow keys
        };

        let navigation = match key {
            NamedKey::ArrowUp => understory_focus::Navigation::Up,
            NamedKey::ArrowDown => understory_focus::Navigation::Down,
            NamedKey::ArrowLeft => understory_focus::Navigation::Left,
            NamedKey::ArrowRight => understory_focus::Navigation::Right,
            _ => return,
        };

        if let Some(new_focus) = policy.next(current_focus.0, navigation, &focus_space) {
            self.update_focus(
                ElementId(new_focus, box_tree.meta(new_focus).flatten().unwrap()),
                true,
            );
        }
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
    pub(crate) fn update_hover_from_point(
        &mut self,
        point: Point,
        pointer: PointerInfo,
        event: &Event,
    ) {
        let mut router = Router::with_parent(
            BoxNodeLookup,
            BoxNodeParentLookup {
                root_view_id: self.window_state.root_view_id,
                box_tree: self.window_state.box_tree.clone(),
            },
        );
        let root = self.window_state.root_view_id;

        let Some((target, path)) = hit_test(root, point) else {
            // No hit - clear hover state
            let hover_events = self.window_state.hover_state.clear();
            for hover_event in hover_events {
                if let HoverEvent::Leave(box_node) = hover_event {
                    box_node.owning_id().request_style();
                }
            }
            return;
        };

        router.set_scope(Some(Box::new(|id| {
            let view_id = id.owning_id();
            !view_id.is_hidden() && !view_id.pointer_events_none()
        })));

        let resolved = ResolvedHit {
            node: target,
            path: Some((&*path).into()),
            depth_key: DepthKey::Z(0),
            localizer: Localizer::default(),
            meta: None::<()>,
        };

        let seq = router.handle_with_hits(&[resolved]);
        // Build hover path using router
        let hover_path = router::path_from_dispatch(&seq);
        self.update_hover_from_path(&hover_path, point, pointer, event);
    }

    pub(crate) fn update_hover_from_path(
        &mut self,
        path: &[ElementId],
        point: Point,
        pointer: PointerInfo,
        event: &Event,
    ) {
        let request_hover = |id: ElementId, window_state: &WindowState| {
            id.owning_id().request_style();
        };
        let events = self.window_state.hover_state.update_path(path);
        for hover_event in events {
            match hover_event {
                HoverEvent::Enter(id) => {
                    let view_id = id.owning_id();
                    request_hover(id, self.window_state);
                    let (point, pointer) = self.window_state.last_pointer;
                    let mut hover_prevented = false;
                    self.event_cx(
                        &FloemDispatch::target(id).with_widget(view_id),
                        &Event::Pointer(PointerEvent::Enter(pointer)),
                        &mut hover_prevented,
                    )
                    .dispatch_one();
                }
                HoverEvent::Leave(id) => {
                    let view_id = id.owning_id();
                    request_hover(id, self.window_state);
                    let (point, pointer) = self.window_state.last_pointer;
                    let mut hover_prevented = false;
                    self.event_cx(
                        &FloemDispatch::target(id).with_widget(view_id),
                        &Event::Pointer(PointerEvent::Leave(pointer)),
                        &mut hover_prevented,
                    )
                    .dispatch_one();
                }
            }
        }

        self.window_state.needs_cursor_resolution = true;
    }
}
