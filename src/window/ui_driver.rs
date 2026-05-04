use floem_reactive::Scope;
use rustc_hash::FxHashMap;
use std::collections::HashMap;

use crate::{
    compositor_surface::CompositorSurfaceId,
    context::{LayoutChanged, StyleCx, UpdateCx, VisualChanged},
    event::{
        CustomEvent, Event, GlobalEventCx, RouteKind, ScrollTo, UpdatePhaseEvent, WindowEvent,
    },
    frame::FrameTime,
    inspector::TimingKind,
    message::{
        CENTRAL_DEFERRED_UPDATE_MESSAGES, CENTRAL_UPDATE_MESSAGES, DEFERRED_UPDATE_MESSAGES,
        UPDATE_MESSAGES, UpdateMessage,
    },
    paint::composition::CompositionPlan,
    platform::menu_types::{Menu as MudaMenu, MenuId},
    style::{StyleSelector, recalc::StyleReason},
    view::{ViewId, process_pending_scope_reparents},
    window::compositor_surface::{CompositorSurfaceEntry, WindowCompositorSurfaces},
    window::handle::{FrameTimingAccumulator, set_current_view},
};

use super::state::WindowState;

use peniko::kurbo::{self, Point, Size, Vec2};
use winit::window::ResizeDirection;

pub(crate) enum PlatformRequest {
    DragWindow,
    FocusWindow,
    DragResizeWindow(ResizeDirection),
    ToggleWindowMaximized,
    SetWindowMaximized(bool),
    MinimizeWindow,
    SetWindowDelta(Vec2),
    SetWindowTitle(String),
    SetWindowTheme {
        theme: Option<winit::window::Theme>,
        effective_scale: f64,
    },
    ShowContextMenu {
        menu: MudaMenu,
        pos: Option<Point>,
    },
    WindowMenu {
        menu: MudaMenu,
        actions: HashMap<MenuId, Box<dyn Fn()>>,
    },
    SetImeAllowed(bool),
    SetImeCursorArea {
        position: Point,
        size: Size,
        user_scale: f64,
    },
    Inspect,
    CaptureMetalFrame,
    WindowVisible(bool),
}

#[derive(Default)]
pub(crate) struct UiUpdateOutcome {
    pub(crate) toggle_hud: bool,
    pub(crate) schedule_repaint: bool,
}

#[derive(Clone)]
pub(crate) struct UiFrameArtifact {
    pub(crate) composition_plan: CompositionPlan,
    pub(crate) compositor_surfaces: FxHashMap<CompositorSurfaceId, CompositorSurfaceEntry>,
    pub(crate) effective_scale: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UiFrameStatus {
    pub(crate) has_next_window_frame_work: bool,
    pub(crate) has_pending_paint: bool,
    pub(crate) has_pending_render: bool,
    pub(crate) has_compositor_surfaces: bool,
    pub(crate) root_size: Size,
}

/// UI-owned window state and work driver.
///
/// This is the in-process version of the future UI-thread owner described in
/// `docs/window-ui-thread-split.md`. It intentionally starts as a thin wrapper
/// around `WindowState` so the first step is mechanical and behavior-preserving:
/// callers still run synchronously, but ownership is no longer represented as a
/// bare field on the main-thread `WindowHandle`.
pub(crate) struct WindowUiDriver {
    pub(crate) root_id: ViewId,
    pub(crate) scope: Scope,
    pub(crate) state: WindowState,
    platform_requests: Vec<PlatformRequest>,
}

impl WindowUiDriver {
    pub(crate) fn new(root_id: ViewId, scope: Scope, state: WindowState) -> Self {
        Self {
            root_id,
            scope,
            state,
            platform_requests: Vec::new(),
        }
    }

    pub(crate) fn request_platform(&mut self, request: PlatformRequest) {
        self.platform_requests.push(request);
    }

    pub(crate) fn take_platform_requests(&mut self) -> Vec<PlatformRequest> {
        std::mem::take(&mut self.platform_requests)
    }

