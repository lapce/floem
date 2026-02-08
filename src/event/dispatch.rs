//! Event dispatch logic for handling events through the view tree.

use peniko::kurbo::{Affine, Point};
use smallvec::SmallVec;
use ui_events::keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey};
use ui_events::pointer::{
    PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerState,
    PointerType, PointerUpdate,
};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use super::dropped_file::FileDragEvent;
use super::nav::view_arrow_navigation;
use super::path::{build_event_path, dispatch_click_through_path, dispatch_through_path, hit_test};
use super::{Event, EventListener, EventPropagation};
use crate::action::show_context_menu;
use crate::style::{Focusable, PointerEvents, PointerEventsProp, StyleSelector};
use crate::view::ViewId;
use crate::view::stacking::collect_stacking_context_items;
use crate::view::view_tab_navigation;
use crate::window::state::{DragState, WindowState};

/// Internal result type for event dispatch operations.
///
/// This is inspired by Chromium's two-level event result system where:
/// - The public API (`EventPropagation`) is what views return to control bubbling
/// - This internal type tracks both propagation AND pointer consumption state
///
/// The three meaningful combinations are:
/// - `Processed`: Event fully handled, stop propagation (view took action)
/// - `Consumed`: Pointer is over this view, but continue bubbling to parents
/// - `Skipped`: View didn't participate (hidden, disabled, or pointer missed)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DispatchOutcome {
    /// Event was fully processed - stop propagation immediately.
    /// The view took action and no other views should handle this event.
    Processed,
    /// A child consumed the pointer (pointer is over a view), but propagation continues.
    /// Used for hover/click tracking - the event bubbles up but siblings are skipped.
    Consumed,
    /// View didn't participate - continue to next sibling.
    /// The view was hidden, disabled, or the pointer wasn't over it.
    Skipped,
}

impl DispatchOutcome {
    /// Returns true if the event was fully processed and propagation should stop.
    pub fn is_processed(self) -> bool {
        matches!(self, DispatchOutcome::Processed)
    }
}

/// Result type for built-in event handlers that may stop propagation.
type BuiltinResult = Option<DispatchOutcome>;

/// Constant for a processed event result (stops propagation).
const PROCESSED: BuiltinResult = Some(DispatchOutcome::Processed);

/// A bundle of helper methods to be used by `View::event` handlers
pub struct EventCx<'a> {
    pub window_state: &'a mut WindowState,
}

