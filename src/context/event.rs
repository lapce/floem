#![allow(unused_imports)]
#![allow(unused)]
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
use understory_box_tree::{NodeFlags, NodeId, QueryFilter};
use understory_event_state::{
    click::ClickResult,
    focus::FocusEvent,
    hover::{self, HoverEvent},
};
use understory_focus::{FocusPolicy, FocusProps, FocusSpace, adapters::box_tree::FocusPropsLookup};
use understory_responder::{
    dispatcher,
    router::{Router, path_from_dispatch},
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
    window_tracking::is_known_root,
};

pub(crate) type FloemDispatch = Dispatch<NodeId, ViewId, Option<()>>;

pub trait WidgetLookupExt {
    fn view_of(&self) -> Option<ViewId>;
    fn parent_of(&self) -> Option<Self>
    where
        Self: std::marker::Sized;
}
impl WidgetLookupExt for NodeId {
    fn view_of(&self) -> Option<ViewId> {
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
        node.view_of()
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
            enabled: VIEW_STORAGE.with_borrow(|s| {
                s.box_tree
                    .borrow()
                    .flags(*node_id)
                    .map(|f| f.contains(NodeFlags::FOCUSABLE))
                    .unwrap_or(false)
            }),
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

pub(crate) struct GlobalEventCx<'a> {
    pub window_state: &'a mut WindowState,
    event: Event,
    dispatch: Option<Rc<[FloemDispatch]>>,
    all_listeners: HashSet<EventListener>,
    listeners: HashMap<NodeId, HashSet<EventListener>>,
    extra_targets: Vec<NodeId>,
}

pub struct EventCx<'a> {
    pub window_state: &'a mut WindowState,
    /// An event that has been transformed to the local coordinate space of the target node
    pub event: Event,
    /// The event phase for this local event
    pub phase: Phase,
    /// The target of this event
    pub target: NodeId,
    pub dispatch: Option<Rc<[FloemDispatch]>>,
    pub listeners: &'a HashSet<EventListener>,
    pub view_id: ViewId,
}
impl<'a> EventCx<'a> {
    /// Dispatch event to a single view at a specific phase
    fn dispatch_one(&mut self) -> Outcome {
        if self.view_id.is_disabled() && !self.event.allow_disabled() {
            return Outcome::Continue;
        }

        // Call view.event() - this will borrow the view mutably
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
        view.borrow_mut().event(self);

        let listener = self.event.listener();
        let listeners: smallvec::SmallVec<[EventListener; 16]> =
            self.listeners.iter().cloned().collect();

        if self.phase != Phase::Capture {
            for listener in listener.iter().chain(&listeners) {
                // Execute event listeners WITHOUT holding view borrow
                // Get handlers WITHOUT holding state borrow
                let handlers = self
                    .view_id
                    .state()
                    .borrow()
                    .event_listeners
                    .get(listener)
                    .cloned();

                if let Some(handlers) = handlers {
                    for handler in handlers {
                        // TODO: only stop here if stop immediate
                        let mut view_ref = view.borrow_mut();
                        let view_as_any: &mut dyn View = &mut **view_ref;
                        (handler.borrow_mut())(view_as_any, self);
                    }
                }
            }
        }

        Outcome::Continue
    }

    pub fn test(window_state: &mut WindowState) -> Self {
        todo!()
    }
}

impl<'a> GlobalEventCx<'a> {
    pub fn new(window_state: &'a mut WindowState, event: Event) -> Self {
        Self {
            window_state,
            event,
            dispatch: None,
            all_listeners: HashSet::new(),
            listeners: HashMap::new(),
            extra_targets: Vec::new(),
        }
    }

    pub fn event_cx(&mut self, dispatch: &FloemDispatch) -> Option<EventCx<'_>> {
        let view_id = dispatch.widget?;
        let transform = VIEW_STORAGE.with_borrow(|s| {
            let box_tree = s.box_tree.borrow();
            box_tree.world_transform(dispatch.node).unwrap_or_default()
        });

        let listeners: &HashSet<_> =
            if let Some(node_listeners) = self.listeners.get_mut(&dispatch.node) {
                node_listeners.extend(self.all_listeners.iter().cloned());
                node_listeners
            } else {
                &self.all_listeners
            };