    pub(crate) fn process_central_messages(&self) {
        CENTRAL_UPDATE_MESSAGES.with_borrow_mut(|central_msgs| {
            if !central_msgs.is_empty() {
                UPDATE_MESSAGES.with_borrow_mut(|msgs| {
                    let removed_central_msgs =
                        std::mem::replace(central_msgs, Vec::with_capacity(central_msgs.len()));
                    for (id, msg) in removed_central_msgs {
                        if let Some(root) = id.try_root() {
                            let msgs = msgs.entry(root).or_default();
                            msgs.push(msg);
                        }
                    }
                });
            }
        });

        CENTRAL_DEFERRED_UPDATE_MESSAGES.with(|central_msgs| {
            if !central_msgs.borrow().is_empty() {
                DEFERRED_UPDATE_MESSAGES.with(|msgs| {
                    let mut msgs = msgs.borrow_mut();
                    let removed_central_msgs = std::mem::replace(
                        &mut *central_msgs.borrow_mut(),
                        Vec::with_capacity(msgs.len()),
                    );
                    for (id, msg) in removed_central_msgs {
                        if let Some(root) = id.try_root() {
                            let msgs = msgs.entry(root).or_default();
                            msgs.push((id, msg));
                        }
                    }
                });
            }
        });
    }

    pub(crate) fn take_update_messages(&self) -> Vec<UpdateMessage> {
        self.process_central_messages();
        UPDATE_MESSAGES.with(|msgs| msgs.borrow_mut().remove(&self.root_id).unwrap_or_default())
    }