impl EventCx<'_> {
    // =========================================================================
    // Public API
    // =========================================================================

    pub fn update_active(&mut self, id: ViewId) {
        self.window_state.update_active(id);
    }

    pub fn is_active(&self, id: ViewId) -> bool {
        self.window_state.is_active(&id)
    }

    #[allow(unused)]
    pub(crate) fn update_focus(&mut self, id: ViewId, keyboard_navigation: bool) {
        self.window_state.update_focus(id, keyboard_navigation);
    }

    // =========================================================================
    // Pointer Capture Processing (inspired by Chromium's ProcessPendingPointerCapture)
    // =========================================================================

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
            let event = Event::LostPointerCapture(pointer_id);
            old_target.apply_event(&EventListener::LostPointerCapture, &event);
        }

        // Fire GotPointerCapture to the new target
        if let Some(new_target) = pending_target {
            // Only set capture if the view is still connected
            if !new_target.is_hidden() {
                self.window_state.set_active_capture(pointer_id, new_target);
                let event = Event::GotPointerCapture(pointer_id);
                new_target.apply_event(&EventListener::GotPointerCapture, &event);

                // If the view was removed during the event handler, clean up
                if new_target.is_hidden() {
                    self.window_state.remove_active_capture(pointer_id);
                    let event = Event::LostPointerCapture(pointer_id);
                    new_target.apply_event(&EventListener::LostPointerCapture, &event);
                }
            }
        }
    }

    /// Process pending pointer captures for all pointers.
    ///
    /// Called before dispatching pointer events to ensure capture state is current.
    #[allow(dead_code)]
    pub(crate) fn process_all_pending_pointer_captures(&mut self) {
        // Collect all pointer IDs that have pending or current captures
        // Using SmallVec to avoid heap allocation in common case
        let pointer_ids: SmallVec<[PointerId; 4]> = self
            .window_state
            .pending_pointer_capture_target
            .iter()
            .map(|(id, _)| *id)
            .chain(
                self.window_state
                    .pointer_capture_target
                    .iter()
                    .map(|(id, _)| *id),
            )
            .collect();

        for pointer_id in pointer_ids {
            self.process_pending_pointer_capture(pointer_id);
        }
    }

    /// Get the pointer ID from a pointer event, if available.
    #[inline]
    fn get_pointer_id_from_event(event: &Event) -> Option<PointerId> {
        match event {
            Event::Pointer(PointerEvent::Down(PointerButtonEvent { pointer, .. }))
            | Event::Pointer(PointerEvent::Up(PointerButtonEvent { pointer, .. })) => {
                pointer.pointer_id
            }
            Event::Pointer(PointerEvent::Move(PointerUpdate { pointer, .. })) => pointer.pointer_id,
            Event::Pointer(PointerEvent::Leave(info))
            | Event::Pointer(PointerEvent::Enter(info))
            | Event::Pointer(PointerEvent::Cancel(info)) => info.pointer_id,
            _ => None,
        }
    }

    // =========================================================================
    // Main Entry Point
    // =========================================================================

    /// Dispatch an event through the view tree with proper state management.
    ///
    /// This is the core event processing logic shared between WindowHandle and HeadlessHarness.
    /// It handles:
    /// - Scaling the event coordinates by the window scale factor
    /// - Pre-dispatch state setup (clearing hover/clicking state as needed)
    /// - Event dispatch to the appropriate views (focus-based, active-based, or unconditional)
    /// - Post-dispatch state management (hover enter/leave, clicking state, focus changes)
    ///
    /// # Arguments
    /// * `root_id` - The root view ID of the window
    /// * `main_view_id` - The main content view ID (for keyboard event dispatch)
    /// * `event` - The event to dispatch
    ///
    /// # Returns
    /// The event propagation result
    pub(crate) fn dispatch_event(
        &mut self,
        root_id: ViewId,
        main_view_id: ViewId,
        event: Event,
    ) -> EventPropagation {
        // Scale the event coordinates by the window scale factor
        let event = event.transform(Affine::scale(self.window_state.scale));

        // Handle pointer move: track cursor position and prepare for hover state changes
        let is_pointer_move = if let Event::Pointer(PointerEvent::Move(pu)) = &event {
            let pos = pu.current.logical_point();
            self.window_state.last_cursor_location = pos;
            Some(pu.pointer)
        } else {
            None
        };

        // On pointer move, save previous hover/dragging state and clear for rebuild
        let (was_hovered, was_dragging_over) = if is_pointer_move.is_some() {
            self.window_state.cursor = None;
            let was_hovered = std::mem::take(&mut self.window_state.hovered);
            let was_dragging_over = std::mem::take(&mut self.window_state.dragging_over);
            (Some(was_hovered), Some(was_dragging_over))
        } else {
            (None, None)
        };

        // Track file hover changes
        let was_file_hovered = if matches!(event, Event::FileDrag(FileDragEvent::DragMoved { .. }))
            || is_pointer_move.is_some()
        {
            if !self.window_state.file_hovered.is_empty() {
                Some(std::mem::take(&mut self.window_state.file_hovered))
            } else {
                None
            }
        } else {
            None
        };

        // On pointer down, clear clicking state and save focus
        let is_pointer_down = matches!(&event, Event::Pointer(PointerEvent::Down { .. }));
        let was_focused = if is_pointer_down {
            self.window_state.clicking.clear();
            self.window_state.focus.take()
        } else {
            self.window_state.focus
        };

        // Dispatch the event based on its type and current state
        if event.needs_focus() {
            self.dispatch_keyboard_event(root_id, main_view_id, &event);
        } else if event.is_pointer() {
            // Process pending pointer captures before dispatching
            // This ensures capture state is current and fires got/lost events
            if let Some(pointer_id) = Self::get_pointer_id_from_event(&event) {
                self.process_pending_pointer_capture(pointer_id);

                // Check if this pointer has an active capture
                if let Some(capture_target) =
                    self.window_state.get_pointer_capture_target(pointer_id)
                {
                    // Route to capture target instead of hit-testing
                    self.dispatch_to_captured_view(capture_target, &event);

                    // On pointer up, release implicit capture
                    if matches!(&event, Event::Pointer(PointerEvent::Up { .. })) {
                        self.window_state
                            .release_pointer_capture_unconditional(pointer_id);
                    }
                } else if self.window_state.active.is_some() {
                    // Legacy active view handling (for drag operations)
                    self.dispatch_to_active_view(root_id, &event);
                } else {
                    // Pointer events use Chromium-style path-based dispatch:
                    // 1. Hit test finds target (z-index aware)
                    // 2. Event path built from target to root (DOM order)
                    // 3. Dispatch through path (capturing + bubbling)
                    // 4. Click events dispatched separately after PointerUp
                    self.dispatch_pointer_event_via_path(root_id, &event);
                }
            } else if self.window_state.active.is_some() {
                self.dispatch_to_active_view(root_id, &event);
            } else {
                self.dispatch_pointer_event_via_path(root_id, &event);
            }
        } else {
            // Non-pointer events use stacking context dispatch
            self.dispatch_to_view(root_id, &event, false);
        }

        // Clear drag_start on pointer up
        if let Event::Pointer(PointerEvent::Up { .. }) = &event {
            self.window_state.drag_start = None;
        }

        // Handle hover state changes - send PointerEnter/Leave events
        if let Some(pointer_info) = is_pointer_move {
            self.handle_hover_changes(
                pointer_info,
                was_hovered.unwrap(),
                was_dragging_over.unwrap(),
                &event,
            );
        }

        // Handle file hover style changes
        if let Some(was_file_hovered) = was_file_hovered {
            for id in was_file_hovered.symmetric_difference(&self.window_state.file_hovered) {
                id.request_style();
            }
        }

        // Handle focus changes
        if was_focused != self.window_state.focus {
            self.window_state
                .focus_changed(was_focused, self.window_state.focus);
        }

        // Update active styles for clicking views on pointer down/up
        // Use selector-aware method to only update views with :active styles
        let is_pointer_up = matches!(&event, Event::Pointer(PointerEvent::Up { .. }));
        if is_pointer_down || is_pointer_up {
            for id in self.window_state.clicking.clone() {
                if self
                    .window_state
                    .has_style_for_sel(id, StyleSelector::Active)
                {
                    id.request_style_for_selector_recursive(StyleSelector::Active);
                }
            }
            if is_pointer_up {
                self.window_state.clicking.clear();
            }
        }

        EventPropagation::Continue
    }

    // =========================================================================
    // Dispatch Methods
    // =========================================================================

    /// Dispatch a pointer event using Chromium-style path-based dispatch.
    ///
    /// This implements the three-phase dispatch model:
    /// 1. Hit test to find the target view (z-index aware)
    /// 2. Build event path from target to root (DOM order)
    /// 3. Dispatch through path:
    ///    - Capturing phase (root → target): `event_before_children`
    ///    - Bubbling phase (target → root): `event_after_children` + listeners
    /// 4. For PointerUp: dispatch Click events as separate synthetic events
    ///
    /// For touch pointers, implicit capture is set on PointerDown following
    /// the W3C Pointer Events spec and Chromium's behavior.
    ///
    /// Returns true if the event was processed.
    pub(crate) fn dispatch_pointer_event_via_path(
        &mut self,
        root_id: ViewId,
        event: &Event,
    ) -> bool {
        // Get the point from the event
        let Some(point) = event.point() else {
            // No point = not a pointer event, fall back to stacking context dispatch
            return self.dispatch_to_view(root_id, event, false).is_processed();
        };

        // Phase 1: Hit test to find the target (z-index aware)
        let Some(target) = hit_test(root_id, point) else {
            // No target found, nothing to dispatch to
            return false;
        };

        // Phase 2: Build event path from target to root (DOM order)
        let path = build_event_path(target);

        // Phase 3: Dispatch through the path (capturing + bubbling)
        let result = dispatch_through_path(&path, event, self);

        // Phase 4: For PointerUp, dispatch Click events separately
        // This matches Chromium's behavior where Click is a synthetic event
        // that bubbles through the entire path after PointerUp completes.
        if matches!(
            event,
            Event::Pointer(ui_events::pointer::PointerEvent::Up { .. })
        ) {
            dispatch_click_through_path(&path, event, self);
        }

        // Implicit touch capture: For touch pointers, automatically capture on PointerDown
        // This follows the W3C Pointer Events spec and Chromium's behavior where
        // touch interactions implicitly capture to the target element.
        // IMPORTANT: This is set AFTER event dispatch, matching Chromium's timing.
        // This allows handlers during PointerDown to call releasePointerCapture()
        // to prevent implicit capture if desired.
        if let Event::Pointer(PointerEvent::Down(PointerButtonEvent { pointer, .. })) = event
            && pointer.pointer_type == PointerType::Touch
            && let Some(pointer_id) = pointer.pointer_id
        {
            // Only set implicit capture if no explicit capture was set during dispatch
            // and no explicit release was requested
            if !self.window_state.has_pending_capture(pointer_id) {
                self.window_state.set_pointer_capture(pointer_id, target);
            }
        }

        result.is_processed()
    }

    /// Internal method used by Floem. This can be called from parent `View`s to propagate an event to the child `View`.
    #[inline]
    pub(crate) fn dispatch_to_view(
        &mut self,
        view_id: ViewId,
        event: &Event,
        directed: bool,
    ) -> DispatchOutcome {
        if view_id.is_hidden() || (view_id.is_disabled() && !event.allow_disabled()) {
            return DispatchOutcome::Skipped;
        }

        let absolute_event = event;
        let view = view_id.view();
        let view_state = view_id.state();

        // Single borrow to extract all needed fields (hot path optimization)
        let (visual_transform, disable_default, is_pointer_none) = {
            let vs = view_state.borrow();
            (
                vs.visual_transform,
                event
                    .listener()
                    .is_some_and(|l| vs.disable_default_events.contains(&l)),
                event.is_pointer()
                    && vs.computed_style.get(PointerEventsProp) == Some(PointerEvents::None),
            )
        };

        let event = absolute_event.clone().transform(visual_transform);
        let can_process = !disable_default && !is_pointer_none;

        // Phase 1: Let view handle event before children
        if can_process
            && view
                .borrow_mut()
                .event_before_children(self, &event)
                .is_processed()
        {
            self.apply_processed_side_effects(view_id, &view_state, &event);
            return DispatchOutcome::Processed;
        }

        // Phase 2: Dispatch to children
        let child_consumed = if !directed {
            match self.dispatch_to_children(view_id, absolute_event) {
                DispatchOutcome::Processed => return DispatchOutcome::Processed,
                DispatchOutcome::Consumed => true,
                DispatchOutcome::Skipped => false,
            }
        } else {
            false
        };

        // Phase 3: Let view handle event after children
        if can_process
            && view
                .borrow_mut()
                .event_after_children(self, &event)
                .is_processed()
        {
            return DispatchOutcome::Processed;
        }

        if is_pointer_none {
            return if child_consumed {
                DispatchOutcome::Consumed
            } else {
                DispatchOutcome::Skipped
            };
        }

        // Phase 4: Built-in behaviors and listeners
        if let Some(result) = can_process
            .then(|| self.handle_default_behaviors(view_id, &view_state, &event, directed))
            .flatten()
        {
            return result;
        }

        DispatchOutcome::Consumed
    }

    /// Dispatch events to children using simplified stacking semantics.
    ///
    /// Iterates through direct children in reverse z-order (highest z-index first).
    /// Each child handles its own children recursively. Children are bounded within
    /// their parent (they never "escape" to compete with ancestors' siblings).
    #[inline]
    fn dispatch_to_children(&mut self, view_id: ViewId, event: &Event) -> DispatchOutcome {
        let items = collect_stacking_context_items(view_id);

        for item in items.iter().rev() {
            let should_send = self.should_send(item.view_id, event);

            // Even if the parent fails clip test, children may be outside clip rect
            // (e.g., dropdowns). We still recurse into the view's dispatch_to_view
            // which will handle its own children.
            if !should_send {
                // Still dispatch to the view in case it has children outside its clip rect
                if self
                    .dispatch_to_view(item.view_id, event, false)
                    .is_processed()
                {
                    return DispatchOutcome::Processed;
                }
                continue;
            }

            let outcome = self.dispatch_to_view(item.view_id, event, false);

            if outcome.is_processed() {
                return DispatchOutcome::Processed;
            }

            if event.is_pointer() && outcome == DispatchOutcome::Consumed {
                return DispatchOutcome::Consumed;
            }
        }

        DispatchOutcome::Skipped
    }

    /// Dispatch keyboard events to focused view with navigation handling.
    fn dispatch_keyboard_event(&mut self, root_id: ViewId, main_view_id: ViewId, event: &Event) {
        let mut processed = false;

        if let Some(id) = self.window_state.focus {
            processed |= self.dispatch_to_view(id, event, true).is_processed();
        }

        if !processed && let Some(listener) = event.listener() {
            processed |= main_view_id
                .apply_event(&listener, event)
                .is_some_and(|prop| prop.is_processed());
        }

        if !processed {
            // Handle Tab and arrow key navigation
            if let Event::Key(KeyboardEvent {
                key,
                modifiers,
                state: KeyState::Down,
                ..
            }) = event
            {
                if *key == Key::Named(NamedKey::Tab)
                    && (modifiers.is_empty() || *modifiers == Modifiers::SHIFT)
                {
                    let backwards = modifiers.contains(Modifiers::SHIFT);
                    view_tab_navigation(root_id, self.window_state, backwards);
                } else if *modifiers == Modifiers::ALT
                    && let Key::Named(
                        name @ (NamedKey::ArrowUp
                        | NamedKey::ArrowDown
                        | NamedKey::ArrowLeft
                        | NamedKey::ArrowRight),
                    ) = key
                {
                    view_arrow_navigation(*name, self.window_state, root_id);
                }
            }

            // Handle keyboard trigger end (space/enter key up on focused element)
            let keyboard_trigger_end = self.window_state.keyboard_navigation
                && event.is_keyboard_trigger()
                && matches!(
                    event,
                    Event::Key(KeyboardEvent {
                        state: KeyState::Up,
                        ..
                    })
                );
            if keyboard_trigger_end && let Some(id) = self.window_state.active {
                if self
                    .window_state
                    .has_style_for_sel(id, StyleSelector::Active)
                {
                    id.request_style_for_selector_recursive(StyleSelector::Active);
                }
                self.window_state.active = None;
            }
        }
    }

    /// Dispatch pointer events to the currently active view (during drag operations).
    fn dispatch_to_active_view(&mut self, root_id: ViewId, event: &Event) {
        if self.window_state.is_dragging() {
            self.dispatch_to_view(root_id, event, false);
        }

        let id = self.window_state.active.unwrap();

        // dispatch_to_view handles the coordinate transformation internally
        // using visual_transform, so we pass the event unchanged
        self.dispatch_to_view(id, event, true);

        if let Event::Pointer(PointerEvent::Up { .. }) = event {
            if self
                .window_state
                .has_style_for_sel(id, StyleSelector::Active)
            {
                id.request_style_for_selector_recursive(StyleSelector::Active);
            }
            self.window_state.active = None;
        }
    }

    /// Dispatch pointer events to a view that has pointer capture.
    ///
    /// Similar to dispatch_to_active_view, but specifically for pointer capture.
    /// Events are transformed to the capture target's local coordinate space.
    fn dispatch_to_captured_view(&mut self, capture_target: ViewId, event: &Event) {
        // dispatch_to_view handles the coordinate transformation internally
        // using visual_transform, so we pass the event unchanged
        self.dispatch_to_view(capture_target, event, true);
    }

    // =========================================================================
    // Dispatch Helpers
    // =========================================================================

    /// Used to determine if you should send an event to another view. This is basically a check for pointer events to see if the pointer is inside a child view and to make sure the current view isn't hidden or disabled.
    /// Usually this is used if you want to propagate an event to a child view
    ///
    /// Note: This function expects event coordinates to be in absolute (window) coordinates,
    /// as used by the stacking context event dispatch. The layout_rect and clip_rect are also
    /// in absolute coordinates, so they can be compared directly.
    #[inline]
    pub fn should_send(&mut self, id: ViewId, event: &Event) -> bool {
        if id.is_hidden() || (id.is_disabled() && !event.allow_disabled()) {
            return false;
        }

        let Some(point) = event.point() else {
            return true;
        };

        let view_state = id.state();
        let vs = view_state.borrow();

        // First check if the point is within the clip bounds.
        // This handles cases where the view is clipped by an ancestor's
        // overflow:hidden or scroll container.
        if !vs.clip_rect.contains(point) {
            return false;
        }

        // Use the absolute layout_rect directly since stacking context dispatch
        // uses absolute event coordinates
        let layout_rect = vs.layout_rect;

        // Apply any style transformations to the rect
        let current_rect = vs.transform.transform_rect_bbox(layout_rect);

        current_rect.contains(point)
    }

    /// Apply side effects when event_before_children returns processed (focus, cursor).
    fn apply_processed_side_effects(
        &mut self,
        view_id: ViewId,
        view_state: &std::cell::RefCell<crate::view::ViewState>,
        event: &Event,
    ) {
        if let Event::Pointer(PointerEvent::Down(PointerButtonEvent { state, .. })) = event
            && view_state.borrow().computed_style.get(Focusable)
        {
            let rect = view_id.get_size().unwrap_or_default().to_rect();
            if rect.contains(state.logical_point()) {
                self.window_state.update_focus(view_id, false);
            }
        }
        if let Event::Pointer(PointerEvent::Move(_)) = event
            && let Some(cursor) = view_state.borrow().combined_style.builtin().cursor()
            && self.window_state.cursor.is_none()
        {
            self.window_state.cursor = Some(cursor);
        }
    }

    // =========================================================================
    // Built-in Event Behaviors
    // =========================================================================

    /// Handle all default behaviors for an event: built-in behaviors and event listeners.
    #[inline]
    fn handle_default_behaviors(
        &mut self,
        view_id: ViewId,
        view_state: &std::cell::RefCell<crate::view::ViewState>,
        event: &Event,
        directed: bool,
    ) -> BuiltinResult {
        // Built-in behaviors by event type
        let result = match event {
            Event::Pointer(PointerEvent::Down(PointerButtonEvent {
                pointer,
                state,
                button,
                ..
            })) => self.handle_pointer_down(view_id, pointer, state, button),

            Event::Pointer(PointerEvent::Move(PointerUpdate { current, .. })) => {
                self.handle_pointer_move(view_id, current, event)
            }

            Event::Pointer(PointerEvent::Up(PointerButtonEvent {
                button,
                pointer,
                state,
            })) => self.handle_pointer_up(view_id, pointer, state, button, event, directed),

            Event::Key(KeyboardEvent {
                state: KeyState::Down,
                ..
            }) => {
                if self.window_state.is_focused(&view_id) && event.is_keyboard_trigger() {
                    view_id.apply_event(&EventListener::Click, event);
                }
                None
            }

            Event::WindowResized(_) => {
                if view_state.borrow().has_style_selectors.has_responsive() {
                    view_id.request_style();
                }
                None
            }

            Event::FileDrag(e @ FileDragEvent::DragMoved { .. }) => {
                if let Some(point) = e.logical_point() {
                    let rect = view_id.get_size().unwrap_or_default().to_rect();
                    if rect.contains(point) {
                        self.window_state.file_hovered.insert(view_id);
                        view_id.request_style();
                    }
                }
                None
            }

            _ => None,
        };

        if result.is_some() {
            return result;
        }

        // Dispatch to registered event listeners
        self.try_dispatch_to_listeners(view_id, event)
    }

    /// Handle built-in PointerDown behaviors: clicking state, focus, drag start, popout menu.
    fn handle_pointer_down(
        &mut self,
        view_id: ViewId,
        pointer: &PointerInfo,
        state: &PointerState,
        button: &Option<PointerButton>,
    ) -> BuiltinResult {
        let view_state = view_id.state();
        self.window_state.clicking.insert(view_id);

        // Always request style update on pointer down to apply Active selector.
        // TODO: optimize by checking has_style_selectors.has(Active) once we verify
        // that has_style_selectors is reliably populated
        self.window_state.style_dirty.insert(view_id);
        view_state
            .borrow_mut()
            .requested_changes
            .insert(crate::view::state::ChangeFlags::STYLE);

        let point = state.logical_point();
        let rect = view_id.get_size().unwrap_or_default().to_rect();
        let on_view = rect.contains(point);

        if pointer.is_primary_pointer() && button.is_none_or(|b| b == PointerButton::Primary) {
            if on_view {
                // Update focus if focusable
                if view_state.borrow().computed_style.get(Focusable) {
                    self.window_state.update_focus(view_id, false);
                }

                // Track pointer down for click/double-click detection
                let needs_tracking = {
                    let listeners = &view_state.borrow().event_listeners;
                    listeners.contains_key(&EventListener::Click)
                        || (state.count == 2 && listeners.contains_key(&EventListener::DoubleClick))
                };
                if needs_tracking {
                    view_state.borrow_mut().last_pointer_down = Some(state.clone());
                }

                // Show popout menu (all platforms show on pointer down)
                if let Some(result) = self.try_show_popout_menu(view_id) {
                    return Some(result);
                }

                // Initialize drag if view supports it
                if view_id.can_drag() && self.window_state.drag_start.is_none() {
                    self.window_state.drag_start = Some((view_id, point));
                }
            }
        } else if button.is_some_and(|b| b == PointerButton::Secondary) && on_view {
            // Secondary button: focus and track for secondary click
            let (is_focusable, has_secondary_click) = {
                let vs = view_state.borrow();
                (
                    vs.computed_style.get(Focusable),
                    vs.event_listeners
                        .contains_key(&EventListener::SecondaryClick),
                )
            };
            if is_focusable {
                self.window_state.update_focus(view_id, false);
            }
            if has_secondary_click {
                view_state.borrow_mut().last_pointer_down = Some(state.clone());
            }
        }

        None
    }

    /// Handle built-in PointerMove behaviors: hover, cursor, drag state updates.
    fn handle_pointer_move(
        &mut self,
        view_id: ViewId,
        current: &PointerState,
        event: &Event,
    ) -> BuiltinResult {
        let view_state = view_id.state();
        let rect = view_id.get_size().unwrap_or_default().to_rect();
        let point = current.logical_point();

        // Track hover/drag-over state
        if rect.contains(point) {
            if self.window_state.is_dragging() {
                if !self.window_state.dragging_over.contains(&view_id) {
                    self.window_state.dragging_over.push(view_id);
                }
                view_id.apply_event(&EventListener::DragOver, event);
            } else {
                if !self.window_state.hovered.contains(&view_id) {
                    self.window_state.hovered.push(view_id);
                }
                let vs = view_state.borrow();
                if let Some(cursor) = vs.combined_style.builtin().cursor()
                    && self.window_state.cursor.is_none()
                {
                    self.window_state.cursor = Some(cursor);
                }
            }
        }

        // Handle drag state updates
        if view_id.can_drag()
            && let Some((_, drag_start)) = self
                .window_state
                .drag_start
                .as_ref()
                .filter(|(drag_id, _)| drag_id == &view_id)
        {
            let offset = point - *drag_start;

            if let Some(dragging) = self
                .window_state
                .dragging
                .as_mut()
                .filter(|d| d.id == view_id && d.released_at.is_none())
            {
                // Update position while dragging
                dragging.offset = drag_start.to_vec2();
                self.window_state.request_paint(view_id);
            } else if offset.x.abs() + offset.y.abs() > 1.0 {
                // Start dragging when moved > 1px
                self.window_state.active = None;
                self.window_state.dragging = Some(DragState {
                    id: view_id,
                    offset: drag_start.to_vec2(),
                    released_at: None,
                    release_location: None,
                });
                self.update_active(view_id);
                self.window_state.request_paint(view_id);
                view_id.apply_event(&EventListener::DragStart, event);
            }
        }

        // Check if PointerMove listener stops propagation
        if view_id
            .apply_event(&EventListener::PointerMove, event)
            .is_some_and(|prop| prop.is_processed())
        {
            return PROCESSED;
        }

        None
    }

    /// Handle built-in PointerUp behaviors: click/double-click, drag end, context menu.
    fn handle_pointer_up(
        &mut self,
        view_id: ViewId,
        pointer: &PointerInfo,
        state: &PointerState,
        button: &Option<PointerButton>,
        event: &Event,
        directed: bool,
    ) -> BuiltinResult {
        let view_state = view_id.state();
        let rect = view_id.get_size().unwrap_or_default().to_rect();
        let on_view = rect.contains(state.logical_point());

        if pointer.is_primary_pointer() && button.is_none_or(|b| b == PointerButton::Primary) {
            // Show popout menu on non-macOS (pointer up)
            #[cfg(not(target_os = "macos"))]
            if on_view && let Some(result) = self.try_show_popout_menu(view_id) {
                return Some(result);
            }

            // Handle drag drop
            if !directed {
                if on_view && let Some(dragging) = self.window_state.dragging.as_mut() {
                    let dragging_id = dragging.id;
                    if view_id
                        .apply_event(&EventListener::Drop, event)
                        .is_some_and(|prop| prop.is_processed())
                    {
                        self.window_state.dragging = None;
                        self.window_state.request_paint(view_id);
                        dragging_id.apply_event(&EventListener::DragEnd, event);
                    }
                }
            } else if let Some(dragging) = self
                .window_state
                .dragging
                .as_mut()
                .filter(|d| d.id == view_id)
            {
                // Directed event to the dragged view itself
                let dragging_id = dragging.id;
                dragging.released_at = Some(Instant::now());
                dragging.release_location = Some(state.logical_point());
                self.window_state.request_paint(view_id);
                dragging_id.apply_event(&EventListener::DragEnd, event);
            }

            // Handle double-click and click
            let last_pointer_down = view_state.borrow_mut().last_pointer_down.take();
            let is_double = last_pointer_down.as_ref().is_some_and(|s| s.count == 2);
            let is_clicking = self.window_state.is_clicking(&view_id);

            if let Some(result) = self.try_click_handler(
                view_id,
                EventListener::DoubleClick,
                on_view,
                is_clicking && is_double,
                event,
            ) {
                return Some(result);
            }

            if let Some(result) = self.try_click_handler(
                view_id,
                EventListener::Click,
                on_view,
                is_clicking && last_pointer_down.is_some(),
                event,
            ) {
                return Some(result);
            }

            // Check if PointerUp listener stops propagation
            if view_id
                .apply_event(&EventListener::PointerUp, event)
                .is_some_and(|prop| prop.is_processed())
            {
                return PROCESSED;
            }
        } else if button.is_some_and(|b| b == PointerButton::Secondary) {
            // Handle secondary click and context menu
            let last_pointer_down = view_state.borrow_mut().last_pointer_down.take();
            if let Some(result) = self.try_click_handler(
                view_id,
                EventListener::SecondaryClick,
                on_view,
                last_pointer_down.is_some(),
                event,
            ) {
                return Some(result);
            }

            if on_view && let Some(result) = self.try_show_context_menu(view_id, state) {
                return Some(result);
            }
        }

        None
    }

    // =========================================================================
    // Click & Listener Helpers
    // =========================================================================

    /// Try to dispatch an event to registered listeners for the view.
    #[inline]
    fn try_dispatch_to_listeners(&self, view_id: ViewId, event: &Event) -> BuiltinResult {
        let listener = event.listener()?;
        let handlers = view_id
            .state()
            .borrow()
            .event_listeners
            .get(&listener)
            .cloned()?;

        // Check if pointer is within view bounds (for pointer events)
        let should_run = event
            .point()
            .map(|pos| {
                view_id
                    .get_size()
                    .unwrap_or_default()
                    .to_rect()
                    .contains(pos)
            })
            .unwrap_or(true);

        if !should_run {
            return None;
        }

        let processed = handlers
            .iter()
            .any(|h| (h.borrow_mut())(event).is_processed());

        if processed {
            return PROCESSED;
        }
        None
    }

    /// Try to trigger click-type handlers (click, double-click, secondary-click).
    ///
    /// The `extra_condition` parameter allows each click type to specify additional
    /// requirements (e.g., double-click requires count == 2, click requires clicking state).
    fn try_click_handler(
        &self,
        view_id: ViewId,
        listener: EventListener,
        on_view: bool,
        extra_condition: bool,
        event: &Event,
    ) -> BuiltinResult {
        if !on_view || !extra_condition {
            return None;
        }

        let handlers = view_id
            .state()
            .borrow()
            .event_listeners
            .get(&listener)
            .cloned()?;

        let processed = handlers
            .iter()
            .any(|h| (h.borrow_mut())(event).is_processed());

        processed.then_some(PROCESSED).flatten()
    }

    // =========================================================================
    // Menu Helpers
    // =========================================================================

    /// Try to show popout menu for a view.
    fn try_show_popout_menu(&self, view_id: ViewId) -> BuiltinResult {
        let view_state = view_id.state();
        let (bottom_left, popout_menu) = {
            let vs = view_state.borrow();
            let layout = vs.layout_rect;
            (Point::new(layout.x0, layout.y1), vs.popout_menu.clone())
        };
        if let Some(menu) = popout_menu {
            show_context_menu(menu(), Some(bottom_left));
            return PROCESSED;
        }
        None
    }

    /// Try to show context menu for a view.
    fn try_show_context_menu(
        &self,
        view_id: ViewId,
        pointer_state: &PointerState,
    ) -> BuiltinResult {
        let view_state = view_id.state();
        let (position, context_menu) = {
            let vs = view_state.borrow();
            let layout = vs.layout_rect;
            let pos = Point::new(
                layout.x0 + pointer_state.logical_point().x,
                layout.y0 + pointer_state.logical_point().y,
            );
            (pos, vs.context_menu.clone())
        };
        if let Some(menu) = context_menu {
            show_context_menu(menu(), Some(position));
            return PROCESSED;
        }
        None
    }

    // =========================================================================
    // State Change Helpers
    // =========================================================================

    /// Handle hover state changes by sending PointerEnter/Leave events.
    /// Uses SmallVec for efficient iteration over small sets of hovered views.
    ///
    /// Optimizations:
    /// - Early exit when hover/drag state hasn't changed (common case)
    /// - Selector-aware style updates to only dirty views with :hover/:active styles
    #[inline]
    fn handle_hover_changes(
        &mut self,
        pointer_info: PointerInfo,
        was_hovered: crate::window::state::ViewIdSmallSet,
        was_dragging_over: crate::window::state::ViewIdSmallSet,
        event: &Event,
    ) {
        // Clone current state to avoid borrow conflicts during mutation
        let hovered = self.window_state.hovered.clone();
        let dragging_over = self.window_state.dragging_over.clone();

        // Fast path: if hover state hasn't changed, skip all processing
        // This is common when the pointer moves within the same view
        let hover_changed = was_hovered != hovered;
        let drag_changed = was_dragging_over != dragging_over;

        if !hover_changed && !drag_changed {
            return;
        }

        if hover_changed {
            // Process views that were hovered but no longer are (leave events)
            for id in was_hovered.iter() {
                if !hovered.contains(id) {
                    // Check if style update is needed
                    let view_state = id.state();
                    let (needs_style_update, has_hover, has_active) = {
                        let vs = view_state.borrow();
                        (
                            vs.has_active_animation()
                                || vs.has_style_selectors.has(StyleSelector::Hover)
                                || vs.has_style_selectors.has(StyleSelector::Active),
                            vs.has_style_selectors.has(StyleSelector::Hover),
                            vs.has_style_selectors.has(StyleSelector::Active),
                        )
                    };
                    if needs_style_update {
                        // Use selector-aware updates to only dirty views with matching selectors
                        if has_hover {
                            id.request_style_for_selector_recursive(StyleSelector::Hover);
                        } else if has_active {
                            id.request_style_for_selector_recursive(StyleSelector::Active);
                        } else {
                            // Has animation but no hover/active selectors
                            id.request_style();
                        }
                    }
                    let leave_event = Event::Pointer(PointerEvent::Leave(pointer_info));
                    self.dispatch_to_view(*id, &leave_event, true);
                }
            }

            // Process views that are now hovered but weren't before (enter events)
            for id in hovered.iter() {
                if !was_hovered.contains(id) {
                    let view_state = id.state();
                    let (needs_style_update, has_hover, has_active) = {
                        let vs = view_state.borrow();
                        (
                            vs.has_active_animation()
                                || vs.has_style_selectors.has(StyleSelector::Hover)
                                || vs.has_style_selectors.has(StyleSelector::Active),
                            vs.has_style_selectors.has(StyleSelector::Hover),
                            vs.has_style_selectors.has(StyleSelector::Active),
                        )
                    };
                    if needs_style_update {
                        // Use selector-aware updates to only dirty views with matching selectors
                        if has_hover {
                            id.request_style_for_selector_recursive(StyleSelector::Hover);
                        } else if has_active {
                            id.request_style_for_selector_recursive(StyleSelector::Active);
                        } else {
                            // Has animation but no hover/active selectors
                            id.request_style();
                        }
                    }
                    id.apply_event(&EventListener::PointerEnter, event);
                }
            }
        }

        if drag_changed {
            // Handle drag leave events (views that were dragged over but no longer are)
            for id in was_dragging_over.iter() {
                if !dragging_over.contains(id) {
                    id.apply_event(&EventListener::DragLeave, event);
                }
            }

            // Handle drag enter events (views that are now being dragged over)
            for id in dragging_over.iter() {
                if !was_dragging_over.contains(id) {
                    id.apply_event(&EventListener::DragEnter, event);
                }
            }
        }
    }
}