        Some(EventCx {
            window_state: self.window_state,
            event: self.event.clone().transform(transform),
            phase: dispatch.phase,
            target: dispatch.node,
            dispatch: self.dispatch.clone(),
            listeners,
            view_id,
        })
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
    ///            │ 5. Call view.event(cx,evt,phase) │
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
    ///                 Yes              No
    ///                  │               │
    ///                  ▼               ▼
    ///            ┌──────────┐    ┌─────────┐
    ///            │Next Phase│    │  Done   │
    ///            │or Bubble │    │         │
    ///            └──────────┘    └─────────┘
    pub fn run(&mut self) {
        // Capture state before routing
        let pre_state = self.pre_route();

        // Route the event
        self.route_event();

        self.handle_default_behaviors();
    }

    /// Capture state before routing and clear sets for population during routing
    fn pre_route(&mut self) -> PreRouteState {
        let event = self.event.clone();
        let is_pointer_move = matches!(event, Event::Pointer(PointerEvent::Move(_)));
        let _is_file_drag = matches!(event, Event::FileDrag(FileDragEvent::DragMoved { .. }));
        let is_pointer_down = matches!(event, Event::Pointer(PointerEvent::Down { .. }));
        let is_pointer_up = matches!(event, Event::Pointer(PointerEvent::Up { .. }));
        static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

        if let Event::Pointer(PointerEvent::Leave(_)) = event {
            self.update_hover_from_path(&[]);
        }

        let mut add_click_listeners = |path: &[NodeId], count: u8, secondary: bool| {
            if let Some(&last) = path.last() {
                self.extra_targets.push(last);
            }
            for &node in path {
                let set = self.listeners.entry(node).or_default();
                if secondary {
                    set.insert(EventListener::SecondaryClick);
                } else {
                    set.insert(EventListener::Click);
                    if count > 1 {
                        set.insert(EventListener::DoubleClick);
                    }
                }
            }
        };

        if let Some(point) = event.point() {
            match event {
                Event::Pointer(PointerEvent::Down(PointerButtonEvent {
                    button, pointer, ..
                })) => {
                    // clear active on start of event handling poiner down
                    let root = self.window_state.root_view_id.visual_id();

                    if let Some(hit) = VIEW_STORAGE.with_borrow(|s| {
                        s.box_tree.borrow().hit_test_point(
                            point,
                            QueryFilter::new().visible().pickable().in_subtree(root),
                        )
                    }) {
                        for hit in &hit.path {
                            self.window_state.style_dirty.insert(*hit);
                        }
                        self.window_state.click_state.on_down(
                            pointer.pointer_id.map(|p| p.get_inner()),
                            button.map(|b| b as u8),
                            hit.path.clone(),
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
                    let root = self.window_state.root_view_id.visual_id();
                    let hit = tree.borrow().hit_test_point(
                        point,
                        QueryFilter::new().visible().pickable().in_subtree(root),
                    );
                    if let Some(hit) = hit {
                        let res = self.window_state.click_state.on_up(
                            pointer.pointer_id.map(|p| p.get_inner()),
                            button.map(|b| b as u8),
                            &hit.path,
                            point,
                            Instant::now().duration_since(*START_TIME).as_millis() as u64,
                        );
                        match res {
                            ClickResult::Click(click_hit) => {
                                for hit in &hit.path {
                                    self.window_state.style_dirty.insert(*hit);
                                }
                                add_click_listeners(
                                    &click_hit,
                                    state.count,
                                    button == Some(PointerButton::Secondary),
                                );
                            }
                            ClickResult::Suppressed(Some(og_target)) => {
                                let common_ancestor_idx = og_target
                                    .iter()
                                    .zip(hit.path.iter())
                                    .position(|(a, b)| a != b)
                                    .unwrap_or(og_target.len().min(hit.path.len()));
                                if common_ancestor_idx > 0 {
                                    let common_path = &hit.path[..common_ancestor_idx];
                                    for node in common_path {
                                        self.window_state.style_dirty.insert(*node);
                                    }
                                    add_click_listeners(
                                        common_path,
                                        state.count,
                                        button == Some(PointerButton::Secondary),
                                    );
                                } else if let Some(vid) = og_target.last().unwrap().view_of() {
                                    vid.request_style_recursive();
                                }
                            }
                            ClickResult::Suppressed(None) => {}
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
                    self.window_state.last_pointer = pu.current.logical_point();
                    let exceeded_nodes = self.window_state.click_state.on_move(
                        pu.pointer.pointer_id.map(|p| p.get_inner()),
                        pu.current.logical_point(),
                    );
                    if let Some(nodes) = exceeded_nodes {
                        for node in nodes {
                            self.window_state.style_dirty.insert(node);
                        }
                    }
                }

                _ => {}
            }
        }

        if is_pointer_down {
            self.window_state.keyboard_navigation = false;
            if let Some(focus) = self.window_state.focus_state.current_path().last() {
                self.window_state.style_dirty.insert(*focus);
            }
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

    /// Handle default behaviors (focus, click, drag, etc.)
    fn handle_default_behaviors(&mut self) {
        // Pointer down
        if self.event.is_click_start() {
            let point = self.event.point().unwrap();
            let tree = VIEW_STORAGE.with_borrow(|s| s.box_tree.clone());
            let root = self.window_state.root_view_id.visual_id();
            let hit = tree.borrow().hit_test_point(
                point,
                QueryFilter::new()
                    .visible()
                    .pickable()
                    .focusable()
                    .in_subtree(root),
            );
            if let Some(hit) = hit {
                self.update_focus(hit.node, false);
            } else {
                self.update_focus_from_path(&[], false);
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

        // Pointer move - hover and drag tracking
        if self.event.updates_hover() {
            let point = self.event.point().unwrap();

            // Handle drag threshold
            // if let Some(drag) = &self.window_state.drag_start {
            //     if drag.0 == view_id {
            //         //&& drag.released_at.is_none() {
            //         let offset = point - drag.1;

            //         // Start actual drag after threshold
            //         if offset.x.abs() + offset.y.abs() > 1.0 {
            //             self.dispatch_one(
            //                 &FloemDispatch::target(view_id.box_node()),
            //                 event.clone(),
            //             );
            //         }
            //     }
            // }
        }

        // Handle tab navigation
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
        }) = self.event
        {
            if modifiers == Modifiers::ALT {
                self.view_arrow_navigation(&name);
            }
        }

        if self.event.is_pointer_up() {
            let old = self.window_state.active.take();
            if let Some(old_id) = old {
                // To remove the styles applied by the Active selector
                self.window_state.style_dirty.insert(old_id);
            }
        }
    }

    /// Update focus to a new view, firing focus enter/leave events
    pub fn update_focus(&mut self, node_id: NodeId, keyboard_navigation: bool) {
        // Build path using router
        let mut router = Router::with_parent(BoxNodeLookup, BoxNodeParentLookup);
        router.set_scope(Some(|id| {
            VIEW_STORAGE
                .with_borrow(|s| s.box_tree.clone())
                .borrow()
                .flags(*id)
                .map(|f| f.contains(NodeFlags::FOCUSABLE))
                .unwrap_or(false)
        }));
        let seq = router.dispatch_for::<()>(node_id);
        let path = path_from_dispatch(&seq);
        self.update_focus_from_path(&path, keyboard_navigation);
    }

    pub fn update_focus_from_path(&mut self, path: &[NodeId], keyboard_navigation: bool) {
        self.window_state
            .focus_state
            .current_path()
            .last()
            .map(|n| self.window_state.style_dirty.insert(*n));

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
                        if let Some(mut local_cx) =
                            self.event_cx(&FloemDispatch::target(id).with_widget_opt(id.view_of()))
                        {
                            let mut set = HashSet::new();
                            set.insert(EventListener::FocusGained);
                            local_cx.listeners = &set;
                            local_cx.dispatch_one();
                        }
                    } else {
                        // This is an ancestor - subtree notification
                        if let Some(mut local_cx) =
                            self.event_cx(&FloemDispatch::target(id).with_widget_opt(id.view_of()))
                        {
                            let mut set = HashSet::new();
                            set.insert(EventListener::FocusEnteredSubtree);
                            local_cx.listeners = &set;
                            local_cx.dispatch_one();
                        }
                    }
                }
                FocusEvent::Leave(id) => {
                    if Some(id) == old_target {
                        // This is the element losing focus
                        if let Some(mut local_cx) =
                            self.event_cx(&FloemDispatch::target(id).with_widget_opt(id.view_of()))
                        {
                            let mut set = HashSet::new();
                            set.insert(EventListener::FocusLost);
                            local_cx.listeners = &set;
                            local_cx.dispatch_one();
                        }
                    } else {
                        // This is an ancestor - subtree notification
                        if let Some(mut local_cx) =
                            self.event_cx(&FloemDispatch::target(id).with_widget_opt(id.view_of()))
                        {
                            let mut set = HashSet::new();
                            set.insert(EventListener::FocusLeftSubtree);
                            local_cx.listeners = &set;
                            local_cx.dispatch_one();
                            local_cx.dispatch_one();
                        }
                    }
                }
            }
        }

        self.window_state
            .focus_state
            .current_path()
            .last()
            .map(|n| self.window_state.style_dirty.insert(*n));

        self.window_state.keyboard_navigation = keyboard_navigation;
    }

    /// Tab navigation using understory_focus for spatial awareness
    pub(crate) fn view_tab_navigation(&mut self, backwards: bool) {
        // Get the focus scope root (could be enhanced to find actual scope boundaries)
        let scope_root = self.window_state.root_view_id.visual_id();

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
                        let root = self.window_state.root_view_id.visual_id();
                        VIEW_STORAGE.with_borrow(|storage| {
                            storage
                                .box_tree
                                .borrow()
                                .hit_test_point(
                                    press.down_position,
                                    QueryFilter::new().visible().pickable().in_subtree(root),
                                )
                                .map(|hit| hit.node)
                        })
                    })
                    .unwrap_or(scope_root)
            });

        // Build focus space
        // TODO: retain this? if there are benefits to doing so
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
            self.update_focus(new_focus, true);
        }
    }

    pub(crate) fn view_arrow_navigation(&mut self, key: &NamedKey) {
        let scope_root = self.window_state.root_view_id.visual_id();
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
            self.update_focus(new_focus, true);
        }
    }

    fn route_event(&mut self) {
        // // Handle active view (for pointer events during drag/active state)
        if let Some(active) = self.window_state.active
            && self.event.is_pointer()
        {
            self.route_directed(active);
        } else if self.event.is_directed() {
            self.handle_keyboard_event();
        } else if self.event.is_spatial() {
            self.route_spatial();
        } else {
            self.broadcast();
        }
    }

    /// Handle keyboard events (focus, tab navigation, etc.)
    fn handle_keyboard_event(&mut self) {
        // Try focused view first
        if let Some(focused) = self.window_state.focus_state.current_path().last() {
            self.route_directed(*focused);
        }
    }

    /// Route directed events (keyboard to focused view)
    fn route_directed(&mut self, node_id: NodeId) -> Option<FloemDispatch> {
        // Create router
        let router = Router::with_parent(BoxNodeLookup, BoxNodeParentLookup);
        // TODO: router filter/ scope?

        // Get dispatch sequence
        let seq = router.dispatch_for(node_id);

        if let Event::Key(KeyboardEvent {
            state: KeyState::Down,
            key,
            ..
        }) = &self.event
        {
            if self.event.is_keyboard_trigger() {
                self.all_listeners.insert(EventListener::Click);
            }
        }

        // Dispatch events
        dispatcher::run(&seq, self, |dispatch, event_cx| {
            if let Some(mut local_cx) = event_cx.event_cx(dispatch) {
                local_cx.dispatch_one()
            } else {
                Outcome::Continue
            }
        })
        .cloned()
    }

    fn route_spatial(&mut self) {
        let point = self.event.point().unwrap();
        let mut router = Router::with_parent(BoxNodeLookup, BoxNodeParentLookup);
        let root = self.window_state.root_view_id.visual_id();

        let hit = VIEW_STORAGE.with_borrow(|s| {
            s.box_tree.borrow().hit_test_point(
                point,
                QueryFilter::new().visible().pickable().in_subtree(root),
            )
        });

        let Some(hit) = hit else {
            // No hit - clear hover state
            let hover_events = self.window_state.hover_state.clear();
            for hover_event in hover_events {
                if let HoverEvent::Leave(box_node) = hover_event {
                    self.window_state.style_dirty.insert(box_node);
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
            let target_path = path_from_dispatch(&target_seq);

            resolved_hits.push(ResolvedHit {
                node: target_node,
                path: Some(target_path),
                depth_key: DepthKey::Z(0),
                localizer: Localizer::default(),
                meta: None::<()>,
            });
        }

        let seq = router.handle_with_hits(&resolved_hits);

        // Build hover path using router
        let hover_path = path_from_dispatch(&seq);

        self.update_hover_from_path(&hover_path);

        // Dispatch events
        dispatcher::run(&seq, self, |dispatch, event_cx| {
            // if let Some(listener) = hover_events.get(&dispatch.node) {
            //     self.listeners.insert(*listener);
            // }
            if let Some(mut local_cx) = event_cx.event_cx(dispatch) {
                local_cx.dispatch_one()
            } else {
                Outcome::Continue
            }
        });
    }

    pub(crate) fn update_hover_from_point(&mut self, point: Point) {
        let mut router = Router::with_parent(BoxNodeLookup, BoxNodeParentLookup);
        let root = self.window_state.root_view_id.visual_id();

        let hit = VIEW_STORAGE.with_borrow(|s| {
            s.box_tree.borrow().hit_test_point(
                point,
                QueryFilter::new().visible().pickable().in_subtree(root),
            )
        });

        let Some(hit) = hit else {
            // No hit - clear hover state
            let hover_events = self.window_state.hover_state.clear();
            for hover_event in hover_events {
                if let HoverEvent::Leave(box_node) = hover_event {
                    self.window_state.style_dirty.insert(box_node);
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

        let seq = router.handle_with_hits(&[resolved]);
        // Build hover path using router
        let hover_path = path_from_dispatch(&seq);
        self.update_hover_from_path(&hover_path);
    }

    pub(crate) fn update_hover_from_path(&mut self, path: &[NodeId]) {
        let hover_events = self
            .window_state
            .hover_state
            .update_path(path)
            .into_iter()
            .filter_map(|hover_event| match hover_event {
                HoverEvent::Enter(id) => {
                    self.window_state.style_dirty.insert(id);
                    Some((id, EventListener::PointerEnter))
                }
                HoverEvent::Leave(id) => {
                    self.window_state.style_dirty.insert(id);
                    let set = self.listeners.entry(id).or_default();
                    set.insert(EventListener::PointerLeave);
                    if let Some(mut local_cx) =
                        self.event_cx(&FloemDispatch::target(id).with_widget_opt(id.view_of()))
                    {
                        local_cx.dispatch_one();
                    }
                    None
                }
            })
            .collect::<HashMap<_, _>>();

        let mut temp = None;
        for hover in self.window_state.hover_state.current_path() {
            if let Some(view_id) = hover.view_of()
                && let Some(cursor) = view_id.state().borrow().cursor()
            {
                temp = Some(cursor);
            }
            // it is important that the node cursors override the widget cursor because non View nodes will have a widget that maps to the parent View that they are associated with
            if let Some(cursor) = self.window_state.cursors.get(hover) {
                temp = Some(*cursor);
            }
        }
        self.window_state.needs_cursor_resolution = false;
        self.window_state.cursor = temp;
    }

    /// Broadcast events to all interested views
    fn broadcast(&mut self) {
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
                            let box_node = view_state.borrow().visual_id;
                            nodes.push(box_node);
                        }
                    }
                }
            }
            nodes
        });

        // Now dispatch to each box node without holding any borrows
        for box_node in box_nodes {
            if let Some(mut local_cx) =
                self.event_cx(&FloemDispatch::target(box_node).with_widget_opt(box_node.view_of()))
            {
                local_cx.dispatch_one();
            }
        }
    }
}