    pub(crate) fn process_update_messages(&mut self) -> UiUpdateOutcome {
        let mut outcome = UiUpdateOutcome::default();
        set_current_view(self.root_id.root());
        loop {
            let msgs = self.take_update_messages();
            if msgs.is_empty() {
                break;
            }
            for msg in msgs {
                let mut cx = UpdateCx {
                    window_state: &mut self.state,
                };
                match msg {
                    UpdateMessage::RequestStyle(id, reason) => {
                        cx.window_state.request_style_with(id, reason);
                    }
                    UpdateMessage::RequestLayout => {
                        cx.window_state.request_layout();
                    }
                    UpdateMessage::MarkViewLayoutDirty(id) => {
                        let _ = id.mark_view_layout_dirty();
                    }
                    UpdateMessage::RequestBoxTreeUpdate => {
                        cx.window_state.request_box_tree_update();
                    }
                    UpdateMessage::RequestBoxTreeUpdateForView(view_id) => {
                        cx.window_state.request_box_tree_update_for_view(view_id);
                    }
                    UpdateMessage::RequestBoxTreeCommit => {
                        cx.window_state.request_box_tree_commit();
                    }
                    UpdateMessage::RequestPaint(id) => {
                        cx.window_state.request_paint(id);
                    }
                    UpdateMessage::Focus(id) => {
                        let keyboard_navigation = cx.window_state.keyboard_navigation;
                        let root_element_id = cx.window_state.root_view_id.get_element_id();
                        GlobalEventCx::new(
                            cx.window_state,
                            root_element_id,
                            Event::Window(WindowEvent::UpdatePhase(
                                UpdatePhaseEvent::ProcessingMessages,
                            )),
                        )
                        .update_focus(id, keyboard_navigation);
                    }
                    UpdateMessage::ClearFocus => {
                        let root_element_id = cx.window_state.root_view_id.get_element_id();
                        GlobalEventCx::new(
                            cx.window_state,
                            root_element_id,
                            Event::Window(WindowEvent::UpdatePhase(
                                UpdatePhaseEvent::ProcessingMessages,
                            )),
                        )
                        .clear_focus();
                    }
                    UpdateMessage::SetPointerCapture {
                        element_id: view_id,
                        pointer_id,
                    } => {
                        cx.window_state.set_pointer_capture(pointer_id, view_id);
                    }
                    UpdateMessage::ReleasePointerCapture {
                        element_id: view_id,
                        pointer_id,
                    } => {
                        cx.window_state.release_pointer_capture(pointer_id, view_id);
                    }
                    UpdateMessage::ScrollTo { id, rect } => {
                        let event = Event::new_custom(ScrollTo { id, rect });
                        GlobalEventCx::new(cx.window_state, self.root_id.get_element_id(), event)
                            .route_normal(RouteKind::bubble_from(id), None);
                    }
                    UpdateMessage::State { id, state } => {
                        let view = id.view();
                        view.borrow_mut().update(&mut cx, state);
                    }
                    UpdateMessage::DragWindow => {
                        self.request_platform(PlatformRequest::DragWindow);
                    }
                    UpdateMessage::FocusWindow => {
                        self.request_platform(PlatformRequest::FocusWindow);
                    }
                    UpdateMessage::DragResizeWindow(direction) => {
                        self.request_platform(PlatformRequest::DragResizeWindow(direction));
                    }
                    UpdateMessage::ToggleWindowMaximized => {
                        self.request_platform(PlatformRequest::ToggleWindowMaximized);
                    }
                    UpdateMessage::SetWindowMaximized(maximized) => {
                        self.request_platform(PlatformRequest::SetWindowMaximized(maximized));
                    }
                    UpdateMessage::MinimizeWindow => {
                        self.request_platform(PlatformRequest::MinimizeWindow);
                    }
                    UpdateMessage::SetWindowDelta(delta) => {
                        self.request_platform(PlatformRequest::SetWindowDelta(delta));
                    }
                    UpdateMessage::WindowScale(scale) => {
                        cx.window_state.user_scale = scale;
                        cx.window_state
                            .update_default_theme(cx.window_state.light_dark_theme);
                        cx.window_state
                            .mark_style_dirty(cx.window_state.root_view_id.get_element_id());
                        let effective_scale = cx.window_state.effective_scale();
                        let root_view_id = cx.window_state.root_view_id;
                        cx.window_state.request_paint(root_view_id);
                        self.root_id.request_layout();
                        self.route_window_event(Event::Window(WindowEvent::ScaleChanged(
                            effective_scale,
                        )));
                        outcome.schedule_repaint = true;
                    }
                    UpdateMessage::ShowContextMenu { menu, pos } => {
                        let (menu, registry) = menu.build();
                        cx.window_state.context_menu.clear();
                        cx.window_state.update_context_menu(registry);
                        self.request_platform(PlatformRequest::ShowContextMenu { menu, pos });
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    UpdateMessage::WindowMenu { menu } => {
                        let (menu, registry) = menu.build();
                        self.request_platform(PlatformRequest::WindowMenu {
                            menu,
                            actions: registry,
                        });
                    }
                    UpdateMessage::SetWindowTitle { title } => {
                        self.request_platform(PlatformRequest::SetWindowTitle(title));
                    }
                    UpdateMessage::SetImeAllowed { allowed } => {
                        self.request_platform(PlatformRequest::SetImeAllowed(allowed));
                    }
                    UpdateMessage::SetImeCursorArea { position, size } => {
                        let user_scale = cx.window_state.user_scale;
                        self.request_platform(PlatformRequest::SetImeCursorArea {
                            position,
                            size,
                            user_scale,
                        });
                    }
                    UpdateMessage::Inspect => {
                        self.request_platform(PlatformRequest::Inspect);
                    }
                    UpdateMessage::CaptureMetalFrame => {
                        self.request_platform(PlatformRequest::CaptureMetalFrame);
                    }
                    UpdateMessage::ToggleHud => {
                        outcome.toggle_hud = true;
                    }
                    UpdateMessage::AddOverlay { view } => {
                        self.root_id.add_child(view);
                        self.root_id.request_all();
                    }
                    UpdateMessage::RemoveOverlay { id } => {
                        cx.window_state.remove_view(id);
                        self.root_id.request_all();
                    }
                    UpdateMessage::RegisterListener(key, id) => {
                        cx.window_state.listeners.entry(key).or_default().push(id);
                        id.state().borrow_mut().registered_listener_keys.push(key);
                    }
                    UpdateMessage::RemoveListener(key, id) => {
                        if let Some(ids) = cx.window_state.listeners.get_mut(&key) {
                            ids.retain(|v| *v != id);
                        }
                        if let Ok(mut state) = id.state().try_borrow_mut() {
                            state.registered_listener_keys.retain(|k| *k != key);
                        }
                    }
                    UpdateMessage::WindowVisible(visible) => {
                        self.request_platform(PlatformRequest::WindowVisible(visible));
                    }
                    UpdateMessage::ViewTransitionAnimComplete(id) => {
                        let num_waiting =
                            id.state().borrow().num_waiting_animations.saturating_sub(1);
                        id.state().borrow_mut().num_waiting_animations = num_waiting;
                    }
                    UpdateMessage::SetTheme(theme) => {
                        self.state.mark_style_dirty_selector(
                            self.state.root_view_id.get_element_id(),
                            StyleSelector::DarkMode,
                        );
                        if let Some(theme) = theme {
                            self.state.update_default_theme(theme);
                            self.state.light_dark_theme = theme;
                            self.state.theme_overriden = true;
                            self.root_id.request_style(StyleReason::inherited());
                            self.route_window_event(Event::Window(WindowEvent::ThemeChanged(
                                theme,
                            )));
                        } else {
                            self.state.theme_overriden = false;
                            self.root_id.request_style(StyleReason::inherited());
                        }
                        self.request_platform(PlatformRequest::SetWindowTheme {
                            theme,
                            effective_scale: self.state.effective_scale(),
                        });
                    }
                    UpdateMessage::RemoveViews(view_ids) => {
                        for view_id in view_ids {
                            cx.window_state.remove_view(view_id);
                        }
                    }
                    UpdateMessage::AddChild {
                        parent_id,
                        mut child,
                    } => {
                        let scope = parent_id.find_scope().unwrap_or_else(Scope::current);
                        let view = child.build(scope);
                        parent_id.add_child(view);
                        parent_id.request_all();
                    }
                    UpdateMessage::AddChildren {
                        parent_id,
                        mut children,
                    } => {
                        let scope = parent_id.find_scope().unwrap_or_else(Scope::current);
                        let views = children.build(scope);
                        parent_id.append_children(views);
                        parent_id.request_all();
                    }
                    UpdateMessage::SetupReactiveChildren { mut setup } => {
                        setup.run();
                    }
                    UpdateMessage::RouteEvent {
                        id,
                        event,
                        route_kind: dispatch_kind,
                        triggered_by,
                    } => {
                        let cx = GlobalEventCx::new(&mut self.state, id, *event);
                        cx.route_normal(dispatch_kind, triggered_by.as_deref());
                    }
                }
            }
        }
        process_pending_scope_reparents();
        self.route_window_event(Event::Window(WindowEvent::UpdatePhase(
            UpdatePhaseEvent::ProcessingMessages,
        )));
        outcome
    }

    pub(crate) fn process_deferred_update_messages(&mut self) {
        self.process_central_messages();
        let msgs = DEFERRED_UPDATE_MESSAGES
            .with(|msgs| msgs.borrow_mut().remove(&self.root_id).unwrap_or_default());
        let mut cx = UpdateCx {
            window_state: &mut self.state,
        };
        for (id, state) in msgs {
            let view = id.view();
            view.borrow_mut().update(&mut cx, state);
        }
    }

    pub(crate) fn route_window_event(&mut self, event: Event) {
        let root_element_id = self.state.root_view_id.get_element_id();
        GlobalEventCx::new(&mut self.state, root_element_id, event).route_window_event();
    }

    pub(crate) fn has_deferred_update_messages(&self) -> bool {
        DEFERRED_UPDATE_MESSAGES.with(|m| {
            m.borrow()
                .get(&self.root_id)
                .map(|m| !m.is_empty())
                .unwrap_or(false)
        })
    }

    pub(crate) fn has_next_frame_work(&self) -> bool {
        self.state.has_next_frame_work()
    }

    pub(crate) fn has_begin_frame_callbacks(&self) -> bool {
        !self.state.begin_frame_callbacks.is_empty()
    }

    pub(crate) fn promote_next_frame_work(&mut self) {
        self.state.promote_next_frame_work();
    }

    pub(crate) fn needs_layout(&self) -> bool {
        self.state.needs_layout
    }

    pub(crate) fn needs_box_tree_commit(&self) -> bool {
        self.state.needs_box_tree_commit || self.state.box_tree.borrow().needs_commit()
    }

    pub(crate) fn needs_box_tree_update(&self) -> bool {
        self.state.needs_box_tree_from_layout
    }

    pub(crate) fn needs_style(&self) -> bool {
        !self.state.style_dirty.is_empty()
    }

    pub(crate) fn has_pending_box_tree_updates(&self) -> bool {
        !self.state.views_needing_box_tree_update.is_empty()
    }

    pub(crate) fn has_current_frame_prepare_work(&self) -> bool {
        self.needs_style()
            || self.needs_layout()
            || self.needs_box_tree_update()
            || self.needs_box_tree_commit()
            || self.has_pending_box_tree_updates()
            || self.has_deferred_update_messages()
    }

    pub(crate) fn run_begin_frame_callbacks(&mut self, frame_time: FrameTime) {
        let callbacks = self.state.take_begin_frame_callbacks();
        for callback in callbacks {
            callback(frame_time);
        }
    }

    pub(crate) fn frame_artifact(
        &self,
        compositor_surfaces: &WindowCompositorSurfaces,
    ) -> UiFrameArtifact {
        UiFrameArtifact {
            composition_plan: self.state.composition_plan.clone(),
            compositor_surfaces: compositor_surfaces.entries().clone(),
            effective_scale: self.state.effective_scale(),
        }
    }

    pub(crate) fn frame_status(&self) -> UiFrameStatus {
        UiFrameStatus {
            has_next_window_frame_work: self.state.has_next_window_frame_work(),
            has_pending_paint: self.state.has_pending_paint(),
            has_pending_render: self.state.has_pending_render(),
            has_compositor_surfaces: self.state.composition_plan.has_compositor_surfaces(),
            root_size: self.state.root_size,
        }
    }

    pub(crate) fn style(
        &mut self,
        active_frame_time: Option<FrameTime>,
        timing: &mut FrameTimingAccumulator,
    ) {
        let start = crate::platform::Instant::now();
        let style_now = active_frame_time
            .map(|frame_time| frame_time.now)
            .unwrap_or_else(crate::platform::Instant::now);
        loop {
            let traversal = self.state.build_style_traversal(self.root_id);
            if traversal.is_empty() {
                break;
            }

            for (view_id, traversal_reason) in traversal {
                let cx =
                    &mut StyleCx::new_at(&mut self.state, view_id, traversal_reason, style_now);
                cx.style_view();
            }
        }

        let root_element_id = self.state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Style));
        GlobalEventCx::new(&mut self.state, root_element_id, event).route_window_event();
        timing.push_absolute_span(
            "Style",
            start,
            crate::platform::Instant::now(),
            TimingKind::Style,
        );
    }

