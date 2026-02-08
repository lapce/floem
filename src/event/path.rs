//! Event path building and dispatch for Chromium-style two-phase event handling.
//!
//! This module separates event handling into two phases:
//! 1. **Hit Testing**: Find the target view under the pointer (accounts for z-index)
//! 2. **Path Building**: Walk from hit target to root, collecting nodes and pre-computing data
//! 3. **Dispatch**: Iterate through the pre-built path (capturing, at-target, bubbling)
//!
//! This separation provides:
//! - Better debuggability (can inspect the path before dispatch)
//! - Lower per-node overhead during dispatch
//! - Immutable path during dispatch (no state changes affecting the path)

use std::cell::RefCell;

use peniko::kurbo::{Affine, Point};
use smallvec::SmallVec;

use crate::view::ViewId;
use crate::view::stacking::{collect_overlays, collect_stacking_context_items};

// ============================================================================
// Hit Test Result Cache
// ============================================================================
//
// A small 2-entry cache for hit test results, inspired by Chromium's Blink engine.
// This exploits the common pattern where multiple events occur at the same location
// (e.g., mousedown, mouseup, click all at the same point).
//
// The cache size of 2 is chosen because:
// 1. It handles the ping-pong pattern of alternating event types
// 2. It's cheap to store and search (O(2) lookup)
// 3. Matches Blink's proven design: HIT_TEST_CACHE_SIZE = 2

/// Cache entry for hit test results.
#[derive(Clone, Copy)]
struct HitTestCacheEntry {
    /// The root view ID for this hit test
    root_id: ViewId,
    /// The point that was tested (in window coordinates)
    point: Point,
    /// The result of the hit test
    result: Option<ViewId>,
}

/// 2-entry hit test result cache.
struct HitTestCache {
    entries: [Option<HitTestCacheEntry>; 2],
    /// Index of next slot to write (round-robin)
    next_slot: usize,
}

impl HitTestCache {
    const fn new() -> Self {
        Self {
            entries: [None, None],
            next_slot: 0,
        }
    }

    /// Look up a cached hit test result.
    /// Returns Some(result) on cache hit, None on cache miss.
    #[inline]
    fn lookup(&self, root_id: ViewId, point: Point) -> Option<Option<ViewId>> {
        for e in self.entries.iter().flatten() {
            // Use bitwise comparison for Point (exact match like Blink)
            if e.root_id == root_id
                && e.point.x.to_bits() == point.x.to_bits()
                && e.point.y.to_bits() == point.y.to_bits()
            {
                return Some(e.result);
            }
        }
        None
    }

    /// Add a hit test result to the cache.
    #[inline]
    fn insert(&mut self, root_id: ViewId, point: Point, result: Option<ViewId>) {
        self.entries[self.next_slot] = Some(HitTestCacheEntry {
            root_id,
            point,
            result,
        });
        self.next_slot = (self.next_slot + 1) % 2;
    }

    /// Clear the cache. Call this when layout or view tree changes.
    #[inline]
    fn clear(&mut self) {
        self.entries = [None, None];
    }
}

thread_local! {
    static HIT_TEST_CACHE: RefCell<HitTestCache> = const { RefCell::new(HitTestCache::new()) };
}

/// Clear the hit test result cache.
/// Call this when layout changes, view tree changes, or at the start of a new frame.
pub fn clear_hit_test_cache() {
    HIT_TEST_CACHE.with(|cache| cache.borrow_mut().clear());
}

/// Pre-computed data for a single node in the event path.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Some fields are for future use and debugging
pub struct EventPathNode {
    /// The view ID for this node.
    pub view_id: ViewId,
    /// Transform from local coordinates to window coordinates.
    /// Use the inverse to convert from window to local.
    pub visual_transform: Affine,
    /// Whether this view has any event listeners registered.
    pub has_event_listeners: bool,
    /// Whether this view has a context menu.
    pub has_context_menu: bool,
    /// Whether this view has a popout menu.
    pub has_popout_menu: bool,
    /// Whether default event handling is disabled for any listener.
    pub has_disabled_defaults: bool,
    /// Whether this view has pointer-events: none style.
    pub is_pointer_none: bool,
    /// Whether this view is focusable.
    pub is_focusable: bool,
    /// Whether this view can be dragged.
    pub can_drag: bool,
}

