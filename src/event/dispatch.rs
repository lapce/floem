//! Event dispatch logic for handling events through the view tree.

use std::{rc::Rc, sync::LazyLock, time::Instant};

use peniko::kurbo::{Affine, Point};
use smallvec::SmallVec;
use ui_events::{
    keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey},
    pointer::{PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo},
};
use understory_box_tree::NodeFlags;
use understory_event_state::{click::ClickResult, hover::HoverEvent};
use winit::keyboard::KeyCode;

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
fn build_ancestor_chain(target: ElementId, box_tree: &BoxTree) -> SmallVec<[ElementId; 64]> {
    let mut path = SmallVec::new();
    let mut current = target;
    let mut visited = std::collections::HashSet::new();
    const MAX_DEPTH: usize = 1000;

    visited.insert(current.0);
    path.push(current);

    while let Some(parent_node) = box_tree.parent_of(current.0) {
        current = box_tree.meta(parent_node).flatten().unwrap();

        if !visited.insert(current.0) || path.len() >= MAX_DEPTH {
            eprintln!("Warning: Detected cycle or excessive depth in box tree parent chain");
            break;
        }

        path.push(current);
    }

    path
}

/// Iterator that yields `Dispatch` items for capture/target/bubble phases.
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
                let bubble_count = self.ancestor_chain.len().saturating_sub(2);
                if self.index < bubble_count {
                    let idx = self.index + 1;
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

fn build_capture_bubble_path(target: ElementId, box_tree: &BoxTree) -> DispatchSequenceIter {
    let ancestor_chain = build_ancestor_chain(target, box_tree);
    DispatchSequenceIter::new(ancestor_chain)
}

/// Defines the routing strategy for dispatching events through the view tree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RouteKind {
    /// Route to a specific target with customizable phases.
    Directed {
        target: ElementId,
        phases: crate::context::Phases,
    },

    /// Route to the currently focused view with specified phases.
    Focused { phases: crate::context::Phases },

    /// Route based on spatial hit testing at a point (pointer events).
    Spatial {
        point: Option<Point>,
        phases: crate::context::Phases,
    },

    /// Route to a target and all its descendants.
    Subtree {
        target: ElementId,
        respect_propagation: bool,
    },

    /// Broadcast to all views in DOM order (global broadcast).
    Broadcast { respect_propagation: bool },
}

/// Event routing data containing routing strategy and event source.
#[derive(Debug, Clone)]
pub struct RouteData {
    pub kind: RouteKind,
    pub source: ElementId,
}

/// Controls whether an event overrides the current event (synthetic) or uses it (normal).
#[expect(clippy::large_enum_variant)]
pub(crate) enum OverrideKind {
    /// Use the global event as-is. No triggered_by.
    Normal,
    /// Replace the event with a synthetic one. Carries the event that caused it.
    Synthetic {
        event: Event,
        triggered_by: Option<Event>,
    },
}

// ============================================================================
// Tier 1: GlobalEventCx — window-event-level state
// ============================================================================

/// Window-event-level context. Holds state that is truly global for the duration
/// of one OS event dispatch. Routing and per-event state lives in [`RouteCx`].
pub(crate) struct GlobalEventCx<'a> {
    pub window_state: &'a mut WindowState,
    /// The original OS event being processed.
    event: Event,
    /// The source element for the original event (usually the window root).
    source: ElementId,
}

impl<'a> GlobalEventCx<'a> {
    pub fn new(window_state: &'a mut WindowState, source: ElementId, event: Event) -> Self {
        Self {
            window_state,
            event,
            source,
        }
    }

    /// Route using the original OS event, with an optional causal event.
    pub fn route_normal(&mut self, kind: RouteKind, _triggered_by: Option<&Event>) {
        self.route(kind, OverrideKind::Normal);
    }

    /// Core routing entry point. Creates a [`RouteCx`] scope for this event,
    /// dispatches it, then runs lifecycle hooks on drop.
    pub fn route(&mut self, kind: RouteKind, override_kind: OverrideKind) -> Option<Dispatch> {
        let mut rcx = RouteCx::new(self, &kind, override_kind);
        rcx.dispatch(kind)
        // RouteCx drops here → finish() → handle_default_behaviors,
        // process_pending_pointer_capture, flush_pending_events
    }

