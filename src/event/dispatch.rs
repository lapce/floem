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
use understory_focus::{FocusPolicy, FocusProps, FocusSpace, adapters::box_tree::FocusPropsLookup};
use understory_responder::{
    dispatcher,
    router::{self, Router, path_from_dispatch},
    types::{
        DepthKey, Dispatch, Localizer, Outcome, ParentLookup, Phase as UnderPhase, ResolvedHit,
        ResolvedHitCow, ResolvedHitRef, WidgetLookup,
    },
};

use crate::{
    BoxTree, ViewId, VisualId,
    action::show_context_menu,
    context::*,
    dropped_file::FileDragEvent,
    event::{Event, EventListener, FocusEvent, ImeEvent, Phase, WindowEvent, path::hit_test},
    style::{Focusable, PointerEvents, PointerEventsProp, StyleSelector},
    view::{VIEW_STORAGE, View},
    visual_id::CastIds,
    window::{WindowState, tracking::is_known_root},
};

pub(crate) type FloemDispatch = Dispatch<VisualId, ViewId, Option<()>>;

pub(crate) struct BoxNodeLookup;
impl WidgetLookup<VisualId> for BoxNodeLookup {
    type WidgetId = ViewId;
    fn widget_of(&self, node: &VisualId) -> Option<Self::WidgetId> {
        Some(node.view_id())
    }
}

pub(crate) struct BoxNodeParentLookup {
    root_view_id: ViewId,
    box_tree: Rc<RefCell<BoxTree>>,
}
impl ParentLookup<VisualId> for BoxNodeParentLookup {
    fn parent_of(&self, node: &VisualId) -> Option<VisualId> {
        self.box_tree
            .borrow()
            .parent_of(node.0)
            .map(|node| VisualId(node, self.root_view_id))
    }
}

/// State captured before routing for post-processing
struct PreRouteState {
    is_pointer_down: bool,
    is_pointer_up: bool,
}

pub(crate) struct GlobalEventCx<'a> {
    pub window_state: &'a mut WindowState,
    dispatch: Option<Rc<[FloemDispatch]>>,
    all_listeners: HashSet<EventListener>,
    listeners: HashMap<VisualId, HashSet<EventListener>>,
    extra_targets: Vec<VisualId>,
    source_event: Option<Event>,
    hit_path: Option<Rc<[VisualId]>>,
}

pub struct EventCx<'a> {
    pub window_state: &'a mut WindowState,
    /// An event that has been transformed to the local coordinate space of the target node
    pub event: Event,
    /// In the case that `event` is a synthetic event like `Click`, the caused by may contain information about the triggering event.
    pub caused_by: Option<Event>,
    /// If the event is a pointer event with a point, this contains the full set of visual ids that were under the pointer.
    pub hit_path: Option<Rc<[VisualId]>>,
    /// The event phase for this local event
    pub phase: Phase,
    /// The target of this event
    pub target: VisualId,
    pub dispatch: Option<Rc<[FloemDispatch]>>,
    pub listeners: &'a HashSet<EventListener>,
    pub view_id: ViewId,
    /// Whether stopImmediatePropagation() was called
    stop_immediate: bool,
}
impl<'a> EventCx<'a> {
    /// Stop propagation to other listeners on this target AND to other nodes in the path.
    /// This is the web's stopImmediatePropagation().
    pub fn stop_immediate_propagation(&mut self) {
        self.stop_immediate = true;
    }

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

        // Track if propagation was stopped (but still run all listeners on this target)
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

        // Run event listeners on this target (unless in Capture phase)
        // Listeners run during Target and Bubble phases
        if self.phase != Phase::Capture {
            let listener = self.event.listener();
            let listeners: smallvec::SmallVec<[EventListener; 16]> =
                self.listeners.iter().cloned().collect();

            for listener in Some(listener).iter().chain(&listeners) {
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
            }
        }

        // After all listeners on this target have run, check if we should continue
        if stop_propagation {
            Outcome::Stop
        } else {
            Outcome::Continue
        }
    }
}

impl<'a> GlobalEventCx<'a> {
    pub fn new(window_state: &'a mut WindowState) -> Self {
        Self {
            window_state,
            source_event: None,
            hit_path: None,
            dispatch: None,
            all_listeners: HashSet::new(),
            listeners: HashMap::new(),
            extra_targets: Vec::new(),
        }
    }

    pub fn event_cx(&mut self, dispatch: &FloemDispatch, event: &Event) -> Option<EventCx<'_>> {
        let view_id = dispatch.widget?;
        let transform = self
            .window_state
            .box_tree
            .borrow()
            .world_transform(dispatch.node.0)
            .unwrap_or_default();

