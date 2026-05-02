use crate::gpu_resources::GpuResources;
use dpi::PhysicalPosition;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use ui_events_winit::WindowEventTranslation;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

#[cfg(target_arch = "wasm32")]
use wgpu::web_sys;

use floem_reactive::{Runtime, SignalUpdate};
use peniko::kurbo::{Point, Size};
use std::{collections::HashMap, rc::Rc, time::Duration};
use ui_events::{
    ScrollDelta,
    pointer::{
        PointerEvent, PointerGesture, PointerGestureEvent, PointerScrollEvent, PointerUpdate,
    },
};
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow},
    window::{Theme, WindowId},
};

use super::{APP_UPDATE_EVENTS, AppConfig, AppEventCallback, AppUpdateEvent, UserEvent};
use crate::{
    AppEvent, Application,
    action::{Timer, TimerToken},
    dropped_file,
    event::dropped_file::FileDragEvent,
    ext_event::EXT_EVENT_HANDLER,
    inspector::{
        Capture,
        profiler::{Profile, ProfileEvent},
    },
    paint::{PaintState, renderer::SharedSceneFragmentRendererPool},
    view::View,
    window::{
        WindowConfig,
        handle::{FrameSchedule, WindowHandle},
        id::process_window_updates,
    },
};

struct PendingContextMenu {
    window_id: WindowId,
    menu: super::MenuWrapper,
    pos: Option<Point>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PointerCoalesceKind {
    Move,
    Scroll,
    Pinch,
    Rotate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PointerCoalesceKey {
    window_id: WindowId,
    pointer_id: Option<ui_events::pointer::PointerId>,
    kind: PointerCoalesceKind,
}

struct PendingPointerEvent {
    event: PointerEvent,
}

fn pointer_coalesce_key(window_id: WindowId, event: &PointerEvent) -> Option<PointerCoalesceKey> {
    let (pointer, kind) = match event {
        PointerEvent::Move(PointerUpdate { pointer, .. }) => (pointer, PointerCoalesceKind::Move),
        PointerEvent::Scroll(PointerScrollEvent { pointer, .. }) => {
            (pointer, PointerCoalesceKind::Scroll)
        }
        PointerEvent::Gesture(PointerGestureEvent {
            pointer,
            gesture: PointerGesture::Pinch(_),
            ..
        }) => (pointer, PointerCoalesceKind::Pinch),
        PointerEvent::Gesture(PointerGestureEvent {
            pointer,
            gesture: PointerGesture::Rotate(_),
            ..
        }) => (pointer, PointerCoalesceKind::Rotate),
        PointerEvent::Down(_)
        | PointerEvent::Up(_)
        | PointerEvent::Cancel(_)
        | PointerEvent::Enter(_)
        | PointerEvent::Leave(_) => return None,
    };
    Some(PointerCoalesceKey {
        window_id,
        pointer_id: pointer.pointer_id,
        kind,
    })
}

fn coalesce_pointer_events(pending: &mut PointerEvent, next: PointerEvent) {
    match (pending, next) {
        (
            PointerEvent::Move(PointerUpdate {
                current, coalesced, ..
            }),
            PointerEvent::Move(mut next),
        ) => {
            coalesced.push(current.clone());
            coalesced.append(&mut next.coalesced);
            *current = next.current;
        }
        (
            PointerEvent::Scroll(PointerScrollEvent { delta, state, .. }),
            PointerEvent::Scroll(next),
        ) => {
            *delta = add_scroll_delta(*delta, next.delta);
            *state = next.state;
        }
        (
            PointerEvent::Gesture(PointerGestureEvent { gesture, state, .. }),
            PointerEvent::Gesture(next),
        ) => {
            *gesture = add_pointer_gesture(gesture.clone(), next.gesture);
            *state = next.state;
        }
        (pending, next) => {
            *pending = next;
        }
    }
}

fn add_scroll_delta(a: ScrollDelta, b: ScrollDelta) -> ScrollDelta {
    match (a, b) {
        (ScrollDelta::LineDelta(ax, ay), ScrollDelta::LineDelta(bx, by)) => {
            ScrollDelta::LineDelta(ax + bx, ay + by)
        }
        (ScrollDelta::PixelDelta(a), ScrollDelta::PixelDelta(b)) => {
            ScrollDelta::PixelDelta(PhysicalPosition::new(a.x + b.x, a.y + b.y))
        }
        (_, b) => b,
    }
}

fn add_pointer_gesture(a: PointerGesture, b: PointerGesture) -> PointerGesture {
    match (a, b) {
        (PointerGesture::Pinch(a), PointerGesture::Pinch(b)) => PointerGesture::Pinch(a + b),
        (PointerGesture::Rotate(a), PointerGesture::Rotate(b)) => PointerGesture::Rotate(a + b),
        (_, b) => b,
    }
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct TimingDiagnostics {
    enabled: bool,
    last_report: Option<Instant>,
    last_tick_index: Option<u64>,
    tick_count: u64,
    tick_gap_count: u64,
    tick_gap_frames: u64,
    max_tick_gap: u64,
    frame_ready_count: u64,
    present_wake_armed: u64,
    present_wake_due_now: u64,
    present_timer_fired: u64,
    present_attempts: u64,
    present_success: u64,
    max_present_timer_late_us: u128,
}

#[cfg(target_os = "macos")]
impl TimingDiagnostics {
    fn new() -> Self {
        Self {
            enabled: std::env::var_os("FLOEM_SUBDUCTION_TIMING_DIAG").is_some(),
            ..Self::default()
        }
    }

    fn record_tick(&mut self, frame_index: u64) {
        if !self.enabled {
            return;
        }
        if let Some(last) = self.last_tick_index {
            let gap = frame_index.saturating_sub(last);
            if gap > 1 {
                self.tick_gap_count = self.tick_gap_count.saturating_add(1);
                self.tick_gap_frames = self.tick_gap_frames.saturating_add(gap - 1);
                self.max_tick_gap = self.max_tick_gap.max(gap);
            }
        }
        self.last_tick_index = Some(frame_index);
        self.tick_count = self.tick_count.saturating_add(1);
    }

    fn maybe_report(&mut self) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        let Some(last_report) = self.last_report else {
            self.last_report = Some(now);
            return;
        };
        if now.duration_since(last_report) < Duration::from_secs(1) {
            return;
        }
        eprintln!(
            "subduction timing: ticks={} tick_gaps={} missing_ticks={} max_tick_gap={} frame_ready={} present_armed={} present_due_now={} present_timer={} present_attempts={} present_ok={} max_present_timer_late={:.3}ms",
            self.tick_count,
            self.tick_gap_count,
            self.tick_gap_frames,
            self.max_tick_gap,
            self.frame_ready_count,
            self.present_wake_armed,
            self.present_wake_due_now,
            self.present_timer_fired,
            self.present_attempts,
            self.present_success,
            self.max_present_timer_late_us as f64 / 1000.0,
        );
        *self = Self {
            enabled: true,
            last_report: Some(now),
            last_tick_index: self.last_tick_index,
            ..Self::default()
        };
    }
}

pub(crate) struct ApplicationHandle {
    window_handles: HashMap<winit::window::WindowId, WindowHandle>,
    next_output_id: u32,
    timers: Vec<Timer>,
    pointer_coalesce_until: HashMap<winit::window::WindowId, Instant>,
    pending_pointer_events: HashMap<PointerCoalesceKey, PendingPointerEvent>,
    pending_context_menus: Vec<PendingContextMenu>,
    pub(crate) event_listener: Option<Box<AppEventCallback>>,
    pub(crate) gpu_resources: Option<GpuResources>,
    pub(crate) scene_renderer_pool: SharedSceneFragmentRendererPool,
    pub(crate) config: AppConfig,
    #[cfg(target_os = "macos")]
    timing_diag: TimingDiagnostics,
}

impl ApplicationHandle {
    pub(crate) fn new(config: AppConfig) -> Self {
        let gpu_resources = config.gpu_resources.clone();
        Self {
            window_handles: HashMap::new(),
            next_output_id: 0,
            timers: Vec::new(),
            pointer_coalesce_until: HashMap::new(),
            pending_pointer_events: HashMap::new(),
            pending_context_menus: Vec::new(),
            event_listener: None,
            gpu_resources,
            scene_renderer_pool: SharedSceneFragmentRendererPool::default(),
            config,
            #[cfg(target_os = "macos")]
            timing_diag: TimingDiagnostics::new(),
        }
    }

    fn finalize_presented_profile_frame(handle: &mut WindowHandle, event: Option<ProfileEvent>) {
        let queued_events = handle.take_profile_events();
        let timing = handle.take_last_timing_report();
        let Some(profile) = handle.profile.as_mut() else {
            return;
        };

        profile.current.events.extend(queued_events);
        if let Some(event) = event {
            profile.current.events.push(event);
        }
        if timing.is_some() {
            profile.current.timing = timing;
            profile.next_frame();
        }
    }

    /// Applies non-render scheduling produced by a frame tick.
    ///
    /// Rendering is not driven from this method. Frame ticks are the render
    /// opportunity; the returned schedule only carries side effects that the
    /// app loop owns, currently pointer-event coalescing while scene work is
    /// deferred to the tick's submit deadline.
    fn apply_window_frame_schedule(
        &mut self,
        window_id: WindowId,
        schedule: FrameSchedule,
        event_loop: &dyn ActiveEventLoop,
    ) {
        match schedule.coalesce_input_until {
            Some(deadline) if deadline > Instant::now() => {
                self.pointer_coalesce_until.insert(window_id, deadline);
            }
            _ => {
                self.pointer_coalesce_until.remove(&window_id);
            }
        }

        if let Some(commit) = schedule.compositor_commit_deadline {
            let token = commit.token;
            let action = move |_| {
                crate::Application::send_proxy_event(UserEvent::CompositorCommitDeadline {
                    window_id,
                    generation: commit.generation,
                    token,
                });
            };
            self.request_timer(
                Timer {
                    token,
                    action: Box::new(action),
                    deadline: commit.deadline,
                    sequence: token.into_raw(),
                },
                event_loop,
            );
        }
    }

    pub(crate) fn handle_user_event(&mut self, event_loop: &dyn ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::AppUpdate => {
                self.handle_update_event(event_loop);
            }
            UserEvent::Idle => {
                self.idle();
            }
            UserEvent::QuitApp => {
                event_loop.exit();
            }
            UserEvent::Reopen {
                has_visible_windows,
            } => {
                if let Some(action) = self.event_listener.as_ref() {
                    action(AppEvent::Reopen {
                        has_visible_windows,
                    });
                }
            }
            UserEvent::GpuResourcesUpdate { window_id } => {
                let handle = self.window_handles.get_mut(&window_id).unwrap();
                if let PaintState::PendingGpuResources { window, rx } = &handle.paint_state {
                    let (gpu_resources, surface_caps) = rx.recv().unwrap().unwrap();
                    let cx = crate::paint::renderer::NewRendererCx {
                        window: window.clone(),
                        gpu_resources: Some(gpu_resources.clone()),
                        surface_caps: Some(surface_caps),
                        transparent: handle.transparent,
                        scale: handle.window_state.effective_scale(),
                        size: handle.window_state.root_size * handle.window_state.os_scale,
                        maximum_drawable_count: handle.maximum_drawable_count,
                    };
                    self.scene_renderer_pool
                        .init_if_needed(&self.config.renderer_chooser, cx);
                    self.gpu_resources = Some(gpu_resources);
                    handle.paint_state = PaintState::Initialized;
                    handle.gpu_resources = self.gpu_resources.clone();
                    handle.init_renderer();
                    if let Some(gpu_resources) = handle.gpu_resources.clone() {
                        handle.event(crate::event::Event::Window(
                            crate::event::WindowEvent::GpuResourcesReady(gpu_resources),
                        ));
                    }
                } else {
                    panic!("Sent a gpu resource update after it had already been initialized");
                }
            }
            UserEvent::ShowContextMenu {
                window_id,
                menu,
                pos,
            } => {
                self.pending_context_menus.push(PendingContextMenu {
                    window_id,
                    menu,
                    pos,
                });
            }
            UserEvent::CompositorSurfaceContent {
                window_id,
                surface_id,
                content,
            } => {
                if let Some(handle) = self.window_handles.get_mut(&window_id) {
                    handle
                        .window_state
                        .set_compositor_surface_content(surface_id, content);
                    handle.refresh_frame_activity();
                }
                self.request_update();
            }
            UserEvent::CompositorSurfaceRequestFrame {
                window_id,
                surface_id,
            } => {
                if let Some(handle) = self.window_handles.get_mut(&window_id) {
                    handle
                        .window_state
                        .compositor_surfaces
                        .request_frame(surface_id);
                    handle.note_compositor_surface_frame_demand();
                    handle.refresh_frame_activity();
                }
                self.request_update();
            }
            UserEvent::CompositorSurfaceProvider {
                window_id,
                surface_id,
                provider,
            } => {
                if let Some(handle) = self.window_handles.get_mut(&window_id) {
                    handle
                        .window_state
                        .set_compositor_surface_provider(surface_id, provider);
                    handle.refresh_frame_activity();
                }
                self.request_update();
            }
            UserEvent::SceneFragmentReady {
                window_id,
                key,
                signature,
                rendered,
                worker_index,
                render_start,
                render_end,
            } => {
                if let Some(handle) = self.window_handles.get_mut(&window_id) {
                    if handle.complete_compositor_scene_render(
                        key,
                        signature,
                        rendered,
                        worker_index,
                        render_start,
                        render_end,
                    ) {
                        handle.refresh_frame_activity();
                    }
                }
                self.request_update();
            }
            UserEvent::LayerHostCommit {
                window_id,
                committed_at,
            } => {
                if let Some(handle) = self.window_handles.get_mut(&window_id) {
                    handle.handle_layer_host_commit(committed_at);
                    handle.refresh_frame_activity();
                }
                self.request_update();
            }
            UserEvent::CompositorCommitDeadline {
                window_id,
                generation,
                token,
            } => {
                if let Some(handle) = self.window_handles.get_mut(&window_id)
                    && handle.handle_compositor_commit_deadline(generation, token)
                {
                    handle.refresh_frame_activity();
                }
                self.request_update();
            }
            UserEvent::FrameTick { window_id, tick } => {
                if crate::frame_source::frame_pacing_diag_enabled() {
                    eprintln!(
                        "floem frame pacing app tick window={:?} tick={} predicted={:?} refresh={:?}",
                        window_id, tick.frame_index, tick.predicted_present, tick.refresh_interval,
                    );
                }
                #[cfg(target_os = "macos")]
                self.timing_diag.record_tick(tick.frame_index);
                #[cfg(target_os = "macos")]
                self.timing_diag.maybe_report();
                if let Some(handle) = self.window_handles.get_mut(&window_id) {
                    handle.refresh_frame_source_target();
                }

                // Frame ticks are the handoff point for continuous input that
                // arrived while the previous compositor frame was active.
                self.flush_coalesced_pointer_events(window_id);
                Application::clear_update_posted();
                self.drain_app_update_events(event_loop);
                if Runtime::has_pending_work() {
                    Runtime::drain_pending_work();
                }

                if let Some(handle) = self.window_handles.get_mut(&window_id) {
                    let schedule = handle.process_frame_tick(tick);
                    handle.refresh_frame_activity();
                    let has_frame_work = handle.has_frame_work();
                    let _ = handle;
                    self.apply_window_frame_schedule(window_id, schedule, event_loop);
                    if has_frame_work {
                        self.request_update();
                    }
                }
            }
        }
    }

    pub(crate) fn handle_update_event(&mut self, event_loop: &dyn ActiveEventLoop) {
        Application::clear_update_posted();
        self.drain_app_update_events(event_loop);

        if Runtime::has_pending_work() {
            Runtime::drain_pending_work();
        }

        self.drain_window_update_messages(event_loop);

        if Runtime::has_pending_work() {
            self.request_update();
        }
        self.update_control_flow(event_loop);
    }

    fn drain_app_update_events(&mut self, event_loop: &dyn ActiveEventLoop) {
        let events = APP_UPDATE_EVENTS.with(|events| {
            let mut events = events.borrow_mut();
            std::mem::take(&mut *events)
        });

        for event in events {
            match event {
                AppUpdateEvent::NewWindow { window_creation } => self.new_window(
                    event_loop,
                    window_creation.view_fn,
                    self.config.global_theme_override,
                    window_creation.config.unwrap_or_default(),
                ),
                AppUpdateEvent::CloseWindow { window_id } => {
                    self.close_window(window_id, event_loop);
                }
                AppUpdateEvent::RequestCloseWindow { window_id } => {
                    if let Some(handle) = self.window_handles.get_mut(&window_id) {
                        handle.event(crate::event::Event::Window(
                            crate::event::WindowEvent::CloseRequested,
                        ));
                    }
                }
                AppUpdateEvent::RequestTimer { timer } => {
                    self.request_timer(timer, event_loop);
                }
                AppUpdateEvent::RequestAnimationFrame {
                    window_id,
                    callback,
                } => {
                    if let Some(handle) = self.window_handles.get_mut(&window_id) {
                        handle.note_animation_frame_demand();
                        handle.window_state.begin_frame_callbacks.push(callback);
                    }
                }
                AppUpdateEvent::CancelTimer { timer } => {
                    self.remove_timer(timer, event_loop);
                }
                AppUpdateEvent::CaptureWindow { window_id, capture } => {
                    capture.set(self.capture_window(window_id).map(Rc::new));
                }
                AppUpdateEvent::CaptureMetalFrame { window_id } => {
                    #[cfg(target_os = "macos")]
                    if let Some(handle) = self.window_handles.get_mut(&window_id) {
                        handle.capture_next_metal_frame();
                    }
                    #[cfg(not(target_os = "macos"))]
                    let _ = window_id;
                }
                AppUpdateEvent::ProfileWindow {
                    window_id,
                    end_profile,
                } => {
                    let handle = self.window_handles.get_mut(&window_id);
                    if let Some(handle) = handle {
                        if let Some(profile) = end_profile {
                            profile.set(handle.profile.take().map(|mut profile| {
                                profile.current.events.extend(handle.take_profile_events());
                                if profile.current.timing.is_none() {
                                    profile.current.timing = handle.pending_profile_timing();
                                }
                                handle.window_state.profile_events_enabled = false;
                                if !profile.current.events.is_empty()
                                    || profile.current.timing.is_some()
                                {
                                    profile.next_frame();
                                }
                                Rc::new(profile)
                            }));
                        } else {
                            handle.window_state.profile_events_enabled = true;
                            handle.window_state.profile_events.clear();
                            handle.profile = Some(Profile::default());
                        }
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                AppUpdateEvent::MenuAction { action_id } => {
                    for (_, handle) in self.window_handles.iter_mut() {
                        if handle.window_state.context_menu.contains_key(&action_id)
                            || handle.window_menu_actions.contains_key(&action_id)
                        {
                            handle.menu_action(&action_id);
                            break;
                        }
                    }
                }
                #[cfg(target_arch = "wasm32")]
                AppUpdateEvent::MenuAction { action_id } => {
                    for (_, handle) in self.window_handles.iter_mut() {
                        if handle.window_state.context_menu.contains_key(&action_id) {
                            handle.menu_action(&action_id);
                            break;
                        }
                    }
                }
                AppUpdateEvent::ThemeChanged { theme } => {
                    self.config.global_theme_override = Some(theme);
                    for window_handle in self.window_handles.values_mut() {
                        window_handle.window_state.light_dark_theme = theme;
                        window_handle.set_theme(Some(theme), false);
                    }
                }
            }
        }
    }

    pub(crate) fn handle_window_event(
        &mut self,
        window_id: winit::window::WindowId,
        event: WindowEvent,
        event_loop: &dyn ActiveEventLoop,
    ) {
        if !self.window_handles.contains_key(&window_id) {
            return;
        }

        let has_profile = self
            .window_handles
            .get(&window_id)
            .is_some_and(|window_handle| window_handle.profile.is_some());
        let start = has_profile.then(|| {
            let name = match event {
                WindowEvent::ActivationTokenDone { .. } => "ActivationTokenDone",
                WindowEvent::SurfaceResized(..) => "Resized",
                WindowEvent::Moved(..) => "Moved",
                WindowEvent::CloseRequested => "CloseRequested",
                WindowEvent::Destroyed => "Destroyed",
                WindowEvent::Focused(..) => "Focused",
                WindowEvent::KeyboardInput { .. } => "KeyboardInput",
                WindowEvent::ModifiersChanged(..) => "ModifiersChanged",
                WindowEvent::Ime(..) => "Ime",
                WindowEvent::PointerMoved { .. } => "PointerMoved",
                WindowEvent::PointerEntered { .. } => "PointerEntered",
                WindowEvent::PointerLeft { .. } => "PointerLeft",
                WindowEvent::MouseWheel { .. } => "MouseWheel",
                WindowEvent::PointerButton { .. } => "PointerButton",
                WindowEvent::TouchpadPressure { .. } => "TouchpadPressure",
                WindowEvent::ScaleFactorChanged { .. } => "ScaleFactorChanged",
                WindowEvent::ThemeChanged(..) => "ThemeChanged",
                WindowEvent::Occluded(..) => "Occluded",
                WindowEvent::RedrawRequested => "RedrawRequested",
                WindowEvent::PinchGesture { .. } => "PinchGesture",
                WindowEvent::PanGesture { .. } => "PanGesture",
                WindowEvent::DoubleTapGesture { .. } => "DoubleTapGesture",
                WindowEvent::RotationGesture { .. } => "RotationGesture",
                WindowEvent::DragDropped { .. } => "DroppedFile",
                WindowEvent::DragEntered { .. } => "DragEntered",
                WindowEvent::DragLeft { .. } => "DragLeft",
                WindowEvent::DragMoved { .. } => "DragMoved",
            };
            (name, Instant::now(), false)
        });

        let event_scale = self
            .window_handles
            .get(&window_id)
            .map(|window_handle| window_handle.window_state.effective_scale())
            .unwrap_or(1.0);
        let discrete_input = matches!(
            &event,
            WindowEvent::KeyboardInput {
                is_synthetic: false,
                ..
            } | WindowEvent::Ime(_)
                | WindowEvent::DoubleTapGesture { .. }
                | WindowEvent::PointerEntered { .. }
                | WindowEvent::PointerLeft { .. }
                | WindowEvent::PointerButton { .. }
                | WindowEvent::TouchpadPressure { .. }
        );
        let continuous_input = matches!(
            &event,
            WindowEvent::MouseWheel { .. }
                | WindowEvent::PinchGesture { .. }
                | WindowEvent::PanGesture { .. }
                | WindowEvent::RotationGesture { .. }
                | WindowEvent::PointerMoved { .. }
        );
        if let Some(window_handle) = self.window_handles.get_mut(&window_id) {
            if discrete_input {
                window_handle.note_discrete_input_frame_demand();
            }
            if continuous_input {
                window_handle.note_continuous_input_frame_demand();
            }
        }

        let translation = self
            .window_handles
            .get_mut(&window_id)
            .and_then(|window_handle| window_handle.event_reducer.reduce(event_scale, &event));
        match translation {
            Some(WindowEventTranslation::Keyboard(ke)) => {
                if let WindowEvent::KeyboardInput { is_synthetic, .. } = event
                    && !is_synthetic
                {
                    if let Some(window_handle) = self.window_handles.get_mut(&window_id) {
                        window_handle.key_event(ke);
                    }
                }
            }
            Some(WindowEventTranslation::Pointer(pe)) => {
                if self.should_coalesce_pointer_event(window_id, &pe) {
                    self.coalesce_pointer_event(window_id, pe);
                    return;
                }
                self.flush_coalesced_pointer_events(window_id);
                if let Some(window_handle) = self.window_handles.get_mut(&window_id) {
                    window_handle.pointer_event(pe);
                }
            }
            None => {}
        }

        let frame_presented = false;
        let Some(window_handle) = self.window_handles.get_mut(&window_id) else {
            return;
        };

        match event {
            WindowEvent::ActivationTokenDone { .. } => {}
            WindowEvent::SurfaceResized(size) => {
                let surface_size = size;
                let size: LogicalSize<f64> =
                    surface_size.to_logical(window_handle.window_state.os_scale);
                let size = Size::new(size.width, size.height);
                if std::env::var_os("FLOEM_RESIZE_DIAG").is_some() {
                    eprintln!(
                        "floem resize event t={:?} physical={}x{} logical={:.2}x{:.2} scale={:.3}",
                        Instant::now(),
                        surface_size.width,
                        surface_size.height,
                        size.width,
                        size.height,
                        window_handle.window_state.effective_scale(),
                    );
                }
                window_handle.size(size);
                self.request_update();
            }
            WindowEvent::Moved(position) => {
                let position: LogicalPosition<f64> =
                    position.to_logical(window_handle.window_state.os_scale);
                let point = Point::new(position.x, position.y);
                window_handle.position(point);
                window_handle.refresh_frame_source_target();
            }
            WindowEvent::CloseRequested => {
                if let Some(handle) = self.window_handles.get_mut(&window_id) {
                    handle.event(crate::event::Event::Window(
                        crate::event::WindowEvent::CloseRequested,
                    ));
                }
            }
            WindowEvent::Destroyed => {
                self.close_window(window_id, event_loop);
            }
            WindowEvent::DragDropped { paths, position } => {
                let logical_pos =
                    PhysicalPosition::new(position.x, position.y).to_logical(event_scale);
                let paths_rc: std::rc::Rc<[std::path::PathBuf]> = paths.clone().into();
                window_handle.file_drag_dropped(FileDragEvent::Drop(
                    dropped_file::FileDragDropped {
                        paths: paths_rc,
                        position: Point::new(logical_pos.x, logical_pos.y),
                    },
                ));
            }
            WindowEvent::DragEntered { paths, position } => {
                let logical_pos =
                    PhysicalPosition::new(position.x, position.y).to_logical(event_scale);
                window_handle.file_drag_start(paths, Point::new(logical_pos.x, logical_pos.y));
            }
            WindowEvent::DragMoved { position } => {
                let logical_pos =
                    PhysicalPosition::new(position.x, position.y).to_logical(event_scale);
                window_handle.file_drag_move(Point::new(logical_pos.x, logical_pos.y));
            }
            WindowEvent::DragLeft { .. } => {
                window_handle.file_drag_end();
            }
            WindowEvent::Focused(focused) => {
                window_handle.focused(focused);
            }
            WindowEvent::KeyboardInput { .. } => {
                // already handled by the ui-events reducer
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                window_handle.modifiers_changed(
                    ui_events_winit::keyboard::from_winit_modifier_state(modifiers.state()),
                );
            }
            WindowEvent::Ime(ime) => {
                window_handle.ime(ime);
            }
            WindowEvent::MouseWheel { .. } => {}
            WindowEvent::PinchGesture {
                delta: _, phase: _, ..
            } => {}
            WindowEvent::TouchpadPressure { .. } => {}
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                window_handle.os_scale(scale_factor);
                window_handle.refresh_frame_source_target();
            }
            WindowEvent::ThemeChanged(theme) => {
                window_handle.set_theme(Some(theme), true);
            }
            WindowEvent::Occluded(occluded) => {
                window_handle.set_occluded(occluded);
                if !occluded {
                    window_handle.refresh_frame_activity();
                    if window_handle.has_frame_work() {
                        self.request_update();
                    }
                }
            }
            WindowEvent::RedrawRequested => {}
            WindowEvent::PanGesture { .. } => {}
            WindowEvent::DoubleTapGesture { .. } => {}
            WindowEvent::RotationGesture { .. } => {}
            WindowEvent::PointerMoved { .. } => {
                //already handled by the ui-events reducer
            }
            WindowEvent::PointerEntered { .. } => {
                //already handled by the ui-events reducer
            }
            WindowEvent::PointerLeft { .. } => {
                //already handled by the ui-events reducer
            }
            WindowEvent::PointerButton { .. } => {
                //already handled by the ui-events reducer
            }
        }
        if let Some((name, start, _new_frame)) = start {
            let end = Instant::now();

            if let Some(window_handle) = self.window_handles.get_mut(&window_id) {
                let event = ProfileEvent {
                    start,
                    end,
                    name: name.to_string(),
                    depth: 0,
                };
                if frame_presented {
                    Self::finalize_presented_profile_frame(window_handle, Some(event));
                } else {
                    let queued_events = window_handle.take_profile_events();
                    if let Some(profile) = window_handle.profile.as_mut() {
                        profile.current.events.extend(queued_events);
                        profile.current.events.push(event);
                    }
                }
            }
        }
        if let Some(handle) = self.window_handles.get_mut(&window_id) {
            handle.refresh_frame_source_target();
        }
        if frame_presented {
            if let Some(handle) = self.window_handles.get_mut(&window_id) {
                handle.refresh_frame_activity();
            }
            self.update_control_flow(event_loop);
            return;
        }
        if let Some(handle) = self.window_handles.get_mut(&window_id) {
            handle.refresh_frame_activity();
        }
        self.process_window_frame_from_event(window_id, event_loop);
        self.update_control_flow(event_loop);
    }

    fn process_window_frame_from_event(
        &mut self,
        window_id: WindowId,
        event_loop: &dyn ActiveEventLoop,
    ) {
        let _ = event_loop;
        if !self.is_coalescing_pointer_events(window_id) {
            self.flush_coalesced_pointer_events(window_id);
        }

        let Some(()) = self.window_handles.get_mut(&window_id).map(|handle| {
            handle.process_update_messages_only();
            handle.refresh_frame_activity();
        }) else {
            return;
        };
    }

    fn should_coalesce_pointer_event(&self, window_id: WindowId, event: &PointerEvent) -> bool {
        self.is_coalescing_pointer_events(window_id)
            && pointer_coalesce_key(window_id, event).is_some()
    }

    fn is_coalescing_pointer_events(&self, window_id: WindowId) -> bool {
        self.pointer_coalesce_until
            .get(&window_id)
            .is_some_and(|until| Instant::now() < *until)
    }

    fn coalesce_pointer_event(&mut self, window_id: WindowId, event: PointerEvent) {
        let Some(key) = pointer_coalesce_key(window_id, &event) else {
            return;
        };
        self.pending_pointer_events
            .entry(key)
            .and_modify(|pending| coalesce_pointer_events(&mut pending.event, event.clone()))
            .or_insert(PendingPointerEvent { event });
        self.request_update();
    }

    fn flush_coalesced_pointer_events(&mut self, window_id: WindowId) {
        let mut pending = Vec::new();
        self.pending_pointer_events.retain(|key, event| {
            if key.window_id == window_id {
                pending.push(event.event.clone());
                false
            } else {
                true
            }
        });
        if pending.is_empty() {
            return;
        }
        let Some(handle) = self.window_handles.get_mut(&window_id) else {
            return;
        };
        for event in pending {
            handle.pointer_event(event);
        }
    }

    pub(crate) fn new_window(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        view_fn: Box<dyn FnOnce(WindowId) -> Box<dyn View>>,
        override_theme: Option<Theme>,
        #[allow(unused_variables)] WindowConfig {
            size,
            min_size,
            max_size,
            position,
            show_titlebar,
            transparent,
            fullscreen,
            window_icon,
            title,
            enabled_buttons,
            resizable,
            undecorated,
            undecorated_shadow,
            window_level,
            theme_override,
            apply_default_theme,
            mac_os_config,
            win_os_config,
            web_config,
            maximum_drawable_count,
        }: WindowConfig,
    ) {
        let logical_size = size.map(|size| LogicalSize::new(size.width, size.height));
        let logical_min_size = min_size.map(|size| LogicalSize::new(size.width, size.height));
        let logical_max_size = max_size.map(|size| LogicalSize::new(size.width, size.height));

        #[cfg(target_os = "macos")]
        let mut mac_attrs = winit::platform::macos::WindowAttributesMacOS::default();

        let mut window_attributes = winit::window::WindowAttributes::default()
            .with_visible(false)
            .with_title(title)
            .with_decorations(!undecorated)
            .with_transparent(transparent)
            .with_fullscreen(fullscreen)
            .with_window_level(window_level)
            .with_window_icon(window_icon)
            .with_resizable(resizable)
            // .with_theme(theme_override)
            .with_enabled_buttons(enabled_buttons);
        if theme_override.is_none() {
            window_attributes = window_attributes.with_theme(override_theme);
        } else {
            window_attributes = window_attributes.with_theme(theme_override);
        }

        #[cfg(target_arch = "wasm32")]
        {
            use wgpu::web_sys::wasm_bindgen::JsCast;

            let parent_id = web_config.expect("Specify an id for the canvas.").canvas_id;
            let doc = web_sys::window()
                .and_then(|win| win.document())
                .expect("Couldn't get document.");
            let canvas = doc
                .get_element_by_id(&parent_id)
                .expect("Couldn't get canvas by supplied id.");
            let canvas = canvas
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .expect("Element behind supplied id is not a canvas.");

            if let Some(size) = logical_size {
                canvas.set_width(size.width as u32);
                canvas.set_height(size.height as u32);
            }

            let web_attrs =
                winit::platform::web::WindowAttributesWeb::default().with_canvas(Some(canvas));
            window_attributes = window_attributes.with_platform_attributes(Box::new(web_attrs));
        };

        if let Some(Point { x, y }) = position {
            window_attributes = window_attributes.with_position(LogicalPosition::new(x, y));
        }

        if let Some(logical_size) = logical_size {
            window_attributes = window_attributes.with_surface_size(logical_size);
        }
        if let Some(logical_min_size) = logical_min_size {
            window_attributes = window_attributes.with_min_surface_size(logical_min_size);
        }
        if let Some(logical_max_size) = logical_max_size {
            window_attributes = window_attributes.with_max_surface_size(logical_max_size);
        }

        #[cfg(not(target_os = "macos"))]
        if !show_titlebar {
            window_attributes = window_attributes.with_decorations(false);
        }

        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowAttributesWindows;
            let mut win =
                WindowAttributesWindows::default().with_undecorated_shadow(undecorated_shadow);
            if let Some(cfg) = win_os_config {
                use crate::window::convert_to_win;
                win = win
                    .with_title_background_color(convert_to_win(cfg.set_title_background_color))
                    .with_border_color(convert_to_win(cfg.set_border_color))
                    .with_skip_taskbar(cfg.set_skip_taskbar)
                    .with_corner_preference(cfg.corner_preference.into())
                    .with_system_backdrop(cfg.set_system_backdrop.into())
                    .with_title_text_color(
                        convert_to_win(cfg.set_title_text_color).unwrap_or_default(),
                    );
            }
            window_attributes = window_attributes.with_platform_attributes(Box::new(win));
        }

        #[cfg(target_os = "macos")]
        if !show_titlebar {
            mac_attrs = mac_attrs
                .with_movable_by_window_background(false)
                .with_title_hidden(true)
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true);
            // .with_traffic_lights_offset(11.0, 16.0);
        }

