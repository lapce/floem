//! Event dispatch logic for handling events through the view tree.

use peniko::kurbo::{Affine, Point};
use smallvec::SmallVec;
use ui_events::keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey};
use ui_events::pointer::{PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use super::stacking::collect_stacking_context_items;
use super::{Event, EventListener, EventPropagation};
use crate::action::show_context_menu;
use crate::dropped_file::FileDragEvent;
use crate::id::ViewId;
use crate::nav::view_arrow_navigation;
use crate::style::{Focusable, PointerEvents, PointerEventsProp, StyleSelector};
use crate::view::view_tab_navigation;
use crate::window_state::{DragState, WindowState};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PointerEventConsumed {
    Yes,
    No,
}

/// A bundle of helper methods to be used by `View::event` handlers
pub struct EventCx<'a> {
    pub window_state: &'a mut WindowState,
    /// When dispatching events in a stacking context, this tracks views whose children are
    /// handled by the parent stacking context. When event dispatch reaches such a view,
    /// it should not dispatch to its children (they're handled separately).
    pub(crate) skip_children_for: Option<ViewId>,
}

impl EventCx<'_> {
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

    /// Internal method used by Floem. This can be called from parent `View`s to propagate an event to the child `View`.
    pub(crate) fn unconditional_view_event(
        &mut self,
        view_id: ViewId,
        event: &Event,
        directed: bool,
    ) -> (EventPropagation, PointerEventConsumed) {
        if view_id.is_hidden() {
            // we don't process events for hidden view
            return (EventPropagation::Continue, PointerEventConsumed::No);
        }
        if view_id.is_disabled() && !event.allow_disabled() {
            // if the view is disabled and the event is not processed
            // for disabled views
            return (EventPropagation::Continue, PointerEventConsumed::No);
        }

        // TODO! Handle file hover

        // The event parameter is in absolute (window) coordinates.
        // We keep a reference for should_send() and stacking context dispatch.
        let absolute_event = event;

        // Convert absolute coordinates to view's local coordinates using the
        // precomputed local_to_root_transform. We clone here because we need
        // both absolute and local versions of the event.
        let local_to_root = view_id.state().borrow().local_to_root_transform;
        let event = absolute_event.clone().transform(local_to_root);

        let view = view_id.view();
        let view_state = view_id.state();

        let disable_default = if let Some(listener) = event.listener() {
            view_state
                .borrow()
                .disable_default_events
                .contains(&listener)
        } else {
            false
        };

        let is_pointer_none = event.is_pointer()
            && view_state.borrow().computed_style.get(PointerEventsProp)
                == Some(PointerEvents::None);

        if !disable_default
            && !is_pointer_none
            && view
                .borrow_mut()
                .event_before_children(self, &event)
                .is_processed()
        {
            if let Event::Pointer(PointerEvent::Down(PointerButtonEvent { state, .. })) = &event {
                if view_state.borrow().computed_style.get(Focusable) {
                    let rect = view_id.get_size().unwrap_or_default().to_rect();
                    let point = state.logical_point();
                    let now_focused = rect.contains(point);
                    if now_focused {
                        self.window_state.update_focus(view_id, false);
                    }
                }
            }
            if let Event::Pointer(PointerEvent::Move(_)) = &event {
                let view_state = view_state.borrow();
                let style = view_state.combined_style.builtin();
                if let Some(cursor) = style.cursor() {
                    if self.window_state.cursor.is_none() {
                        self.window_state.cursor = Some(cursor);
                    }
                }
            }
            return (EventPropagation::Stop, PointerEventConsumed::Yes);
        }

        let mut view_pointer_event_consumed = PointerEventConsumed::No;

        // Dispatch events to children using true CSS stacking context semantics.
        // Views that don't create stacking contexts have their children participate
        // in the parent's stacking context, so we skip them here (they're handled separately).
        if !directed && self.skip_children_for != Some(view_id) {
            // Collect all items in this stacking context and iterate in reverse
            // (highest z-index first, so topmost elements receive events first)
            let items = collect_stacking_context_items(view_id);

            // Track the consuming item's parent chain for event bubbling
            let mut consuming_item_parent_chain: Option<&SmallVec<[ViewId; 8]>> = None;

            for item in items.iter().rev() {
                // Use should_send (with absolute coordinates) to check if the event point
                // is inside the item. The absolute_event and layout_rect are both in
                // window coordinates.
                if !self.should_send(item.view_id, absolute_event) {
                    continue;
                }

                // For non-stacking-context items, mark them so their children
                // aren't processed again (they're in our flat list)
                if !item.creates_context {
                    self.skip_children_for = Some(item.view_id);
                }

                // Pass the absolute event reference - unconditional_view_event
                // converts to local using the item's own local_to_root_transform.
                let (event_propagation, pointer_event_consumed) =
                    self.unconditional_view_event(item.view_id, absolute_event, false);

                // Clear the skip flag
                self.skip_children_for = None;

                if event_propagation.is_processed() {
                    return (EventPropagation::Stop, PointerEventConsumed::Yes);
                }
                if event.is_pointer() && pointer_event_consumed == PointerEventConsumed::Yes {
                    // if a child's pointer event was consumed because pointer-events: auto
                    // we don't pass the pointer event the next child
                    // also, we mark pointer_event_consumed to be yes
                    // so that it will be bubbled up the parent
                    view_pointer_event_consumed = PointerEventConsumed::Yes;
                    // Track the consuming item's cached parent chain for bubbling
                    consuming_item_parent_chain = Some(&item.parent_chain);
                    break;
                }
            }

            // Event bubbling: if a child consumed the event but didn't stop propagation,
            // bubble up through its cached parent chain until we reach the stacking context root
            if let Some(parent_chain) = consuming_item_parent_chain {
                // Iterate through the cached parent chain (ordered from immediate parent to root)
                for &ancestor_id in parent_chain.iter() {
                    // Pass absolute event reference - each ancestor converts to local
                    // using its own local_to_root_transform
                    let (event_propagation, _) =
                        self.unconditional_view_event(ancestor_id, absolute_event, true);
                    if event_propagation.is_processed() {
                        return (EventPropagation::Stop, PointerEventConsumed::Yes);
                    }
                }
            }
        }

        if !disable_default
            && !is_pointer_none
            && view
                .borrow_mut()
                .event_after_children(self, &event)
                .is_processed()
        {
            return (EventPropagation::Stop, PointerEventConsumed::Yes);
        }

        if is_pointer_none {
            // if pointer-events: none, we don't handle the pointer event
            return (EventPropagation::Continue, view_pointer_event_consumed);
        }

        // CLARIFY: should this be disabled when disable_default?
        if !disable_default {
            let popout_menu = || {
                let bottom_left = {
                    let layout = view_state.borrow().layout_rect;
                    Point::new(layout.x0, layout.y1)
                };

                let popout_menu = view_state.borrow().popout_menu.clone();
                show_context_menu(popout_menu?(), Some(bottom_left));
                Some((EventPropagation::Stop, PointerEventConsumed::Yes))
            };

            match &event {
                Event::Pointer(PointerEvent::Down(PointerButtonEvent {
                    pointer,
                    state,
                    button,
                    ..
                })) => {
                    self.window_state.clicking.insert(view_id);
                    let point = state.logical_point();
                    if pointer.is_primary_pointer()
                        && button.is_none_or(|b| b == PointerButton::Primary)
                    {
                        let rect = view_id.get_size().unwrap_or_default().to_rect();
                        let on_view = rect.contains(point);

                        if on_view {
                            if view_state.borrow().computed_style.get(Focusable) {
                                // if the view can be focused, we update the focus
                                self.window_state.update_focus(view_id, false);
                            }
                            if state.count == 2
                                && view_state
                                    .borrow()
                                    .event_listeners
                                    .contains_key(&EventListener::DoubleClick)
                            {
                                view_state.borrow_mut().last_pointer_down = Some(state.clone());
                            }
                            if view_state
                                .borrow()
                                .event_listeners
                                .contains_key(&EventListener::Click)
                            {
                                view_state.borrow_mut().last_pointer_down = Some(state.clone());
                            }

                            #[cfg(target_os = "macos")]
                            if let Some((ep, pec)) = popout_menu() {
                                return (ep, pec);
                            };

                            let bottom_left = {
                                let layout = view_state.borrow().layout_rect;
                                Point::new(layout.x0, layout.y1)
                            };
                            let popout_menu = view_state.borrow().popout_menu.clone();
                            if let Some(menu) = popout_menu {
                                show_context_menu(menu(), Some(bottom_left));
                                return (EventPropagation::Stop, PointerEventConsumed::Yes);
                            }
                            if view_id.can_drag() && self.window_state.drag_start.is_none() {
                                self.window_state.drag_start = Some((view_id, point));
                            }
                        }
                    } else if button.is_some_and(|b| b == PointerButton::Secondary) {
                        let rect = view_id.get_size().unwrap_or_default().to_rect();
                        let on_view = rect.contains(point);

                        if on_view {
                            if view_state.borrow().computed_style.get(Focusable) {
                                // if the view can be focused, we update the focus
                                self.window_state.update_focus(view_id, false);
                            }
                            if view_state
                                .borrow()
                                .event_listeners
                                .contains_key(&EventListener::SecondaryClick)
                            {
                                view_state.borrow_mut().last_pointer_down = Some(state.clone());
                            }
                        }
                    }
                }
                Event::Pointer(PointerEvent::Move(PointerUpdate { current, .. })) => {
                    let rect = view_id.get_size().unwrap_or_default().to_rect();
                    if rect.contains(current.logical_point()) {
                        if self.window_state.is_dragging() {
                            self.window_state.dragging_over.insert(view_id);
                            view_id.apply_event(&EventListener::DragOver, &event);
                        } else {
                            self.window_state.hovered.insert(view_id);
                            let view_state = view_state.borrow();
                            let style = view_state.combined_style.builtin();
                            if let Some(cursor) = style.cursor() {
                                if self.window_state.cursor.is_none() {
                                    self.window_state.cursor = Some(cursor);
                                }
                            }
                        }
                    }
                    if view_id.can_drag() {
                        if let Some((_, drag_start)) = self
                            .window_state
                            .drag_start
                            .as_ref()
                            .filter(|(drag_id, _)| drag_id == &view_id)
                        {
                            let offset = current.logical_point() - *drag_start;
                            if let Some(dragging) = self
                                .window_state
                                .dragging
                                .as_mut()
                                .filter(|d| d.id == view_id && d.released_at.is_none())
                            {
                                // update the mouse position if the view is dragging and not released
                                dragging.offset = drag_start.to_vec2();
                                self.window_state.request_paint(view_id);
                            } else if offset.x.abs() + offset.y.abs() > 1.0 {
                                // start dragging when moved 1 px
                                self.window_state.active = None;
                                self.window_state.dragging = Some(DragState {
                                    id: view_id,
                                    offset: drag_start.to_vec2(),
                                    released_at: None,
                                    release_location: None,
                                });
                                self.update_active(view_id);
                                self.window_state.request_paint(view_id);
                                view_id.apply_event(&EventListener::DragStart, &event);
                            }
                        }
                    }
                    if view_id
                        .apply_event(&EventListener::PointerMove, &event)
                        .is_some_and(|prop| prop.is_processed())
                    {
                        return (EventPropagation::Stop, PointerEventConsumed::Yes);
                    }
                }
                Event::Pointer(PointerEvent::Up(PointerButtonEvent {
                    button,
                    pointer,
                    state,
                })) => {
                    if pointer.is_primary_pointer()
                        && button.is_none_or(|b| b == PointerButton::Primary)
                    {
                        let rect = view_id.get_size().unwrap_or_default().to_rect();
                        let on_view = rect.contains(state.logical_point());

                        #[cfg(not(target_os = "macos"))]
                        if on_view {
                            if let Some((ep, pec)) = popout_menu() {
                                return (ep, pec);
                            };
                        }

                        if !directed {
                            if on_view {
                                if let Some(dragging) = self.window_state.dragging.as_mut() {
                                    let dragging_id = dragging.id;
                                    if view_id
                                        .apply_event(&EventListener::Drop, &event)
                                        .is_some_and(|prop| prop.is_processed())
                                    {
                                        // if the drop is processed, we set dragging to none so that the animation
                                        // for the dragged view back to its original position isn't played.
                                        self.window_state.dragging = None;
                                        self.window_state.request_paint(view_id);
                                        dragging_id.apply_event(&EventListener::DragEnd, &event);
                                    }
                                }
                            }
                        } else if let Some(dragging) = self
                            .window_state
                            .dragging
                            .as_mut()
                            .filter(|d| d.id == view_id)
                        {
                            let dragging_id = dragging.id;
                            dragging.released_at = Some(Instant::now());
                            dragging.release_location = Some(state.logical_point());
                            self.window_state.request_paint(view_id);
                            dragging_id.apply_event(&EventListener::DragEnd, &event);
                        }

                        let last_pointer_down = view_state.borrow_mut().last_pointer_down.take();

                        let event_listeners = view_state.borrow().event_listeners.clone();
                        if let Some(handlers) = event_listeners.get(&EventListener::DoubleClick) {
                            view_state.borrow_mut();
                            if on_view
                                && self.window_state.is_clicking(&view_id)
                                && last_pointer_down
                                    .as_ref()
                                    .map(|s| s.count == 2)
                                    .unwrap_or(false)
                                && handlers.iter().fold(false, |handled, handler| {
                                    handled | (handler.borrow_mut())(&event).is_processed()
                                })
                            {
                                return (EventPropagation::Stop, PointerEventConsumed::Yes);
                            }
                        }

                        if let Some(handlers) = event_listeners.get(&EventListener::Click) {
                            if on_view
                                && self.window_state.is_clicking(&view_id)
                                && last_pointer_down.is_some()
                                && handlers.iter().fold(false, |handled, handler| {
                                    handled | (handler.borrow_mut())(&event).is_processed()
                                })
                            {
                                return (EventPropagation::Stop, PointerEventConsumed::Yes);
                            }
                        }

                        if view_id
                            .apply_event(&EventListener::PointerUp, &event)
                            .is_some_and(|prop| prop.is_processed())
                        {
                            return (EventPropagation::Stop, PointerEventConsumed::Yes);
                        }
                    } else if button.is_some_and(|b| b == PointerButton::Secondary) {
                        let rect = view_id.get_size().unwrap_or_default().to_rect();
                        let on_view = rect.contains(state.logical_point());

                        let last_pointer_down = view_state.borrow_mut().last_pointer_down.take();
                        let event_listeners = view_state.borrow().event_listeners.clone();
                        if let Some(handlers) = event_listeners.get(&EventListener::SecondaryClick)
                        {
                            if on_view
                                && last_pointer_down.is_some()
                                && handlers.iter().fold(false, |handled, handler| {
                                    handled | (handler.borrow_mut())(&event).is_processed()
                                })
                            {
                                return (EventPropagation::Stop, PointerEventConsumed::Yes);
                            }
                        }

                        let viewport_event_position = {
                            let layout = view_state.borrow().layout_rect;
                            Point::new(
                                layout.x0 + state.logical_point().x,
                                layout.y0 + state.logical_point().y,
                            )
                        };
                        let context_menu = view_state.borrow().context_menu.clone();
                        if let Some(menu) = context_menu {
                            show_context_menu(menu(), Some(viewport_event_position));
                            return (EventPropagation::Stop, PointerEventConsumed::Yes);
                        }
                    }
                }
                Event::Key(KeyboardEvent {
                    state: KeyState::Down,
                    ..
                }) => {
                    if self.window_state.is_focused(&view_id) && event.is_keyboard_trigger() {
                        view_id.apply_event(&EventListener::Click, &event);
                    }
                }
                Event::WindowResized(_) => {
                    if view_state.borrow().has_style_selectors.has_responsive() {
                        view_id.request_style();
                    }
                }
                Event::FileDrag(e @ FileDragEvent::DragMoved { .. }) => {
                    if let Some(point) = e.logical_point() {
                        let rect = view_id.get_size().unwrap_or_default().to_rect();
                        let on_view = rect.contains(point);
                        if on_view {
                            self.window_state.file_hovered.insert(view_id);
                            view_id.request_style();
                        }
                    }
                }
                _ => (),
            }
        }

        if !disable_default {
            if let Some(listener) = event.listener() {
                let event_listeners = view_state.borrow().event_listeners.clone();
                if let Some(handlers) = event_listeners.get(&listener).cloned() {
                    let should_run = if let Some(pos) = event.point() {
                        let rect = view_id.get_size().unwrap_or_default().to_rect();
                        rect.contains(pos)
                    } else {
                        true
                    };
                    if should_run
                        && handlers.iter().fold(false, |handled, handler| {
                            handled | (handler.borrow_mut())(&event).is_processed()
                        })
                    {
                        return (EventPropagation::Stop, PointerEventConsumed::Yes);
                    }
                }
            }
        }

        (EventPropagation::Continue, PointerEventConsumed::Yes)
    }

    /// Used to determine if you should send an event to another view. This is basically a check for pointer events to see if the pointer is inside a child view and to make sure the current view isn't hidden or disabled.
    /// Usually this is used if you want to propagate an event to a child view
    ///
    /// Note: This function expects event coordinates to be in absolute (window) coordinates,
    /// as used by the stacking context event dispatch. The layout_rect and clip_rect are also
    /// in absolute coordinates, so they can be compared directly.
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

    /// Dispatch an event through the view tree with proper state management.
    ///
    /// This is the core event processing logic shared between WindowHandle and TestHarness.
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
            // Keyboard events: send to focused view first, then bubble up
            let mut processed = false;

            if let Some(id) = self.window_state.focus {
                processed |= self
                    .unconditional_view_event(id, &event, true)
                    .0
                    .is_processed();
            }

            if !processed {
                if let Some(listener) = event.listener() {
                    processed |= main_view_id
                        .apply_event(&listener, &event)
                        .is_some_and(|prop| prop.is_processed());
                }
            }

            if !processed {
                // Handle Tab and arrow key navigation
                if let Event::Key(KeyboardEvent {
                    key,
                    modifiers,
                    state: KeyState::Down,
                    ..
                }) = &event
                {
                    if *key == Key::Named(NamedKey::Tab)
                        && (modifiers.is_empty() || *modifiers == Modifiers::SHIFT)
                    {
                        let backwards = modifiers.contains(Modifiers::SHIFT);
                        view_tab_navigation(root_id, self.window_state, backwards);
                    } else if *modifiers == Modifiers::ALT {
                        if let Key::Named(
                            name @ (NamedKey::ArrowUp
                            | NamedKey::ArrowDown
                            | NamedKey::ArrowLeft
                            | NamedKey::ArrowRight),
                        ) = key
                        {
                            view_arrow_navigation(*name, self.window_state, root_id);
                        }
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
                if keyboard_trigger_end {
                    if let Some(id) = self.window_state.active {
                        if self
                            .window_state
                            .has_style_for_sel(id, StyleSelector::Active)
                        {
                            id.request_style_recursive();
                        }
                        self.window_state.active = None;
                    }
                }
            }
        } else if self.window_state.active.is_some() && event.is_pointer() {
            // Pointer events while dragging: send to active view
            if self.window_state.is_dragging() {
                self.unconditional_view_event(root_id, &event, false);
            }

            let id = self.window_state.active.unwrap();

            {
                let window_origin = id.state().borrow().window_origin;
                let layout = id.get_layout().unwrap_or_default();
                let viewport = id.state().borrow().viewport.unwrap_or_default();
                let transform = Affine::translate((
                    window_origin.x - layout.location.x as f64 + viewport.x0,
                    window_origin.y - layout.location.y as f64 + viewport.y0,
                ));
                let transformed_event = event.clone().transform(transform);
                self.unconditional_view_event(id, &transformed_event, true);
            }

            if let Event::Pointer(PointerEvent::Up { .. }) = &event {
                if self
                    .window_state
                    .has_style_for_sel(id, StyleSelector::Active)
                {
                    id.request_style_recursive();
                }
                self.window_state.active = None;
            }
        } else {
            // Normal event dispatch through view tree
            self.unconditional_view_event(root_id, &event, false);
        }

        // Clear drag_start on pointer up
        if let Event::Pointer(PointerEvent::Up { .. }) = &event {
            self.window_state.drag_start = None;
        }

        // Handle hover state changes - send PointerEnter/Leave events
        if let Some(info) = is_pointer_move {
            let hovered = &self.window_state.hovered.clone();
            for id in was_hovered.unwrap().symmetric_difference(hovered) {
                let view_state = id.state();
                if view_state.borrow().has_active_animation()
                    || view_state
                        .borrow()
                        .has_style_selectors
                        .has(StyleSelector::Hover)
                    || view_state
                        .borrow()
                        .has_style_selectors
                        .has(StyleSelector::Active)
                {
                    id.request_style();
                }
                if hovered.contains(id) {
                    id.apply_event(&EventListener::PointerEnter, &event);
                } else {
                    let leave_event = Event::Pointer(PointerEvent::Leave(info));
                    self.unconditional_view_event(*id, &leave_event, true);
                }
            }

            // Handle drag enter/leave events
            let dragging_over = &self.window_state.dragging_over.clone();
            for id in was_dragging_over
                .unwrap()
                .symmetric_difference(dragging_over)
            {
                if dragging_over.contains(id) {
                    id.apply_event(&EventListener::DragEnter, &event);
                } else {
                    id.apply_event(&EventListener::DragLeave, &event);
                }
            }
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

        // Request style updates for clicking views on pointer down
        if is_pointer_down {
            for id in self.window_state.clicking.clone() {
                if self
                    .window_state
                    .has_style_for_sel(id, StyleSelector::Active)
                {
                    id.request_style_recursive();
                }
            }
        }

        // On pointer up, request style updates and clear clicking state
        if matches!(&event, Event::Pointer(PointerEvent::Up { .. })) {
            for id in self.window_state.clicking.clone() {
                if self
                    .window_state
                    .has_style_for_sel(id, StyleSelector::Active)
                {
                    id.request_style_recursive();
                }
            }
            self.window_state.clicking.clear();
        }

        EventPropagation::Continue
    }
}