    /// Route the original OS window event. This is the primary entry point
    /// called once per OS event from the window handle.
    pub fn route_window_event(&mut self) {
        match &self.event {
            Event::Pointer(pointer_event) => {
                // Pointer leave — clear all hover state.
                let pointer_id = pointer_event.pointer_info().pointer_id;
                let capture_target =
                    pointer_id.and_then(|id| self.window_state.get_pointer_capture_target(id));
                if let Some(point) = pointer_event.logical_point() {
                    self.window_state.last_pointer = (point, pointer_event.pointer_info())
                }

                if let Some(capture_target) = capture_target {
                    self.route(
                        RouteKind::Directed {
                            target: capture_target,
                            phases: Phases::STANDARD,
                        },
                        OverrideKind::Normal,
                    );
                } else if let Some(point) = pointer_event.logical_point() {
                    self.route(
                        RouteKind::Spatial {
                            point: Some(point),
                            phases: Phases::STANDARD,
                        },
                        OverrideKind::Normal,
                    );
                } else {
                    // Pointer Enter / Leave / Cancel with no capture and no point:
                    // not routed to any view, but lifecycle still runs (hover clearing, etc.)
                    self.route_lifecycle_only();
                }
            }
            Event::Key(ke) => {
                if ke.is_shortcut_like() {
                    // Try the focused path first with capture → bubble.
                    let consumed = self.route(
                        RouteKind::Focused {
                            phases: Phases::STANDARD,
                        },
                        OverrideKind::Normal,
                    );

                    // If no focus or focus path didn't consume it, fall back to registry.
                    if consumed.is_none() {
                        let listener_keys = self.event.listener_keys();
                        let mut interested: SmallVec<[ViewId; 16]> = SmallVec::new();
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
                            let result = self.route(
                                RouteKind::Directed {
                                    target: id.get_element_id(),
                                    phases: Phases::TARGET,
                                },
                                OverrideKind::Normal,
                            );
                            if result.is_some() {
                                break;
                            }
                        }
                    }
                } else {
                    // Typing keys: capture → target → bubble on the focused element.
                    self.route(
                        RouteKind::Focused {
                            phases: Phases::STANDARD,
                        },
                        OverrideKind::Normal,
                    );
                }
            }
            Event::Ime(_) => {
                self.route(
                    RouteKind::Focused {
                        phases: Phases::STANDARD,
                    },
                    OverrideKind::Normal,
                );
            }
            Event::FileDrag(fde) => {
                let point = fde.logical_point();
                self.route(
                    RouteKind::Spatial {
                        point: Some(point),
                        phases: Phases::TARGET,
                    },
                    OverrideKind::Normal,
                );
            }
            Event::Window(we) => {
                if matches!(we, WindowEvent::ChangeUnderCursor) {
                    let point = self.window_state.last_pointer.0;
                    RouteCx::new_lifecycle(self).update_hover_from_point(point);
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
                    self.route(
                        RouteKind::Directed {
                            target: id.get_element_id(),
                            phases: Phases::TARGET,
                        },
                        OverrideKind::Normal,
                    );
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

        if let Event::Pointer(PointerEvent::Leave(_)) = &self.event {
            self.update_hover_from_path(&[]);
        }
    }

    /// Create a lifecycle-only scope: no dispatch, but `handle_default_behaviors`
    /// and pending event flushing will run when the scope drops.
    fn route_lifecycle_only(&mut self) {
        let _rcx = RouteCx::new_lifecycle(self);
    }

    /// Update hover from a path without dispatching any event to views.
    /// Used by `file_drag_end` and similar external callers.
    pub fn update_hover_from_path(&mut self, path: &[ElementId]) {
        RouteCx::new_lifecycle(self).update_hover_from_path(path);
    }

    /// Update focus to a specific element without dispatching an event to views.
    pub fn update_focus(&mut self, element_id: ElementId, keyboard_navigation: bool) {
        RouteCx::new_lifecycle(self).update_focus(element_id, keyboard_navigation);
    }

    /// Update focus from an already-built path without dispatching an event to views.
    pub fn clear_focus(&mut self) {
        RouteCx::new_lifecycle(self).clear_focus();
    }
}

// ============================================================================
// Tier 2: RouteCx — per-event scope
// ============================================================================

/// Per-event context. One `RouteCx` exists for each event dispatched — whether
/// the original OS event or a synthetic event generated during processing.
///
/// Internal routing strategies (spatial → directed) happen as plain method calls
/// on the same `RouteCx`; they do not create a new scope. Lifecycle hooks
/// (`handle_default_behaviors`, pointer capture, pending flush) run exactly once
/// when this scope ends via [`Drop`].
pub(crate) struct RouteCx<'r, 'w> {
    pub gcx: &'r mut GlobalEventCx<'w>,
    /// The active event for this route scope (may be a synthetic override).
    pub event: Event,
    /// Visual hit path for the active event's point, if any.
    pub hit_path: Option<Rc<[ElementId]>>,
    /// The full dispatch sequence for this event (capture/target/bubble steps).
    /// Pre-built for directed routes; set after hit-test for spatial routes.
    pub dispatch: Option<Rc<[Dispatch]>>,
    pub source: ElementId,
    /// The event that caused this one, if synthetic.
    pub triggered_by: Option<Event>,
    /// Events generated during dispatch that will each get their own `RouteCx`.
    pending_events: SmallVec<[(RouteKind, Event); 8]>,
    /// Events generated during dispatch that will each get their own `RouteCx` that are preventable with `prevent_default`.
    pending_default_events: SmallVec<[(RouteKind, Event); 8]>,
    /// Guard against double-finish (e.g. explicit call + drop).
    finished: bool,
    prevent_default: bool,
}

// ============================================================================
// RouteCx — Construction
// ============================================================================

impl<'r, 'w> RouteCx<'r, 'w> {
    /// Full constructor. Sets up event/triggered_by, computes hit path,
    /// and pre-builds the dispatch sequence for directed routes.
    fn new(gcx: &'r mut GlobalEventCx<'w>, kind: &RouteKind, override_kind: OverrideKind) -> Self {
        let (event, triggered_by) = match override_kind {
            OverrideKind::Normal => (gcx.event.clone(), None),
            OverrideKind::Synthetic {
                event,
                triggered_by,
            } => (event, triggered_by),
        };

        let source = gcx.source;

        // Compute the hit path if the event has a spatial point.
        let hit_path = event
            .point()
            .and_then(|point| hit_test(gcx.window_state.root_view_id, point));

        // Pre-build the dispatch sequence for non-TARGET directed routes so
        // EventCx handlers can inspect the full sequence.
        let dispatch = match kind {
            RouteKind::Directed { target, phases } if *phases != Phases::TARGET => {
                let box_tree = gcx.window_state.box_tree.borrow();
                let seq: Vec<Dispatch> = build_capture_bubble_path(*target, &box_tree)
                    .filter(|d| phases.matches(&d.phase))
                    .collect();
                drop(box_tree);
                if seq.is_empty() {
                    None
                } else {
                    Some(Rc::from(seq))
                }
            }
            _ => None,
        };

        Self {
            gcx,
            event,
            hit_path,
            dispatch,
            source,
            triggered_by,
            pending_events: SmallVec::new(),
            pending_default_events: SmallVec::new(),
            finished: false,
            prevent_default: false,
        }
    }

    /// Lifecycle-only constructor. No dispatch sequence or hit path.
    /// Used for hover/focus updates and for the "no dispatch target" path in
    /// `route_window_event` — the scope still runs lifecycle hooks on drop.
    fn new_lifecycle(gcx: &'r mut GlobalEventCx<'w>) -> Self {
        let event = gcx.event.clone();
        let source = gcx.source;
        Self {
            gcx,
            event,
            hit_path: None,
            dispatch: None,
            source,
            triggered_by: None,
            pending_events: SmallVec::new(),
            pending_default_events: SmallVec::new(),
            finished: false,
            prevent_default: false,
        }
    }
}

// ============================================================================
// RouteCx — Dispatch (internal routing, no lifecycle hooks)
// ============================================================================

impl RouteCx<'_, '_> {
    /// Main dispatch entry point. Routes the event according to `kind`.
    /// Does NOT run lifecycle hooks — those run in [`finish`] via [`Drop`].
    pub(crate) fn dispatch(&mut self, kind: RouteKind) -> Option<Dispatch> {
        match kind {
            RouteKind::Directed { target, phases } => self.dispatch_directed(target, phases),
            RouteKind::Focused { phases } => {
                if let Some(focus) = &self.gcx.window_state.focus_state {
                    self.dispatch_directed(*focus, phases)
                } else if phases.contains(Phases::BROADCAST) {
                    self.dispatch_global(true);
                    None
                } else {
                    None
                }
            }
            RouteKind::Spatial { point, phases } => {
                let point = point.or_else(|| self.event.point());
                if let Some(point) = point {
                    self.dispatch_spatial(point, phases)
                } else {
                    None
                }
            }
            RouteKind::Subtree {
                target,
                respect_propagation,
            } => {
                self.dispatch_subtree(target, respect_propagation);
                None
            }
            RouteKind::Broadcast {
                respect_propagation,
            } => {
                self.dispatch_global(respect_propagation);
                None
            }
        }
    }

    /// Dispatch through the capture/target/bubble path to a specific target.
    /// Called directly by `dispatch_spatial` — no new RouteCx is created.
    pub(crate) fn dispatch_directed(
        &mut self,
        target: ElementId,
        phases: crate::context::Phases,
    ) -> Option<Dispatch> {
        use crate::context::Phases;

        if let Some(path) = self.hit_path.clone().as_deref() {
            // Enter/Leave events are hover notifications — they are the *result* of hover
            // tracking, not events that should trigger another round of hover recalculation.
            // Calling update_hover_from_path for them causes infinite recursion:
            // FileDragEvent::Enter → update_hover_from_path → push synthetic FileDragEvent::Enter
            // → route_synthetic → dispatch_directed → update_hover_from_path → ...
            let is_hover_notification = matches!(
                &self.event,
                Event::Pointer(PointerEvent::Enter(_) | PointerEvent::Leave(_))
                    | Event::FileDrag(FileDragEvent::Enter(_) | FileDragEvent::Leave(_))
            );
            if (self.event.is_pointer() || self.event.is_file_drag()) && !is_hover_notification {
                self.update_hover_from_path(path);
            }
            if self.event.is_pointer_down() {
                if let Some(hit) = path.last().copied() {
                    self.update_focus(hit, false);
                }
            }
        }

        // Keyboard trigger → queue a synthetic Click on the focused element.
        if self.event.is_keyboard_trigger() {
            if let Some(focus) = &self.gcx.window_state.focus_state {
                self.pending_events.push((
                    RouteKind::Directed {
                        target: *focus,
                        phases: Phases::STANDARD,
                    },
                    Event::Interaction(InteractionEvent::Click),
                ));
            }
        }

        let result = if phases == Phases::TARGET {
            // Fast path: single target step, no need for the full ancestor chain.
            let d = Dispatch::target(target);
            let outcome = self.dispatch_event_cx(&d);
            if outcome.is_stop() { Some(d) } else { None }
        } else {
            // Use dispatch pre-built in new(), or build it now (spatial routes).
            if self.dispatch.is_none() {
                let box_tree = self.gcx.window_state.box_tree.borrow();
                let seq: Vec<Dispatch> = build_capture_bubble_path(target, &box_tree)
                    .filter(|d| phases.matches(&d.phase))
                    .collect();
                drop(box_tree);
                if !seq.is_empty() {
                    self.dispatch = Some(Rc::from(seq.as_slice()));
                }
            }

            if let Some(dispatch) = self.dispatch.clone() {
                run_dispatch(dispatch.iter().copied(), |d| self.dispatch_event_cx(&d))
            } else {
                None
            }
        };

        // Keyboard event not consumed by the focused view → fall back to global.
        if result.is_none() && phases.contains(Phases::BROADCAST) {
            self.dispatch_global(true);
            None
        } else {
            result
        }
    }

    /// Spatial hit-test routing. Updates hover/focus/pointer state, then
    /// calls `dispatch_directed` on the same scope (no new RouteCx).
    fn dispatch_spatial(
        &mut self,
        point: Point,
        phases: crate::context::Phases,
    ) -> Option<Dispatch> {
        if self.event.is_pointer_down() {
            self.gcx.window_state.keyboard_navigation = false;
        }

        // Override hit path with spatial point (may differ from event point).
        let path = hit_test(self.gcx.window_state.root_view_id, point);
        self.hit_path = path.clone();

        let path = self.hit_path.clone().unwrap_or_default();

        self.handle_pointer_state_updates();

        if let Some(target) = path.last().copied() {
            // Build and cache the dispatch sequence for this target.
            if self.dispatch.is_none() {
                let box_tree = self.gcx.window_state.box_tree.borrow();
                let seq: Vec<Dispatch> = build_capture_bubble_path(target, &box_tree)
                    .filter(|d| phases.matches(&d.phase))
                    .collect();
                drop(box_tree);
                if !seq.is_empty() {
                    self.dispatch = Some(Rc::from(seq.as_slice()));
                }
            }
            // Direct call — same RouteCx, lifecycle runs once when outer scope drops.
            self.dispatch_directed(target, phases)
        } else {
            None
        }
    }

    /// Dispatch to a target and all its descendants.
    fn dispatch_subtree(&mut self, target: ElementId, respect_propagation: bool) {
        let target_view_id = target.owning_id();
        self.dispatch_tree_recursive(target_view_id, respect_propagation);
    }

    /// Dispatch to all views in DOM order (global broadcast).
    fn dispatch_global(&mut self, respect_propagation: bool) {
        let root = self.gcx.window_state.root_view_id.get_element_id();
        self.dispatch_subtree(root, respect_propagation);
    }

    /// Recursive tree walk for subtree/broadcast dispatch.
    fn dispatch_tree_recursive(&mut self, view_id: ViewId, respect_propagation: bool) {
        let d = Dispatch::target(view_id.get_element_id());
        let outcome = self.dispatch_event_cx(&d);

        if respect_propagation && outcome.is_stop() {
            return;
        }

        for child_id in view_id.children() {
            self.dispatch_tree_recursive(child_id, respect_propagation);
        }
    }
}

// ============================================================================
// Tier 3: EventCx — per-node dispatch step (construction lives on RouteCx)
// ============================================================================

impl RouteCx<'_, '_> {
    /// Create an `EventCx` for `dispatch_step` and call `dispatch_one`.
    pub fn dispatch_event_cx(&mut self, dispatch_step: &Dispatch) -> Outcome {
        if self.event.is_pointer()
            && dispatch_step.target_element_id.is_view()
            && dispatch_step
                .target_element_id
                .owning_id()
                .pointer_events_none()
        {
            return Outcome::Continue;
        }

        let world_transform = match self
            .gcx
            .window_state
            .box_tree
            .borrow()
            .world_transform(dispatch_step.target_element_id.0)
        {
            Ok(transform) => transform,
            Err(transform) => transform.value().unwrap(),
        }
        .inverse();

        let mut cx = EventCx {
            window_state: self.gcx.window_state,
            event: self.event.clone().transform(world_transform),
            world_transform,
            triggered_by: self.triggered_by.as_ref(),
            hit_path: self.hit_path.clone(),
            phase: dispatch_step.phase,
            target: dispatch_step.target_element_id,
            source: self.source,
            dispatch: self.dispatch.clone(),
            source_id: self.source,
            default_prevented: &mut self.prevent_default,
            stop_immediate: false,
        };
        cx.dispatch_one()
    }
}

pub struct EventCx<'a> {
    pub window_state: &'a mut WindowState,
    /// An event transformed to the local coordinate space of the target node.
    pub event: Event,
    /// The transform from window space to the local space of the target element.
    pub world_transform: Affine,
    /// The event that caused this synthetic event, if any. Not transformed.
    pub triggered_by: Option<&'a Event>,
    /// All visual IDs under the pointer for pointer events.
    pub hit_path: Option<Rc<[ElementId]>>,
    /// The event phase for this local event.
    pub phase: Phase,
    /// The target of this event.
    pub target: ElementId,
    /// The visual ID that is the source/origin of this event.
    pub source: ElementId,
    /// The full dispatch sequence for this event.
    pub dispatch: Option<Rc<[Dispatch]>>,
    /// The element that caused the event to be dispatched.
    pub source_id: ElementId,
    /// Whether preventDefault() was called (shared across all phases).
    default_prevented: &'a mut bool,
    /// Whether stopImmediatePropagation() was called.
    stop_immediate: bool,
}

impl<'a> EventCx<'a> {
    /// Stop propagation to other listeners on this target AND to other nodes in the path.
    pub fn stop_immediate_propagation(&mut self) {
        self.stop_immediate = true;
    }

