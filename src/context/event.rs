#![allow(unused_imports)]
use std::{
    collections::{HashMap, HashSet},
    slice,
    sync::LazyLock,
};

use peniko::kurbo::Point;
use ui_events::{
    keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey},
    pointer::{
        PointerButton, PointerButtonEvent, PointerEvent, PointerInfo, PointerState, PointerUpdate,
    },
};
use understory_box_tree::{NodeId, QueryFilter};
use understory_focus::{FocusPolicy, FocusProps, FocusSpace, adapters::box_tree::FocusPropsLookup};
use understory_responder::{
    click::ClickResult,
    dispatcher,
    focus::FocusEvent,
    hover::{self, HoverEvent, HoverState},
    router::Router,
    types::{
        DepthKey, Dispatch, Localizer, Outcome, ParentLookup, Phase, ResolvedHit, WidgetLookup,
    },
};

use crate::{
    action::show_context_menu,
    context::*,
    dropped_file::FileDragEvent,
    event::{Event, EventListener},
    id::ViewId,
    style::{Focusable, PointerEvents, PointerEventsProp, StyleSelector},
    view::View,
    view_storage::{VIEW_STORAGE, ViewStorage},
    window_state::WindowState,
};

pub(crate) type FloemDispatch = Dispatch<NodeId, ViewId, Option<()>>;

pub trait WidgetLookupExt {
    fn widget_of(&self) -> Option<ViewId>;
    fn parent_of(&self) -> Option<Self>
    where
        Self: std::marker::Sized;
}
impl WidgetLookupExt for NodeId {
    fn widget_of(&self) -> Option<ViewId> {
        VIEW_STORAGE.with_borrow(|s| s.box_node_to_view.borrow().get(self).copied())
    }
    fn parent_of(&self) -> Option<Self> {
        VIEW_STORAGE.with_borrow(|s| s.box_tree.borrow().parent_of(*self))
    }
}
pub(crate) struct BoxNodeLookup;
impl WidgetLookup<NodeId> for BoxNodeLookup {
    type WidgetId = ViewId;
    fn widget_of(&self, node: &NodeId) -> Option<Self::WidgetId> {
        node.widget_of()
    }
}

pub(crate) struct BoxNodeParentLookup;
impl ParentLookup<NodeId> for BoxNodeParentLookup {
    fn parent_of(&self, node: &NodeId) -> Option<NodeId> {
        node.parent_of()
    }
}

impl FocusPropsLookup<NodeId> for WindowState {
    fn props(&self, node_id: &NodeId) -> FocusProps {
        // Convert NodeId to ViewId and get focus properties
        FocusProps {
            enabled: self.focusable.contains(node_id),
            order: None,
            group: None,
            autofocus: false,
            policy_hint: None,
        }
    }
}

/// State captured before routing for post-processing
struct PreRouteState {
    was_dragging_over: Option<HashSet<ViewId>>,
    is_pointer_down: bool,
    is_pointer_up: bool,
}

pub struct EventCx<'a> {
    pub window_state: &'a mut WindowState,
    dispatch: Option<Rc<[Dispatch<NodeId, ViewId, Option<()>>]>>,
    listeners: HashSet<EventListener>,
    extra_targets: Vec<NodeId>,
}

impl<'a> EventCx<'a> {
    pub fn new(window_state: &'a mut WindowState) -> Self {
        Self {
            window_state,
            dispatch: None,
            listeners: HashSet::new(),
            extra_targets: Vec::new(),
        }
    }