        #[cfg(target_os = "macos")]
        if undecorated {
            // A palette-style window that will only obtain window focus but
            // not actually propagate the first mouse click it receives is
            // very unlikely to be expected behavior - these typically are
            // used for something that offers a quick choice and are closed
            // in a single pointer gesture.
            mac_attrs = mac_attrs.with_accepts_first_mouse(true);
        }

        #[cfg(target_os = "macos")]
        if let Some(mac) = &mac_os_config {
            if let Some(val) = mac.movable_by_window_background {
                mac_attrs = mac_attrs.with_movable_by_window_background(val);
            }
            if let Some(val) = mac.titlebar_transparent {
                mac_attrs = mac_attrs.with_titlebar_transparent(val);
            }
            if let Some(val) = mac.titlebar_hidden {
                mac_attrs = mac_attrs.with_titlebar_hidden(val);
            }
            if let Some(val) = mac.title_hidden {
                mac_attrs = mac_attrs.with_title_hidden(val);
            }
            if let Some(val) = mac.full_size_content_view {
                mac_attrs = mac_attrs.with_fullsize_content_view(val);
            }
            if let Some(val) = mac.unified_titlebar {
                mac_attrs = mac_attrs.with_unified_titlebar(val);
            }
            if let Some(val) = mac.movable {
                mac_attrs = mac_attrs.with_movable_by_window_background(val);
            }
            if let Some(val) = mac.accepts_first_mouse {
                mac_attrs = mac_attrs.with_accepts_first_mouse(val);
            }
            if let Some(val) = mac.option_as_alt {
                mac_attrs = mac_attrs.with_option_as_alt(val.into());
            }
            if let Some(title) = &mac.tabbing_identifier {
                mac_attrs = mac_attrs.with_tabbing_identifier(title.as_str());
            }
            if let Some(disallow_hidpi) = mac.disallow_high_dpi {
                mac_attrs = mac_attrs.with_disallow_hidpi(disallow_hidpi);
            }
            if let Some(shadow) = mac.has_shadow {
                mac_attrs = mac_attrs.with_has_shadow(shadow);
            }
            if let Some(hide) = mac.titlebar_buttons_hidden {
                mac_attrs = mac_attrs.with_titlebar_buttons_hidden(hide)
            }
            // if let Some(panel) = mac.panel {
            //     window_attributes = window_attributes.with_panel(panel)
            // }
            window_attributes = window_attributes.with_platform_attributes(Box::new(mac_attrs));
        }