    /// Prevent any default actions for this event.
    pub fn prevent_default(&mut self) {
        *self.default_prevented = true;
    }

    /// Check if preventDefault() was called on this event.
    pub fn is_default_prevented(&self) -> bool {
        *self.default_prevented
    }

    /// Request pointer capture for this element.
    pub fn request_pointer_capture(&mut self, pointer_id: PointerId) -> bool {
        self.window_state
            .set_pointer_capture(pointer_id, self.target)
    }

    /// Request that this element start tracking a drag.
    ///
    /// Should be called in response to `PointerCaptureEvent::Gained`.
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

        let mut stop_propagation = false;

        let view_result = view.borrow_mut().event(self);

        if self.stop_immediate {
            return Outcome::Stop;
        }

        if view_result.is_stop() {
            stop_propagation = true;
        }

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

        if stop_propagation {
            Outcome::Stop
        } else {
            Outcome::Continue
        }
    }
}

// ============================================================================
// RouteCx — Lifecycle (RAII via Drop)
// ============================================================================

impl Drop for RouteCx<'_, '_> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            self.finish();
        }
    }
}

impl RouteCx<'_, '_> {
    /// Run lifecycle hooks for this event scope. Called by `Drop`.
    /// Guarded by `finished` so it only ever runs once.
    fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;