    /// Main entry point for all window events
    ///
    /// ┌─────────────────────────────────────────────────────────────┐
    /// │                    Event Arrives (Window Space)             │
    /// └────────────────────────┬────────────────────────────────────┘
    ///                          │
    ///                          ▼
    ///                   ┌──────────────┐
    ///                   │ Event Type?  │
    ///                   └──┬────┬────┬─┘
    ///                      │    │    │
    ///          ┌───────────┘    │    └──────────┐
    ///          │                │               │
    ///          ▼                ▼               ▼
    ///     ┌─────────┐    ┌──────────┐    ┌──────────┐
    ///     │ Pointer │    │ Directed │    │Broadcast │
    ///     │ (Mouse) │    │(Keyboard)│    │ (Resize) │
    ///     └────┬────┘    └────┬─────┘    └────┬─────┘
    ///          │              │               │
    ///          ▼              ▼               │
    ///   ┌─────────────┐ ┌──────────────┐      │
    ///   │ Hit Test @  │ │Get Focused   │      │
    ///   │ Point       │ │View BoxNode  │      │
    ///   │(Box Tree)   │ └──────┬───────┘      │
    ///   └──────┬──────┘        │              │
    ///          │               │              │
    ///          ▼               ▼              │
    ///   ┌─────────────┐ ┌──────────────┐      │
    ///   │ Convert to  │ │Reconstruct   │      │
    ///   │ResolvedHit  │ │Path to Root  │      │
    ///   └──────┬──────┘ └──────┬───────┘      │
    ///          │               │              │
    ///          └───────┬───────┘              │
    ///                  │                      │
    ///                  ▼                      │
    ///         ┌─────────────────┐             │
    ///         │ Router.dispatch │             │
    ///         │ (Capture→ Target│             │
    ///         │  → Bubble seq)  │             │
    ///         └────────┬────────┘             │
    ///                  │                      │
    ///                  ▼                      │
    ///         ┌─────────────────┐             │
    ///         │Update HoverState│             │
    ///         │(Enter/Leave)    │             │
    ///         └────────┬────────┘             │
    ///                  │                      │
    ///                  ▼                      ▼
    ///            ┌──────────────────────────────────┐
    ///            │   For Each Dispatch/View         │
    ///            └──────────────┬───────────────────┘
    ///                           │
    ///                           ▼
    ///            ┌──────────────────────────────────┐
    ///            │ 1. BoxNode → ViewId              │
    ///            └──────────────┬───────────────────┘
    ///                           ▼
    ///            ┌──────────────────────────────────┐
    ///            │ 2. Check: Hidden? Disabled?      │
    ///            │    → Skip if blocked             │
    ///            └──────────────┬───────────────────┘
    ///                           ▼
    ///            ┌──────────────────────────────────┐
    ///            │ 3. Transform Event               │
    ///            │    Window→ View-Local Coords     │
    ///            └──────────────┬───────────────────┘
    ///                           ▼
    ///            ┌──────────────────────────────────┐
    ///            │ 4. Check disable_default?        │
    ///            │    → Skip defaults if set        │
    ///            └──────────────┬───────────────────┘
    ///                           ▼
    ///            ┌──────────────────────────────────┐
    ///            │ 5. Call view.event(cx, evt, phase)│
    ///            │    → Outcome::Stop? Exit         │
    ///            └──────────────┬───────────────────┘
    ///                           ▼
    ///            ┌──────────────────────────────────┐
    ///            │ 6. Execute Event Listeners       │
    ///            │    → Outcome::Stop? Exit         │
    ///            └──────────────┬───────────────────┘
    ///                           ▼
    ///            ┌──────────────────────────────────┐
    ///            │ 7. Handle Default Behaviors:     │
    ///            │    - Focus on click              │
    ///            │    - Drag start/end              │
    ///            │    - Click/DoubleClick detection │
    ///            │    - Cursor updates              │
    ///            │    - State tracking              │
    ///            │    → Outcome::Stop? Exit         │
    ///            └──────────────┬───────────────────┘
    ///                           │
    ///                           ▼
    ///                     ┌──────────┐
    ///                     │Continue? │
    ///                     └────┬─────┘
    ///                          │
    ///                  ┌───────┴───────┐
    ///                  │               │
    ///                 Yes             No
    ///                  │               │
    ///                  ▼               ▼
    ///            ┌──────────┐    ┌─────────┐
    ///            │Next Phase│    │  Done   │
    ///            │or Bubble │    │         │
    ///            └──────────┘    └─────────┘
    pub fn window_event(&mut self, event: Event) {
        // Capture state before routing
        let pre_state = self.pre_route(&event);

        // Route the event
        self.route_event(event.clone());

        // Post-routing state management
        self.post_route(pre_state, &event);
    }

