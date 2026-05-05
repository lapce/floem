use std::{cell::RefCell, rc::Rc, sync::mpsc};

use floem_reactive::{Runtime, Scope, SignalUpdate};
use peniko::kurbo::Size;
use rustc_hash::FxHashMap;
use winit::window::{Theme, WindowId};

use crate::{
    compositor_surface::CompositorSurfaceId,
    event::{Event, PaintPresentInfo, UpdatePhaseEvent, WindowEvent},
    frame::FrameTime,
    gpu_resources::GpuResources,
    inspector::{
        CAPTURE, Capture,
        profiler::{PROFILE, Profile, ProfileEvent},
    },
    platform::{Instant, menu_types::MenuId},
    view::{View, ViewId},
    window::compositor_surface::{CompositorSurfaceEntry, WindowCompositorSurfaces},
};

use super::{
    handle::FrameTimingAccumulator,
    state::WindowState,
    ui_driver::{
        PlatformRequest, UiFrameStatus, UiPlatformEvent, UiSceneSubmission, WindowUiDriver,
    },
};

pub(crate) enum UiCommand {
    Run(Box<dyn FnOnce(&mut WindowUiDriver) + Send>),
    RunInspector(UncheckedSend<Box<dyn FnOnce(&mut WindowUiDriver)>>),
    Stop,
}

pub(crate) struct UncheckedSend<T>(T);

// Transitional bridge for inspector capture data, which is still Rc-based from
// the single-UI-thread design. Keep this private and do not use it for normal
// UI/runtime traffic.
unsafe impl<T> Send for UncheckedSend<T> {}

pub(crate) enum WindowUiRuntime {
    Direct(RefCell<WindowUiDriver>),
    Threaded { tx: mpsc::Sender<UiCommand> },
}

impl WindowUiRuntime {
    pub(crate) fn new_direct(root_id: ViewId, scope: Scope, state: WindowState) -> Self {
        Self::Direct(RefCell::new(WindowUiDriver::new(root_id, scope, state)))
    }

    pub(crate) fn new_threaded(
        window_id: WindowId,
        root_size: Size,
        os_theme: Option<Theme>,
        os_scale: f64,
        view_fn: impl FnOnce(WindowId) -> Box<dyn View> + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<UiCommand>();
        std::thread::Builder::new()
            .name(format!("floem-ui-{window_id:?}"))
            .spawn(move || {
                Runtime::init_on_ui_thread();
                let mut driver =
                    WindowUiDriver::new_window(window_id, root_size, os_theme, os_scale, view_fn);
                while let Ok(command) = rx.recv() {
                    match command {
                        UiCommand::Run(f) => f(&mut driver),
                        UiCommand::RunInspector(f) => f.0(&mut driver),
                        UiCommand::Stop => break,
                    }
                }
                let root_id = driver.root_id_for_legacy_tracking();
                driver.dispose_scope();
                super::tracking::remove_root_window_id_mapping(&root_id);
            })
            .expect("failed to spawn Floem UI thread");
        Self::Threaded { tx }
    }