        if !self.prevent_default {
            self.handle_default_behaviors();
        }

        if let Event::Pointer(pe) = &self.event {
            if let Some(pointer_id) = pe.pointer_info().pointer_id {
                self.process_pending_pointer_capture(pointer_id);
            }
        }

        self.flush_pending_events();
    }

    /// Route a synthetic event, carrying the current event as `triggered_by`.
    fn route_synthetic(&mut self, kind: RouteKind, event: Event) {
        let triggered_by = Some(self.event.clone());
        self.gcx.route(
            kind,
            OverrideKind::Synthetic {
                event,
                triggered_by,
            },
        );
    }

    /// Flush all pending events. Each gets its own [`RouteCx`] scope (and thus
    /// its own lifecycle), with the current event as `triggered_by`.
    fn flush_pending_events(&mut self) {
        let pending = std::mem::take(&mut self.pending_events);
        for (kind, event) in pending {
            self.route_synthetic(kind, event);
        }
        let pending = std::mem::take(&mut self.pending_default_events);
        if !self.prevent_default {
            for (kind, event) in pending {
                self.route_synthetic(kind, event);
            }
        }
    }
}

// ============================================================================
// RouteCx — Default Behaviors
// ============================================================================

impl RouteCx<'_, '_> {
    /// Handle browser-style default behaviors: drag threshold, drag events,
    /// pointer-up capture release, tab/arrow navigation, context menus, etc.
    fn handle_default_behaviors(&mut self) {
        // Pointer move — check drag threshold and dispatch active-drag events.
        let pointer_move = match &self.event {
            Event::Pointer(PointerEvent::Move(pu)) => Some(pu.clone()),
            _ => None,
        };

        if let Some(pu) = pointer_move {
            let box_tree = self.gcx.window_state.box_tree.clone();
            if let Some(drag_dispatch) = self
                .gcx
                .window_state
                .drag_tracker
                .check_threshold(&pu, &box_tree.borrow())
            {
                self.gcx.window_state.needs_box_tree_commit = true;
                self.dispatch_drag_event(drag_dispatch);
            }
            if let Some(_active) = &self.gcx.window_state.drag_tracker.active_drag {
                self.gcx.window_state.needs_box_tree_from_layout = true;
                let hover_path = self
                    .hit_path
                    .as_ref()
                    .map(|p| p.iter().as_slice())
                    .unwrap_or(&[]);
                let drag_events = self
                    .gcx
                    .window_state
                    .drag_tracker
                    .on_pointer_move(&pu, hover_path);
                for drag_event in drag_events {
                    self.dispatch_drag_event(drag_event);
                }
            }
        }

        // Pointer up — end drag and release capture.
        let pe = match &self.event {
            Event::Pointer(PointerEvent::Up(pe)) => Some(pe.clone()),
            _ => None,
        };
        if let Some(pe) = pe {
            let drag_events = self.gcx.window_state.drag_tracker.on_pointer_up(&pe);
            for drag_event in drag_events {
                self.dispatch_drag_event(drag_event);
            }
            if let Some(pointer_id) = pe.pointer.pointer_id {
                self.gcx
                    .window_state
                    .release_pointer_capture_unconditional(pointer_id);
            }
        }

        // Pointer cancel — abort drag and release capture.
        let pi = match &self.event {
            Event::Pointer(PointerEvent::Cancel(pi)) => Some(*pi),
            _ => None,
        };
        if let Some(pi) = pi {
            let drag_events = self.gcx.window_state.drag_tracker.on_pointer_cancel(pi);
            for drag_event in drag_events {
                self.dispatch_drag_event(drag_event);
            }
            if let Some(pointer_id) = pi.pointer_id {
                self.gcx
                    .window_state
                    .release_pointer_capture_unconditional(pointer_id);
            }
        }

        // Tab navigation.
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

        // Arrow navigation.
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

        // Window resized — mark responsive styles dirty.
        if let Event::Window(WindowEvent::Resized(_)) = &self.event {
            // TODO: mark responsive styles dirty when the style system supports it
        }

        // Context / popout menus (platform-specific timing).
        let pbe = match &self.event {
            Event::Pointer(PointerEvent::Down(pbe)) if cfg!(target_os = "macos") => Some(pbe),
            Event::Pointer(PointerEvent::Up(pbe)) if !cfg!(target_os = "macos") => Some(pbe),
            _ => None,
        };
        if let Some(pbe) = pbe {
            self.handle_menu_events(&pbe.clone());
        }
    }