    /// Capture state before routing and clear sets for population during routing
    fn pre_route(&mut self, event: &Event) -> PreRouteState {
        let is_pointer_move = matches!(event, Event::Pointer(PointerEvent::Move(_)));
        let _is_file_drag = matches!(event, Event::FileDrag(FileDragEvent::DragMoved { .. }));
        let is_pointer_down = matches!(event, Event::Pointer(PointerEvent::Down { .. }));
        let is_pointer_up = matches!(event, Event::Pointer(PointerEvent::Up { .. }));
        static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

        if let Some(point) = event.point() {
            match event {
                Event::Pointer(PointerEvent::Down(PointerButtonEvent {
                    button, pointer, ..
                })) => {
                    if let Some(hit) = VIEW_STORAGE.with_borrow(|s| {
                        s.box_tree
                            .borrow()
                            .hit_test_point(point, QueryFilter::new().visible().pickable())
                    }) {
                        for hit in &hit.path {
                            if let Some(vid) = hit.widget_of() {
                                vid.request_style();
                            }
                        }
                        self.window_state.click_state.on_down(
                            pointer.pointer_id.map(|p| p.get_inner()),
                            button.map(|b| b as u8),
                            &hit.path,
                            point,
                            Instant::now().duration_since(*START_TIME).as_millis() as u64,
                        );
                    }
                }
                Event::Pointer(PointerEvent::Up(PointerButtonEvent {
                    button,
                    pointer,
                    state,
                })) => {
                    let tree = VIEW_STORAGE.with_borrow(|s| s.box_tree.clone());
                    let hit = tree
                        .borrow()
                        .hit_test_point(point, QueryFilter::new().visible().pickable());
                    if let Some(hit) = hit {
                        let res = self.window_state.click_state.on_up(
                            pointer.pointer_id.map(|p| p.get_inner()),
                            button.map(|b| b as u8),
                            hit.node,
                            point,
                            Instant::now().duration_since(*START_TIME).as_millis() as u64,
                        );
                        match res {
                            ClickResult::Click(click_hit) => {
                                for hit in &hit.path {
                                    if let Some(vid) = hit.widget_of() {
                                        vid.request_style();
                                    }
                                }
                                self.extra_targets.push(click_hit);
                                match state.count {
                                    1 => {
                                        self.listeners.insert(EventListener::Click);
                                    }
                                    _ => {
                                        self.listeners.insert(EventListener::Click);
                                        self.listeners.insert(EventListener::DoubleClick);
                                    }
                                }
                            }
                            ClickResult::None(Some(og_target)) => {
                                if let Some(vid) = og_target.widget_of() {
                                    vid.request_style_recursive();
                                }
                            }
                            ClickResult::None(None) => {}
                        }
                    } else {
                        self.window_state.click_state.clear();
                    }
                }
                Event::Pointer(PointerEvent::Cancel(PointerInfo { pointer_id, .. })) => {
                    self.window_state
                        .click_state
                        .cancel(pointer_id.map(|p| p.get_inner()));
                }
                Event::Pointer(PointerEvent::Move(pu)) => {
                    self.window_state.last_cursor_location = pu.current.logical_point();
                    let exceeded_node = self.window_state.click_state.on_move(
                        pu.pointer.pointer_id.map(|p| p.get_inner()),
                        pu.current.logical_point(),
                    );
                    if let Some(node) = exceeded_node
                        && let Some(vid) = node.widget_of()
                        && self
                            .window_state
                            .has_style_for_sel(vid, StyleSelector::Clicking)
                    {
                        vid.request_style();
                    }
                }

                _ => {}
            }
        }

        if is_pointer_down {
            self.window_state.keyboard_navigation = false;
            if let Some(focus) = self.window_state.focus_state.current_path().last() {
                if let Some(vid) = focus.widget_of() {
                    vid.request_style()
                }
            }
        }

        // Clear cursor on pointer move
        if is_pointer_move {
            self.window_state.cursor = None;
        }

        // Capture dragging over state
        let was_dragging_over = if is_pointer_move {
            Some(std::mem::take(&mut self.window_state.dragging_over))
        } else {
            None
        };

        PreRouteState {
            was_dragging_over,
            is_pointer_down,
            is_pointer_up,
        }
    }