impl EventPathNode {
    /// Check if this node needs any event processing at all.
    #[allow(unused)]
    pub fn needs_processing(&self) -> bool {
        self.has_event_listeners
            || self.has_context_menu
            || self.has_popout_menu
            || self.is_focusable
            || self.can_drag
    }
}

/// The event path from target to root.
///
/// Built once per event, then used for capturing and bubbling phases.
/// The path is immutable during dispatch, ensuring consistent behavior.
#[derive(Debug)]
pub struct EventPath {
    /// Nodes from target (index 0) to root (last index).
    /// This ordering makes bubbling a simple forward iteration.
    nodes: SmallVec<[EventPathNode; 16]>,
}

impl EventPath {
    /// Create an empty event path.
    pub fn new() -> Self {
        Self {
            nodes: SmallVec::new(),
        }
    }

    /// Create an event path with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            nodes: SmallVec::with_capacity(capacity),
        }
    }

    /// Add a node to the path (should be called from target towards root).
    pub fn push(&mut self, node: EventPathNode) {
        self.nodes.push(node);
    }

    /// Returns true if the path is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns the number of nodes in the path.
    #[allow(unused)]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Get the target node (first in path, deepest in tree).
    #[allow(unused)]
    pub fn target(&self) -> Option<&EventPathNode> {
        self.nodes.first()
    }

    /// Get a node by index (0 = target, len-1 = root).
    #[allow(unused)]
    pub fn get(&self, index: usize) -> Option<&EventPathNode> {
        self.nodes.get(index)
    }

    /// Iterate from target to root (bubbling order).
    pub fn iter_bubbling(&self) -> impl Iterator<Item = &EventPathNode> {
        self.nodes.iter()
    }

    /// Iterate from root to target (capturing order).
    pub fn iter_capturing(&self) -> impl Iterator<Item = &EventPathNode> {
        self.nodes.iter().rev()
    }

    /// Check if any node in the path has event listeners.
    #[allow(unused)]
    pub fn has_any_listeners(&self) -> bool {
        self.nodes.iter().any(|n| n.has_event_listeners)
    }

    /// Check if any node in the path needs processing.
    #[allow(unused)]
    pub fn has_any_processing(&self) -> bool {
        self.nodes.iter().any(|n| n.needs_processing())
    }
}

impl Default for EventPath {
    fn default() -> Self {
        Self::new()
    }
}

/// Build an event path from a target view up to the root.
///
/// This walks the parent chain and pre-computes all data needed for dispatch,
/// avoiding repeated RefCell borrows during the dispatch phase.
pub fn build_event_path(target: ViewId) -> EventPath {
    use crate::style::{Focusable, PointerEvents, PointerEventsProp};

    let mut path = EventPath::with_capacity(16);
    let mut current = Some(target);

    while let Some(view_id) = current {
        // Skip hidden views entirely - they shouldn't be in the path
        if view_id.is_hidden() {
            current = view_id.parent();
            continue;
        }

        let view_state = view_id.state();
        let borrowed = view_state.borrow();

        let node = EventPathNode {
            view_id,
            visual_transform: borrowed.visual_transform,
            has_event_listeners: !borrowed.event_listeners.is_empty(),
            has_context_menu: borrowed.context_menu.is_some(),
            has_popout_menu: borrowed.popout_menu.is_some(),
            has_disabled_defaults: !borrowed.disable_default_events.is_empty(),
            is_pointer_none: borrowed
                .computed_style
                .get(PointerEventsProp)
                .map(|p| p == PointerEvents::None)
                .unwrap_or(false),
            is_focusable: borrowed.computed_style.get(Focusable),
            can_drag: view_id.can_drag(),
        };

        drop(borrowed); // Explicit drop before next iteration's borrow

        path.push(node);
        current = view_id.parent();
    }

    path
}