        let listeners: &HashSet<_> =
            if let Some(node_listeners) = self.listeners.get_mut(&dispatch.node) {
                node_listeners.extend(self.all_listeners.iter().cloned());
                node_listeners.extend(std::iter::once(event.listener()));
                node_listeners
            } else {
                &self.all_listeners
            };

        Some(EventCx {
            window_state: self.window_state,
            event: event.clone().transform(transform),
            caused_by: self.source_event.clone().map(|c| c.transform(transform)),
            hit_path: self.hit_path.clone(),
            phase: dispatch.phase.into(),
            target: dispatch.node,
            dispatch: self.dispatch.clone(),
            listeners,
            view_id,
            stop_immediate: false,
        })
    }

    pub fn run(&mut self, event: Event) {
        self.source_event = Some(event.clone());
        // Capture state before routing
        let pre_state = self.pre_route(&event);

        // Route the event
        self.route_event(&event);

        self.handle_default_behaviors(&event);
    }

    /// Capture state before routing and clear sets for population during routing
    fn pre_route(&mut self, event: &Event) -> PreRouteState {
        let event = event.clone();
        let is_pointer_move = matches!(event, Event::Pointer(PointerEvent::Move(_)));
        let is_pointer_down = matches!(event, Event::Pointer(PointerEvent::Down { .. }));
        let is_pointer_up = matches!(event, Event::Pointer(PointerEvent::Up { .. }));
        static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

        if let Event::Pointer(PointerEvent::Leave(info)) = event {
            self.update_hover_from_path(&[], self.window_state.last_pointer.0, info, &event);
        }

        let mut add_click_listeners = |path: &[VisualId], count: u8, secondary: bool| {
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
            let path = hit_test(self.window_state.root_view_id, point).map(|v| v.1);
            self.hit_path = path.clone();
            match event {
                Event::Pointer(PointerEvent::Down(PointerButtonEvent {
                    button, pointer, ..
                })) => {
                    // clear active on start of event handling pointer down
                    if let Some(path) = &path {
                        for hit in path.iter() {
                            if let Some(id) = hit.exact_view_id() {
                                id.request_style();
                            }
                        }
                        self.window_state.click_state.on_down(
                            pointer.pointer_id.map(|p| p.get_inner()),
                            button.map(|b| b as u8),
                            path.clone(),
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
                    if let Some(path) = &path {
                        let hit_path_len = path.len();
                        let res = self.window_state.click_state.on_up(
                            pointer.pointer_id.map(|p| p.get_inner()),
                            button.map(|b| b as u8),
                            path,
                            point,
                            Instant::now().duration_since(*START_TIME).as_millis() as u64,
                        );
                        for hit in path.iter() {
                            if let Some(id) = hit.exact_view_id() {
                                id.request_style();
                            }
                        }
                        match res {
                            ClickResult::Click(click_hit) => {
                                add_click_listeners(
                                    &click_hit,
                                    state.count,
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
                                    add_click_listeners(
                                        common_path,
                                        state.count,
                                        button == Some(PointerButton::Secondary),
                                    );
                                } else if let Some(vid) = og_target.last().unwrap().exact_view_id()
                                {
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
                    self.window_state.last_pointer = (pu.current.logical_point(), pu.pointer);
                    let exceeded_nodes = self.window_state.click_state.on_move(
                        pu.pointer.pointer_id.map(|p| p.get_inner()),
                        pu.current.logical_point(),
                    );
                    if let Some(visual_ids) = exceeded_nodes {
                        for visual_id in visual_ids.iter() {
                            if let Some(view_id) = visual_id.exact_view_id() {
                                self.window_state.style_dirty.insert(view_id);
                            }
                        }
                    }
                }

                _ => {}
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

        PreRouteState {
            is_pointer_down,
            is_pointer_up,
        }
    }

    /// Handle default behaviors (focus, click, drag, etc.)
    fn handle_default_behaviors(&mut self, event: &Event) {
        // Pointer down
        if let Event::Pointer(PointerEvent::Down(pe)) = event {
            let point = pe.state.logical_point();
            if let Some(hit) = self.hit_path.as_ref().and_then(|p| p.last().copied()) {
                self.update_focus(hit, false);
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
        if let Event::Pointer(PointerEvent::Move(pu)) = event {
            self.update_hover_from_point(pu.current.logical_point(), pu.pointer, event);

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
        }) = &event
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
        }) = event
        {
            if *modifiers == Modifiers::ALT {
                self.view_arrow_navigation(name);
            }
        }

        if let Event::Pointer(PointerEvent::Up(pe)) = event {
            let old = self.window_state.active.take();
            if let Some(old_id) = old.and_then(|id| id.exact_view_id()) {
                // To remove the styles applied by the Active selector
                self.window_state.style_dirty.insert(old_id);
            }
        }

        // Handle WindowResized for responsive styles
        if matches!(&event, Event::Window(WindowEvent::Resized(_))) {
            // Mark all views with responsive styles as needing style update
            VIEW_STORAGE.with_borrow(|storage| {
                for view_id in storage.view_ids.keys() {
                    if let Some(state) = storage.states.get(view_id) {
                        self.window_state.style_dirty.insert(view_id);
                    }
                }
            });
        }

        // Handle FileDrag for hover tracking
        if let Event::FileDrag(FileDragEvent::DragMoved { position, .. }) = &event {
            let point = Point::new(position.x, position.y);
            if let Some(path) = &self.hit_path {
                let hover_events = self.window_state.file_hover_state.update_path(path);
                for hover_event in hover_events {
                    match hover_event {
                        HoverEvent::Enter(visual_id) | HoverEvent::Leave(visual_id) => {
                            if let Some(view_id) = visual_id.exact_view_id() {
                                self.window_state.style_dirty.insert(view_id);
                            }
                        }
                    }
                }
            }
        }

        // Clear file hover on drag leave
        if let Event::FileDrag(FileDragEvent::DragLeft { .. }) = &event {
            let hover_events = self.window_state.file_hover_state.clear();
            for hover_event in hover_events {
                if let HoverEvent::Leave(visual_id) = hover_event {
                    if let Some(view_id) = visual_id.exact_view_id() {
                        self.window_state.style_dirty.insert(view_id);
                    }
                }
            }
        }
    }

    /// Update focus to a new view, firing focus enter/leave events
    pub fn update_focus(&mut self, visual_id: VisualId, keyboard_navigation: bool) {
        // Build path using router
        let mut router = Router::with_parent(
            BoxNodeLookup,
            BoxNodeParentLookup {
                root_view_id: self.window_state.root_view_id,
                box_tree: self.window_state.box_tree.clone(),
            },
        );
        router.set_scope(Some(|id, _bl, pl| {
            pl.box_tree
                .borrow()
                .flags(id.0)
                .map(|f| f.contains(NodeFlags::FOCUSABLE | NodeFlags::VISIBLE))
                .unwrap_or(false)
        }));
        let seq = router.dispatch_for::<()>(visual_id);
        let path = router::path_from_dispatch(&seq);
        self.update_focus_from_path(&path, keyboard_navigation);
    }

    pub fn update_focus_from_path(&mut self, path: &[VisualId], keyboard_navigation: bool) {
        self.window_state
            .focus_state
            .current_path()
            .last()
            .and_then(|id| id.exact_view_id())
            .map(|id| self.window_state.style_dirty.insert(id));

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
                        if let Some(mut local_cx) = self.event_cx(
                            &FloemDispatch::target(id).with_widget(id.view_id()),
                            &Event::Focus(FocusEvent::Gained),
                        ) {
                            local_cx.dispatch_one();
                        }
                    } else {
                        // This is an ancestor - subtree notification
                        if let Some(mut local_cx) = self.event_cx(
                            &FloemDispatch::target(id).with_widget(id.view_id()),
                            &Event::Focus(FocusEvent::EnteredSubtree),
                        ) {
                            local_cx.dispatch_one();
                        }
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
                        if let Some(mut local_cx) = self.event_cx(
                            &FloemDispatch::target(id).with_widget(id.view_id()),
                            &Event::Focus(FocusEvent::Lost),
                        ) {
                            local_cx.dispatch_one();
                        }
                    } else {
                        // This is an ancestor - subtree notification
                        if let Some(mut local_cx) = self.event_cx(
                            &FloemDispatch::target(id).with_widget(id.view_id()),
                            &Event::Focus(FocusEvent::LeftSubtree),
                        ) {
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
            .and_then(|id| id.exact_view_id())
            .map(|n| self.window_state.style_dirty.insert(n));

        self.window_state.keyboard_navigation = keyboard_navigation;
    }

    /// Tab navigation using understory_focus for spatial awareness
    pub(crate) fn view_tab_navigation(&mut self, backwards: bool) {
        // Get the focus scope root (could be enhanced to find actual scope boundaries)
        let scope_root = self.window_state.root_view_id.get_visual_id();

        let current_focus = self
            .window_state
            .focus_state
            .current_path()
            .last()
            .cloned()
            .unwrap_or_else(|| {
                self.window_state
                    .click_state
                    .last_click()
                    .and_then(|press| {
                        self.window_state
                            .box_tree
                            .borrow()
                            .hit_test_point(
                                press.down_position,
                                QueryFilter::new().visible().focusable(),
                            )
                            .map(|hit| VisualId(hit.node, self.window_state.root_view_id))
                    })
                    .unwrap_or(scope_root)
            });

        // Build focus space
        // TODO: retain this? if there are benefits to doing so
        let mut focus_entries = Vec::new();

        let focus_space = understory_focus::adapters::box_tree::build_focus_space_for_scope(
            &self.window_state.box_tree.clone().borrow(),
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
            self.update_focus(VisualId(new_focus, self.window_state.root_view_id), true);
        }
    }

    pub(crate) fn view_arrow_navigation(&mut self, key: &NamedKey) {
        let scope_root = self.window_state.root_view_id.get_visual_id();
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
        let focus_space = understory_focus::adapters::box_tree::build_focus_space_for_scope(
            &self.window_state.box_tree.borrow(),
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
            self.update_focus(VisualId(new_focus, self.window_state.root_view_id), true);
        }
    }

    fn route_event(&mut self, event: &Event) {
        // // Handle active view (for pointer events during drag/active state)
        match event {
            Event::Pointer(pointer_event) => {
                if let Some(active) = self.window_state.active
                    && event.is_pointer()
                {
                    self.route_directed(active, event);
                } else if let Some(point) = pointer_event.logical_point() {
                    self.route_spatial(pointer_event, point)
                } else {
                    self.route_dom(event)
                }
            }
            Event::Key(_) => {
                if let Some(focus) = self.window_state.focus_state.current_path().last() {
                    self.route_directed(*focus, event);
                } else {
                    self.route_dom(event);
                }
            }
            Event::FileDrag(_) | Event::Ime(_) | Event::Window(_) => self.route_dom(event),
            Event::PointerCapture(_) | Event::Focus(_) | Event::Interaction(_) | Event::Drag(_) => {
                unreachable!("pointer capture is an internal event and doesn't have a route target")
            }
        }
    }

    /// Route directed events (keyboard to focused view)
    fn route_directed(&mut self, target: VisualId, event: &Event) -> Option<FloemDispatch> {
        // Create router
        let router = Router::with_parent(
            BoxNodeLookup,
            BoxNodeParentLookup {
                root_view_id: self.window_state.root_view_id,
                box_tree: self.window_state.box_tree.clone(),
            },
        );
        // TODO: router filter/ scope?

        // Get dispatch sequence
        let seq = router.dispatch_for(target);

        if let Event::Key(KeyboardEvent {
            state: KeyState::Down,
            key,
            ..
        }) = event
        {
            if event.is_keyboard_trigger() {
                self.all_listeners.insert(EventListener::Click);
            }
        }

        // Dispatch events
        dispatcher::run(seq, self, |dispatch, event_cx| {
            if let Some(mut local_cx) = event_cx.event_cx(dispatch, event) {
                local_cx.dispatch_one()
            } else {
                Outcome::Continue
            }
        })
    }

    fn route_spatial(&mut self, event: &PointerEvent, point: Point) {
        let mut router = Router::with_parent(
            BoxNodeLookup,
            BoxNodeParentLookup {
                root_view_id: self.window_state.root_view_id,
                box_tree: self.window_state.box_tree.clone(),
            },
        );
        let root = self.window_state.root_view_id;

        let Some(path) = self.hit_path.clone() else {
            // No hit - clear hover state
            self.update_focus_from_path(&[], false);
            return;
        };

        let target = path.last().unwrap();

        self.update_hover_from_path(
            &path,
            point,
            event.pointer_info(),
            &Event::Pointer(event.clone()),
        );

        router.set_scope(Some(|bn, bl, pl| {
            let view_id = bl
                .widget_of(bn)
                .expect("all visual ids are associated with a ViewId");
            !view_id.is_hidden() && !view_id.pointer_events_none()
        }));

        let resolved = ResolvedHitCow {
            node: *target,
            path: Some((&*path).into()),
            depth_key: DepthKey::Z(0),
            localizer: Localizer::default(),
            meta: None::<()>,
        };

        let mut resolved_hits = vec![resolved];

        // Add resolved hits for extra targets
        for &target_node in &self.extra_targets {
            // You might need to compute the path for each target
            let target_seq = router.dispatch_for::<()>(target_node);
            let target_path = router::path_from_dispatch(&target_seq);

            resolved_hits.push(ResolvedHitCow {
                node: target_node,
                path: Some(target_path.into()),
                depth_key: DepthKey::Z(0),
                localizer: Localizer::default(),
                meta: None::<()>,
            });
        }

        let seq = router.handle_with_hits(&resolved_hits);

        // Dispatch events
        dispatcher::run(seq, self, |dispatch, event_cx| {
            let event = Event::Pointer(event.clone());
            if let Some(mut local_cx) = event_cx.event_cx(dispatch, &event) {
                local_cx.dispatch_one()
            } else {
                Outcome::Continue
            }
        });
    }

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
                if let HoverEvent::Leave(box_node) = hover_event
                    && let Some(view_id) = box_node.exact_view_id()
                {
                    view_id.request_style();
                }
            }
            return;
        };

        router.set_scope(Some(|bn, bl, pl| {
            let view_id = bl
                .widget_of(bn)
                .expect("all visual ids are associated with a ViewId");
            !view_id.is_hidden() && !view_id.pointer_events_none()
        }));

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
        path: &[VisualId],
        point: Point,
        pointer: PointerInfo,
        event: &Event,
    ) {
        let request_hover = |id: VisualId, window_state: &WindowState| {
            if let Some(view_id) = id.exact_view_id() {
                view_id.request_style();
            }
        };
        for hover_event in self.window_state.hover_state.update_path(path) {
            match hover_event {
                HoverEvent::Enter(id) => {
                    request_hover(id, self.window_state);
                    let set = self.listeners.entry(id).or_default();
                    set.insert(EventListener::PointerEnter);
                    let (point, pointer) = self.window_state.last_pointer;
                    if let Some(mut local_cx) = self.event_cx(
                        &FloemDispatch::target(id).with_widget(id.view_id()),
                        &Event::Pointer(PointerEvent::Enter(pointer)),
                    ) {
                        local_cx.dispatch_one();
                    };
                }
                HoverEvent::Leave(id) => {
                    request_hover(id, self.window_state);
                    let set = self.listeners.entry(id).or_default();
                    set.insert(EventListener::PointerLeave);
                    let (point, pointer) = self.window_state.last_pointer;
                    if let Some(mut local_cx) = self.event_cx(
                        &FloemDispatch::target(id).with_widget(id.view_id()),
                        &Event::Pointer(PointerEvent::Leave(pointer)),
                    ) {
                        local_cx.dispatch_one();
                    };
                }
            }
        }

        self.window_state.needs_cursor_resolution = true;
    }

    /// Route events to all views in DOM order, respecting propagation
    fn route_dom(&mut self, event: &Event) {
        let root = self.window_state.root_view_id;

        // Pass lazy iterator directly to dispatcher - no allocation unless needed
        dispatcher::run(dom_order_iter(root), self, |dispatch, event_cx| {
            if let Some(mut local_cx) = event_cx.event_cx(dispatch, event) {
                local_cx.dispatch_one()
            } else {
                Outcome::Continue
            }
        });
    }
}

/// Stack entry for tree traversal - tracks position in iteration
struct IterState {
    node: ViewId,
    child_index: usize,
    children: Rc<crate::view::stacking::StackingContextItems>,
}

/// Create a lazy iterator for DOM order traversal
/// Only allocates the traversal stack - stops immediately on propagation stop
fn dom_order_iter(root: ViewId) -> impl Iterator<Item = FloemDispatch> {
    use crate::view::stacking::collect_stacking_context_items;

    std::iter::from_fn({
        let mut stack: Vec<IterState> = vec![];
        let mut pending = Some(root);

        move || loop {
            // Process pending node first
            if let Some(node) = pending.take() {
                // Get children for this node
                let children = collect_stacking_context_items(node);

                // Push state for this node's children
                stack.push(IterState {
                    node,
                    child_index: 0,
                    children,
                });

                // Emit dispatch for current node
                return Some(FloemDispatch {
                    node: node.get_visual_id(),
                    widget: Some(node),
                    phase: UnderPhase::Target,
                    localizer: Localizer {},
                    meta: None,
                });
            }

            // Pop stack and advance to next child
            let state = stack.last_mut()?;

            if state.child_index < state.children.len() {
                let child = state.children[state.child_index].view_id;
                state.child_index += 1;
                pending = Some(child);
            } else {
                stack.pop();
            }
        }
    })
}