    /// Post-routing: detect changes and fire enter/leave events
    fn post_route(&mut self, pre_state: PreRouteState, event: &Event) {
        // Handle hover enter/leave via HoverState (managed during routing)

        // Handle drag enter/leave
        // if let Some(was_dragging_over) = pre_state.was_dragging_over {
        //     let dragging_over = self.window_state.dragging_over.clone();
        //     for id in was_dragging_over.symmetric_difference(&dragging_over) {
        //         if dragging_over.contains(&id) {
        //             id.apply_event(&EventListener::DragEnter, event);
        //         } else {
        //             id.apply_event(&EventListener::DragLeave, event);
        //         }
        //     }
        // }

        if pre_state.is_pointer_up {
            let old = self.window_state.active.take();
            if let Some(old_id) = old.and_then(|old| old.widget_of()) {
                // To remove the styles applied by the Active selector
                old_id.request_style();
            }
        }
    }

    /// Handle default behaviors (focus, click, drag, etc.)
    fn handle_default_behaviors(&mut self, view_id: ViewId, event: Event) -> bool {
        let view_rect = view_id.layout_rect_local();

        // Pointer down
        if event.is_click_start() {
            if let Some(point) = event.point() {
                // Record this view as having pointer down
                // self.window_state.pointer_down_view = Some(view_id);

                // Update focus if focusable
                let focusable = view_id.state().borrow().computed_style.get(Focusable);
                if focusable {
                    self.update_focus(view_id.box_node(), false, event.clone());
                }

                // TODO: make work and work with window space coords
                // Potential drag start
                // if view_id.can_drag() {
                //     self.window_state.drag_start = Some(DragState {
                //         id: view_id,
                //         start_point: point,
                //         released_at: None,
                //         release_point: None,
                //     });
                // }
            }
        }

        // Pointer up
        if event.is_click_end() {
            if let Some(point) = event.point() {
                if view_rect.contains(point) {
                    // Check if this is the same view that had pointer down
                    // let is_click = self.window_state.pointer_down_view == Some(view_id);

                    // if is_click {
                    //     let last_down = view_id.state().borrow_mut().last_pointer_down.take();

                    //     if last_down.is_some() {
                    //         // Fire click
                    //         view_id.apply_event(&EventListener::Click, event);

                    //         // Fire double-click if applicable
                    //         if event.is_double_click() {
                    //             view_id.apply_event(&EventListener::DoubleClick, event);
                    //         }

                    //         return true;
                    //     }
                    // }
                }
            }
        }

        // Pointer move - hover and drag tracking
        if event.updates_hover() {
            if let Some(point) = event.point() {
                // Update cursor
                let cursor = view_id.state().borrow().combined_style.builtin().cursor();
                if let Some(cursor) = cursor {
                    if self.window_state.cursor.is_none() {
                        self.window_state.cursor = Some(cursor);
                    }
                }

                // Handle drag threshold
                if let Some(drag) = &self.window_state.drag_start {
                    if drag.0 == view_id {
                        //&& drag.released_at.is_none() {
                        let offset = point - drag.1;

                        // Start actual drag after threshold
                        if offset.x.abs() + offset.y.abs() > 1.0 {
                            self.dispatch_one(
                                &FloemDispatch::target(view_id.box_node()),
                                event.clone(),
                            );
                        }
                    }
                }
            }
        }

        false
    }

    /// Update focus to a new view, firing focus enter/leave events
    pub fn update_focus(&mut self, node_id: NodeId, keyboard_navigation: bool, event: Event) {
        // Build path using router
        let router = Router::with_parent(BoxNodeLookup, BoxNodeParentLookup);
        let seq = router.dispatch_for::<()>(node_id);
        let path = hover::path_from_dispatch(&seq);
        self.update_focus_from_path(&path, keyboard_navigation, event);
    }