/// Perform hit testing to find the target view under a point.
///
/// This walks the stacking context in reverse z-order (highest z-index first),
/// recursively checking children of stacking context items. Returns the first
/// (topmost, deepest) view that contains the point.
///
/// Results are cached in a 2-entry cache to optimize repeated hit tests
/// at the same location (common during event sequences like click).
///
/// # Arguments
/// * `root_id` - The root view to start hit testing from
/// * `point` - The point in absolute (window) coordinates
///
/// # Returns
/// The target ViewId if a view was found at the point, None otherwise.
pub fn hit_test(root_id: ViewId, point: Point) -> Option<ViewId> {
    // Check cache first
    if let Some(cached_result) = HIT_TEST_CACHE.with(|cache| cache.borrow().lookup(root_id, point))
    {
        return cached_result;
    }

    // Chromium-style: early exit if point is outside the root's visible area.
    // This handles clicks outside the window bounds (which can't hit anything).
    {
        let root_state = root_id.state();
        let root_clip = root_state.borrow().clip_rect;
        if !root_clip.contains(point) {
            HIT_TEST_CACHE.with(|cache| cache.borrow_mut().insert(root_id, point, None));
            return None;
        }
    }

    // Cache miss - perform the actual hit test
    //
    // First check overlays (they're painted on top, so hit test them first)
    // Overlays are sorted by z-index in collect_overlays, so we iterate in reverse
    // to check the highest z-index first.
    let overlays = collect_overlays(root_id);
    for overlay_id in overlays.iter().rev() {
        // Skip hidden or disabled overlays
        if overlay_id.is_hidden() || overlay_id.is_disabled() {
            continue;
        }

        // Hit test within the overlay
        if let Some(hit) = hit_test_stacking_context(*overlay_id, point) {
            HIT_TEST_CACHE.with(|cache| cache.borrow_mut().insert(root_id, point, Some(hit)));
            return Some(hit);
        }
    }

    // No overlay hit - check the regular view tree
    let result = hit_test_stacking_context(root_id, point);

    // Store result in cache
    HIT_TEST_CACHE.with(|cache| cache.borrow_mut().insert(root_id, point, result));

    result
}

/// Hit test within a view, checking children in reverse z-order.
///
/// In the simplified stacking model, every view is a stacking context and z-index
/// only competes with siblings. Children are bounded within their parent.
fn hit_test_stacking_context(parent_id: ViewId, point: Point) -> Option<ViewId> {
    use crate::style::{PointerEvents, PointerEventsProp};

    let items = collect_stacking_context_items(parent_id);

    // Iterate in reverse (highest z-index first, so topmost elements checked first)
    for item in items.iter().rev() {
        // Skip hidden or disabled views
        if item.view_id.is_hidden() || item.view_id.is_disabled() {
            continue;
        }

        // Check if point is within this view's bounds
        let view_state = item.view_id.state();
        let vs = view_state.borrow();

        // Check pointer-events: none - skip this view but still check children
        let is_pointer_none = vs
            .computed_style
            .get(PointerEventsProp)
            .map(|p| p == PointerEvents::None)
            .unwrap_or(false);

        // Check this view's clip_rect for hit testing.
        //
        // Each view has its own clip_rect computed during layout (see layout/cx.rs).
        // For normal flow elements, clip_rect = parent_clip_rect.intersect(view_rect),
        // which ensures children cannot receive events outside their parent's bounds.
        // For absolute/fixed elements, clip_rect equals their own bounds.
        //
        // If point is outside clip_rect, we still recurse to children because they may
        // have different clipping contexts (e.g., absolute-positioned dropdowns).
        if !vs.clip_rect.contains(point) {
            drop(vs);
            if let Some(target) = hit_test_stacking_context(item.view_id, point) {
                return Some(target);
            }
            continue;
        }

        // Check the view's layout rect (already includes transforms from layout)
        // Note: layout_rect is already transformed during layout (see layout/cx.rs:255-256),
        // so we should NOT apply vs.transform again here.
        let layout_rect = vs.layout_rect;
        drop(vs); // Drop borrow before recursing

        if !layout_rect.contains(point) {
            // Check children anyway (the layout_rect might not contain point but children might)
            if let Some(target) = hit_test_stacking_context(item.view_id, point) {
                return Some(target);
            }
            continue;
        }

        // Point is inside this view. Recursively check children for a deeper target.
        if let Some(child_target) = hit_test_stacking_context(item.view_id, point) {
            return Some(child_target);
        }

        // If pointer-events: none, skip this view but we already checked children above
        if is_pointer_none {
            continue;
        }

        // No child matched, this view is the target
        return Some(item.view_id);
    }

    None
}