    /// Helper: dispatch a `DragEventDispatch` variant as a synthetic route.
    fn dispatch_drag_event(&mut self, drag_dispatch: DragEventDispatch) {
        use crate::context::Phases;
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

        if button == PointerButton::Secondary {
            let context_menu = view_state.borrow().context_menu.clone();
            if let Some(menu) = context_menu {
                let position = pbe.state.logical_point();
                show_context_menu(menu(), Some(position));
                self.gcx.window_state.click_state.clear();
            }
        }

        if button == PointerButton::Primary {
            let popout_menu = view_state.borrow().popout_menu.clone();
            if let Some(menu) = popout_menu {
                let bounds = self
                    .gcx
                    .window_state
                    .box_tree
                    .borrow()
                    .world_bounds(hit.owning_id().get_element_id().0)
                    .ok()
                    .unwrap_or_default();
                let bottom_left = Point::new(bounds.x0, bounds.y1);
                show_context_menu(menu(), Some(bottom_left));
                self.gcx.window_state.click_state.clear();
            }
        }
    }
}

// ============================================================================
// RouteCx — Pointer Capture Processing
// ============================================================================

impl RouteCx<'_, '_> {
    /// Process pending pointer capture changes.
    ///
    /// Fires `LostPointerCapture` to the old target, then `GainedPointerCapture`
    /// to the new target, matching Chromium's two-phase capture model.
    pub(crate) fn process_pending_pointer_capture(&mut self, pointer_id: PointerId) {
        let current_target = self.gcx.window_state.get_pointer_capture_target(pointer_id);
        let pending_target = self.gcx.window_state.get_pending_capture_target(pointer_id);

        if current_target == pending_target {
            return;
        }

        if let Some(old_target) = current_target {
            self.gcx.window_state.remove_active_capture(pointer_id);
            let event = Event::PointerCapture(PointerCaptureEvent::Lost(pointer_id));
            self.route_synthetic(
                RouteKind::Directed {
                    target: old_target,
                    phases: Phases::TARGET,
                },
                event,
            );
        }

        if let Some(new_target) = pending_target {
            if !new_target.owning_id().is_hidden() {
                self.gcx
                    .window_state
                    .set_active_capture(pointer_id, new_target);
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

                if new_target.owning_id().is_hidden() {
                    self.gcx.window_state.remove_active_capture(pointer_id);
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
// RouteCx — Focus Management
// ============================================================================

impl RouteCx<'_, '_> {
    /// Update focus to a new element, firing FocusGained/FocusLost events.
    pub fn update_focus(&mut self, element_id: ElementId, keyboard_navigation: bool) {
        let new_focus = self.resolve_focus_target(element_id, keyboard_navigation);

        let old_focus = self.gcx.window_state.focus_state;

        if old_focus == new_focus {
            return;
        }
        self.gcx.window_state.focus_state = new_focus;

        if let Some(old) = old_focus {
            self.gcx.window_state.style_dirty.insert(old.owning_id());
            self.pending_default_events.push((
                RouteKind::Directed {
                    target: old,
                    phases: Phases::STANDARD,
                },
                Event::Focus(FocusEvent::Lost),
            ));
        }

        if let Some(new_focus) = new_focus {
            self.gcx
                .window_state
                .style_dirty
                .insert(new_focus.owning_id());
            self.pending_default_events.push((
                RouteKind::Directed {
                    target: new_focus,
                    phases: Phases::STANDARD,
                },
                Event::Focus(FocusEvent::Gained),
            ));
        }

        self.gcx.window_state.keyboard_navigation = keyboard_navigation;
    }

    pub fn clear_focus(&mut self) {
        let old_focus = self.gcx.window_state.focus_state.take();

        if let Some(old) = old_focus {
            self.gcx.window_state.style_dirty.insert(old.owning_id());
            self.pending_default_events.push((
                RouteKind::Directed {
                    target: old,
                    phases: Phases::STANDARD,
                },
                Event::Focus(FocusEvent::Lost),
            ));
        }
    }

    /// Walk up from `target` to find the first ancestor (or self) that is
    /// visible and focusable (or keyboard-navigable if using keyboard nav).
    fn resolve_focus_target(
        &self,
        target: ElementId,
        keyboard_navigation: bool,
    ) -> Option<ElementId> {
        let required = if keyboard_navigation {
            NodeFlags::VISIBLE | NodeFlags::KEYBOARD_NAVIGABLE
        } else {
            NodeFlags::VISIBLE | NodeFlags::FOCUSABLE
        };
        let box_tree = self.gcx.window_state.box_tree.borrow();
        std::iter::successors(Some(target), |&cur| {
            box_tree
                .parent_of(cur.0)
                .map(|p| box_tree.meta(p).flatten().unwrap())
        })
        .find(|id| {
            box_tree
                .flags(id.0)
                .map(|f| f.contains(required))
                .unwrap_or(false)
        })
    }
}

// ============================================================================
// RouteCx — Keyboard Navigation
// ============================================================================

impl RouteCx<'_, '_> {
    pub(crate) fn view_tab_navigation(&mut self, backwards: bool) {
        let scope_root = self.gcx.window_state.root_view_id.get_element_id();

        let current_focus = self.gcx.window_state.focus_state.unwrap_or(scope_root);

        // TODO: replace with non-build_focus_space traversal
        let _ = (scope_root, current_focus, backwards);
    }

    pub(crate) fn view_arrow_navigation(&mut self, key: &NamedKey) {
        // TODO: replace with non-build_focus_space traversal
        let _ = key;
    }
}

// ============================================================================
// RouteCx — Hover State Management
// ============================================================================

impl RouteCx<'_, '_> {
    pub(crate) fn update_hover_from_point(&mut self, point: Point) {
        let path = hit_test(self.gcx.window_state.root_view_id, point);
        if let Some(path) = path {
            self.update_hover_from_path(&path);
        }
    }

    pub(crate) fn update_hover_from_path(&mut self, path: &[ElementId]) {
        if matches!(&self.event, Event::FileDrag(FileDragEvent::Drop(..))) {
            // Drop: clear hover, re-enter with pointer enters.
            let leave_events = self.gcx.window_state.hover_state.update_path(&[]);
            for event in leave_events {
                #[expect(irrefutable_let_patterns)]
                if let HoverEvent::Leave(target) | HoverEvent::Enter(target) = &event {
                    if target.is_view() {
                        self.gcx.window_state.style_dirty.insert(target.owning_id());
                    }
                }
            }
            let enter_events = self.gcx.window_state.hover_state.update_path(path);
            Self::push_hover_events(
                &mut self.pending_events,
                enter_events,
                false,
                self.gcx.window_state,
            );
        } else if matches!(&self.event, Event::FileDrag(FileDragEvent::Enter(..))) {
            // Drag start: pointer leaves then file-drag enters.
            let leave_events = self.gcx.window_state.hover_state.update_path(&[]);
            Self::push_hover_events(
                &mut self.pending_events,
                leave_events,
                false,
                self.gcx.window_state,
            );
            let enter_events = self.gcx.window_state.hover_state.update_path(path);
            Self::push_hover_events(
                &mut self.pending_events,
                enter_events,
                true,
                self.gcx.window_state,
            );
        } else {
            let events = self.gcx.window_state.hover_state.update_path(path);
            let use_file_drag = self.gcx.window_state.file_drag_paths.is_some();
            Self::push_hover_events(
                &mut self.pending_events,
                events,
                use_file_drag,
                self.gcx.window_state,
            );
        }
        self.gcx.window_state.needs_cursor_resolution = true;
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
                    dbg!(target);
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

        let Some((point, path)) = pe.logical_point().zip(self.hit_path.clone()) else {
            return;
        };

        match pe {
            PointerEvent::Down(PointerButtonEvent {
                button, pointer, ..
            }) => {
                for hit in path
                    .iter()
                    .filter(|id| id.is_view())
                    .map(|id| id.owning_id())
                {
                    self.gcx.window_state.style_dirty.insert(hit);
                }
                self.gcx.window_state.click_state.on_down(
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
                    .gcx
                    .window_state
                    .click_state
                    .cancel(pointer_id.map(|p| p.get_inner()))
                {
                    for target in canceled.target.iter() {
                        self.gcx.window_state.style_dirty.insert(target.owning_id());
                    }
                }
            }
            PointerEvent::Move(pu) => {
                self.gcx.window_state.last_pointer = (pu.current.logical_point(), pu.pointer);
                let _exceeded_nodes = self.gcx.window_state.click_state.on_move(
                    pu.pointer.pointer_id.map(|p| p.get_inner()),
                    pu.current.logical_point(),
                );
                // if let Some(element_ids) = exceeded_nodes {
                //     for id in element_ids
                //         .iter()
                //         .filter(|id| id.is_view())
                //         .map(|id| id.owning_id())
                //     {
                //         self.gcx.window_state.style_dirty.insert(id);
                //     }
                // }
            }
            _ => {}
        }
    }

    fn handle_click_events(
        &mut self,
        new_path: &Rc<[ElementId]>,
        point: Point,
        pointer_id: Option<PointerId>,
        button: Option<PointerButton>,
        count: u8,
    ) {
        let hit_path_len = new_path.len();
        let res = self.gcx.window_state.click_state.on_up(
            pointer_id.map(|p| p.get_inner()),
            button.map(|b| b as u8),
            new_path,
            point,
            Instant::now().duration_since(*START_TIME).as_millis() as u64,
        );

        for hit in new_path.iter() {
            if hit.is_view() {
                self.gcx.window_state.style_dirty.insert(hit.owning_id());
            }
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
                    .zip(new_path.iter())
                    .position(|(a, b)| a != b)
                    .unwrap_or(og_target.len().min(hit_path_len));
                if common_ancestor_idx > 0 {
                    let common_path = &new_path[..common_ancestor_idx];
                    self.push_interaction_events(
                        common_path.last().copied(),
                        count,
                        button == Some(PointerButton::Secondary),
                    );
                    for target in &og_target.iter().as_slice()[common_ancestor_idx..] {
                        if target.is_view() {
                            self.gcx.window_state.style_dirty.insert(target.owning_id());
                        }
                    }
                } else {
                    for target in og_target.iter() {
                        if target.is_view() {
                            self.gcx.window_state.style_dirty.insert(target.owning_id());
                        }
                    }
                }
            }
            ClickResult::Suppressed(None) => {}
        }
    }

    fn push_interaction_events(&mut self, target: Option<ElementId>, count: u8, secondary: bool) {
        if let Some(id) = target {
            if id.is_view() {
                self.gcx.window_state.style_dirty.insert(id.owning_id());
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
}

pub trait KeyEventExt {
    fn is_shortcut_like(&self) -> bool;
}

impl KeyEventExt for ui_events::keyboard::KeyboardEvent {
    /// Returns `true` for key combos that look like shortcuts rather than text input.
    ///
    /// Shortcut-like means:
    /// - Any Ctrl/Cmd/Alt modifier (Shift alone doesn't count — that's just uppercase)
    /// - Bare function keys (F1–F24), Escape, Delete, PrintScreen, etc.
    /// - Tab (even unmodified — it's navigation, not text)
    fn is_shortcut_like(&self) -> bool {
        let has_command_modifier = self
            .modifiers
            .intersects(Modifiers::CONTROL | Modifiers::META | Modifiers::ALT);
        if has_command_modifier {
            return true;
        }
        matches!(
            self.code,
            KeyCode::Escape
                | KeyCode::Tab
                | KeyCode::Delete
                | KeyCode::F1
                | KeyCode::F2
                | KeyCode::F3
                | KeyCode::F4
                | KeyCode::F5
                | KeyCode::F6
                | KeyCode::F7
                | KeyCode::F8
                | KeyCode::F9
                | KeyCode::F10
                | KeyCode::F11
                | KeyCode::F12
        )
    }
}