    pub fn update_focus_from_path(
        &mut self,
        path: &[NodeId],
        keyboard_navigation: bool,
        event: Event,
    ) {
        self.window_state
            .focus_state
            .current_path()
            .last()
            .map(|n| n.widget_of().map(|vid| vid.request_style()));

        // Update focus state and get enter/leave events
        let old_target = self.window_state.focus_state.current_path().last().copied();
        let new_target = path.last().copied();
        let focus_events = self.window_state.focus_state.update_path(path);
        // TODO: Make this not resend events for focus gained. just insert the listener and let the normal dispatch happen

        // Fire focus events
        for focus_event in focus_events {
            match focus_event {
                FocusEvent::Enter(id) => {
                    if Some(id) == new_target {
                        // This is the actual focus target
                        self.listeners.insert(EventListener::FocusGained);
                        self.dispatch_one(&FloemDispatch::target(id), event.clone());
                        self.listeners.remove(&EventListener::FocusGained);
                    } else {
                        // This is an ancestor - subtree notification
                        self.listeners.insert(EventListener::FocusEnteredSubtree);
                        self.dispatch_one(&FloemDispatch::target(id), event.clone());
                        self.listeners.remove(&EventListener::FocusEnteredSubtree);
                    }
                }
                FocusEvent::Leave(id) => {
                    if Some(id) == old_target {
                        // This is the element losing focus
                        self.listeners.insert(EventListener::FocusLost);
                        self.dispatch_one(&FloemDispatch::target(id), event.clone());
                        self.listeners.remove(&EventListener::FocusLost);
                    } else {
                        // This is an ancestor - subtree notification
                        self.listeners.insert(EventListener::FocusLeftSubtree);
                        self.dispatch_one(&FloemDispatch::target(id), event.clone());
                        self.listeners.remove(&EventListener::FocusLeftSubtree);
                    }
                }
            }
        }

        self.window_state
            .focus_state
            .current_path()
            .last()
            .map(|n| n.widget_of().map(|vid| vid.request_style()));

        self.window_state.keyboard_navigation = keyboard_navigation;
    }

    /// Tab navigation using understory_focus for spatial awareness
    pub(crate) fn view_tab_navigation(&mut self, backwards: bool, event: Event) {
        // Get the focus scope root (could be enhanced to find actual scope boundaries)
        let scope_root = self.window_state.root_view_id.box_node();

        let current_focus = self
            .window_state
            .focus_state
            .current_path()
            .last()
            .cloned()
            .unwrap_or_else(|| {
                self.window_state
                    .click_state
                    .last_press()
                    .and_then(|press| {
                        VIEW_STORAGE.with_borrow(|storage| {
                            storage
                                .box_tree
                                .borrow()
                                .hit_test_point(
                                    press.down_position,
                                    QueryFilter::new().visible().pickable(),
                                )
                                .map(|hit| hit.node)
                        })
                    })
                    .unwrap_or(scope_root)
            });

        // Build focus space
        let mut focus_entries = Vec::new();

        let box_tree = VIEW_STORAGE.with_borrow(|s| s.box_tree.clone());

        let focus_space = understory_focus::adapters::box_tree::build_focus_space_for_scope(
            &box_tree.borrow(),
            scope_root,
            self.window_state,
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

        if let Some(new_focus) = policy.next(current_focus, navigation, &focus_space) {
            self.update_focus(new_focus, true, event);
        }
    }

    pub(crate) fn view_arrow_navigation(&mut self, key: &NamedKey, event: Event) {
        let scope_root = self.window_state.root_view_id.box_node();
        let current_focus = match self.window_state.focus_state.current_path().last().cloned() {
            Some(id) => id,
            None => {
                // No current focus, do tab navigation instead
                let backwards = matches!(key, NamedKey::ArrowUp | NamedKey::ArrowLeft);
                self.view_tab_navigation(backwards, event);
                return;
            }
        };

        let mut focus_entries = Vec::new();
        let box_tree = VIEW_STORAGE.with_borrow(|s| s.box_tree.clone());
        let focus_space = understory_focus::adapters::box_tree::build_focus_space_for_scope(
            &box_tree.borrow(),
            scope_root,
            self.window_state,
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

        if let Some(new_focus) = policy.next(current_focus, navigation, &focus_space) {
            self.update_focus(new_focus, true, event);
        }
    }

    fn route_event(&mut self, event: Event) {
        // // Handle active view (for pointer events during drag/active state)
        if let Some(active) = self.window_state.active
            && event.is_pointer()
        {
            self.route_directed(active, event);
        } else if event.is_directed() {
            self.handle_keyboard_event(event.clone());
        } else if event.is_spatial() {
            self.route_spatial(event);
        } else {
            self.broadcast(event);
        }
    }

    /// Handle keyboard events (focus, tab navigation, etc.)
    fn handle_keyboard_event(&mut self, event: Event) {
        // Try focused view first
        if let Some(focused) = self.window_state.focus_state.current_path().last() {
            self.route_directed(*focused, event.clone());
        }

        // Handle tab navigation
        if let Event::Key(KeyboardEvent {
            key: Key::Named(NamedKey::Tab),
            modifiers,
            state: KeyState::Down,
            ..
        }) = &event
        {
            if modifiers.is_empty() || *modifiers == Modifiers::SHIFT {
                let backwards = modifiers.contains(Modifiers::SHIFT);
                self.view_tab_navigation(backwards, event.clone());
            }
        }

        // Handle arrow navigation
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
        }) = &event
        {
            if *modifiers == Modifiers::ALT {
                self.view_arrow_navigation(name, event.clone());
            }
        }
    }