/// Result of dispatching an event through a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchResult {
    /// Event was processed and propagation should stop.
    Processed,
    /// Event was not processed, continue with default handling.
    Continue,
}

impl DispatchResult {
    pub fn is_processed(&self) -> bool {
        matches!(self, DispatchResult::Processed)
    }
}

/// Dispatch an event through a pre-built path.
///
/// This implements Chromium-style two-phase dispatch:
/// 1. **Capturing phase**: Iterate from root to target, calling `event_before_children`
/// 2. **Bubbling phase**: Iterate from target to root, calling `event_after_children` and listeners
///
/// The path is immutable during dispatch, ensuring consistent behavior even if
/// handlers modify the view tree.
pub fn dispatch_through_path(
    path: &EventPath,
    event: &super::Event,
    cx: &mut super::dispatch::EventCx<'_>,
) -> DispatchResult {
    if path.is_empty() {
        return DispatchResult::Continue;
    }

    // ========== CAPTURING PHASE (root to target) ==========
    // Iterate from root (last in path) to target (first in path)
    for node in path.iter_capturing() {
        if node.is_pointer_none && event.is_pointer() {
            // Skip pointer events for pointer-events: none
            continue;
        }

        // Transform event to local coordinates
        let local_event = event.clone().transform(node.visual_transform);

        // Call event_before_children
        let view = node.view_id.view();
        let result = view.borrow_mut().event_before_children(cx, &local_event);

        if result.is_processed() {
            // Handle focus on pointer down if event_before_children processed it
            handle_focus_on_processed(node, &local_event, cx);
            return DispatchResult::Processed;
        }
    }

    // ========== BUBBLING PHASE (target to root) ==========
    // Iterate from target (first in path) to root (last in path)
    for node in path.iter_bubbling() {
        if node.is_pointer_none && event.is_pointer() {
            // Skip pointer events for pointer-events: none
            continue;
        }

        // Transform event to local coordinates
        let local_event = event.clone().transform(node.visual_transform);

        // Handle built-in behaviors for ALL nodes in the path (not just target).
        // This matches the original stacking-context dispatch behavior where
        // handle_default_behaviors runs for every view that receives the event.
        // Critical for: clicking set tracking, last_pointer_down, hover state, etc.
        let result = handle_builtin_behaviors(node, &local_event, event, cx);
        if result.is_processed() {
            return DispatchResult::Processed;
        }

        // Call event_after_children
        let view = node.view_id.view();
        let result = view.borrow_mut().event_after_children(cx, &local_event);

        if result.is_processed() {
            return DispatchResult::Processed;
        }

        // Call event listeners
        if !node.has_disabled_defaults
            && let Some(listener) = local_event.listener()
        {
            let result = node.view_id.apply_event(&listener, &local_event);
            if result.is_some_and(|r| r.is_processed()) {
                return DispatchResult::Processed;
            }
        }
    }

    DispatchResult::Continue
}