    pub(crate) fn layout(&mut self, timing: &mut FrameTimingAccumulator) {
        let start = crate::platform::Instant::now();
        self.state.compute_layout();
        let taffy_end = crate::platform::Instant::now();
        let box_tree_start = taffy_end;
        self.state.update_box_tree_from_layout();
        let box_tree_end = crate::platform::Instant::now();

        let root_element_id = self.state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Layout));
        GlobalEventCx::new(&mut self.state, root_element_id, event).route_window_event();
        timing.push_absolute_span("Layout", start, box_tree_end, TimingKind::Layout);
        timing.push_absolute_span("Taffy", start, taffy_end, TimingKind::Layout);
        timing.push_absolute_span(
            "BoxTreeUpdate",
            box_tree_start,
            box_tree_end,
            TimingKind::BoxTree,
        );
    }

    pub(crate) fn update_box_tree_from_layout(&mut self, timing: &mut FrameTimingAccumulator) {
        let start = crate::platform::Instant::now();
        self.state.update_box_tree_from_layout();
        let root_element_id = self.state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::BoxTreeUpdate));
        GlobalEventCx::new(&mut self.state, root_element_id, event).route_window_event();
        timing.push_absolute_span(
            "BoxTreeUpdate",
            start,
            crate::platform::Instant::now(),
            TimingKind::BoxTree,
        );
    }

    pub(crate) fn process_pending_box_tree_updates(&mut self, timing: &mut FrameTimingAccumulator) {
        let start = crate::platform::Instant::now();
        self.state.process_pending_box_tree_updates();
        let root_element_id = self.state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(
            UpdatePhaseEvent::BoxTreePendingUpdates,
        ));
        GlobalEventCx::new(&mut self.state, root_element_id, event).route_window_event();
        timing.push_absolute_span(
            "BoxTreePendingUpdates",
            start,
            crate::platform::Instant::now(),
            TimingKind::BoxTree,
        );
    }

    pub(crate) fn commit_box_tree(&mut self, timing: &mut FrameTimingAccumulator) {
        let start = crate::platform::Instant::now();
        self.state.commit_box_tree();
        self.state.needs_box_tree_commit = false;

        let has_layout_listener: smallvec::SmallVec<[ViewId; 64]> = self
            .state
            .listeners
            .get(&LayoutChanged::listener_key())
            .into_iter()
            .flatten()
            .copied()
            .collect();
        for id in has_layout_listener {
            if let Some(layout) = id.get_layout() {
                let window_origin = id.get_layout_window_origin();
                let new_box = kurbo::Rect::from_origin_size(
                    (layout.location.x as f64, layout.location.y as f64),
                    (layout.size.width as f64, layout.size.height as f64),
                );
                let new_content_box = kurbo::Rect::from_origin_size(
                    (layout.content_box_x() as f64, layout.content_box_y() as f64),
                    (
                        layout.content_box_width() as f64,
                        layout.content_box_height() as f64,
                    ),
                );
                let new_layout = LayoutChanged {
                    new_box,
                    new_content_box,
                    new_window_origin: window_origin,
                };
                let (old_layout, element_id) = {
                    let state = id.state();
                    let mut state = state.borrow_mut();
                    let old: Option<LayoutChanged> = state.layout;
                    state.layout = Some(new_layout);
                    let element_id = state.element_id;
                    (old, element_id)
                };
                if old_layout.is_none_or(|old| old != new_layout) {
                    use crate::context::Phases;
                    GlobalEventCx::new(&mut self.state, element_id, Event::new_custom(new_layout))
                        .route_normal(
                            RouteKind::Directed {
                                target: element_id,
                                phases: Phases::TARGET,
                            },
                            None,
                        );
                }
            }
        }

        let needs_moved: smallvec::SmallVec<[ViewId; 64]> = self
            .state
            .listeners
            .get(&VisualChanged::listener_key())
            .into_iter()
            .flatten()
            .copied()
            .collect();
        for id in needs_moved {
            let transform = id.get_visual_transform();
            let visual_aabb = id.get_visual_rect();
            let element_id = id.get_element_id();

            let new_visual = VisualChanged {
                new_visual_aabb: visual_aabb,
                new_world_transform: transform,
            };

            let old_visual = {
                let state = id.state();
                let mut state = state.borrow_mut();
                let old = state.visual_change;
                state.visual_change = Some(new_visual);
                old
            };

            if old_visual.is_none_or(|old| old != new_visual) {
                use crate::context::Phases;
                GlobalEventCx::new(&mut self.state, element_id, Event::new_custom(new_visual))
                    .route_normal(
                        RouteKind::Directed {
                            target: element_id,
                            phases: Phases::TARGET,
                        },
                        None,
                    );
            }
        }

        let root_element_id = self.state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::BoxTreeCommit));
        GlobalEventCx::new(&mut self.state, root_element_id, event).route_window_event();

        timing.push_absolute_span(
            "BoxTreeCommit",
            start,
            crate::platform::Instant::now(),
            TimingKind::BoxTree,
        );
    }
}