    /// Route directed events (keyboard to focused view)
    fn route_directed(&mut self, node_id: NodeId, mut event: Event) -> Option<FloemDispatch> {
        // Create router
        let router = Router::with_parent(BoxNodeLookup, BoxNodeParentLookup);

        // Get dispatch sequence
        let seq = router.dispatch_for(node_id);

        if let Event::Key(KeyboardEvent {
            state: KeyState::Down,
            key,
            ..
        }) = &event
        {
            if *key == Key::Named(NamedKey::Enter)
                || matches!(key, Key::Character(key) if key == " ")
            {
                self.listeners.insert(EventListener::Click);
            }
        }

        // Dispatch events
        dispatcher::run(&seq, &mut event, |dispatch, event| {
            self.dispatch_one(dispatch, event.clone())
        })
        .cloned()
    }

    fn route_spatial(&mut self, mut event: Event) {
        let point = event.point().unwrap();
        let mut router = Router::with_parent(BoxNodeLookup, BoxNodeParentLookup);

        let hit = VIEW_STORAGE.with_borrow(|s| {
            s.box_tree
                .borrow()
                .hit_test_point(point, QueryFilter::new().visible().pickable())
        });

        let Some(hit) = hit else {
            // No hit - clear hover state
            let hover_events = self.window_state.hover_state.clear();
            for hover_event in hover_events {
                if let HoverEvent::Leave(box_node) = hover_event {
                    if let Some(view_id) = VIEW_STORAGE
                        .with_borrow(|s| s.box_node_to_view.borrow().get(&box_node).copied())
                    {
                        view_id.request_style();
                    }
                }
            }
            return;
        };

        router.set_scope(Some(|bn| {
            VIEW_STORAGE
                .with_borrow(|s| s.box_node_to_view.borrow().get(bn).cloned())
                .map(|v| !v.is_hidden() && !v.pointer_events_none())
                .unwrap_or(false)
        }));

        let resolved = ResolvedHit {
            node: hit.node,
            path: Some(hit.path),
            depth_key: DepthKey::Z(0),
            localizer: Localizer::default(),
            meta: None::<()>,
        };

        let mut resolved_hits = vec![resolved];

        // Add resolved hits for extra targets
        for &target_node in &self.extra_targets {
            // You might need to compute the path for each target
            let target_seq = router.dispatch_for::<()>(target_node);
            let target_path = hover::path_from_dispatch(&target_seq);

            resolved_hits.push(ResolvedHit {
                node: target_node,
                path: Some(target_path),
                depth_key: DepthKey::Z(0), // Different depth to distinguish from hit test
                localizer: Localizer::default(),
                meta: None::<()>,
            });
        }

        let seq = router.handle_with_hits(&resolved_hits);

        // Build hover path using router
        let hover_path = hover::path_from_dispatch(&seq);

        // Update focus state and get enter/leave events
        let hover_events = self
            .window_state
            .hover_state
            .update_path(&hover_path)
            .into_iter()
            .filter_map(|hover_event| match hover_event {
                HoverEvent::Enter(id) => Some((id, EventListener::PointerEnter)),
                HoverEvent::Leave(id) => {
                    if let Some(id) = id.widget_of() {
                        id.request_style()
                    }
                    self.listeners.insert(EventListener::PointerLeave);
                    self.dispatch_one(&FloemDispatch::target(id), event.clone());
                    self.listeners.remove(&EventListener::PointerLeave);
                    None
                }
            })
            .collect::<HashMap<_, _>>();

        // Dispatch events
        dispatcher::run(&seq, &mut event, |dispatch, event| {
            if let Some(listener) = hover_events.get(&dispatch.node) {
                self.listeners.insert(*listener);
            }
            self.dispatch_one(dispatch, event.clone())
        });
    }