        let Ok(window) = event_loop.create_window(window_attributes) else {
            return;
        };
        #[cfg(target_os = "macos")]
        if let Some(mac) = &mac_os_config
            && let Some((x, y)) = mac.traffic_lights_offset
        {
            use raw_window_handle::HasWindowHandle;

            if let Ok(wh) = window.window_handle() {
                use raw_window_handle::RawWindowHandle;

                if let RawWindowHandle::AppKit(app_kit) = wh.as_raw() {
                    let _ = setup_traffic_light_constraints_all_pixels(&app_kit, x, y, 6.);
                }
            }
        }
        let window_id = window.id();
        let output_id = self.next_output_id;
        self.next_output_id = self.next_output_id.saturating_add(1);
        let window_handle = WindowHandle::new(
            window,
            output_id,
            self.gpu_resources.clone(),
            self.config.renderer_chooser.clone(),
            self.scene_renderer_pool.clone(),
            self.config.wgpu_features,
            self.config.wgpu_backends,
            view_fn,
            transparent,
            apply_default_theme,
            maximum_drawable_count,
        );
        self.window_handles.insert(window_id, window_handle);
    }

    fn close_window(&mut self, window_id: WindowId, event_loop: &dyn ActiveEventLoop) {
        let _ = event_loop;
        self.pointer_coalesce_until.remove(&window_id);
        self.pending_pointer_events
            .retain(|key, _| key.window_id != window_id);
        if let Some(handle) = self.window_handles.get_mut(&window_id) {
            handle.destroy();
        }
        self.window_handles.remove(&window_id);
        if self.window_handles.is_empty() && self.config.exit_on_close {
            event_loop.exit();
        }
    }

    fn capture_window(&mut self, window_id: WindowId) -> Option<Capture> {
        self.window_handles
            .get_mut(&window_id)
            .map(|handle| handle.capture())
    }

    pub(crate) fn idle(&mut self) {
        let ext_events = { std::mem::take(&mut *EXT_EVENT_HANDLER.queue.lock()) };

        for trigger in ext_events {
            trigger.notify();
        }
        self.request_update();
    }

    pub(crate) fn request_update(&self) {
        Application::request_update();
    }

    fn drain_window_update_messages(&mut self, event_loop: &dyn ActiveEventLoop) {
        for (window_id, handle) in self.window_handles.iter_mut() {
            handle.process_update_messages_only();
            handle.refresh_frame_activity();

            while process_window_updates(window_id) {
                handle.process_update_messages_only();
                handle.refresh_frame_activity();
            }
        }

        let _ = event_loop;
    }

    fn update_control_flow(&self, event_loop: &dyn ActiveEventLoop) {
        if let Some(deadline) = self.timers.first().map(|timer| timer.deadline) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn request_timer(&mut self, timer: Timer, event_loop: &dyn ActiveEventLoop) {
        self.timers.push(timer);
        self.timers
            .sort_by_key(|timer| (timer.deadline, timer.sequence));
        self.fire_timer(event_loop);
    }

    fn remove_timer(&mut self, timer: TimerToken, event_loop: &dyn ActiveEventLoop) {
        self.timers.retain(|entry| entry.token != timer);
        if self.timers.is_empty() {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn fire_timer(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.timers.is_empty() {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }

        if let Some(deadline) = self.timers.first().map(|timer| timer.deadline) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
    }

    pub(crate) fn handle_timer(&mut self, event_loop: &dyn ActiveEventLoop) {
        let now = Instant::now();
        if self
            .timers
            .first()
            .is_some_and(|timer| timer.deadline <= now)
        {
            let mut any_timer_fired = false;
            while self
                .timers
                .first()
                .is_some_and(|timer| timer.deadline <= now)
            {
                let timer = self.timers.remove(0);
                let token = timer.token;
                (timer.action)(token);
                any_timer_fired = true;
            }
            if any_timer_fired {
                self.request_update();
            }
        }
        self.fire_timer(event_loop);
    }

    pub(crate) fn flush_deferred_context_menus(&mut self) {
        let pending = std::mem::take(&mut self.pending_context_menus);
        for item in pending {
            if let Some(handle) = self.window_handles.get_mut(&item.window_id) {
                handle.show_context_menu(item.menu.0, item.pos);
            }
        }
    }
}

/// Sets up traffic light button constraints with precise pixel positioning.
///
/// # Parameters
/// - `leading_pixels`: Distance from left edge of title bar to close button
///   (typically 10.0 for standard macOS positioning)
/// - `top_pixels`: Distance from top edge of title bar to **top edge** of
///   buttons
/// - `button_spacing_pixels`: Spacing between traffic light buttons (typically
///   6.0 for native macOS appearance)
///
/// # Calculating `top_pixels` for vertical centering
/// Traffic light buttons are typically 13pt tall, so to center them:
/// `top_pixels = (top_bar_height - 13.0) / 2.0`
///
/// # Example for centering in a 30pt top bar
/// ```rust,ignore
/// // Standard horizontal position (10pt), centered vertically (8.5pt from top), standard spacing (6pt)
/// setup_traffic_light_constraints_all_pixels(view_handle, 10.0, 8.5, 6.0)?;
/// ```
///
/// # Common values
/// - Standard positioning: `(10.0, 8.0, 6.0)`
/// - Centered in 30pt bar: `(10.0, 8.5, 6.0)`
/// - Centered in 40pt bar: `(10.0, 13.5, 6.0)`
/// - Centered in 50pt bar: `(10.0, 18.5, 6.0)`
#[cfg(target_os = "macos")]
fn setup_traffic_light_constraints_all_pixels(
    view_handle: &raw_window_handle::AppKitWindowHandle,
    leading_pixels: f64,
    top_pixels: f64,
    button_spacing_pixels: f64,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use {
        objc2_app_kit::{NSLayoutAttribute, NSLayoutConstraint, NSLayoutRelation, NSWindowButton},
        objc2_foundation::NSArray,
    };

    let ns_view = view_handle.ns_view.cast::<objc2_app_kit::NSView>();
    let ns_view = unsafe { &*ns_view.as_ptr() };
    let window = ns_view
        .window()
        .ok_or("View must be attached to a window")?;

    let close_button = window.standardWindowButton(NSWindowButton::CloseButton);
    let miniaturize_button = window.standardWindowButton(NSWindowButton::MiniaturizeButton);
    let zoom_button = window.standardWindowButton(NSWindowButton::ZoomButton);
    let title_bar_view = close_button
        .as_ref()
        .and_then(|button| unsafe { button.superview() })
        .ok_or("Could not find title bar container view")?;

    unsafe {
        // Set up close button with exact pixel positioning
        if let Some(close_btn) = &close_button {
            close_btn.setTranslatesAutoresizingMaskIntoConstraints(false);

            let leading = NSLayoutConstraint::constraintWithItem_attribute_relatedBy_toItem_attribute_multiplier_constant(
                close_btn,
                NSLayoutAttribute::Leading,
                NSLayoutRelation::Equal,
                Some(&title_bar_view),
                NSLayoutAttribute::Leading,
                1.0,
                leading_pixels,
            );

            let top = NSLayoutConstraint::constraintWithItem_attribute_relatedBy_toItem_attribute_multiplier_constant(
                close_btn,
                NSLayoutAttribute::Top,
                NSLayoutRelation::Equal,
                Some(&title_bar_view),
                NSLayoutAttribute::Top,
                1.0,
                top_pixels,
            );

            title_bar_view.addConstraints(&NSArray::from_slice(&[&*leading, &*top]));
        }

        // Set up other buttons with custom spacing
        if let (Some(mini_btn), Some(close_btn)) = (&miniaturize_button, &close_button) {
            mini_btn.setTranslatesAutoresizingMaskIntoConstraints(false);

            let leading = NSLayoutConstraint::constraintWithItem_attribute_relatedBy_toItem_attribute_multiplier_constant(
                mini_btn,
                NSLayoutAttribute::Leading,
                NSLayoutRelation::Equal,
                Some(close_btn),
                NSLayoutAttribute::Trailing,
                1.0,
                button_spacing_pixels,
            );

            let center_y = NSLayoutConstraint::constraintWithItem_attribute_relatedBy_toItem_attribute_multiplier_constant(
                mini_btn,
                NSLayoutAttribute::CenterY,
                NSLayoutRelation::Equal,
                Some(close_btn),
                NSLayoutAttribute::CenterY,
                1.0,
                0.0,
            );

            title_bar_view.addConstraints(&NSArray::from_slice(&[&*leading, &*center_y]));
        }

        if let (Some(zoom_btn), Some(mini_btn)) = (&zoom_button, &miniaturize_button) {
            zoom_btn.setTranslatesAutoresizingMaskIntoConstraints(false);

            let leading = NSLayoutConstraint::constraintWithItem_attribute_relatedBy_toItem_attribute_multiplier_constant(
                zoom_btn,
                NSLayoutAttribute::Leading,
                NSLayoutRelation::Equal,
                Some(mini_btn),
                NSLayoutAttribute::Trailing,
                1.0,
                button_spacing_pixels,
            );

            let center_y = NSLayoutConstraint::constraintWithItem_attribute_relatedBy_toItem_attribute_multiplier_constant(
                zoom_btn,
                NSLayoutAttribute::CenterY,
                NSLayoutRelation::Equal,
                Some(mini_btn),
                NSLayoutAttribute::CenterY,
                1.0,
                0.0,
            );

            title_bar_view.addConstraints(&NSArray::from_slice(&[&*leading, &*center_y]));
        }
    }

    Ok(())
}