    fn call<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut WindowUiDriver) -> R + Send + 'static,
    ) -> R {
        match self {
            Self::Direct(driver) => f(&mut driver.borrow_mut()),
            Self::Threaded { tx } => {
                let (result_tx, result_rx) = mpsc::sync_channel(1);
                tx.send(UiCommand::Run(Box::new(move |driver| {
                    let _ = result_tx.send(f(driver));
                })))
                .expect("Floem UI thread stopped");
                result_rx.recv().expect("Floem UI thread stopped")
            }
        }
    }

    pub(crate) fn call_inspector_capture<R: 'static>(
        &self,
        f: impl FnOnce(&mut WindowUiDriver) -> R + 'static,
    ) -> R {
        match self {
            Self::Direct(driver) => f(&mut driver.borrow_mut()),
            Self::Threaded { tx } => {
                let (result_tx, result_rx) = mpsc::sync_channel(1);
                tx.send(UiCommand::RunInspector(UncheckedSend(Box::new(
                    move |driver| {
                        let _ = result_tx.send(UncheckedSend(f(driver)));
                    },
                ))))
                .expect("Floem UI thread stopped");
                result_rx.recv().expect("Floem UI thread stopped").0
            }
        }
    }

    pub(crate) fn set_inspector_capture(&self, capture: Capture) {
        self.call_inspector_capture(move |_| {
            CAPTURE.with(|signal| signal.set(Some(Rc::new(capture))));
        });
    }

    pub(crate) fn set_inspector_profile(&self, profile: Profile) {
        self.call_inspector_capture(move |_| {
            PROFILE.with(|signal| signal.set(Some(Rc::new(profile))));
        });
    }

    pub(crate) fn with_direct<R>(&self, f: impl FnOnce(&mut WindowUiDriver) -> R) -> R {
        match self {
            Self::Direct(driver) => f(&mut driver.borrow_mut()),
            Self::Threaded { .. } => {
                panic!("attempted to access UI-thread-only state from the main thread")
            }
        }
    }

    pub(crate) fn current_theme(&self) -> Theme {
        self.call(|ui| ui.current_theme())
    }

    pub(crate) fn effective_scale(&self) -> f64 {
        self.call(|ui| ui.effective_scale())
    }

    pub(crate) fn os_scale(&self) -> f64 {
        self.call(|ui| ui.os_scale())
    }

    pub(crate) fn root_physical_size(&self) -> Size {
        self.call(|ui| ui.root_physical_size())
    }

    pub(crate) fn user_scale(&self) -> f64 {
        self.call(|ui| ui.user_scale())
    }

    pub(crate) fn update_os_scale(&self, os_scale: f64) {
        self.call(move |ui| ui.update_os_scale(os_scale));
    }

    pub(crate) fn set_theme(&self, theme: Option<Theme>, change_from_os: bool) -> bool {
        self.call(move |ui| ui.set_theme(theme, change_from_os))
    }

    pub(crate) fn resize(&self, size: Size, is_maximized: bool) {
        self.call(move |ui| ui.resize(size, is_maximized));
    }

    pub(crate) fn maximize_changed(&self, is_maximized: bool) {
        self.call(move |ui| ui.maximize_changed(is_maximized));
    }

    pub(crate) fn request_root_paint(&self) {
        self.call(|ui| ui.request_root_paint());
    }

    pub(crate) fn toggle_hud(&self) {
        self.call(|ui| ui.toggle_hud());
    }

    pub(crate) fn route_platform_event(
        &self,
        event: UiPlatformEvent,
    ) -> super::ui_driver::UiUpdateOutcome {
        self.call(move |ui| ui.route_platform_event(event))
    }

    pub(crate) fn route_gpu_resources_ready(&self, gpu_resources: GpuResources) {
        self.call(move |ui| {
            ui.route_window_event(Event::Window(WindowEvent::GpuResourcesReady(gpu_resources)));
        });
    }

    pub(crate) fn route_closed(&self) {
        self.call(|ui| ui.route_window_event(Event::Window(WindowEvent::Closed)));
    }

    pub(crate) fn route_close_requested(&self) {
        self.call(|ui| ui.route_window_event(Event::Window(WindowEvent::CloseRequested)));
    }

    pub(crate) fn route_update_phase_complete(&self) {
        self.call(|ui| {
            ui.route_window_event(Event::Window(WindowEvent::UpdatePhase(
                UpdatePhaseEvent::Complete,
            )));
        });
    }

    pub(crate) fn route_paint_present(&self, info: PaintPresentInfo) {
        self.call(move |ui| {
            ui.record_present(&info);
            ui.route_window_event(Event::Window(WindowEvent::UpdatePhase(
                UpdatePhaseEvent::PaintPresent(info),
            )));
        });
    }

    pub(crate) fn route_event_local(&self, event: Event) {
        self.with_direct(|ui| ui.route_window_event(event));
    }

    pub(crate) fn process_update_messages(&self) -> super::ui_driver::UiUpdateOutcome {
        self.call(|ui| ui.process_update_messages())
    }

    pub(crate) fn process_deferred_update_messages(&self) {
        self.call(|ui| ui.process_deferred_update_messages());
    }

    pub(crate) fn take_platform_requests(&self) -> Vec<PlatformRequest> {
        self.call(|ui| ui.take_platform_requests())
    }

    pub(crate) fn frame_status(&self) -> UiFrameStatus {
        self.call(|ui| ui.frame_status())
    }

    pub(crate) fn has_next_frame_work(&self) -> bool {
        self.call(|ui| ui.has_next_frame_work())
    }

    pub(crate) fn has_begin_frame_callbacks(&self) -> bool {
        self.call(|ui| ui.has_begin_frame_callbacks())
    }

    pub(crate) fn has_current_frame_prepare_work(&self) -> bool {
        self.call(|ui| ui.has_current_frame_prepare_work())
    }

    pub(crate) fn has_deferred_update_messages(&self) -> bool {
        self.call(|ui| ui.has_deferred_update_messages())
    }

    pub(crate) fn has_pending_box_tree_updates(&self) -> bool {
        self.call(|ui| ui.has_pending_box_tree_updates())
    }

    pub(crate) fn needs_layout(&self) -> bool {
        self.call(|ui| ui.needs_layout())
    }

    pub(crate) fn needs_box_tree_commit(&self) -> bool {
        self.call(|ui| ui.needs_box_tree_commit())
    }

    pub(crate) fn needs_box_tree_update(&self) -> bool {
        self.call(|ui| ui.needs_box_tree_update())
    }

    pub(crate) fn needs_style(&self) -> bool {
        self.call(|ui| ui.needs_style())
    }

    pub(crate) fn promote_next_frame_work(&self, frame_time: FrameTime) {
        self.call(move |ui| ui.promote_next_frame_work(frame_time));
    }

    pub(crate) fn reset_layer_pacing_state(&self) {
        self.call(|ui| ui.reset_layer_pacing_state());
    }

    pub(crate) fn run_begin_frame_callbacks(&self, frame_time: FrameTime) {
        self.call(move |ui| ui.run_begin_frame_callbacks(frame_time));
    }

    pub(crate) fn style(
        &self,
        active_frame_time: Option<FrameTime>,
        mut timing: FrameTimingAccumulator,
    ) -> FrameTimingAccumulator {
        self.call(move |ui| {
            ui.style(active_frame_time, &mut timing);
            timing
        })
    }

    pub(crate) fn layout(&self, mut timing: FrameTimingAccumulator) -> FrameTimingAccumulator {
        self.call(move |ui| {
            ui.layout(&mut timing);
            timing
        })
    }

    pub(crate) fn update_box_tree_from_layout(
        &self,
        mut timing: FrameTimingAccumulator,
    ) -> FrameTimingAccumulator {
        self.call(move |ui| {
            ui.update_box_tree_from_layout(&mut timing);
            timing
        })
    }

    pub(crate) fn process_pending_box_tree_updates(
        &self,
        mut timing: FrameTimingAccumulator,
    ) -> FrameTimingAccumulator {
        self.call(move |ui| {
            ui.process_pending_box_tree_updates(&mut timing);
            timing
        })
    }

    pub(crate) fn commit_box_tree(
        &self,
        mut timing: FrameTimingAccumulator,
    ) -> FrameTimingAccumulator {
        self.call(move |ui| {
            ui.commit_box_tree(&mut timing);
            timing
        })
    }

    pub(crate) fn scene_submission(
        &self,
        compositor_surfaces: FxHashMap<CompositorSurfaceId, CompositorSurfaceEntry>,
    ) -> UiSceneSubmission {
        self.call(move |ui| UiSceneSubmission {
            composition_plan: ui.state.composition_plan.clone(),
            compositor_surfaces,
            effective_scale: ui.effective_scale(),
        })
    }

    pub(crate) fn prepare_display_list(
        &self,
        gpu_resources: Option<GpuResources>,
        has_layer_host: bool,
        record_paint_order: bool,
        compositor_surfaces: FxHashMap<CompositorSurfaceId, CompositorSurfaceEntry>,
        mut timing: FrameTimingAccumulator,
    ) -> (UiSceneSubmission, FrameTimingAccumulator) {
        self.call(move |ui| {
            let surfaces = WindowCompositorSurfaces::from_entries(compositor_surfaces);
            let submission = ui.prepare_display_list(
                gpu_resources,
                has_layer_host,
                record_paint_order,
                &surfaces,
                &mut timing,
            );
            (submission, timing)
        })
    }

    pub(crate) fn clear_pending_damage(&self) {
        self.call(|ui| ui.clear_pending_damage());
    }

    pub(crate) fn resolve_cursor_icon(&self) -> Option<winit::cursor::CursorIcon> {
        self.call(|ui| ui.resolve_cursor_icon())
    }

    pub(crate) fn has_context_menu_action(&self, id: &MenuId) -> bool {
        let id = id.clone();
        self.call(move |ui| ui.has_context_menu_action(&id))
    }

    pub(crate) fn has_window_menu_action(&self, id: &MenuId) -> bool {
        let id = id.clone();
        self.call(move |ui| ui.has_window_menu_action(&id))
    }

    pub(crate) fn run_context_menu_action(&self, id: &MenuId) -> bool {
        let id = id.clone();
        self.call(move |ui| ui.run_context_menu_action(&id))
    }

    pub(crate) fn run_window_menu_action(&self, id: &MenuId) -> bool {
        let id = id.clone();
        self.call(move |ui| ui.run_window_menu_action(&id))
    }

    pub(crate) fn set_profile_events_enabled(&self, enabled: bool) {
        self.call(move |ui| ui.set_profile_events_enabled(enabled));
    }

    pub(crate) fn clear_profile_events(&self) {
        self.call(|ui| ui.clear_profile_events());
    }

    pub(crate) fn take_profile_events(&self) -> Vec<ProfileEvent> {
        self.call(|ui| ui.take_profile_events())
    }

    pub(crate) fn record_profile_instant(&self, name: &'static str, at: Instant) {
        self.call(move |ui| ui.record_profile_instant(name, at));
    }

    pub(crate) fn dispose_scope(&self) {
        match self {
            Self::Direct(driver) => driver.borrow_mut().dispose_scope(),
            Self::Threaded { tx } => {
                let _ = tx.send(UiCommand::Stop);
            }
        }
    }

    pub(crate) fn remove_window_tracking(&self, window_id: &WindowId) {
        match self {
            Self::Direct(driver) => {
                super::tracking::remove_window_id_mapping(
                    &driver.borrow().root_id_for_legacy_tracking(),
                    window_id,
                );
            }
            Self::Threaded { .. } => {
                super::tracking::remove_platform_window_mapping(window_id);
            }
        }
    }

    pub(crate) fn remove_root_view(&self) {
        self.call(|ui| ui.remove_root_view());
    }

    pub(crate) fn clear_root_box_tree(&self) {
        self.call(|ui| ui.clear_root_box_tree());
    }
}