    /// Broadcast events to all interested views
    fn broadcast(&mut self, event: Event) -> Option<FloemDispatch> {
        // Collect all box nodes first without holding borrows
        let box_nodes: Vec<understory_box_tree::NodeId> = VIEW_STORAGE.with_borrow(|s| {
            let root_view_id = self.window_state.root_view_id;
            let mut nodes = Vec::with_capacity(s.view_ids.len());

            for view_id in s.view_ids.keys() {
                // Check if this view has the matching root
                if let Some(view_root) = s.root.get(view_id) {
                    if *view_root == Some(root_view_id) {
                        // Get the box node from the view state
                        if let Some(view_state) = s.states.get(view_id) {
                            let box_node = view_state.borrow().box_node;
                            nodes.push(box_node);
                        }
                    }
                }
            }
            nodes
        });

        // Now dispatch to each box node without holding any borrows
        for box_node in box_nodes {
            let dispatch = FloemDispatch::target(box_node);
            self.dispatch_one(&dispatch, event.clone());
        }

        None
    }

    /// Dispatch event to a single view at a specific phase
    fn dispatch_one(&mut self, dispatch: &FloemDispatch, event: Event) -> Outcome {
        // Convert box node to view id WITHOUT holding borrows
        let view_id = dispatch.widget;

        let Some(view_id) = view_id else {
            return Outcome::Continue;
        };

        // Check disabled (don't hold any borrows)
        if view_id.is_disabled() && !event.allow_disabled() {
            return Outcome::Continue;
        }

        if self.listeners.contains(&EventListener::PointerEnter) {
            view_id.request_style();
        }

        // Transform event to view-local coordinates WITHOUT holding borrows
        let local_event = self.window_event_to_view(view_id, event.clone());

        // create a scope so that view is dropped.
        {
            // Call view.event() - this will borrow the view mutably
            // CRITICAL: No other borrows of view_storage, states, or views must be held here
            VIEW_STORAGE.with(|s| {
                assert!(
                    s.try_borrow_mut().is_ok(),
                    "VIEW_STORAGE is already borrowed when calling view.event()"
                );
            });
            assert!(
                view_id.state().try_borrow_mut().is_ok(),
                "ViewState is already borrowed when calling view.event()"
            );
            let view = view_id.view();
            assert!(
                view.try_borrow_mut().is_ok(),
                "View is already borrowed when calling view.event()"
            );
            view.borrow_mut().event(self, &local_event, dispatch.phase);
        }

        for listener in local_event.listener().iter().chain(&self.listeners) {
            // Execute event listeners WITHOUT holding view borrow
            // Get handlers WITHOUT holding state borrow
            let handlers = view_id
                .state()
                .borrow()
                .event_listeners
                .get(listener)
                .cloned();

            if let Some(handlers) = handlers {
                for handler in handlers {
                    // TODO: only stop here if stop immediate
                    let view = view_id.view();
                    let mut view_ref = view.borrow_mut();
                    let view_as_any: &mut dyn View = &mut **view_ref;
                    (handler.borrow_mut())(view_as_any, &local_event);
                }
            }
        }

        // Handle default behaviors (only if not disabled and on bubble phase)
        // !disable_default
        if !matches!(dispatch.phase, Phase::Capture | Phase::Target)
            && self.handle_default_behaviors(view_id, local_event)
        {
            return Outcome::Stop;
        }

        Outcome::Continue
    }

    pub fn window_event_to_view(&self, id: ViewId, event: Event) -> Event {
        id.world_transform()
            .map(|t| event.clone().transform(t))
            .unwrap_or(event)
    }
}