/// Dispatch Click/DoubleClick/SecondaryClick events through the path after PointerUp.
///
/// This implements Chromium-style synthetic click dispatch:
/// - Click events are generated AFTER PointerUp completes (not inline during PointerUp)
/// - Click events bubble through the entire path (target to root)
/// - Each node in the path gets a chance to handle the Click
/// - Handlers returning Stop prevent further bubbling
///
/// # Arguments
/// * `path` - The pre-built event path
/// * `event` - The original PointerUp event (used to get coordinates)
/// * `cx` - The event context
///
/// # Returns
/// DispatchResult indicating if any handler stopped propagation
pub fn dispatch_click_through_path(
    path: &EventPath,
    event: &super::Event,
    cx: &mut super::dispatch::EventCx<'_>,
) -> DispatchResult {
    use ui_events::pointer::{PointerButton, PointerButtonEvent, PointerEvent};

    // Only process PointerUp events
    let super::Event::Pointer(PointerEvent::Up(PointerButtonEvent {
        button,
        pointer,
        state,
    })) = event
    else {
        return DispatchResult::Continue;
    };

    if path.is_empty() {
        return DispatchResult::Continue;
    }

    let is_primary =
        pointer.is_primary_pointer() && button.is_none_or(|b| b == PointerButton::Primary);
    let is_secondary = button.is_some_and(|b| b == PointerButton::Secondary);

    if !is_primary && !is_secondary {
        return DispatchResult::Continue;
    }

    // Determine which click type to dispatch
    let click_listener = if is_primary {
        super::EventListener::Click
    } else {
        super::EventListener::SecondaryClick
    };

    // Bubbling phase: dispatch Click from target to root
    for node in path.iter_bubbling() {
        if node.is_pointer_none {
            continue;
        }
        if node.has_disabled_defaults {
            continue;
        }

        let view_id = node.view_id;
        let view_state = view_id.state();

        // Transform event to local coordinates for bounds check
        let local_event = event.clone().transform(node.visual_transform);
        let local_point = state.logical_point();
        let local_point = node.visual_transform.inverse() * local_point;

        let rect = view_id.get_size().unwrap_or_default().to_rect();
        let on_view = rect.contains(local_point);

        if !on_view {
            continue;
        }

        // Check if this view was clicking (had pointer down on it)
        let is_clicking = cx.window_state.is_clicking(&view_id);

        // Get last_pointer_down state (only consume at target, check at ancestors)
        let last_pointer_down = view_state.borrow_mut().last_pointer_down.take();

        // For primary click
        if is_primary {
            // Double-click check (only if count == 2)
            let is_double = last_pointer_down.as_ref().is_some_and(|s| s.count == 2);
            if is_double && is_clicking {
                let handlers = view_state
                    .borrow()
                    .event_listeners
                    .get(&super::EventListener::DoubleClick)
                    .cloned();

                if let Some(handlers) = handlers {
                    let processed = handlers
                        .iter()
                        .any(|h| (h.borrow_mut())(&local_event).is_processed());
                    if processed {
                        return DispatchResult::Processed;
                    }
                }
            }

            // Regular click
            if is_clicking || last_pointer_down.is_some() {
                let handlers = view_state
                    .borrow()
                    .event_listeners
                    .get(&click_listener)
                    .cloned();

                if let Some(handlers) = handlers {
                    let processed = handlers
                        .iter()
                        .any(|h| (h.borrow_mut())(&local_event).is_processed());
                    if processed {
                        return DispatchResult::Processed;
                    }
                }
            }
        } else {
            // Secondary click
            if last_pointer_down.is_some() {
                let handlers = view_state
                    .borrow()
                    .event_listeners
                    .get(&click_listener)
                    .cloned();

                if let Some(handlers) = handlers {
                    let processed = handlers
                        .iter()
                        .any(|h| (h.borrow_mut())(&local_event).is_processed());
                    if processed {
                        return DispatchResult::Processed;
                    }
                }
            }
        }
    }

    DispatchResult::Continue
}

/// Handle focus update when event_before_children processes a pointer down event.
fn handle_focus_on_processed(
    node: &EventPathNode,
    event: &super::Event,
    cx: &mut super::dispatch::EventCx<'_>,
) {
    use super::Event;
    use ui_events::pointer::{PointerButtonEvent, PointerEvent};

    if let Event::Pointer(PointerEvent::Down(PointerButtonEvent { state, .. })) = event
        && node.is_focusable
    {
        let rect = node.view_id.get_size().unwrap_or_default().to_rect();
        if rect.contains(state.logical_point()) {
            cx.window_state.update_focus(node.view_id, false);
        }
    }
}

/// Handle built-in behaviors at the target node.
///
/// This includes: focus, click tracking, drag, hover, cursor, menus, etc.
fn handle_builtin_behaviors(
    node: &EventPathNode,
    local_event: &super::Event,
    _absolute_event: &super::Event,
    cx: &mut super::dispatch::EventCx<'_>,
) -> DispatchResult {
    use super::Event;
    use crate::action::show_context_menu;
    use crate::window::state::DragState;
    use ui_events::pointer::{PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate};

    let view_id = node.view_id;
    let view_state = view_id.state();

    match local_event {
        Event::Pointer(PointerEvent::Down(PointerButtonEvent {
            pointer,
            state,
            button,
            ..
        })) => {
            cx.window_state.clicking.insert(view_id);
            let point = state.logical_point();

            if pointer.is_primary_pointer() && button.is_none_or(|b| b == PointerButton::Primary) {
                let rect = view_id.get_size().unwrap_or_default().to_rect();
                let on_view = rect.contains(point);

                if on_view {
                    if node.is_focusable {
                        cx.window_state.update_focus(view_id, false);
                    }

                    // Track for double-click
                    if state.count == 2 {
                        let has_double_click = view_state
                            .borrow()
                            .event_listeners
                            .contains_key(&super::EventListener::DoubleClick);
                        if has_double_click {
                            view_state.borrow_mut().last_pointer_down = Some(state.clone());
                        }
                    }

                    // Track for click
                    let has_click = view_state
                        .borrow()
                        .event_listeners
                        .contains_key(&super::EventListener::Click);
                    if has_click {
                        view_state.borrow_mut().last_pointer_down = Some(state.clone());
                    }

                    // Popout menu
                    if node.has_popout_menu {
                        let (bottom_left, popout_menu) = {
                            let borrowed = view_state.borrow();
                            let layout = borrowed.layout_rect;
                            (
                                peniko::kurbo::Point::new(layout.x0, layout.y1),
                                borrowed.popout_menu.clone(),
                            )
                        };
                        if let Some(menu) = popout_menu {
                            show_context_menu(menu(), Some(bottom_left));
                            return DispatchResult::Processed;
                        }
                    }

                    // Drag start tracking
                    if node.can_drag && cx.window_state.drag_start.is_none() {
                        cx.window_state.drag_start = Some((view_id, point));
                    }
                }
            } else if button.is_some_and(|b| b == PointerButton::Secondary) {
                let rect = view_id.get_size().unwrap_or_default().to_rect();
                let on_view = rect.contains(point);

                if on_view {
                    if node.is_focusable {
                        cx.window_state.update_focus(view_id, false);
                    }

                    // Track for secondary click
                    let has_secondary = view_state
                        .borrow()
                        .event_listeners
                        .contains_key(&super::EventListener::SecondaryClick);
                    if has_secondary {
                        view_state.borrow_mut().last_pointer_down = Some(state.clone());
                    }
                }
            }
        }

        Event::Pointer(PointerEvent::Move(PointerUpdate { current, .. })) => {
            let rect = view_id.get_size().unwrap_or_default().to_rect();
            if rect.contains(current.logical_point()) {
                if cx.window_state.is_dragging() {
                    if !cx.window_state.dragging_over.contains(&view_id) {
                        cx.window_state.dragging_over.push(view_id);
                    }
                    view_id.apply_event(&super::EventListener::DragOver, local_event);
                } else {
                    if !cx.window_state.hovered.contains(&view_id) {
                        cx.window_state.hovered.push(view_id);
                    }
                    let cursor = view_state.borrow().combined_style.builtin().cursor();
                    if let Some(cursor) = cursor
                        && cx.window_state.cursor.is_none()
                    {
                        cx.window_state.cursor = Some(cursor);
                    }
                }
            }

            // Drag handling
            if node.can_drag
                && let Some((drag_id, drag_start)) = cx.window_state.drag_start.as_ref()
                && drag_id == &view_id
            {
                let drag_start = *drag_start;
                let offset = current.logical_point() - drag_start;

                if let Some(dragging) = cx
                    .window_state
                    .dragging
                    .as_mut()
                    .filter(|d| d.id == view_id && d.released_at.is_none())
                {
                    dragging.offset = drag_start.to_vec2();
                    cx.window_state.request_paint(view_id);
                } else if offset.x.abs() + offset.y.abs() > 1.0 {
                    cx.window_state.active = None;
                    cx.window_state.dragging = Some(DragState {
                        id: view_id,
                        offset: drag_start.to_vec2(),
                        released_at: None,
                        release_location: None,
                    });
                    cx.update_active(view_id);
                    cx.window_state.request_paint(view_id);
                    view_id.apply_event(&super::EventListener::DragStart, local_event);
                }
            }
        }

        Event::Pointer(PointerEvent::Up(PointerButtonEvent {
            button,
            pointer,
            state,
        })) => {
            if pointer.is_primary_pointer() && button.is_none_or(|b| b == PointerButton::Primary) {
                let rect = view_id.get_size().unwrap_or_default().to_rect();
                let on_view = rect.contains(state.logical_point());

                // Handle drop
                if on_view && let Some(dragging) = cx.window_state.dragging.as_mut() {
                    let dragging_id = dragging.id;
                    if view_id
                        .apply_event(&super::EventListener::Drop, local_event)
                        .is_some_and(|prop| prop.is_processed())
                    {
                        cx.window_state.dragging = None;
                        cx.window_state.request_paint(view_id);
                        dragging_id.apply_event(&super::EventListener::DragEnd, local_event);
                    }
                }

                // NOTE: Click/DoubleClick events are NOT fired here!
                // They are dispatched as separate synthetic events after PointerUp completes.
                // See dispatch_click_through_path() which is called after dispatch_through_path().
                // This matches Chromium's behavior where Click is a separate event that bubbles
                // through the entire DOM path, not just fired at the target.
            } else if button.is_some_and(|b| b == PointerButton::Secondary) {
                let rect = view_id.get_size().unwrap_or_default().to_rect();
                let on_view = rect.contains(state.logical_point());

                // Context menu (fires immediately, before secondary click)
                if on_view && node.has_context_menu {
                    let (position, context_menu) = {
                        let borrowed = view_state.borrow();
                        let layout = borrowed.layout_rect;
                        let pos = peniko::kurbo::Point::new(
                            layout.x0 + state.logical_point().x,
                            layout.y0 + state.logical_point().y,
                        );
                        (pos, borrowed.context_menu.clone())
                    };
                    if let Some(menu) = context_menu {
                        show_context_menu(menu(), Some(position));
                        return DispatchResult::Processed;
                    }
                }

                // NOTE: SecondaryClick events are dispatched separately after PointerUp.
            }
        }

        _ => {}
    }

    DispatchResult::Continue
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_path() {
        let path = EventPath::new();
        assert!(path.is_empty());
        assert_eq!(path.len(), 0);
        assert!(path.target().is_none());
    }

    #[test]
    fn test_path_iteration_order() {
        let mut path = EventPath::new();

        // Simulate target -> parent -> grandparent
        let target_id = ViewId::new();
        let parent_id = ViewId::new();
        let root_id = ViewId::new();

        path.push(EventPathNode {
            view_id: target_id,
            visual_transform: Affine::IDENTITY,
            has_event_listeners: true,
            has_context_menu: false,
            has_popout_menu: false,
            has_disabled_defaults: false,
            is_pointer_none: false,
            is_focusable: false,
            can_drag: false,
        });
        path.push(EventPathNode {
            view_id: parent_id,
            visual_transform: Affine::IDENTITY,
            has_event_listeners: false,
            has_context_menu: false,
            has_popout_menu: false,
            has_disabled_defaults: false,
            is_pointer_none: false,
            is_focusable: false,
            can_drag: false,
        });
        path.push(EventPathNode {
            view_id: root_id,
            visual_transform: Affine::IDENTITY,
            has_event_listeners: false,
            has_context_menu: false,
            has_popout_menu: false,
            has_disabled_defaults: false,
            is_pointer_none: false,
            is_focusable: false,
            can_drag: false,
        });

        // Bubbling: target -> parent -> root
        let bubbling: Vec<_> = path.iter_bubbling().map(|n| n.view_id).collect();
        assert_eq!(bubbling, vec![target_id, parent_id, root_id]);

        // Capturing: root -> parent -> target
        let capturing: Vec<_> = path.iter_capturing().map(|n| n.view_id).collect();
        assert_eq!(capturing, vec![root_id, parent_id, target_id]);
    }
}
