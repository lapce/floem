#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
use std::{cell::RefCell, mem, rc::Rc, sync::Arc};

use crate::event::{
    CustomEvent, PaintPresentInfo, PaintPresentLayer, RouteKind, ScrollTo, UpdatePhaseEvent,
};
use crate::paint::{PaintState, renderer::SharedSceneFragmentRendererPool};
use crate::platform::menu_types::{Menu as MudaMenu, MenuId};
use crate::style::StyleSelector;
use crate::style::recalc::StyleReason;
#[cfg(target_os = "windows")]
use muda::MenuTheme as MudaMenuTheme;

use crate::platform::{Duration, Instant};
#[cfg(target_os = "macos")]
use subduction_backend_apple::{MetalCaptureScopeGuard, request_next_metal_capture};
use ui_events::keyboard::{Key, KeyboardEvent, Modifiers, NamedKey};
use ui_events::pointer::PointerEvent;
use ui_events_winit::WindowEventReducer;

use winit::window::{
    ImeCapabilities, ImeEnableRequest, ImeHint, ImePurpose, ImeRequest, ImeRequestData,
};

use crate::action::TimerToken;
use crate::frame_source::{FrameSource, new_window_frame_source};
use crate::gpu_resources::GpuResources;
use floem_reactive::{RwSignal, Scope, SignalGet, SignalUpdate};
use imaging::Brush;
use peniko::color::palette;
use peniko::kurbo::{self, Point, Size};
use understory_frame_pacing::{
    BeginFrameTiming, CompositorCommitReason, CompositorCommitResult, CompositorFrameAction,
    CompositorFrameScheduler, CompositorWorkStatus, DisplayTiming as PacingDisplayTiming,
    Duration as PacingDuration, FrameDemand as PacingFrameDemand, FrameOpportunity,
    FrameTimingEstimate, Time as PacingTime, plan_frame,
};
use winit::{
    cursor::CursorIcon,
    dpi::{LogicalPosition, LogicalSize, PhysicalSize},
    event::Ime,
    window::{Window, WindowId},
};

use super::state::WindowState;
use super::tracking::{remove_window_id_mapping, store_window_id_mapping};
use crate::app::MenuWrapper;
#[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
use crate::platform::context_menu::context_menu_view;
#[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
use crate::reactive::SignalWith;
#[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
use crate::unit::UnitExt;
use crate::view::{LayoutTree, VIEW_STORAGE};
#[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
use crate::views::{Container, Decorators, Stack};
use crate::{
    Application,
    app::UserEvent,
    context::{LayoutChanged, StyleCx, UpdateCx, VisualChanged},
    event::{
        Event, GlobalEventCx, ImeEvent, WindowEvent, clear_hit_test_cache,
        dropped_file::FileDragEvent,
    },
    frame::{FrameDemand, FrameTime},
    inspector::{
        self, Capture, CaptureState, CapturedView, TimingKind, TimingReport, TimingThread,
        profiler::Profile,
    },
    message::{
        CENTRAL_DEFERRED_UPDATE_MESSAGES, CENTRAL_UPDATE_MESSAGES, CURRENT_RUNNING_VIEW_HANDLE,
        DEFERRED_UPDATE_MESSAGES, UPDATE_MESSAGES, UpdateMessage,
    },
    style::{CursorStyle, Style},
    theme::default_theme,
    view::{IntoView, View, ViewId},
};

const COMPOSITOR_DEADLINE_FUDGE: Duration = Duration::from_millis(1);
const DEFAULT_FRAME_WORK_ESTIMATE: Duration = Duration::from_millis(2);
const DEFAULT_GPU_WORK_ESTIMATE: Duration = Duration::ZERO;
const DEFAULT_TIMER_WAKEUP_ESTIMATE: Duration = Duration::from_millis(1);
/// The top-level window handle that owns the winit `Window`.
/// Meant only for use with the root view of the application.
/// Owns the [`WindowState`] and is responsible for
/// - processing all requests to update the [`WindowState`] from the reactive system
/// - processing all requests to update the animation state from the reactive system
/// - requesting a new animation frame from the backend
pub(crate) struct WindowHandle {
    pub(crate) window: Arc<dyn winit::window::Window>,
    window_id: WindowId,
    /// The root view ID for this window.
    pub(crate) id: ViewId,
    /// Reactive Scope for this `WindowHandle`
    scope: Scope,
    pub(crate) window_state: WindowState,
    pub(crate) paint_state: PaintState,
    size: RwSignal<Size>,
    default_theme: Option<Style>,
    pub(crate) profile: Option<Profile>,
    is_maximized: bool,
    pub(crate) transparent: bool,
    pub(crate) modifiers: Modifiers,
    pub(crate) window_position: Point,
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
    pub(crate) context_menu: RwSignal<Option<(MudaMenu, Point, bool)>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) window_menu_actions: HashMap<MenuId, Box<dyn Fn()>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) window_menu: Option<MudaMenu>,
    pub(crate) event_reducer: WindowEventReducer,
    pub(crate) gpu_resources: Option<GpuResources>,
    pub(crate) scene_renderer_pool: SharedSceneFragmentRendererPool,
    pub(crate) maximum_drawable_count: u32,
    is_occluded: bool,
    pending_timing: FrameTimingAccumulator,
    next_frame_id: u64,
    last_timing_report: Option<TimingReport>,
    frame_source: FrameSource,
    frame_scheduler_time_origin: Instant,
    active_frame_time: Option<FrameTime>,
    pending_layer_host_commit: Option<PendingLayerHostCommit>,
    compositor_frame_scheduler: CompositorFrameScheduler,
    pending_compositor_commit: Option<PendingCompositorCommit>,
    next_compositor_commit_generation: u64,
    active_begin_frame_started_at: Option<Instant>,
    pending_scene_frame_work_started_at: Option<Instant>,
    pending_scene_frame_work_cpu_end_at: Option<Instant>,
    frame_work_estimate: Duration,
    gpu_work_estimate: Duration,
    timer_wakeup_estimate: Duration,
    hud: crate::hud::Hud,
    pending_presented_layers: Vec<crate::window::compositor::PresentedLayer>,
    pending_active_layers: Vec<crate::window::compositor::PresentedLayer>,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct FrameSchedule {
    /// Suppress high-frequency input delivery while the active compositor
    /// frame is waiting for scene readiness or its commit deadline.
    ///
    /// The app loop owns event coalescing, so the window returns the deadline
    /// instead of mutating app state.
    pub(crate) coalesce_input_until: Option<Instant>,
    pub(crate) compositor_commit_deadline: Option<CompositorCommitDeadlineSchedule>,
}

#[derive(Clone, Copy)]
pub(crate) struct CompositorCommitDeadlineSchedule {
    pub(crate) deadline: Instant,
    pub(crate) generation: u64,
    pub(crate) token: TimerToken,
}

#[derive(Clone, Copy)]
struct PendingCompositorCommit {
    deadline: Instant,
    generation: u64,
    token: TimerToken,
    submitted_at: Instant,
    scene_ready_at: Option<Instant>,
}

#[derive(Clone, Copy)]
struct PendingLayerHostCommit {
    submitted_at: Instant,
    frame_time: Option<FrameTime>,
}

fn compositor_commit_reason_label(reason: CompositorCommitReason) -> &'static str {
    match reason {
        CompositorCommitReason::SceneReady => "scene-ready",
        CompositorCommitReason::Deadline => "deadline",
        CompositorCommitReason::ReadyCarry => "ready-carry",
    }
}

fn surface_extent(size: Size, os_scale: f64) -> PhysicalSize<u32> {
    let physical = size * os_scale;
    PhysicalSize::new(
        physical.width.max(1.0).round() as u32,
        physical.height.max(1.0).round() as u32,
    )
}

fn smooth_duration_estimate(previous: Duration, observed: Duration) -> Duration {
    if observed >= previous {
        return observed;
    }
    let previous_ns = previous.as_nanos();
    let observed_ns = observed.as_nanos();
    Duration::from_nanos(((previous_ns * 7 + observed_ns) / 8).min(u64::MAX as u128) as u64)
}

#[derive(Clone, Debug, Default)]
struct FrameTimingAccumulator {
    anchor: Option<Instant>,
    spans: Vec<inspector::TimingSpan>,
}

impl FrameTimingAccumulator {
    fn absorb(&mut self, mut other: Self) {
        match (self.anchor, other.anchor) {
            (Some(self_anchor), Some(other_anchor)) if other_anchor < self_anchor => {
                let delta = self_anchor.saturating_duration_since(other_anchor);
                for span in &mut self.spans {
                    span.shift_by(delta);
                }
                self.anchor = Some(other_anchor);
            }
            (Some(self_anchor), Some(other_anchor)) if other_anchor > self_anchor => {
                let delta = other_anchor.saturating_duration_since(self_anchor);
                for span in &mut other.spans {
                    span.shift_by(delta);
                }
            }
            (None, Some(other_anchor)) => {
                self.anchor = Some(other_anchor);
            }
            _ => {}
        }
        self.spans.extend(other.spans);
    }

    fn push_absolute_span(
        &mut self,
        label: &'static str,
        start: Instant,
        end: Instant,
        kind: TimingKind,
    ) {
        self.push_absolute_span_on_thread(label, start, end, kind, TimingThread::Main);
    }

    fn push_absolute_span_on_thread(
        &mut self,
        label: &'static str,
        start: Instant,
        end: Instant,
        kind: TimingKind,
        thread: TimingThread,
    ) {
        if end <= start {
            return;
        }

        let anchor = match self.anchor {
            Some(anchor) if start < anchor => {
                let delta = anchor.saturating_duration_since(start);
                for span in &mut self.spans {
                    span.shift_by(delta);
                }
                self.anchor = Some(start);
                start
            }
            Some(anchor) => anchor,
            None => {
                self.anchor = Some(start);
                start
            }
        };

        self.spans.push(inspector::TimingSpan::new_on_thread(
            label,
            start.saturating_duration_since(anchor),
            end.saturating_duration_since(start),
            kind,
            thread,
        ));
    }

    fn has_kind(&self, kind: TimingKind) -> bool {
        self.spans.iter().any(|span| span.kind == kind)
    }

    fn max_duration_for_label(&self, label: &'static str) -> Duration {
        self.spans
            .iter()
            .filter(|span| span.label == label)
            .map(|span| span.duration)
            .max()
            .unwrap_or(Duration::ZERO)
    }

    fn max_duration_for_label_since(&self, label: &'static str, since: Instant) -> Duration {
        let Some(anchor) = self.anchor else {
            return Duration::ZERO;
        };
        let since = since.saturating_duration_since(anchor);
        self.spans
            .iter()
            .filter(|span| span.label == label && span.start >= since)
            .map(|span| span.duration)
            .max()
            .unwrap_or(Duration::ZERO)
    }

    fn active_frame_work_duration(&self) -> Duration {
        self.active_frame_work_duration_since_abs(None)
    }

    fn active_frame_work_duration_since(&self, since: Instant) -> Duration {
        self.active_frame_work_duration_since_abs(Some(since))
    }

    fn active_frame_work_duration_since_abs(&self, since: Option<Instant>) -> Duration {
        let since = since.and_then(|since| {
            self.anchor
                .map(|anchor| since.saturating_duration_since(anchor))
        });
        let mut intervals: Vec<(Duration, Duration)> = self
            .spans
            .iter()
            .filter(|span| {
                matches!(span.label, "PrepareFrame" | "Paint" | "Render")
                    || matches!(
                        span.kind,
                        TimingKind::Style | TimingKind::Layout | TimingKind::BoxTree
                    )
            })
            .filter(|span| since.is_none_or(|since| span.start >= since))
            .map(|span| (span.start, span.start.saturating_add(span.duration)))
            .filter(|(start, end)| end > start)
            .collect();

        if intervals.is_empty() {
            return Duration::ZERO;
        }

        intervals.sort_by_key(|(start, _)| *start);
        let mut total = Duration::ZERO;
        let (mut current_start, mut current_end) = intervals[0];
        for (start, end) in intervals.into_iter().skip(1) {
            if start <= current_end {
                current_end = current_end.max(end);
            } else {
                total += current_end.saturating_sub(current_start);
                current_start = start;
                current_end = end;
            }
        }
        total + current_end.saturating_sub(current_start)
    }

    fn sum_duration_for_kind(&self, kind: TimingKind) -> Duration {
        self.spans
            .iter()
            .filter(|span| span.kind == kind)
            .map(|span| span.duration)
            .sum()
    }

    fn build_timing_report(self) -> TimingReport {
        let Some(anchor) = self.anchor else {
            return TimingReport::default();
        };

        let mut timings = TimingReport::new(Some(anchor), Duration::ZERO);
        for span in self.spans {
            if span.duration > Duration::ZERO {
                timings.push_span_on_thread(
                    span.label,
                    span.start,
                    span.duration,
                    span.kind,
                    span.thread,
                );
            }
        }
        timings.total = timings.thread_total(TimingThread::Main);
        timings
    }

    fn cloned_timing_report(&self) -> Option<TimingReport> {
        if self.spans.is_empty() {
            return None;
        }
        Some(self.clone().build_timing_report())
    }
}

impl Drop for WindowHandle {
    fn drop(&mut self) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            s.box_tree.remove(&self.id);
        })
    }
}

impl WindowHandle {
    pub(crate) fn take_profile_events(&mut self) -> Vec<crate::inspector::profiler::ProfileEvent> {
        self.window_state
            .profile_events
            .drain(..)
            .map(|event| crate::inspector::profiler::ProfileEvent {
                start: event.start,
                end: event.end,
                name: event.name,
                depth: 0,
            })
            .collect()
    }

    pub(crate) fn pending_profile_timing(&self) -> Option<TimingReport> {
        self.pending_timing.cloned_timing_report()
    }

    pub(crate) fn record_profile_instant(&mut self, name: &'static str, at: Instant) {
        self.window_state.record_profile_instant(name, at);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        window: Box<dyn winit::window::Window>,
        output_id: u32,
        gpu_resources: Option<GpuResources>,
        renderer_chooser: crate::paint::renderer::RendererChooser,
        scene_renderer_pool: SharedSceneFragmentRendererPool,
        required_features: wgpu::Features,
        backends: Option<wgpu::Backends>,
        view_fn: impl FnOnce(winit::window::WindowId) -> Box<dyn View> + 'static,
        transparent: bool,
        apply_default_theme: bool,
        maximum_drawable_count: u32,
    ) -> Self {
        let window_view_id = ViewId::new_root();
        let window_winit_id = window.id();
        let scope = Scope::new();
        let os_scale = window.scale_factor();
        let size: LogicalSize<f64> = window.surface_size().to_logical(os_scale);
        let size = Size::new(size.width, size.height);
        let size = scope.create_rw_signal(Size::new(size.width, size.height));
        let os_theme = window.theme();
        // let current_theme = apply_theme.unwrap_or(os_theme.unwrap_or(winit::window::Theme::Light));
        let is_maximized = window.is_maximized();

        set_current_view(window_view_id);

        #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
        let context_menu = scope.create_rw_signal(None);

        #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32")))]
        let main_view = scope.enter(move || view_fn(window_winit_id));

        #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
        let view = scope.enter(move || {
            let main_view = view_fn(window_id);
            Stack::new((
                Container::new(main_view).style(|s| s.size(100.pct(), 100.pct())),
                context_menu_view(scope, context_menu, size),
            ))
            .style(|s| s.size(100.pct(), 100.pct()))
            .into_any()
        });

        let hud = crate::hud::Hud::new();
        let hud_view = scope.enter({
            let hud = hud.clone();
            move || hud.view().into_any()
        });

        window_view_id.set_children([main_view, hud_view]);
        // window_view_id.add_child(hud_view);

        let window_view = WindowView { id: window_view_id };
        window_view_id.set_view(window_view.into_any());

        let window: Arc<dyn Window> = window.into();
        store_window_id_mapping(window_view_id, window_winit_id, &window);
        let frame_size = size.get_untracked() * os_scale;
        let prefer_gpu_installers = !crate::paint::renderer::force_cpu_requested();

        let paint_state = if let Some(resources) = gpu_resources.clone() {
            Self::new_gpu_backed_paint_state(
                &renderer_chooser,
                &scene_renderer_pool,
                window.clone(),
                resources,
                transparent,
                os_scale,
                frame_size,
                maximum_drawable_count,
            )
        } else if prefer_gpu_installers {
            Self::new_pending_paint_state(window.clone(), frame_size, required_features, backends)
        } else {
            Self::new_cpu_backed_paint_state()
        };

        let paint_state_initialized = paint_state.is_initialized();

        let window_state = WindowState::new(window_view_id, os_theme, os_scale);

        let mut window_handle = Self {
            window,
            window_id: window_winit_id,
            id: window_view_id,
            scope,
            paint_state,
            size,
            default_theme: match apply_default_theme {
                true => Some(default_theme(
                    window_state.light_dark_theme,
                    window_state.effective_scale(),
                )),
                false => None,
            },
            window_state,
            is_maximized,
            transparent,
            profile: None,
            modifiers: Modifiers::default(),
            window_position: Point::ZERO,
            #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
            context_menu,
            #[cfg(not(target_arch = "wasm32"))]
            window_menu_actions: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            window_menu: None,
            event_reducer: WindowEventReducer::default(),
            gpu_resources,
            scene_renderer_pool,
            maximum_drawable_count,
            is_occluded: false,
            pending_timing: FrameTimingAccumulator::default(),
            next_frame_id: 1,
            last_timing_report: None,
            frame_source: new_window_frame_source(window_winit_id, output_id),
            frame_scheduler_time_origin: Instant::now(),
            active_frame_time: None,
            pending_layer_host_commit: None,
            compositor_frame_scheduler: CompositorFrameScheduler::new(),
            pending_compositor_commit: None,
            next_compositor_commit_generation: 1,
            active_begin_frame_started_at: None,
            pending_scene_frame_work_started_at: None,
            pending_scene_frame_work_cpu_end_at: None,
            frame_work_estimate: DEFAULT_FRAME_WORK_ESTIMATE,
            gpu_work_estimate: DEFAULT_GPU_WORK_ESTIMATE,
            timer_wakeup_estimate: DEFAULT_TIMER_WAKEUP_ESTIMATE,
            hud,
            pending_presented_layers: Vec::new(),
            pending_active_layers: Vec::new(),
        };
        if paint_state_initialized {
            window_handle.init_renderer();
            if let Some(gpu_resources) = window_handle.gpu_resources.clone() {
                window_handle.event(Event::Window(WindowEvent::GpuResourcesReady(gpu_resources)));
            }
        }

        window_handle
            .window_state
            .compositor
            .ensure_platform_presenter(window_winit_id, window_handle.window.as_ref());
        window_handle.refresh_frame_source_target();
        window_handle
            .window_state
            .set_root_size(size.get_untracked());
        window_handle
            .window_state
            .update_screen_size_bp(size.get_untracked());
        window_handle.process_update_no_paint();

        window_handle.window_state.light_dark_theme =
            os_theme.unwrap_or(winit::window::Theme::Light);

        window_handle.event(Event::Window(WindowEvent::ThemeChanged(
            window_handle.window_state.light_dark_theme,
        )));
        window_handle
            .window_state
            .mark_style_dirty_selector(window_handle.id.get_element_id(), StyleSelector::DarkMode);
        window_handle.size(size.get_untracked());
        window_handle
    }

    fn new_gpu_backed_paint_state(
        renderer_chooser: &crate::paint::renderer::RendererChooser,
        scene_renderer_pool: &SharedSceneFragmentRendererPool,
        window: Arc<dyn Window>,
        gpu_resources: GpuResources,
        transparent: bool,
        os_scale: f64,
        size: Size,
        maximum_drawable_count: u32,
    ) -> PaintState {
        let surface_caps = subduction::wgpu::ExternalSurfaceCapabilities {
            formats: vec![
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureFormat::Bgra8Unorm,
            ],
            color_spaces: vec![subduction::wgpu::SurfaceColorSpace::Srgb],
            dynamic_ranges: vec![subduction::wgpu::SurfaceDynamicRange::Standard],
            alpha_modes: vec![
                wgpu::CompositeAlphaMode::PreMultiplied,
                wgpu::CompositeAlphaMode::Opaque,
            ],
            usages: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            max_size: None,
            supports_frame_opportunities: true,
            supports_render_targets: true,
            supports_submitted_textures: true,
        };
        let cx = crate::paint::renderer::NewRendererCx {
            window,
            gpu_resources: Some(gpu_resources),
            surface_caps: Some(surface_caps),
            transparent,
            size,
            scale: os_scale,
            maximum_drawable_count,
        };
        scene_renderer_pool.init_if_needed(renderer_chooser, cx);
        PaintState::Initialized
    }

    fn new_cpu_backed_paint_state() -> PaintState {
        // CPU fallback will be reintroduced as a separate path after the GPU
        // compositor renderer pool is the only renderer path.
        PaintState::Headless
    }

    fn new_pending_paint_state(
        window: Arc<dyn Window>,
        size: Size,
        required_features: wgpu::Features,
        backends: Option<wgpu::Backends>,
    ) -> PaintState {
        let gpu_resources_rx = GpuResources::request(
            move |window_id| {
                Application::send_proxy_event(UserEvent::GpuResourcesUpdate { window_id });
            },
            required_features,
            backends,
            window.clone(),
        );
        PaintState::new_pending(window, gpu_resources_rx, size)
    }

    /// Creates a headless WindowHandle for testing purposes.
    ///
    /// This constructor creates a WindowHandle with a MockWindow and no GPU resources,
    /// suitable for testing the event handling and view update logic without a real window.
    ///
    /// # Arguments
    /// * `root_id` - The root ViewId (from TestRoot)
    /// * `view` - The root view for this window
    /// * `size` - The virtual window size
    /// * `scale` - The window scale factor (default 1.0)
    pub(crate) fn new_headless(
        root_id: ViewId,
        view: impl IntoView,
        size_val: Size,
        os_scale: f64,
    ) -> Self {
        use super::mock::MockWindow;

        let scope = Scope::new();
        let mock_window = MockWindow::with_size(size_val.width as u32, size_val.height as u32);
        let window_id = mock_window.id();
        let id = root_id;
        let size = scope.create_rw_signal(size_val);
        let os_theme = mock_window.theme();
        let is_maximized = mock_window.is_maximized();

        // Root is already set by TestRoot, but set it again to be safe
        set_current_view(id);

        // Convert the view
        let main_view = view.into_view();
        let widget: Box<dyn View> = main_view.into_any();

        id.set_children([widget]);

        let window_view = WindowView { id };
        id.set_view(window_view.into_any());

        let window: Arc<dyn Window> = Arc::new(mock_window);
        store_window_id_mapping(id, window_id, &window);

        // Headless windows are used for tests and benchmarks where we want to exercise Floem's
        // paint traversal and retained display-list building without touching any real rendering
        // backend. Keep a no-op rasterizer here even when CPU/GPU renderer features are enabled.
        let paint_state = PaintState::Headless;

        let window_state = WindowState::new(id, os_theme, os_scale);

        let mut window_handle = Self {
            window,
            window_id,
            id,
            scope,
            paint_state,
            size,
            default_theme: Some(default_theme(
                window_state.light_dark_theme,
                window_state.effective_scale(),
            )),
            window_state,
            is_maximized,
            transparent: false,
            profile: None,
            modifiers: Modifiers::default(),
            window_position: Point::ZERO,
            #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
            context_menu: scope.create_rw_signal(None),
            #[cfg(not(target_arch = "wasm32"))]
            window_menu_actions: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            window_menu: None,
            event_reducer: WindowEventReducer::default(),
            gpu_resources: None,
            scene_renderer_pool: SharedSceneFragmentRendererPool::default(),
            maximum_drawable_count: 2,
            is_occluded: false,
            pending_timing: FrameTimingAccumulator::default(),
            next_frame_id: 1,
            last_timing_report: None,
            frame_source: new_window_frame_source(window_id, 0),
            frame_scheduler_time_origin: Instant::now(),
            active_frame_time: None,
            pending_layer_host_commit: None,
            compositor_frame_scheduler: CompositorFrameScheduler::new(),
            pending_compositor_commit: None,
            next_compositor_commit_generation: 1,
            active_begin_frame_started_at: None,
            pending_scene_frame_work_started_at: None,
            pending_scene_frame_work_cpu_end_at: None,
            frame_work_estimate: DEFAULT_FRAME_WORK_ESTIMATE,
            gpu_work_estimate: DEFAULT_GPU_WORK_ESTIMATE,
            timer_wakeup_estimate: DEFAULT_TIMER_WAKEUP_ESTIMATE,
            hud: crate::hud::Hud::new(),
            pending_presented_layers: Vec::new(),
            pending_active_layers: Vec::new(),
        };

        window_handle
            .window_state
            .set_root_size(size.get_untracked());

        window_handle.window_state.light_dark_theme =
            os_theme.unwrap_or(winit::window::Theme::Light);

        // Run initial style and layout passes
        window_handle.process_update_messages();
        // Mark root view as needing style so initial style pass runs compute_combined
        // and populates has_style_selectors for selector detection
        window_handle.id.request_style(StyleReason::full_recalc());
        window_handle.process_update_messages();
        let mut initial_timing = FrameTimingAccumulator::default();
        window_handle.style(&mut initial_timing);
        window_handle.layout(&mut initial_timing);
        window_handle.commit_box_tree(&mut initial_timing);

        window_handle
    }

    pub(crate) fn init_renderer(&mut self) {
        // On the web, we need to get the canvas size once. The size will be updated automatically
        // when the canvas element is resized subsequently. This is the correct place to do so
        // because the renderer is not initialized until now.
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWeb;

            let rect = self.window.canvas().unwrap().get_bounding_client_rect();
            // let rect = canvas.get_bounding_client_rect();
            let size = LogicalSize::new(rect.width(), rect.height());
            self.size(Size::new(size.width, size.height));
        }
        // Startup should enter through the same explicit frame pipeline as all
        // later work. Eagerly painting here can leave a frame underway before
        // the first scheduled app update, which suppresses the initial tick-
        // driven frame turn and leaves the window blank until another event
        // arrives.
        self.window.set_visible(true);
        self.window_state
            .request_paint(self.window_state.root_view_id);
        self.refresh_frame_activity();
        Application::request_update();
    }

    pub fn event(&mut self, event: Event) {
        set_current_view(self.id.root());

        // Check event type for platform-specific context menu handling
        #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
        let is_pointer_down = matches!(&event, Event::Pointer(PointerEvent::Down { .. }));
        #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
        let is_pointer_up = matches!(&event, Event::Pointer(PointerEvent::Up { .. }));

        let root_element_id = self.window_state.root_view_id.get_element_id();
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();

        // Platform-specific context menu handling
        #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
        {
            if is_pointer_down
                && self.context_menu.with_untracked(|c| {
                    c.as_ref()
                        .map(|(_, _, had_pointer_down)| !*had_pointer_down)
                        .unwrap_or(false)
                })
            {
                // we had a pointer down event
                // if context menu is still shown
                // we should hide it
                self.context_menu.set(None);
            }
            if is_pointer_up && self.context_menu.with_untracked(|c| c.is_some()) {
                // we had a pointer up event
                // if context menu is still shown
                // we should hide it
                self.context_menu.set(None);
            }
        }
    }

    pub(crate) fn window_id(&self) -> WindowId {
        self.window_id
    }

    pub(crate) fn os_scale(&mut self, os_scale: f64) {
        self.window_state.os_scale = os_scale;
        self.window_state
            .update_default_theme(self.window_state.light_dark_theme);
        self.window_state
            .mark_style_dirty(self.window_state.root_view_id.get_element_id());
        let scale = self.window_state.effective_scale();
        self.event(Event::Window(WindowEvent::ScaleChanged(scale)));
        self.window_state
            .request_paint(self.window_state.root_view_id);
        self.schedule_repaint();
    }

    pub(crate) fn set_theme(&mut self, theme: Option<winit::window::Theme>, change_from_os: bool) {
        if change_from_os && self.window_state.theme_overriden {
            // if the window theme has been set manually then changes from the os shouldn't do anything
            return;
        }
        self.window_state.mark_style_dirty_selector(
            self.window_state.root_view_id.get_element_id(),
            StyleSelector::DarkMode,
        );
        if let Some(theme) = theme {
            // Only override the theme with the default if the user did not provide one
            if self.default_theme.is_some() {
                self.default_theme =
                    Some(default_theme(theme, self.window_state.effective_scale()));
            }
            // Update the default theme in WindowState for style computation
            self.window_state.update_default_theme(theme);
            self.window_state.light_dark_theme = theme;
            if !change_from_os {
                self.window_state.theme_overriden = true
            }
            #[cfg(target_os = "windows")]
            {
                self.set_menu_theme_for_windows(theme);
            }
            // Mark dark mode changed if theme actually changed
            // if theme != old_theme {}
        } else {
            self.window_state.theme_overriden = false;
        }
        if !change_from_os {
            self.window.set_theme(theme);
        }
        self.id.request_style(StyleReason::inherited());
        if let Some(theme) = theme {
            self.event(Event::Window(WindowEvent::ThemeChanged(theme)));
        }
    }

    pub(crate) fn size(&mut self, size: Size) {
        self.size.set(size);
        self.record_frame_demand(FrameDemand::CONTINUOUS_INPUT);
        self.preempt_active_frame_for_resize();

        // Update root size first so any style work triggered by resize observes
        // the new width instead of the previous frame's value.
        self.window_state.set_root_size(size);

        self.window_state.update_screen_size_bp(size);
        self.event(Event::Window(WindowEvent::Resized(size)));

        let is_maximized = self.window.is_maximized();
        if is_maximized != self.is_maximized {
            self.is_maximized = is_maximized;
            self.event(Event::Window(WindowEvent::MaximizeChanged(is_maximized)));
        }

        self.resize_present_surface_to_window();
        self.window_state
            .request_paint(self.window_state.root_view_id);
        self.schedule_repaint();
    }

    fn preempt_active_frame_for_resize(&mut self) {
        if !self.compositor_frame_scheduler.has_active_frame()
            && self.pending_compositor_commit.is_none()
            && !self.window_state.compositor.has_pending_scene_renders()
            && !self.window_state.compositor.has_pending_commit_work()
        {
            return;
        }

        if let Some(pending) = self.pending_compositor_commit.take() {
            pending.token.cancel();
        }
        let discarded = self
            .window_state
            .compositor
            .discard_pending_scene_frame_work("resize");
        self.pending_scene_frame_work_started_at = None;
        self.pending_scene_frame_work_cpu_end_at = None;
        self.pending_timing = FrameTimingAccumulator::default();
        self.active_frame_time = None;
        self.note_begin_frame_finished();
        if discarded {
            self.record_frame_demand(FrameDemand::CONTINUOUS_INPUT);
        }
    }

    fn resize_present_surface_to_window(&mut self) {
        if !self.paint_state.is_initialized() {
            return;
        }

        self.refresh_frame_source_target();
    }

    pub(crate) fn position(&mut self, point: Point) {
        self.window_position = point;
        self.event(Event::Window(WindowEvent::Moved(point)));
    }

    pub(crate) fn file_drag_dropped(&mut self, file_drag_event: FileDragEvent) {
        // Store paths in window state for tracking during drag
        self.window_state.file_drag_paths = None;
        self.event(Event::FileDrag(file_drag_event));
    }

    pub(crate) fn file_drag_start(&mut self, paths: Vec<std::path::PathBuf>, position: Point) {
        // Store paths and dispatch as a move event to trigger hit testing
        let paths_rc: Rc<[std::path::PathBuf]> = paths.into();
        self.window_state.file_drag_paths = Some(paths_rc.clone());
        self.event(Event::FileDrag(FileDragEvent::Move(
            crate::event::dropped_file::FileDragMove {
                paths: paths_rc,
                position,
            },
        )));
    }

    pub(crate) fn file_drag_move(&mut self, position: Point) {
        if let Some(paths) = &self.window_state.file_drag_paths {
            self.event(Event::FileDrag(FileDragEvent::Move(
                crate::event::dropped_file::FileDragMove {
                    paths: paths.clone(),
                    position,
                },
            )));
        }
    }

    pub(crate) fn file_drag_end(&mut self) {
        // Clear paths and file hover state
        self.window_state.file_drag_paths = None;
        set_current_view(self.id.root());
        let root_element_id = self.window_state.root_view_id.get_element_id();
        GlobalEventCx::new(&mut self.window_state, root_element_id, Event::Extracted)
            .update_hover_from_path(&[]);
        self.process_update_messages_only();
        self.refresh_frame_activity();
    }

    pub(crate) fn key_event(&mut self, key_event: KeyboardEvent) {
        let is_altgr = key_event.key == Key::Named(NamedKey::AltGraph);
        let toggle_hud = !key_event.state.is_down() && key_event.key == Key::Named(NamedKey::F10);
        if key_event.state.is_down() {
            if is_altgr {
                self.modifiers.set(Modifiers::ALT_GRAPH, true);
            }
        } else if is_altgr {
            self.modifiers.set(Modifiers::ALT_GRAPH, false);
        }
        self.event(Event::Key(key_event));
        if toggle_hud {
            self.toggle_hud();
        }
    }

    fn toggle_hud(&mut self) {
        self.hud.toggle();
        self.window_state.request_paint(self.id);
        self.refresh_frame_activity();
    }

    pub(crate) fn pointer_event(&mut self, pointer_event: PointerEvent) {
        self.event(Event::Pointer(pointer_event));
    }

    pub(crate) fn focused(&mut self, focused: bool) {
        if focused {
            #[cfg(target_os = "macos")]
            if let Some(window_menu) = &self.window_menu {
                window_menu.init_for_nsapp();
            }
            self.event(Event::Window(WindowEvent::FocusGained));
        } else {
            self.event(Event::Window(WindowEvent::FocusLost));
        }
    }

    fn style(&mut self, timing: &mut FrameTimingAccumulator) {
        let start = Instant::now();
        // Transitions need a monotonic style sample time. During a backend frame
        // tick, use the tick's semantic frame time so animation work is
        // frame-consistent. Outside a frame tick, do not ask the frame clock for
        // `current_frame_time()`: it may return the last planned presentation
        // timestamp, which can stay unchanged across event/update-only style
        // passes and make transitions produce no intermediate deltas.
        let style_now = self
            .active_frame_time
            .map(|frame_time| frame_time.now)
            .unwrap_or_else(Instant::now);
        // Loop until no more views need styling
        // This handles the case where styling a parent marks children dirty
        // (e.g., when inherited properties change)
        loop {
            // Build explicit traversal order
            let traversal = self.window_state.build_style_traversal(self.id);
            if traversal.is_empty() {
                break;
            }

            // Style each view in order, passing the global change for first iteration
            for (view_id, traversal_reason) in traversal {
                let cx = &mut StyleCx::new_at(
                    &mut self.window_state,
                    view_id,
                    traversal_reason,
                    style_now,
                );
                cx.style_view();
            }
        }

        // Clear pending child changes after style pass completes
        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Style));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();
        let end = Instant::now();
        timing.push_absolute_span("Style", start, end, TimingKind::Style);
    }

    fn layout(&mut self, timing: &mut FrameTimingAccumulator) {
        let start = Instant::now();
        self.window_state.compute_layout();
        let taffy_end = Instant::now();
        // Update box tree from layout after layout completes
        let box_tree_start = taffy_end;
        self.window_state.update_box_tree_from_layout();
        let box_tree_end = Instant::now();

        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Layout));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();
        timing.push_absolute_span("Layout", start, box_tree_end, TimingKind::Layout);
        timing.push_absolute_span("Taffy", start, taffy_end, TimingKind::Layout);
        timing.push_absolute_span(
            "BoxTreeUpdate",
            box_tree_start,
            box_tree_end,
            TimingKind::BoxTree,
        );
    }

    fn update_box_tree_from_layout(&mut self, timing: &mut FrameTimingAccumulator) {
        let start = Instant::now();
        self.window_state.update_box_tree_from_layout();
        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::BoxTreeUpdate));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();
        let end = Instant::now();
        timing.push_absolute_span("BoxTreeUpdate", start, end, TimingKind::BoxTree);
    }

    fn process_pending_box_tree_updates(&mut self, timing: &mut FrameTimingAccumulator) {
        let start = Instant::now();
        self.window_state.process_pending_box_tree_updates();
        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(
            UpdatePhaseEvent::BoxTreePendingUpdates,
        ));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();
        let end = Instant::now();
        timing.push_absolute_span("BoxTreePendingUpdates", start, end, TimingKind::BoxTree);
    }

    fn commit_box_tree(&mut self, timing: &mut FrameTimingAccumulator) {
        let start = Instant::now();
        self.window_state.commit_box_tree();
        self.window_state.needs_box_tree_commit = false;

        let has_layout_listener: smallvec::SmallVec<[ViewId; 64]> = self
            .window_state
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
                    use crate::event::RouteKind;
                    GlobalEventCx::new(
                        &mut self.window_state,
                        element_id,
                        Event::new_custom(new_layout),
                    )
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
            .window_state
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
                use crate::event::RouteKind;
                GlobalEventCx::new(
                    &mut self.window_state,
                    element_id,
                    Event::new_custom(new_visual),
                )
                .route_normal(
                    RouteKind::Directed {
                        target: element_id,
                        phases: Phases::TARGET,
                    },
                    None,
                );
            }
        }

        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::BoxTreeCommit));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();

        let end = Instant::now();
        timing.push_absolute_span("BoxTreeCommit", start, end, TimingKind::BoxTree);
    }

    /// Promotes frame-clock work into the current update turn.
    ///
    /// Promotes queued next-frame work inside an active frame tick.
    ///
    /// This lifts "next frame" work into the current state, runs begin-frame
    /// callbacks using the tick's `FrameTime`, and processes updates those
    /// callbacks produce. It deliberately does not paint or commit.
    pub(crate) fn prepare_frame(&mut self) -> bool {
        let has_current_prepare_work = self.has_current_frame_prepare_work();
        if !has_current_prepare_work && !self.window_state.has_next_frame_work() {
            return false;
        }

        let prepare_start = Instant::now();
        if self.active_frame_time.is_none() {
            self.active_frame_time = Some(self.frame_time_at(prepare_start));
        }
        self.window_state.promote_next_frame_work();
        self.run_begin_frame_callbacks();
        self.process_update_no_paint();
        self.pending_timing.push_absolute_span(
            "PrepareFrame",
            prepare_start,
            Instant::now(),
            TimingKind::Total,
        );
        true
    }

    fn prepare_current_composition_plan(&mut self) -> bool {
        if !self.paint_state.is_initialized() {
            return false;
        }

        let mut timing = std::mem::take(&mut self.pending_timing);
        if !self.prepare_display_list_for_render(&mut timing) {
            self.pending_timing = timing;
            return false;
        }
        self.pending_timing = timing;
        true
    }

    fn pull_compositor_surface_frame(&mut self) -> bool {
        let effective_scale = self.window_state.effective_scale();
        let main_frame_time = self.current_frame_time();
        self.active_frame_time = Some(main_frame_time);
        let update = self.window_state.compositor_surfaces.pull_frame(
            |_| main_frame_time,
            &self.window_state.composition_plan,
            &mut self.window_state.compositor,
            effective_scale,
            self.gpu_resources.as_ref(),
        );
        if update.content_changed {
            for surface_id in &update.changed_surfaces {
                self.window_state
                    .compositor
                    .invalidate_compositor_surface_content(*surface_id);
            }
            self.record_frame_demand(FrameDemand::COMPOSITOR_SURFACE);
        }
        if crate::frame_source::frame_pacing_diag_enabled() {
            eprintln!(
                "floem frame pacing external update window={:?} content_changed={} changed_surfaces={} request_next_frame={}",
                self.window_id,
                update.content_changed,
                update.changed_surfaces.len(),
                update.request_next_frame,
            );
        }
        true
    }

    fn pull_compositor_surfaces_for_window_present(&mut self) {
        if !self.window_state.composition_plan.has_compositor_surfaces()
            || !self.window_state.compositor_surfaces.has_frame_pull()
        {
            return;
        }

        let effective_scale = self.window_state.effective_scale();
        let frame_time = self.current_frame_time();
        let update = self.window_state.compositor_surfaces.pull_frame(
            |_| frame_time,
            &self.window_state.composition_plan,
            &mut self.window_state.compositor,
            effective_scale,
            self.gpu_resources.as_ref(),
        );
        if update.content_changed {
            for surface_id in &update.changed_surfaces {
                self.window_state
                    .compositor
                    .invalidate_compositor_surface_content(*surface_id);
            }
            self.record_frame_demand(FrameDemand::COMPOSITOR_SURFACE);
        }
        if crate::frame_source::frame_pacing_diag_enabled() {
            eprintln!(
                "floem frame pacing external present-update window={:?} content_changed={} changed_surfaces={} request_next_frame={}",
                self.window_id,
                update.content_changed,
                update.changed_surfaces.len(),
                update.request_next_frame,
            );
        }
    }

    pub(crate) fn render_compositor_scene_layers(&mut self, reason: &'static str) -> Option<usize> {
        if !self.paint_state.is_initialized() {
            return None;
        }
        let Some(renderer_pool) = self.scene_renderer_pool.get() else {
            return None;
        };
        let start = Instant::now();
        let frame_index = self.current_frame_time().frame_index;
        let Some(gpu_resources) = self.gpu_resources.as_ref() else {
            return None;
        };
        let plan = self.window_state.composition_plan.clone();
        if crate::frame_source::frame_pacing_diag_enabled() {
            eprintln!(
                "floem window render_compositor_scene_layers reason={} window={:?} frame={} pending_paint={} pending_render={} root_size={:?} plan_items={}",
                reason,
                self.window_id,
                frame_index,
                self.window_state.has_pending_paint(),
                self.window_state.has_pending_render(),
                self.window_state.root_size,
                plan.items.len(),
            );
        }
        let scheduled_scene_frames = self.window_state.compositor.render_scene_layers(
            self.window_id,
            &plan,
            self.window_state.compositor_surfaces.entries(),
            gpu_resources,
            &renderer_pool,
            self.window_state.effective_scale(),
        );
        let end = Instant::now();
        self.pending_timing.push_absolute_span(
            "CompositorRenderSceneLayers",
            start,
            end,
            TimingKind::Renderer,
        );
        Some(scheduled_scene_frames)
    }

    pub(crate) fn complete_compositor_scene_render(
        &mut self,
        key: crate::paint::composition::CompositionKey,
        signature: crate::window::compositor::SceneRenderSignature,
        rendered: bool,
        worker_index: usize,
        render_start: Instant,
        render_end: Instant,
        gpu_end: Instant,
    ) -> bool {
        let Some(gpu_resources) = self.gpu_resources.as_ref() else {
            return false;
        };
        let start = Instant::now();
        let completed = self.window_state.compositor.complete_scene_render(
            key,
            signature,
            rendered,
            gpu_resources,
        );
        if completed {
            self.pending_timing.push_absolute_span_on_thread(
                "Render",
                render_start,
                render_end,
                TimingKind::Renderer,
                TimingThread::Renderer(worker_index),
            );
            if gpu_end > render_end {
                self.pending_timing.push_absolute_span_on_thread(
                    "RenderGpuWait",
                    render_end,
                    gpu_end,
                    TimingKind::Renderer,
                    TimingThread::Renderer(worker_index),
                );
                self.gpu_work_estimate =
                    smooth_duration_estimate(self.gpu_work_estimate, gpu_end - render_end);
            }
            self.pending_timing.push_absolute_span(
                "CompositorSceneFragmentComplete",
                start,
                Instant::now(),
                TimingKind::Renderer,
            );
            self.pending_scene_frame_work_cpu_end_at = Some(
                self.pending_scene_frame_work_cpu_end_at
                    .map(|end| end.max(render_end))
                    .unwrap_or(render_end),
            );
            if self.window_state.compositor.has_pending_scene_renders() {
                return true;
            }
            let now = Instant::now().max(gpu_end);
            if let (Some(started_at), Some(cpu_end)) = (
                self.pending_scene_frame_work_started_at.take(),
                self.pending_scene_frame_work_cpu_end_at.take(),
            ) {
                let observed_frame_work = cpu_end.saturating_duration_since(started_at);
                self.frame_work_estimate =
                    smooth_duration_estimate(self.frame_work_estimate, observed_frame_work)
                        .max(Duration::from_micros(500));
            }
            let scene_ready_generation = self
                .pending_compositor_commit
                .map(|pending| pending.generation);
            let scene_ready_deadline = self
                .pending_compositor_commit
                .map(|pending| pending.deadline);
            if let Some(pending) = self.pending_compositor_commit.as_mut()
                && pending.scene_ready_at.is_none()
            {
                pending.scene_ready_at = Some(now);
                let ready_delta_ms = if now >= pending.deadline {
                    now.saturating_duration_since(pending.deadline)
                        .as_secs_f64()
                        * 1000.0
                } else {
                    -(pending
                        .deadline
                        .saturating_duration_since(now)
                        .as_secs_f64()
                        * 1000.0)
                };
                if crate::frame_source::frame_pacing_diag_enabled() {
                    eprintln!(
                        "floem compositor scene-ready timing window={:?} generation={} ready_vs_deadline={:.3}ms submitted_age={:.3}ms deadline={:?} ready_at={:?} pending_commit_work={}",
                        self.window_id,
                        pending.generation,
                        ready_delta_ms,
                        now.saturating_duration_since(pending.submitted_at)
                            .as_secs_f64()
                            * 1000.0,
                        pending.deadline,
                        now,
                        self.window_state.compositor.has_pending_commit_work(),
                    );
                }
            }
            let status = self.compositor_work_status();
            let action = self
                .compositor_frame_scheduler
                .on_scene_ready(self.pacing_time(now), status);
            if crate::frame_source::frame_pacing_diag_enabled() {
                eprintln!(
                    "floem compositor scene-ready decision window={:?} generation={:?} action={:?} pending_scene={} pending_commit_work={} now_vs_deadline={:?}ms",
                    self.window_id,
                    scene_ready_generation,
                    action,
                    status.scene_jobs_pending,
                    status.commit_ready,
                    scene_ready_deadline.map(|deadline| {
                        if now >= deadline {
                            now.saturating_duration_since(deadline).as_secs_f64() * 1000.0
                        } else {
                            -(deadline.saturating_duration_since(now).as_secs_f64() * 1000.0)
                        }
                    }),
                );
            }
            match action {
                CompositorFrameAction::Commit { reason, .. } => {
                    let pending_token = self.pending_compositor_commit.map(|pending| pending.token);
                    let reason = compositor_commit_reason_label(reason);
                    let result = self.attempt_compositor_commit(reason);
                    if crate::frame_source::frame_pacing_diag_enabled() {
                        eprintln!(
                            "floem compositor scene-ready commit-result window={:?} generation={:?} reason={} result={:?} pending_scene_after={} pending_commit_work_after={}",
                            self.window_id,
                            scene_ready_generation,
                            reason,
                            result,
                            self.window_state.compositor.has_pending_scene_renders(),
                            self.window_state.compositor.has_pending_commit_work(),
                        );
                    }
                    let committed = result == CompositorCommitResult::Committed;
                    self.apply_compositor_commit_result(result);
                    if committed {
                        if let Some(token) = pending_token {
                            token.cancel();
                        }
                    } else {
                        self.record_frame_demand(FrameDemand::ANIMATION);
                    }
                }
                _ => {
                    self.record_frame_demand(FrameDemand::ANIMATION);
                }
            }
        }
        completed
    }

    fn commit_compositor_frame(&mut self, reason: &'static str) -> Option<usize> {
        let start = Instant::now();
        let scheduled_scene_frames = self.render_compositor_scene_layers(reason)?;
        let deadline = self
            .active_compositor_deadline_instant()
            .unwrap_or_else(|| self.current_frame_time().interval.deadline_max);
        let status = self.compositor_work_status();
        let action = self
            .compositor_frame_scheduler
            .on_frame_submitted(self.pacing_time(deadline), status);
        if crate::frame_source::frame_pacing_diag_enabled() {
            eprintln!(
                "floem compositor frame-submitted window={:?} reason={} scheduled_scene_frames={} action={:?} pending_scene={} pending_commit_work={} deadline_from_now={:.3}ms",
                self.window_id,
                reason,
                scheduled_scene_frames,
                action,
                status.scene_jobs_pending,
                status.commit_ready,
                deadline.saturating_duration_since(start).as_secs_f64() * 1000.0,
            );
        }
        match action {
            CompositorFrameAction::ArmDeadline(_) => {
                self.arm_compositor_commit_deadline_at(start, deadline);
            }
            CompositorFrameAction::Commit { reason, .. } => {
                let _ =
                    self.commit_requested_compositor_frame(compositor_commit_reason_label(reason));
            }
            _ => {
                if !self.window_state.compositor.has_pending_scene_renders() {
                    let _ = self.commit_requested_compositor_frame(reason);
                }
            }
        }
        Some(scheduled_scene_frames)
    }

    fn commit_requested_compositor_frame(&mut self, reason: &'static str) -> bool {
        let result = self.attempt_compositor_commit(reason);
        let committed = result == CompositorCommitResult::Committed;
        self.apply_compositor_commit_result(result);
        committed
    }

    fn attempt_compositor_commit(&mut self, reason: &'static str) -> CompositorCommitResult {
        self.attempt_compositor_commit_inner(reason, false)
    }

    fn attempt_independent_compositor_surface_commit(
        &mut self,
        reason: &'static str,
    ) -> CompositorCommitResult {
        self.attempt_compositor_commit_inner(reason, true)
    }

    fn attempt_compositor_commit_inner(
        &mut self,
        reason: &'static str,
        independent_compositor_surface_only: bool,
    ) -> CompositorCommitResult {
        let Some(gpu_resources) = self.gpu_resources.as_ref() else {
            return CompositorCommitResult::NoWork;
        };
        let start = Instant::now();
        let submitted_at = self
            .pending_compositor_commit
            .map(|pending| pending.submitted_at)
            .unwrap_or(start);
        let commit = if independent_compositor_surface_only {
            self.window_state
                .compositor
                .commit_independent_compositor_surface_work(&gpu_resources.queue)
        } else {
            self.window_state
                .compositor
                .commit_ready_layer_tree(&gpu_resources.queue)
        };
        let scene_pending_at_commit = self.window_state.compositor.has_pending_scene_renders();
        let Some(commit) = commit else {
            if crate::frame_source::frame_pacing_diag_enabled() {
                eprintln!(
                    "floem compositor commit no-op reason={} window={:?} pending_scene={} pending_commit_work={} pending_deadline={}",
                    reason,
                    self.window_id,
                    scene_pending_at_commit,
                    self.window_state.compositor.has_pending_commit_work(),
                    self.pending_compositor_commit.is_some(),
                );
            }
            if !scene_pending_at_commit && !self.window_state.compositor.has_pending_commit_work() {
                self.pending_compositor_commit = None;
            }
            return CompositorCommitResult::NoWork;
        };
        self.pending_presented_layers = commit.layers;
        self.pending_active_layers = commit.active_layers;
        self.pending_compositor_commit = None;
        if self.waits_for_layer_host_commit_feedback() {
            self.arm_layer_host_commit_feedback(submitted_at);
        } else {
            let frame_time = self.active_frame_time;
            self.route_paint_present_event(submitted_at, frame_time);
            self.finish_presented_frame(Some(submitted_at), submitted_at, frame_time);
        }
        let commit_label = if scene_pending_at_commit {
            "CompositorCarryCommit"
        } else {
            "CompositorCommit"
        };
        self.record_profile_instant(commit_label, start);
        self.pending_timing.push_absolute_span(
            commit_label,
            start,
            Instant::now(),
            TimingKind::Present,
        );
        if crate::frame_source::frame_pacing_diag_enabled() {
            eprintln!(
                "floem compositor commit ok reason={} window={:?} submitted_age={:.3}ms pending_scene={} pending_commit_work={}",
                reason,
                self.window_id,
                start.saturating_duration_since(submitted_at).as_secs_f64() * 1000.0,
                scene_pending_at_commit,
                self.window_state.compositor.has_pending_commit_work(),
            );
        }
        CompositorCommitResult::Committed
    }

    fn apply_compositor_commit_result(&mut self, result: CompositorCommitResult) {
        match self.compositor_frame_scheduler.on_commit_attempt(result) {
            CompositorFrameAction::FinishFrame {
                pending: Some(pending),
            } => {
                self.active_begin_frame_started_at = None;
                let demand = Self::frame_demand_from_pacing(pending.demand);
                if !demand.is_empty() {
                    self.compositor_frame_scheduler
                        .request_frame(Self::pacing_frame_demand(demand));
                }
            }
            CompositorFrameAction::FinishFrame { pending: None } => {
                self.active_begin_frame_started_at = None;
            }
            CompositorFrameAction::Idle => {}
            _ => {}
        }
    }

    fn arm_compositor_commit_deadline_at(&mut self, submitted_at: Instant, deadline: Instant) {
        let now = Instant::now();
        let generation = self.next_compositor_commit_generation;
        self.next_compositor_commit_generation = self
            .next_compositor_commit_generation
            .wrapping_add(1)
            .max(1);
        let token = TimerToken::next();
        if self.window_state.compositor.has_pending_scene_renders() {
            self.pending_scene_frame_work_started_at
                .get_or_insert(self.active_begin_frame_started_at.unwrap_or(submitted_at));
            self.pending_scene_frame_work_cpu_end_at = None;
        } else {
            self.pending_scene_frame_work_started_at = None;
            self.pending_scene_frame_work_cpu_end_at = None;
        }
        if let Some(previous) = self
            .pending_compositor_commit
            .replace(PendingCompositorCommit {
                deadline,
                generation,
                token,
                submitted_at,
                scene_ready_at: if self.window_state.compositor.has_pending_scene_renders() {
                    None
                } else {
                    Some(now)
                },
            })
        {
            previous.token.cancel();
        }
        if crate::frame_source::frame_pacing_diag_enabled() {
            eprintln!(
                "floem compositor deadline arm window={:?} generation={} due_in={:.3}ms submitted_age={:.3}ms pending_scene={}",
                self.window_id,
                generation,
                deadline.saturating_duration_since(now).as_secs_f64() * 1000.0,
                now.saturating_duration_since(submitted_at).as_secs_f64() * 1000.0,
                self.window_state.compositor.has_pending_scene_renders(),
            );
        }
        if deadline <= now {
            let _ = self.commit_compositor_deadline_now(generation);
        }
    }

    fn commit_compositor_deadline_now(&mut self, generation: u64) -> bool {
        let pending_scene = self.window_state.compositor.has_pending_scene_renders();
        let pending_commit_work = self.window_state.compositor.has_pending_commit_work();
        let reason = match self.compositor_frame_scheduler.on_deadline() {
            CompositorFrameAction::Commit { reason, .. } => compositor_commit_reason_label(reason),
            _ => "deadline",
        };
        if pending_scene {
            if self
                .window_state
                .compositor
                .has_independent_compositor_surface_commit_work()
            {
                let result = self.attempt_independent_compositor_surface_commit("deadline-surface");
                let committed = result == CompositorCommitResult::Committed;
                self.record_frame_demand(FrameDemand::ANIMATION);
                return committed;
            }
            eprintln!(
                "floem stutter deadline-skip-pending-scene window={:?} generation={} reason={} pending_commit_work={}",
                self.window_id, generation, reason, pending_commit_work,
            );
            self.pending_compositor_commit = None;
            self.apply_compositor_commit_result(CompositorCommitResult::NoWork);
            self.record_frame_demand(FrameDemand::ANIMATION);
            return false;
        }
        self.commit_requested_compositor_frame(reason)
    }

    pub(crate) fn handle_compositor_commit_deadline(
        &mut self,
        generation: u64,
        token: TimerToken,
    ) -> bool {
        if !self
            .pending_compositor_commit
            .is_some_and(|pending| pending.generation == generation && pending.token == token)
        {
            if crate::frame_source::frame_pacing_diag_enabled() {
                eprintln!(
                    "floem compositor deadline stale window={:?} generation={} token={:?}",
                    self.window_id, generation, token,
                );
            }
            return false;
        }
        if let Some(pending) = self.pending_compositor_commit {
            let now = Instant::now();
            let late_by_ms = now
                .checked_duration_since(pending.deadline)
                .map(|duration| duration.as_secs_f64() * 1000.0)
                .unwrap_or(0.0);
            let scene_ready_vs_deadline_ms = pending.scene_ready_at.map(|ready_at| {
                if ready_at >= pending.deadline {
                    ready_at
                        .saturating_duration_since(pending.deadline)
                        .as_secs_f64()
                        * 1000.0
                } else {
                    -(pending
                        .deadline
                        .saturating_duration_since(ready_at)
                        .as_secs_f64()
                        * 1000.0)
                }
            });
            if self.window_state.compositor.has_pending_scene_renders() || late_by_ms > 0.5 {
                eprintln!(
                    "floem stutter compositor-deadline window={:?} generation={} late_by={:.3}ms submitted_age={:.3}ms scene_ready_vs_deadline={:?}ms pending_scene={} pending_commit_work={}",
                    self.window_id,
                    generation,
                    late_by_ms,
                    now.saturating_duration_since(pending.submitted_at)
                        .as_secs_f64()
                        * 1000.0,
                    scene_ready_vs_deadline_ms,
                    self.window_state.compositor.has_pending_scene_renders(),
                    self.window_state.compositor.has_pending_commit_work(),
                );
            }
        }
        if crate::frame_source::frame_pacing_diag_enabled() {
            eprintln!(
                "floem compositor deadline fire window={:?} generation={} pending_scene={} pending_commit_work={}",
                self.window_id,
                generation,
                self.window_state.compositor.has_pending_scene_renders(),
                self.window_state.compositor.has_pending_commit_work(),
            );
        }
        self.commit_compositor_deadline_now(generation)
    }

    fn take_compositor_commit_deadline_schedule(&self) -> Option<CompositorCommitDeadlineSchedule> {
        let pending = self.pending_compositor_commit?;
        Some(CompositorCommitDeadlineSchedule {
            deadline: pending.deadline,
            generation: pending.generation,
            token: pending.token,
        })
    }

    fn current_frame_schedule(&self) -> FrameSchedule {
        let compositor_commit_deadline = self.take_compositor_commit_deadline_schedule();
        FrameSchedule {
            coalesce_input_until: compositor_commit_deadline.map(|deadline| deadline.deadline),
            compositor_commit_deadline,
        }
    }

    fn prepare_display_list_for_render(&mut self, timing: &mut FrameTimingAccumulator) -> bool {
        if !self.paint_state.is_initialized() {
            return false;
        }
        let start = Instant::now();
        let id = self.id;
        let gpu_resources = self.gpu_resources.clone();
        let window = self.window.clone();
        let record_paint_order = crate::paint::is_paint_order_tracking_enabled();
        let window_state = &mut self.window_state;
        let mut cx = crate::paint::GlobalPaintCx {
            window_state,
            gpu_resources,
            window,
            record_paint_order,
        };
        cx.prepare_display_list(id);
        let end = Instant::now();
        timing.push_absolute_span("Paint", start, end, TimingKind::Paint);
        timing.push_absolute_span("Scene", start, end, TimingKind::Paint);
        true
    }

    /// Executes one paint pass.
    ///
    /// In the window model, painting includes scene generation and handing the
    /// work to the renderer backend. It does not present. Live frame progression
    /// uses this from `paint_frame`, while capture/headless code may use it
    /// directly for an off-pipeline paint. Timing is recorded into
    /// `pending_timing` as a side effect because paint is the primary action.
    pub fn paint(&mut self, _frame_id: u64) -> bool {
        if !self.prepare_current_composition_plan() {
            return false;
        }
        self.pull_compositor_surfaces_for_window_present();
        true
    }

    fn current_frame_time(&mut self) -> FrameTime {
        if let Some(frame_time) = self.active_frame_time {
            return frame_time;
        }

        self.frame_time_at(Instant::now())
    }

    fn frame_time_at(&mut self, now: Instant) -> FrameTime {
        self.frame_source
            .current_frame_time(self.window.as_ref(), now, false)
    }

    fn waits_for_layer_host_commit_feedback(&self) -> bool {
        cfg!(target_os = "macos") && self.window_state.compositor.has_layer_host()
    }

    fn record_frame_demand(&mut self, demand: FrameDemand) {
        self.compositor_frame_scheduler
            .request_frame(Self::pacing_frame_demand(demand));
    }

    fn pacing_frame_demand(demand: FrameDemand) -> PacingFrameDemand {
        let mut pacing = PacingFrameDemand::NONE;
        if demand.contains(FrameDemand::ANIMATION) {
            pacing.insert(PacingFrameDemand::Animation);
        }
        if demand.contains(FrameDemand::DISCRETE_INPUT) {
            pacing.insert(PacingFrameDemand::Input);
        }
        if demand.contains(FrameDemand::CONTINUOUS_INPUT) {
            pacing.insert(PacingFrameDemand::ContinuousInput);
        }
        if demand.contains(FrameDemand::COMPOSITOR_SURFACE) {
            pacing.insert(PacingFrameDemand::Animation);
        }
        if pacing.is_empty() && !demand.is_empty() {
            pacing.insert(PacingFrameDemand::Animation);
        }
        pacing
    }

    fn frame_demand_from_pacing(demand: PacingFrameDemand) -> FrameDemand {
        let mut frame_demand = FrameDemand::empty();
        if demand.contains(PacingFrameDemand::Animation) {
            frame_demand.insert(FrameDemand::ANIMATION);
        }
        if demand.contains(PacingFrameDemand::Input) {
            frame_demand.insert(FrameDemand::DISCRETE_INPUT);
        }
        if demand.contains(PacingFrameDemand::ContinuousInput) {
            frame_demand.insert(FrameDemand::CONTINUOUS_INPUT);
        }
        frame_demand
    }

    fn pacing_time(&self, instant: Instant) -> PacingTime {
        PacingTime::from_nanos(
            instant
                .saturating_duration_since(self.frame_scheduler_time_origin)
                .as_nanos()
                .min(i64::MAX as u128) as i64,
        )
    }

    fn instant_from_pacing_time(&self, time: PacingTime) -> Instant {
        self.frame_scheduler_time_origin
            .checked_add(Duration::from_nanos(time.as_nanos().max(0) as u64))
            .unwrap_or(self.frame_scheduler_time_origin)
    }

    fn active_compositor_deadline_instant(&self) -> Option<Instant> {
        self.compositor_frame_scheduler
            .active_frame()
            .map(|frame| self.instant_from_pacing_time(frame.timing.deadline))
    }

    fn begin_frame_timing_for_frame_time(
        &self,
        frame_time: FrameTime,
        now: Instant,
    ) -> BeginFrameTiming {
        let frame_interval = frame_time.frame_interval;
        let frame_time_instant = frame_time.now;
        let deadline_instant = frame_time.interval.deadline_max.max(now);
        let now = self.pacing_time(now);
        let frame_time = self.pacing_time(frame_time_instant);
        let deadline = self.pacing_time(deadline_instant);
        let interval =
            PacingDuration::from_nanos(frame_interval.as_nanos().min(u64::MAX as u128) as u64);
        BeginFrameTiming {
            now,
            frame_time,
            deadline,
            interval,
        }
    }

    fn note_begin_frame_finished(&mut self) {
        match self.compositor_frame_scheduler.on_commit_complete() {
            CompositorFrameAction::FinishFrame {
                pending: Some(pending),
            } => {
                self.active_begin_frame_started_at = None;
                let demand = Self::frame_demand_from_pacing(pending.demand);
                if crate::frame_source::frame_pacing_diag_enabled() {
                    eprintln!(
                        "floem begin frame finish window={:?} pending={} demand={:?}",
                        self.window_id, pending.sequence, demand,
                    );
                }
                if !demand.is_empty() {
                    self.compositor_frame_scheduler
                        .request_frame(Self::pacing_frame_demand(demand));
                }
            }
            _ => {
                self.active_begin_frame_started_at = None;
                if crate::frame_source::frame_pacing_diag_enabled() {
                    eprintln!(
                        "floem begin frame finish window={:?} pending=none",
                        self.window_id,
                    );
                }
            }
        }
    }

    fn compositor_work_status(&self) -> CompositorWorkStatus {
        CompositorWorkStatus {
            can_draw: self.window_is_visible(),
            needs_frame_work: self.window_state.has_next_frame_work()
                || self.has_current_frame_prepare_work()
                || !self.window_state.begin_frame_callbacks.is_empty()
                || self.window_state.has_pending_paint()
                || self.window_state.has_pending_render()
                || self.has_compositor_surface_pull(),
            scene_jobs_pending: self.window_state.compositor.has_pending_scene_renders(),
            commit_ready: self.window_state.compositor.has_pending_commit_work(),
        }
    }

    fn has_compositor_surface_pull(&self) -> bool {
        !self.window_state.has_pending_paint()
            && !self.window_state.has_pending_render()
            && self.window_state.composition_plan.has_compositor_surfaces()
            && self.window_state.compositor_surfaces.has_frame_pull()
    }

    fn status_frame_demand(&self, status: CompositorWorkStatus) -> FrameDemand {
        if !status.can_draw {
            return FrameDemand::empty();
        }
        let mut demand = FrameDemand::empty();
        if status.needs_frame_work || status.scene_jobs_pending || status.commit_ready {
            demand.insert(FrameDemand::ANIMATION);
        }
        demand
    }

    fn has_active_compositor_frame_work(&self) -> bool {
        self.pending_compositor_commit.is_some()
            || self.window_state.compositor.has_pending_scene_renders()
            || self.window_state.compositor.has_pending_commit_work()
    }

    fn has_ready_compositor_commit(&self) -> bool {
        self.pending_compositor_commit.is_some()
            && !self.window_state.compositor.has_pending_scene_renders()
            && self.window_state.compositor.has_pending_commit_work()
    }

    fn arm_layer_host_commit_feedback(&mut self, submitted_at: Instant) -> bool {
        self.pending_layer_host_commit = Some(PendingLayerHostCommit {
            submitted_at,
            frame_time: self.active_frame_time,
        });
        true
    }

    pub(crate) fn note_discrete_input_frame_demand(&mut self) {
        self.record_frame_demand(FrameDemand::DISCRETE_INPUT);
    }

    pub(crate) fn note_continuous_input_frame_demand(&mut self) {
        self.record_frame_demand(FrameDemand::CONTINUOUS_INPUT);
    }

    pub(crate) fn note_animation_frame_demand(&mut self) {
        self.record_frame_demand(FrameDemand::ANIMATION);
    }

    pub(crate) fn note_compositor_surface_frame_demand(&mut self) {
        self.record_frame_demand(FrameDemand::COMPOSITOR_SURFACE);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn capture_next_metal_frame(&mut self) {
        request_next_metal_capture();
        self.window_state
            .request_paint(self.window_state.root_view_id);
        self.note_animation_frame_demand();
        self.schedule_repaint();
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn begin_metal_capture_on_frame_tick(&mut self) {
        if self
            .gpu_resources
            .as_ref()
            .and_then(|resources| MetalCaptureScopeGuard::begin_frame(&resources.queue))
            .is_some()
        {
            self.window_state.compositor.mark_metal_capture_active();
        }
    }

    pub(crate) fn refresh_frame_source_target(&mut self) {
        self.frame_source
            .refresh_window_target(self.window.as_ref());
    }

    pub(crate) fn poll_gpu_callbacks(&self) {
        if let Some(gpu_resources) = &self.gpu_resources {
            let _ = gpu_resources.device.poll(wgpu::PollType::Poll);
        }
    }

    pub(crate) fn record_deferred_frame_timer_lateness(&mut self, lateness: Duration) {
        self.timer_wakeup_estimate = smooth_duration_estimate(self.timer_wakeup_estimate, lateness);
    }

    fn run_begin_frame_callbacks(&mut self) {
        if self.window_state.begin_frame_callbacks.is_empty() {
            return;
        }
        let frame_time = self.current_frame_time();
        let callbacks = self.window_state.take_begin_frame_callbacks();
        for callback in callbacks {
            callback(frame_time);
        }
    }

    fn receive_frame_tick(&mut self, tick: subduction_core::timing::FrameTick) {
        if crate::frame_source::frame_pacing_diag_enabled() {
            eprintln!(
                "floem frame pacing window tick window={:?} tick={}",
                self.window_id, tick.frame_index,
            );
        }
        self.frame_source.receive_frame_tick(tick);
    }

    pub(crate) fn schedule_frame_tick_work(
        &mut self,
        tick: subduction_core::timing::FrameTick,
    ) -> Instant {
        let now = Instant::now();
        self.receive_frame_tick(tick);
        let frame_time = self.frame_time_at(now);
        let interval = PacingDuration::from_nanos(
            frame_time.frame_interval.as_nanos().min(u64::MAX as u128) as u64,
        );
        let start_before_present_estimate = self
            .frame_work_estimate
            .saturating_add(self.gpu_work_estimate)
            .saturating_add(COMPOSITOR_DEADLINE_FUDGE)
            .saturating_add(self.timer_wakeup_estimate);
        let estimate = FrameTimingEstimate {
            pre_surface_work: PacingDuration::from_nanos(
                start_before_present_estimate
                    .as_nanos()
                    .min(u64::MAX as u128) as u64,
            ),
            surface_work: PacingDuration::ZERO,
            gpu_work: PacingDuration::ZERO,
            safety_margin: PacingDuration::ZERO,
        };
        let demand =
            Self::pacing_frame_demand(self.status_frame_demand(self.compositor_work_status()));
        let decision = plan_frame(
            PacingDisplayTiming::fixed(interval),
            estimate,
            demand,
            FrameOpportunity {
                now: self.pacing_time(now),
                predicted_present_time: frame_time
                    .interval
                    .predicted_present
                    .map(|present| self.pacing_time(present)),
                frame_interval: Some(interval),
                last_present_time: None,
                pending_target_present_time: None,
            },
        );
        let start_at = self
            .instant_from_pacing_time(decision.pre_surface_work_start)
            .max(now);
        start_at
    }

    /// Starts or stops the frame clock from window-owned frame state.
    ///
    /// This is the only thing event/update paths should do after queueing
    /// frame work. It does not receive ticks, prepare, paint, or commit; it
    /// just tells the clock whether it should keep producing frame ticks.
    pub(crate) fn refresh_frame_activity(&mut self) {
        self.frame_source.set_active(
            self.compositor_frame_scheduler
                .needs_frame_source(self.compositor_work_status()),
        );
    }

    pub(crate) fn has_frame_work(&self) -> bool {
        self.compositor_frame_scheduler
            .needs_frame_source(self.compositor_work_status())
    }

    /// Processes one backend frame tick.
    ///
    /// The order is:
    /// 1. install the tick into the frame clock
    /// 2. classify accumulated frame demand
    /// 3. promote next-frame work and run begin-frame callbacks when needed
    /// 4. prepare current style/layout/display-list work
    /// 5. render scene fragments and commit when all required work is ready
    ///
    /// Event/update handling only queues work and calls `refresh_frame_activity`.
    /// Render/compositor work must enter here with an explicit tick so the
    /// method does not need latent "frame signal pending" state.
    pub(crate) fn process_frame_tick(
        &mut self,
        tick: subduction_core::timing::FrameTick,
    ) -> FrameSchedule {
        self.record_profile_instant("BeginFrameWork", Instant::now());
        // Frame timing is valid for one backend frame opportunity. Do not let a
        // previous prepared frame's predicted-present deadline leak into a later
        // pending-render/compositor-only commit path.
        self.active_frame_time = None;
        if self
            .frame_source
            .latest_tick()
            .is_none_or(|latest| latest.frame_index != tick.frame_index)
        {
            self.receive_frame_tick(tick);
        }
        #[cfg(target_os = "macos")]
        self.begin_metal_capture_on_frame_tick();

        let now = Instant::now();
        if crate::frame_source::frame_pacing_diag_enabled() {
            eprintln!(
                "floem frame advance begin window={:?} next_work={} next_window_work={} compositor_frame_pull={} plan_surfaces={} begin_callbacks={} pending_paint={} pending_render={} can_render={} status_demand={:?} active_begin={} pending_begin={} active_work={} pending_deadline={}",
                self.window_id,
                self.window_state.has_next_frame_work(),
                self.window_state.has_next_window_frame_work(),
                self.window_state.compositor_surfaces.has_frame_pull(),
                self.window_state.composition_plan.has_compositor_surfaces(),
                !self.window_state.begin_frame_callbacks.is_empty(),
                self.window_state.has_pending_paint(),
                self.window_state.has_pending_render(),
                self.window_is_visible(),
                self.status_frame_demand(self.compositor_work_status()),
                self.compositor_frame_scheduler.has_active_frame(),
                self.compositor_frame_scheduler.has_pending_frame(),
                self.has_active_compositor_frame_work(),
                self.pending_compositor_commit.is_some(),
            );
        }
        // Begin-frame callbacks are animation-timeline work; classify the
        // opportunity before the frame clock computes pacing.
        if !self.window_state.begin_frame_callbacks.is_empty() {
            self.record_frame_demand(FrameDemand::ANIMATION);
        }
        let window_visible = self.window_is_visible();
        if !window_visible {
            if crate::frame_source::frame_pacing_diag_enabled() {
                eprintln!(
                    "floem frame advance blocked window={:?} reason=can_render_now_false",
                    self.window_id,
                );
            }
            return FrameSchedule::default();
        }

        let status = self.compositor_work_status();
        let frame_demand = self.status_frame_demand(status);
        let frame_time = self.frame_time_at(now);

        if self.has_ready_compositor_commit()
            && self.commit_requested_compositor_frame("scene-ready")
        {
            if crate::frame_source::frame_pacing_diag_enabled() {
                eprintln!(
                    "floem frame advance flushed-ready-compositor window={:?}",
                    self.window_id,
                );
            }
        }

        let has_compositor_surface_pull = self.has_compositor_surface_pull();
        let has_new_frame_work = self.compositor_frame_scheduler.needs_frame_source(status);
        if crate::frame_source::frame_pacing_diag_enabled() {
            eprintln!(
                "floem frame advance work window={:?} has_new={} surface_pull={} current_prepare={} scene_pending={} commit_ready={} pending_begin={} active_work={}",
                self.window_id,
                has_new_frame_work,
                has_compositor_surface_pull,
                self.has_current_frame_prepare_work(),
                status.scene_jobs_pending,
                status.commit_ready,
                self.compositor_frame_scheduler.has_pending_frame(),
                self.has_active_compositor_frame_work(),
            );
        }
        if !has_new_frame_work {
            if crate::frame_source::frame_pacing_diag_enabled() {
                eprintln!("floem frame advance idle window={:?}", self.window_id);
            }
            return self.current_frame_schedule();
        }
        if let Some(active) = self.compositor_frame_scheduler.active_frame() {
            let active_age_ms = self
                .active_begin_frame_started_at
                .map(|started| now.saturating_duration_since(started).as_secs_f64() * 1000.0);
            eprintln!(
                "floem stutter tick-while-active window={:?} tick={} active_sequence={} pending_begin={} active_age={:?}ms pending_scene={} commit_ready={} pending_deadline={} pending_paint={} pending_render={}",
                self.window_id,
                tick.frame_index,
                active.sequence,
                self.compositor_frame_scheduler.has_pending_frame(),
                active_age_ms,
                status.scene_jobs_pending,
                status.commit_ready,
                self.pending_compositor_commit.is_some(),
                self.window_state.has_pending_paint(),
                self.window_state.has_pending_render(),
            );
        }
        let begin_timing = self.begin_frame_timing_for_frame_time(frame_time, now);
        match self
            .compositor_frame_scheduler
            .on_begin_frame(begin_timing, status)
        {
            CompositorFrameAction::Idle => {
                if crate::frame_source::frame_pacing_diag_enabled() {
                    eprintln!("floem begin frame idle window={:?}", self.window_id);
                }
            }
            CompositorFrameAction::StartFrame(frame) => {
                self.active_frame_time = Some(frame_time);
                self.active_begin_frame_started_at = Some(now);
                let active_demand = Self::frame_demand_from_pacing(frame.demand);
                if crate::frame_source::frame_pacing_diag_enabled() {
                    eprintln!(
                        "floem begin frame start window={:?} sequence={} demand={:?} deadline_from_now={:.3}ms interval={:.3}ms",
                        self.window_id,
                        frame.sequence,
                        active_demand,
                        (frame.timing.deadline - frame.timing.now).as_nanos() as f64 / 1_000_000.0,
                        frame.timing.interval.as_nanos() as f64 / 1_000_000.0,
                    );
                }
            }
            CompositorFrameAction::Commit { reason, .. } => {
                let reason = compositor_commit_reason_label(reason);
                let _ = self.commit_requested_compositor_frame(reason);
            }
            CompositorFrameAction::Coalesced { active, pending } => {
                if self.has_active_compositor_frame_work()
                    || self.pending_compositor_commit.is_some()
                {
                    eprintln!(
                        "floem stutter coalesced-frame window={:?} active={} pending={} demand={:?} active_work={} pending_deadline={} deadline_from_now={:.3}ms scene_pending={} commit_ready={}",
                        self.window_id,
                        active.sequence,
                        pending.sequence,
                        frame_demand,
                        self.has_active_compositor_frame_work(),
                        self.pending_compositor_commit.is_some(),
                        (pending.timing.deadline - pending.timing.now).as_nanos() as f64
                            / 1_000_000.0,
                        self.window_state.compositor.has_pending_scene_renders(),
                        self.window_state.compositor.has_pending_commit_work(),
                    );
                }
                if crate::frame_source::frame_pacing_diag_enabled() {
                    eprintln!(
                        "floem frame advance coalesced window={:?} active={} pending={} demand={:?} active_work={} pending_deadline={} deadline_from_now={:.3}ms",
                        self.window_id,
                        active.sequence,
                        pending.sequence,
                        frame_demand,
                        self.has_active_compositor_frame_work(),
                        self.pending_compositor_commit.is_some(),
                        (pending.timing.deadline - pending.timing.now).as_nanos() as f64
                            / 1_000_000.0,
                    );
                }
                return self.current_frame_schedule();
            }
            CompositorFrameAction::ArmDeadline(_) | CompositorFrameAction::FinishFrame { .. } => {}
        }

        let prepared = self.prepare_frame();

        if has_compositor_surface_pull {
            self.pull_compositor_surface_frame();
        }

        if self.window_state.has_pending_paint() || self.window_state.has_pending_render() {
            let prepared_plan = self.prepare_current_composition_plan();
            if prepared_plan {
                self.pull_compositor_surfaces_for_window_present();
            }
            let scheduled_scene_frames = if prepared_plan {
                self.commit_compositor_frame("frame")
            } else {
                None
            };
            if scheduled_scene_frames.is_none()
                && (self.window_state.has_pending_paint() || self.window_state.has_pending_render())
            {
                eprintln!(
                    "floem stutter no-render-scheduled window={:?} prepared_plan={} pending_paint={} pending_render={} scene_pending={} commit_ready={}",
                    self.window_id,
                    prepared_plan,
                    self.window_state.has_pending_paint(),
                    self.window_state.has_pending_render(),
                    self.window_state.compositor.has_pending_scene_renders(),
                    self.window_state.compositor.has_pending_commit_work(),
                );
            }
            if crate::frame_source::frame_pacing_diag_enabled() {
                eprintln!(
                    "floem frame advance rendered window={:?} scheduled={} pending_paint={} pending_render={}",
                    self.window_id,
                    scheduled_scene_frames.is_some(),
                    self.window_state.has_pending_paint(),
                    self.window_state.has_pending_render(),
                );
            }
        } else if self.has_active_compositor_frame_work() {
            let submitted_at = Instant::now();
            let scheduled_scene_frames = self.commit_compositor_frame("external");
            let _ = submitted_at;
            if crate::frame_source::frame_pacing_diag_enabled() {
                eprintln!(
                    "floem frame advance compositor-only window={:?} committed={} pending_paint=false pending_render=false",
                    self.window_id,
                    scheduled_scene_frames.is_some(),
                );
            }
        } else if crate::frame_source::frame_pacing_diag_enabled() {
            eprintln!(
                "floem frame advance no_render window={:?} prepared={} pending_paint=false pending_render=false",
                self.window_id, prepared,
            );
        }

        if !self.has_active_compositor_frame_work() {
            // Chromium finishes its BeginFrame lifecycle after the deadline
            // has drawn/submitted or found no drawable damage. Do the same
            // here; otherwise fixed-rate displays can coalesce the next real
            // tick behind a frame that already has no outstanding work.
            self.note_begin_frame_finished();
        }

        if crate::frame_source::frame_pacing_diag_enabled() {
            eprintln!(
                "floem frame advance end window={:?} prepared={} next_work={} pending_paint={} pending_render={} schedule=none",
                self.window_id,
                prepared,
                self.window_state.has_next_frame_work(),
                self.window_state.has_pending_paint(),
                self.window_state.has_pending_render(),
            );
        }

        self.current_frame_schedule()
    }

    fn finish_presented_frame(
        &mut self,
        submitted_at: Option<Instant>,
        presented_at: Instant,
        submitted_frame_time: Option<FrameTime>,
    ) -> bool {
        self.window_state.clear_pending_damage();
        let feedback_matches_active = self
            .active_frame_time
            .zip(submitted_frame_time)
            .is_none_or(|(active, submitted)| active.frame_index == submitted.frame_index);
        if !feedback_matches_active
            && let (Some(active), Some(submitted)) = (self.active_frame_time, submitted_frame_time)
        {
            eprintln!(
                "floem stutter stale-present-feedback window={:?} submitted_frame={} active_frame={} submitted_at={:?} presented_at={:?}",
                self.window_id,
                submitted.frame_index,
                active.frame_index,
                submitted_at,
                presented_at,
            );
        }
        let presented =
            feedback_matches_active && self.pending_timing.has_kind(TimingKind::Present);
        let submitted_at_for_deadline = submitted_at.unwrap_or(presented_at);
        let missed_deadline = submitted_frame_time
            .map(|frame_time| submitted_at_for_deadline > frame_time.interval.deadline_max);
        if missed_deadline == Some(true)
            && let Some(frame_time) = submitted_frame_time
        {
            eprintln!(
                "floem render missed deadline window={:?} frame={} submitted_late_by={:.3}ms deadline={:?} submitted_at={:?} predicted_present={:?}",
                self.window_id,
                frame_time.frame_index,
                submitted_at_for_deadline
                    .saturating_duration_since(frame_time.interval.deadline_max)
                    .as_secs_f64()
                    * 1000.0,
                frame_time.interval.deadline_max,
                submitted_at_for_deadline,
                frame_time.interval.predicted_present,
            );
        }
        if presented {
            let update = mem::take(&mut self.pending_timing);
            let submitted_at = submitted_at_for_deadline;
            let feedback_start = self
                .active_frame_time
                .filter(|active| {
                    submitted_frame_time
                        .is_none_or(|submitted| submitted.frame_index == active.frame_index)
                })
                .or(submitted_frame_time)
                .map(|frame_time| frame_time.interval.deadline_min)
                .unwrap_or(submitted_at);
            let render_cpu = update.active_frame_work_duration_since(feedback_start);
            let _present_cpu = update
                .max_duration_for_label_since("Present", feedback_start)
                .saturating_sub(
                    update.max_duration_for_label_since("AcquireSurface", feedback_start),
                );
            if crate::frame_source::frame_pacing_diag_enabled() {
                eprintln!(
                    "floem frame pacing window feedback active_cpu={:.3}ms accumulated_cpu={:.3}ms prepare={:.3}ms paint={:.3}ms render={:.3}ms style_sum={:.3}ms layout_sum={:.3}ms boxtree_sum={:.3}ms present={:.3}ms acquire={:.3}ms compose={:.3}ms present_call={:.3}ms",
                    render_cpu.as_secs_f64() * 1000.0,
                    update.active_frame_work_duration().as_secs_f64() * 1000.0,
                    update.max_duration_for_label("PrepareFrame").as_secs_f64() * 1000.0,
                    update.max_duration_for_label("Paint").as_secs_f64() * 1000.0,
                    update.max_duration_for_label("Render").as_secs_f64() * 1000.0,
                    update
                        .sum_duration_for_kind(TimingKind::Style)
                        .as_secs_f64()
                        * 1000.0,
                    update
                        .sum_duration_for_kind(TimingKind::Layout)
                        .as_secs_f64()
                        * 1000.0,
                    update
                        .sum_duration_for_kind(TimingKind::BoxTree)
                        .as_secs_f64()
                        * 1000.0,
                    update.max_duration_for_label("Present").as_secs_f64() * 1000.0,
                    update
                        .max_duration_for_label("AcquireSurface")
                        .as_secs_f64()
                        * 1000.0,
                    update.max_duration_for_label("Compose").as_secs_f64() * 1000.0,
                    update.max_duration_for_label("PresentCall").as_secs_f64() * 1000.0,
                );
            }
            self.record_profile_instant("FramePresented", presented_at);
            let timing_report = update.build_timing_report();
            if self.profile.is_some() {
                let queued_events = self.take_profile_events();
                if let Some(profile) = self.profile.as_mut() {
                    profile.current.events.extend(queued_events);
                    profile.current.timing = Some(timing_report);
                    profile.next_frame();
                }
                self.last_timing_report = None;
            } else {
                self.last_timing_report = Some(timing_report);
            }
        }
        if presented && feedback_matches_active {
            self.active_frame_time = None;
        }
        let frame_index = self.next_frame_id.saturating_sub(1);
        self.window_state
            .compositor_surfaces
            .release_outcomes(|outcome| {
                outcome.frame_index = frame_index;
                outcome.outcome.draw_completed = presented;
                outcome.outcome.missed_deadline = missed_deadline;
            });
        presented
    }

    pub(crate) fn handle_layer_host_commit(&mut self, committed_at: Instant) {
        let Some(pending) = self.pending_layer_host_commit.take() else {
            return;
        };
        let submitted_at = pending.submitted_at;
        self.route_paint_present_event(committed_at, pending.frame_time);
        let pacing_presented_at = self
            .active_frame_time
            .filter(|active| {
                pending
                    .frame_time
                    .is_none_or(|submitted| submitted.frame_index == active.frame_index)
            })
            .or(pending.frame_time)
            .and_then(|frame_time| frame_time.interval.predicted_present)
            .filter(|predicted_present| *predicted_present >= committed_at)
            .unwrap_or(committed_at);
        if let Some(frame_time) = pending.frame_time {
            let interval = frame_time.frame_interval;
            let interval_ms = interval.as_secs_f64() * 1000.0;
            let commit_margin_ms = frame_time.interval.predicted_present.map(|predicted| {
                if predicted >= committed_at {
                    predicted
                        .saturating_duration_since(committed_at)
                        .as_secs_f64()
                        * 1000.0
                } else {
                    -(committed_at
                        .saturating_duration_since(predicted)
                        .as_secs_f64()
                        * 1000.0)
                }
            });
            let submitted_to_commit_ms = committed_at
                .saturating_duration_since(submitted_at)
                .as_secs_f64()
                * 1000.0;
            let presented_late_ms = frame_time.interval.predicted_present.and_then(|predicted| {
                (committed_at > predicted).then(|| {
                    committed_at
                        .saturating_duration_since(predicted)
                        .as_secs_f64()
                        * 1000.0
                })
            });
            if presented_late_ms.is_some_and(|late| late > 1.0)
                || submitted_to_commit_ms > interval_ms * 1.5
            {
                eprintln!(
                    "floem stutter layer-host-feedback window={:?} frame={} commit_margin={:?}ms presented_late={:?}ms submitted_to_commit={:.3}ms interval={:.3}ms",
                    self.window_id,
                    frame_time.frame_index,
                    commit_margin_ms,
                    presented_late_ms,
                    submitted_to_commit_ms,
                    interval_ms,
                );
            }
        }
        if crate::frame_source::frame_pacing_diag_enabled() {
            eprintln!(
                "floem layer host commit feedback window={:?} committed_to_predicted={:.3}ms feedback_presented_at={:?}",
                self.window_id,
                pacing_presented_at
                    .saturating_duration_since(committed_at)
                    .as_secs_f64()
                    * 1000.0,
                pacing_presented_at,
            );
        }
        self.finish_presented_frame(Some(submitted_at), pacing_presented_at, pending.frame_time);
    }

    pub(crate) fn take_last_timing_report(&mut self) -> Option<TimingReport> {
        self.last_timing_report.take()
    }

    fn route_paint_present_event(&mut self, presented_at: Instant, frame_time: Option<FrameTime>) {
        let target_frame_interval = frame_time.map(|frame| frame.frame_interval);
        let missed_deadline =
            frame_time.is_some_and(|frame| presented_at > frame.interval.deadline_max);
        let layers = mem::take(&mut self.pending_presented_layers)
            .into_iter()
            .map(|layer| PaintPresentLayer {
                layer_id: layer.layer_id.index(),
                source_element_id: layer.source_element_id,
                debug_name: layer.debug_name,
                target_frame_interval,
                missed_deadline,
            })
            .collect::<Vec<_>>();
        let active_layers = mem::take(&mut self.pending_active_layers)
            .into_iter()
            .map(|layer| PaintPresentLayer {
                layer_id: layer.layer_id.index(),
                source_element_id: layer.source_element_id,
                debug_name: layer.debug_name,
                target_frame_interval,
                missed_deadline: false,
            })
            .collect::<Vec<_>>();
        let info = PaintPresentInfo {
            presented_at,
            layers,
            active_layers,
        };
        self.hud.record_present(&info);
        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::PaintPresent(
            info,
        )));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();
    }

    fn capture_image(&mut self) -> crate::paint::renderer::CaptureOutput {
        let total_start = Instant::now();
        if let Some(mut output) = self.capture_composited_image(total_start) {
            output.timing.total = total_start.elapsed();
            return output;
        }

        crate::paint::renderer::CaptureOutput {
            error: Some("capture without compositor renderer pool is not implemented".to_owned()),
            timing: crate::paint::renderer::CaptureTiming {
                total: total_start.elapsed(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn capture_composited_image(
        &mut self,
        total_start: Instant,
    ) -> Option<crate::paint::renderer::CaptureOutput> {
        if !self.window_state.compositor.has_layer_host() {
            return None;
        }
        let gpu_resources = self.gpu_resources.clone()?;
        if !self.paint_state.is_initialized() {
            return None;
        }
        let renderer_pool = self.scene_renderer_pool.get()?;

        let _ = self.render_compositor_scene_layers("capture");
        let extent = surface_extent(self.window_state.root_size, self.window_state.os_scale);
        let frame_size = Size::new(f64::from(extent.width), f64::from(extent.height));
        let background = self.capture_background();
        let capture = match self.window_state.compositor.capture_scene(
            &self.window_state.composition_plan,
            frame_size,
            self.window_state.effective_scale(),
            background,
        ) {
            Ok(capture) => capture,
            Err(error) => {
                return Some(crate::paint::renderer::CaptureOutput {
                    error: Some(error),
                    timing: crate::paint::renderer::CaptureTiming {
                        total: total_start.elapsed(),
                        ..Default::default()
                    },
                    ..Default::default()
                });
            }
        };

        Some(crate::paint::renderer::capture_source_with_external_images(
            &renderer_pool,
            &gpu_resources,
            frame_size,
            capture.scene,
            capture.resources,
        ))
    }

    fn capture_background(&self) -> Option<Brush> {
        if self.transparent {
            None
        } else {
            Some(
                self.default_theme
                    .as_ref()
                    .and_then(|theme| theme.get(crate::style::Background))
                    .unwrap_or(Brush::Solid(palette::css::WHITE)),
            )
        }
    }

    pub(crate) fn capture(&mut self) -> Capture {
        // Capture the view before we run `style` and `layout` to catch missing `request_style`` or
        // `request_layout` flags.
        let root = CapturedView::capture(self.id, &mut self.window_state);

        fn get_taffy_depth(taffy: Rc<RefCell<LayoutTree>>, root: taffy::tree::NodeId) -> usize {
            let children = taffy.borrow().children(root).unwrap();
            if children.is_empty() {
                1
            } else {
                children
                    .iter()
                    .map(|child| get_taffy_depth(taffy.clone(), *child))
                    .max()
                    .unwrap()
                    + 1
            }
        }

        let mut update = FrameTimingAccumulator::default();
        self.style(&mut update);
        let taffy_root_node = self.id.state().borrow().layout_id;
        self.layout(&mut update);
        self.commit_box_tree(&mut update);
        let mut timing = FrameTimingAccumulator::default();
        let _ = self.prepare_display_list_for_render(&mut timing);
        let capture_output = self.capture_image();
        update.absorb(timing);
        let timings = update.build_timing_report();
        let window_size = self.window_state.root_size;
        let state = CaptureState::collect_from(self.id, &self.window_state);

        let capture = Capture {
            timings,
            taffy_node_count: self.id.taffy().borrow().total_node_count(),
            taffy_depth: get_taffy_depth(self.id.taffy(), taffy_root_node),
            window: capture_output.image,
            window_capture_error: capture_output.error,
            window_size,
            root: Rc::new(root),
            state,
            renderer: self
                .scene_renderer_pool
                .get()
                .map(|pool| pool.debug_info())
                .unwrap_or_else(|| "Renderer: uninitialized scene fragment pool".to_owned()),
        };
        // Process any updates produced by capturing
        self.process_update();

        capture
    }

    pub(crate) fn process_update(&mut self) {
        self.process_update_no_paint();
    }

    /// Drains queued update messages without running style/layout/box-tree work.
    ///
    /// Live event/update paths use this to turn messages into dirty flags, then
    /// let the next frame tick run the actual frame pipeline with authoritative
    /// frame timing. Capture/headless code can still use `process_update_no_paint`
    /// when it deliberately needs a synchronous pipeline.
    pub(crate) fn process_update_messages_only(&mut self) {
        loop {
            self.process_update_messages();
            if !self.has_deferred_update_messages() {
                break;
            }
            self.process_deferred_update_messages();
        }

        self.set_cursor();
    }

    pub(crate) fn set_occluded(&mut self, is_occluded: bool) {
        self.is_occluded = is_occluded;
    }

    pub(crate) fn window_is_visible(&self) -> bool {
        !self.is_occluded && self.window.is_visible().unwrap_or(true)
    }

    /// Processes updates up to a shared budget and returns whether this window is quiescent.
    #[cfg(test)]
    pub(crate) fn process_update_budgeted(&mut self, start: Instant, budget: Duration) -> bool {
        let mut iterations = 0usize;
        const MAX_ITERS: usize = 32;

        loop {
            loop {
                self.process_update_messages();
                let needs_style = self.needs_style();
                let needs_layout = self.needs_layout();
                let needs_box_update = self.needs_box_tree_update();
                let needs_box = self.needs_box_tree_commit();
                let has_pending_box_updates =
                    !self.window_state.views_needing_box_tree_update.is_empty();
                if !needs_layout
                    && !needs_style
                    && !needs_box
                    && !has_pending_box_updates
                    && !needs_box_update
                {
                    break;
                }

                if needs_style {
                    let mut timing = std::mem::take(&mut self.pending_timing);
                    self.style(&mut timing);
                    self.pending_timing = timing;
                }

                if self.needs_layout() {
                    let mut timing = std::mem::take(&mut self.pending_timing);
                    self.layout(&mut timing);
                    self.pending_timing = timing;
                }

                if self.needs_box_tree_update() {
                    let mut timing = std::mem::take(&mut self.pending_timing);
                    self.update_box_tree_from_layout(&mut timing);
                    self.pending_timing = timing;
                }

                if !self.window_state.views_needing_box_tree_update.is_empty() {
                    let mut timing = std::mem::take(&mut self.pending_timing);
                    self.process_pending_box_tree_updates(&mut timing);
                    self.pending_timing = timing;
                }

                if self.needs_box_tree_commit() {
                    let mut timing = std::mem::take(&mut self.pending_timing);
                    self.commit_box_tree(&mut timing);
                    self.pending_timing = timing;
                }

                iterations += 1;
                if iterations >= MAX_ITERS || start.elapsed() >= budget {
                    return false;
                }
            }

            if !self.has_deferred_update_messages() {
                break;
            }
            self.process_deferred_update_messages();

            iterations += 1;
            if iterations >= MAX_ITERS || start.elapsed() >= budget {
                return false;
            }
        }

        self.set_cursor();

        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Complete));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();

        true
    }

    /// Processes updates and runs style and layout if needed.
    pub(crate) fn process_update_no_paint(&mut self) {
        loop {
            loop {
                self.process_update_messages();
                let needs_style = self.needs_style();
                let needs_layout = self.needs_layout();
                let needs_box_update = self.needs_box_tree_update();
                let needs_box = self.needs_box_tree_commit();
                let has_pending_box_updates =
                    !self.window_state.views_needing_box_tree_update.is_empty();
                if !needs_layout
                    && !needs_style
                    && !needs_box
                    && !has_pending_box_updates
                    && !needs_box_update
                {
                    break;
                }

                if needs_style {
                    let mut timing = std::mem::take(&mut self.pending_timing);
                    self.style(&mut timing);
                    self.pending_timing = timing;
                }

                if self.needs_layout() {
                    let mut timing = std::mem::take(&mut self.pending_timing);
                    self.layout(&mut timing);
                    self.pending_timing = timing;
                }

                if self.needs_box_tree_update() {
                    let mut timing = std::mem::take(&mut self.pending_timing);
                    self.update_box_tree_from_layout(&mut timing);
                    self.pending_timing = timing;
                }

                // Process any pending individual box tree updates after layout
                if !self.window_state.views_needing_box_tree_update.is_empty() {
                    let mut timing = std::mem::take(&mut self.pending_timing);
                    self.process_pending_box_tree_updates(&mut timing);
                    self.pending_timing = timing;
                }

                if self.needs_box_tree_commit() {
                    let mut timing = std::mem::take(&mut self.pending_timing);
                    self.commit_box_tree(&mut timing);
                    self.pending_timing = timing;
                }
            }
            if !self.has_deferred_update_messages() {
                break;
            }
            self.process_deferred_update_messages();
        }

        self.set_cursor();

        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(UpdatePhaseEvent::Complete));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();
    }

    fn process_central_messages(&self) {
        CENTRAL_UPDATE_MESSAGES.with_borrow_mut(|central_msgs| {
            if !central_msgs.is_empty() {
                UPDATE_MESSAGES.with_borrow_mut(|msgs| {
                    // We need to retain any messages which are for a view that either belongs
                    // to a different window, or which does not yet have a root
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

    pub(crate) fn process_update_messages(&mut self) {
        set_current_view(self.id.root());
        loop {
            self.process_central_messages();
            let msgs =
                UPDATE_MESSAGES.with(|msgs| msgs.borrow_mut().remove(&self.id).unwrap_or_default());
            if msgs.is_empty() {
                break;
            }
            for msg in msgs {
                let mut cx = UpdateCx {
                    window_state: &mut self.window_state,
                };
                match msg {
                    UpdateMessage::RequestStyle(id, reason) => {
                        self.window_state.request_style_with(id, reason);
                    }
                    UpdateMessage::RequestLayout => {
                        self.window_state.request_layout();
                    }
                    UpdateMessage::MarkViewLayoutDirty(id) => {
                        let _ = id.mark_view_layout_dirty();
                    }
                    UpdateMessage::RequestBoxTreeUpdate => {
                        self.window_state.request_box_tree_update();
                    }
                    UpdateMessage::RequestBoxTreeUpdateForView(view_id) => {
                        self.window_state.request_box_tree_update_for_view(view_id);
                    }
                    UpdateMessage::RequestBoxTreeCommit => {
                        self.window_state.request_box_tree_commit();
                    }
                    UpdateMessage::RequestPaint(id) => {
                        cx.window_state.request_paint(id);
                    }
                    UpdateMessage::Focus(id) => {
                        // because we do not call route, the processing messages event is not sent.
                        // this is desirable because the process messages event will be sent explicitly another time.
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
                        // because we do not call route, the processing messages event is not sent.
                        // this is desirable because the process messages event will be sent explicitly another time.
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
                        let event = ScrollTo { id, rect };
                        let event = Event::new_custom(event);
                        GlobalEventCx::new(&mut self.window_state, self.id.get_element_id(), event)
                            .route_normal(RouteKind::bubble_from(id), None);
                    }
                    UpdateMessage::State { id, state } => {
                        let view = id.view();
                        view.borrow_mut().update(&mut cx, state);
                    }
                    UpdateMessage::DragWindow => {
                        let _ = self.window.drag_window();
                    }
                    UpdateMessage::FocusWindow => {
                        self.window.focus_window();
                    }
                    UpdateMessage::DragResizeWindow(direction) => {
                        let _ = self.window.drag_resize_window(direction);
                    }
                    UpdateMessage::ToggleWindowMaximized => {
                        self.window.set_maximized(!self.window.is_maximized());
                    }
                    UpdateMessage::SetWindowMaximized(maximized) => {
                        self.window.set_maximized(maximized);
                    }
                    UpdateMessage::MinimizeWindow => {
                        self.window.set_minimized(true);
                    }
                    UpdateMessage::SetWindowDelta(delta) => {
                        let pos = self.window_position + delta;
                        self.window
                            .set_outer_position(winit::dpi::Position::Logical(
                                winit::dpi::LogicalPosition::new(pos.x, pos.y),
                            ));
                    }
                    UpdateMessage::WindowScale(scale) => {
                        cx.window_state.user_scale = scale;
                        cx.window_state
                            .update_default_theme(cx.window_state.light_dark_theme);
                        cx.window_state
                            .mark_style_dirty(cx.window_state.root_view_id.get_element_id());
                        let scale = cx.window_state.effective_scale();
                        let root_view_id = cx.window_state.root_view_id;
                        self.event(Event::Window(WindowEvent::ScaleChanged(scale)));
                        self.id.request_layout();
                        self.window_state.request_paint(root_view_id);
                        self.schedule_repaint();
                    }
                    UpdateMessage::ShowContextMenu { menu, pos } => {
                        let (menu, registry) = menu.build();
                        cx.window_state.context_menu.clear();
                        cx.window_state.update_context_menu(registry);

                        // Queue the context menu to show after this event completes
                        Application::send_proxy_event(UserEvent::ShowContextMenu {
                            window_id: self.window_id,
                            menu: MenuWrapper(menu),
                            pos,
                        });
                    }
                    UpdateMessage::WindowMenu { menu } => {
                        self.window_menu_actions.clear();
                        let (menu, registry) = menu.build();
                        self.window_menu_actions = registry;
                        #[cfg(target_os = "macos")]
                        {
                            menu.init_for_nsapp();
                        }
                        #[cfg(target_os = "windows")]
                        {
                            self.init_menu_for_windows(&menu);
                        }
                        self.window_menu = Some(menu);
                    }
                    UpdateMessage::SetWindowTitle { title } => {
                        self.window.set_title(&title);
                    }
                    UpdateMessage::SetImeAllowed { allowed } => {
                        if self.window.ime_capabilities().is_some() != allowed {
                            let ime = if allowed {
                                let position = LogicalPosition::new(0, 0);
                                let size = LogicalSize::new(0, 0);
                                let request_data = ImeRequestData::default()
                                    .with_cursor_area(position.into(), size.into())
                                    .with_hint_and_purpose(ImeHint::NONE, ImePurpose::Normal);

                                ImeRequest::Enable(
                                    ImeEnableRequest::new(
                                        ImeCapabilities::new()
                                            .with_hint_and_purpose()
                                            .with_cursor_area(),
                                        request_data,
                                    )
                                    .unwrap(),
                                )
                            } else {
                                ImeRequest::Disable
                            };

                            self.window.request_ime_update(ime).unwrap();
                        }
                    }
                    UpdateMessage::SetImeCursorArea { position, size } => {
                        if self
                            .window
                            .ime_capabilities()
                            .map(|caps| caps.cursor_area())
                            .unwrap_or(false)
                        {
                            let position =
                                winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(
                                    position.x * self.window_state.user_scale,
                                    position.y * self.window_state.user_scale,
                                ));
                            let size = winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
                                size.width * self.window_state.user_scale,
                                size.height * self.window_state.user_scale,
                            ));
                            self.window
                                .request_ime_update(ImeRequest::Update(
                                    ImeRequestData::default().with_cursor_area(position, size),
                                ))
                                .unwrap();
                        }
                    }
                    UpdateMessage::Inspect => {
                        inspector::capture(self.window_id);
                    }
                    UpdateMessage::CaptureMetalFrame => {
                        #[cfg(target_os = "macos")]
                        self.capture_next_metal_frame();
                    }
                    UpdateMessage::ToggleHud => {
                        self.toggle_hud();
                    }
                    UpdateMessage::AddOverlay { view } => {
                        self.id.add_child(view);
                        self.id.request_all();
                    }
                    UpdateMessage::RemoveOverlay { id } => {
                        cx.window_state.remove_view(id);
                        self.id.request_all();
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
                        self.window.set_visible(visible);
                    }
                    UpdateMessage::ViewTransitionAnimComplete(id) => {
                        let num_waiting =
                            id.state().borrow().num_waiting_animations.saturating_sub(1);
                        id.state().borrow_mut().num_waiting_animations = num_waiting;
                    }
                    UpdateMessage::SetTheme(theme) => {
                        self.set_theme(theme, false);

                        #[cfg(target_os = "windows")]
                        if let Some(new) = theme {
                            self.set_menu_theme_for_windows(new);
                        }
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
                        // Resolve scope by looking up parent's scope
                        let scope = parent_id.find_scope().unwrap_or_else(Scope::current);
                        let view = child.build(scope);
                        parent_id.add_child(view);
                        parent_id.request_all();
                    }
                    UpdateMessage::AddChildren {
                        parent_id,
                        mut children,
                    } => {
                        // Resolve scope by looking up parent's scope
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
                        let cx = GlobalEventCx::new(&mut self.window_state, id, *event);
                        cx.route_normal(dispatch_kind, triggered_by.as_deref());
                    }
                }
            }
        }
        // After all messages are processed, re-parent any scopes that couldn't find
        // a parent scope earlier (because the view tree wasn't fully assembled yet).
        crate::view::process_pending_scope_reparents();
        let root_element_id = self.window_state.root_view_id.get_element_id();
        let event = Event::Window(WindowEvent::UpdatePhase(
            UpdatePhaseEvent::ProcessingMessages,
        ));
        GlobalEventCx::new(&mut self.window_state, root_element_id, event).route_window_event();
    }

    fn process_deferred_update_messages(&mut self) {
        self.process_central_messages();
        let msgs = DEFERRED_UPDATE_MESSAGES
            .with(|msgs| msgs.borrow_mut().remove(&self.id).unwrap_or_default());
        let mut cx = UpdateCx {
            window_state: &mut self.window_state,
        };
        for (id, state) in msgs {
            let view = id.view();
            view.borrow_mut().update(&mut cx, state);
        }
    }

    fn needs_layout(&mut self) -> bool {
        self.window_state.needs_layout
    }

    fn needs_box_tree_commit(&mut self) -> bool {
        self.window_state.needs_box_tree_commit
            || self.window_state.box_tree.borrow().needs_commit()
    }

    fn needs_box_tree_update(&mut self) -> bool {
        self.window_state.needs_box_tree_from_layout
    }

    fn needs_style(&mut self) -> bool {
        !self.window_state.style_dirty.is_empty()
    }

    fn has_current_frame_prepare_work(&self) -> bool {
        !self.window_state.style_dirty.is_empty()
            || self.window_state.needs_layout
            || self.window_state.needs_box_tree_from_layout
            || self.window_state.needs_box_tree_commit
            || self.window_state.box_tree.borrow().needs_commit()
            || !self.window_state.views_needing_box_tree_update.is_empty()
            || self.has_deferred_update_messages()
    }

    fn has_deferred_update_messages(&self) -> bool {
        DEFERRED_UPDATE_MESSAGES.with(|m| {
            m.borrow()
                .get(&self.id)
                .map(|m| !m.is_empty())
                .unwrap_or(false)
        })
    }

    fn set_cursor(&mut self) {
        if self.window_state.needs_cursor_resolution {
            let mut temp = None;
            for hover in self.window_state.hover_state.current_path() {
                if hover.is_view()
                    && let Some(cursor) = hover.owning_id().state().borrow().cursor()
                {
                    temp = Some(cursor);
                }
                // it is important that the node cursors override the widget cursor because non View nodes will have a widget that maps to the parent View that they are associated with
                if let Some(cursor) = self.window_state.element_id_cursors.get(hover) {
                    temp = Some(*cursor);
                }
            }
            self.window_state.needs_cursor_resolution = false;
            self.window_state.cursor = temp;
        }
        let cursor = match self.window_state.cursor {
            Some(CursorStyle::Default) => CursorIcon::Default,
            Some(CursorStyle::Pointer) => CursorIcon::Pointer,
            Some(CursorStyle::Progress) => CursorIcon::Progress,
            Some(CursorStyle::Wait) => CursorIcon::Wait,
            Some(CursorStyle::Crosshair) => CursorIcon::Crosshair,
            Some(CursorStyle::Text) => CursorIcon::Text,
            Some(CursorStyle::Move) => CursorIcon::Move,
            Some(CursorStyle::Grab) => CursorIcon::Grab,
            Some(CursorStyle::Grabbing) => CursorIcon::Grabbing,
            Some(CursorStyle::ColResize) => CursorIcon::ColResize,
            Some(CursorStyle::RowResize) => CursorIcon::RowResize,
            Some(CursorStyle::WResize) => CursorIcon::WResize,
            Some(CursorStyle::EResize) => CursorIcon::EResize,
            Some(CursorStyle::NwResize) => CursorIcon::NwResize,
            Some(CursorStyle::NeResize) => CursorIcon::NeResize,
            Some(CursorStyle::SwResize) => CursorIcon::SwResize,
            Some(CursorStyle::SeResize) => CursorIcon::SeResize,
            Some(CursorStyle::SResize) => CursorIcon::SResize,
            Some(CursorStyle::NResize) => CursorIcon::NResize,
            Some(CursorStyle::NeswResize) => CursorIcon::NeswResize,
            Some(CursorStyle::NwseResize) => CursorIcon::NwseResize,
            None => CursorIcon::Default,
        };
        if cursor != self.window_state.last_cursor_icon {
            self.window.set_cursor(cursor.into());
            self.window_state.last_cursor_icon = cursor;
        }
    }

    fn schedule_repaint(&self) {
        Application::request_update();
    }

    pub(crate) fn destroy(&mut self) {
        self.event(Event::Window(WindowEvent::Closed));
        self.scope.dispose();
        remove_window_id_mapping(&self.id, &self.window_id);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn show_context_menu(&self, menu: MudaMenu, pos: Option<Point>) {
        use dispatch2::DispatchQueue;
        use muda::{
            ContextMenu,
            dpi::{LogicalPosition, Position},
        };
        use raw_window_handle::HasWindowHandle;
        use raw_window_handle::RawWindowHandle;

        if let RawWindowHandle::AppKit(handle) = self.window.window_handle().unwrap().as_raw() {
            let ns_view = handle.ns_view.as_ptr() as usize;
            let scale = self.window_state.user_scale;
            let height = self.size.get_untracked().height;
            let logical_pos = pos.map(|pos| (pos.x * scale, (height - pos.y) * scale));

            struct SendMenu(MudaMenu);
            unsafe impl Send for SendMenu {}
            impl SendMenu {
                unsafe fn show(self, ns_view: usize, logical_pos: Option<(f64, f64)>) {
                    unsafe {
                        self.0.show_context_menu_for_nsview(
                            ns_view as _,
                            logical_pos.map(|(x, y)| Position::Logical(LogicalPosition::new(x, y))),
                        );
                    }
                }
            }

            let menu = SendMenu(menu);
            DispatchQueue::main().exec_async(move || {
                unsafe {
                    menu.show(ns_view, logical_pos);
                };
            });
        }
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn show_context_menu(&self, menu: MudaMenu, pos: Option<Point>) {
        use muda::{
            ContextMenu,
            dpi::{LogicalPosition, Position},
        };
        use raw_window_handle::HasWindowHandle;
        use raw_window_handle::RawWindowHandle;

        if let RawWindowHandle::Win32(handle) = self.window.window_handle().unwrap().as_raw() {
            unsafe {
                menu.show_context_menu_for_hwnd(
                    isize::from(handle.hwnd),
                    pos.map(|pos| {
                        Position::Logical(LogicalPosition::new(
                            pos.x * self.window_state.user_scale,
                            pos.y * self.window_state.user_scale,
                        ))
                    }),
                );
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn init_menu_for_windows(&self, menu: &MudaMenu) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        if let RawWindowHandle::Win32(handle) = self.window.window_handle().unwrap().as_raw() {
            unsafe {
                let menu_theme = match (
                    self.window_state.theme_overriden,
                    self.window_state.light_dark_theme,
                ) {
                    (false, winit::window::Theme::Light) => MudaMenuTheme::Light,
                    (false, winit::window::Theme::Dark) => MudaMenuTheme::Dark,
                    (true, winit::window::Theme::Light) => MudaMenuTheme::Light,
                    (true, winit::window::Theme::Dark) => MudaMenuTheme::Dark,
                };
                let _ = menu.init_for_hwnd_with_theme(isize::from(handle.hwnd), menu_theme);
                let _ = menu.show_for_hwnd(isize::from(handle.hwnd));
            }
        }
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn set_menu_theme_for_windows(&self, theme: winit::window::Theme) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        if let RawWindowHandle::Win32(handle) = self.window.window_handle().unwrap().as_raw()
            && let Some(menu) = &self.window_menu
        {
            unsafe {
                let menu_theme = match theme {
                    winit::window::Theme::Light => MudaMenuTheme::Light,
                    winit::window::Theme::Dark => MudaMenuTheme::Dark,
                };
                let _ = menu.set_theme_for_hwnd(handle.hwnd.into(), menu_theme);
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
    pub(crate) fn show_context_menu(&self, menu: MudaMenu, pos: Option<Point>) {
        let pos = pos.unwrap_or(self.window_state.last_pointer.0);
        let pos = Point::new(
            pos.x / self.window_state.user_scale,
            pos.y / self.window_state.user_scale,
        );
        self.context_menu.set(Some((menu, pos, false)));
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn menu_action(&mut self, id: &MenuId) {
        set_current_view(self.id);
        if let Some(action) = self.window_state.context_menu.get(id) {
            (*action)();
            self.process_update_messages_only();
            self.refresh_frame_activity();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn menu_action(&mut self, id: &MenuId) {
        set_current_view(self.id);
        if let Some(action) = self.window_state.context_menu.get(id) {
            (*action)();
            self.process_update_messages_only();
            self.refresh_frame_activity();
        } else if let Some(action) = self.window_menu_actions.get(id) {
            (*action)();
            self.process_update_messages_only();
            self.refresh_frame_activity();
        }
    }

    pub(crate) fn ime(&mut self, ime: Ime) {
        let floem_ime = match ime {
            Ime::Enabled => ImeEvent::Enabled,
            Ime::Preedit(text, cursor) => ImeEvent::Preedit { text, cursor },
            Ime::Commit(text) => ImeEvent::Commit(text),
            Ime::Disabled => ImeEvent::Disabled,
            Ime::DeleteSurrounding {
                before_bytes,
                after_bytes,
            } => ImeEvent::DeleteSurrounding {
                before_bytes,
                after_bytes,
            },
        };
        self.event(Event::Ime(floem_ime));
    }

    pub(crate) fn modifiers_changed(&mut self, modifiers: Modifiers) {
        let is_altgr = self.modifiers.contains(Modifiers::ALT_GRAPH);
        let mut modifiers: Modifiers = modifiers;
        if is_altgr {
            modifiers.set(Modifiers::ALT_GRAPH, true);
        }
        self.modifiers = modifiers;
    }

    /// Clean up the window's view tree and reactive scope.
    ///
    /// This removes all views from VIEW_STORAGE and disposes the reactive scope,
    /// ensuring proper cleanup for test isolation.
    pub(crate) fn cleanup(&mut self) {
        // Dispose the reactive scope FIRST to clean up effects.
        // This stops any reactive effects from running during cleanup.
        self.scope.dispose();

        // Clear ALL message queues to prevent stale messages from affecting
        // future windows that might reuse the same ViewId slots.
        // We clear all messages, not just those for views we're removing,
        // because the reactive scope disposal above might have queued new messages.
        CENTRAL_UPDATE_MESSAGES.with_borrow_mut(|msgs| {
            msgs.clear();
        });
        UPDATE_MESSAGES.with_borrow_mut(|msgs| {
            msgs.clear();
        });
        CENTRAL_DEFERRED_UPDATE_MESSAGES.with_borrow_mut(|msgs| {
            msgs.clear();
        });
        DEFERRED_UPDATE_MESSAGES.with_borrow_mut(|msgs| {
            msgs.clear();
        });

        // Remove all views starting from the root
        self.window_state.remove_view(self.id);

        // Clear all caches that might hold stale ViewId references.
        // This is crucial for test isolation when tests run on the same thread.
        clear_hit_test_cache();

        // Remove the window from the global window tracking map.
        // This is crucial for test isolation - if not done, the old root ViewId
        // will still be considered a "known root" when the ViewId slot is reused.
        remove_window_id_mapping(&self.id, &self.window_id);
    }
}

pub(crate) fn get_current_view() -> ViewId {
    CURRENT_RUNNING_VIEW_HANDLE
        .with(|running| *running.borrow())
        .expect("view id must have been set before getting")
}
/// Set this view handle to the current running view handle
pub(crate) fn set_current_view(id: ViewId) {
    CURRENT_RUNNING_VIEW_HANDLE.with(|running| {
        *running.borrow_mut() = Some(id);
    });
}

/// A view representing a window which manages the main window view and any overlays.
struct WindowView {
    id: ViewId,
}

impl View for WindowView {
    fn id(&self) -> ViewId {
        self.id
    }

    fn view_style(&self) -> Option<crate::style::Style> {
        Some(Style::new().width_full().height_full())
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Window".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{Event, WindowEvent, listener},
        view::HasViewId,
        views::{Decorators, Empty, Stack},
    };
    use std::{cell::Cell, rc::Rc};

    /// Test that we can create a headless WindowHandle.
    #[test]
    fn test_headless_window_handle_creation() {
        let root_id = ViewId::new_root();
        set_current_view(root_id);
        let view = Empty::new().style(|s| s.size(100.0, 100.0));
        let window_handle = WindowHandle::new_headless(root_id, view, Size::new(800.0, 600.0), 1.0);

        // Just verify creation doesn't panic
        assert!(window_handle.window_state.os_scale > 0.0);
    }

    /// Test that headless WindowHandle can dispatch events.
    #[test]
    fn test_headless_event_dispatch() {
        use ui_events::pointer::{
            PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerType,
        };

        let root_id = ViewId::new_root();
        set_current_view(root_id);
        let view = Empty::new().style(|s| s.size(100.0, 100.0));
        let mut window_handle =
            WindowHandle::new_headless(root_id, view, Size::new(800.0, 600.0), 1.0);

        // Create a pointer down event
        let event = Event::Pointer(PointerEvent::Down(PointerButtonEvent {
            state: ui_events::pointer::PointerState {
                position: dpi::PhysicalPosition::new(50.0, 50.0),
                count: 1,
                ..Default::default()
            },
            button: Some(PointerButton::Primary),
            pointer: PointerInfo {
                pointer_id: Some(PointerId::PRIMARY),
                persistent_device_id: None,
                pointer_type: PointerType::Mouse,
            },
        }));

        // Dispatch should not panic
        window_handle.event(event);
    }

    /// Test that headless WindowHandle runs process_update correctly.
    #[test]
    fn test_headless_process_update() {
        use ui_events::pointer::{
            PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerType,
        };

        let root_id = ViewId::new_root();
        set_current_view(root_id);
        let view = Empty::new().style(|s| s.size(100.0, 100.0));
        let mut window_handle =
            WindowHandle::new_headless(root_id, view, Size::new(800.0, 600.0), 1.0);

        // Dispatch pointer down
        window_handle.event(Event::Pointer(PointerEvent::Down(PointerButtonEvent {
            state: ui_events::pointer::PointerState {
                position: dpi::PhysicalPosition::new(50.0, 50.0),
                count: 1,
                ..Default::default()
            },
            button: Some(PointerButton::Primary),
            pointer: PointerInfo {
                pointer_id: Some(PointerId::PRIMARY),
                persistent_device_id: None,
                pointer_type: PointerType::Mouse,
            },
        })));

        // Dispatch pointer up
        window_handle.event(Event::Pointer(PointerEvent::Up(PointerButtonEvent {
            state: ui_events::pointer::PointerState {
                position: dpi::PhysicalPosition::new(50.0, 50.0),
                count: 1,
                ..Default::default()
            },
            button: Some(PointerButton::Primary),
            pointer: PointerInfo {
                pointer_id: Some(PointerId::PRIMARY),
                persistent_device_id: None,
                pointer_type: PointerType::Mouse,
            },
        })));

        // All should complete without panic
    }

    #[test]
    fn test_headless_window_destroy_emits_window_closed() {
        let root_id = ViewId::new_root();
        set_current_view(root_id);

        let closed_count = Rc::new(Cell::new(0));
        let closed_count_for_listener = closed_count.clone();

        let view = Empty::new().style(|s| s.size(100.0, 100.0)).on_event_cont(
            listener::WindowClosed,
            move |_cx, _| {
                closed_count_for_listener.set(closed_count_for_listener.get() + 1);
            },
        );

        let mut window_handle =
            WindowHandle::new_headless(root_id, view, Size::new(800.0, 600.0), 1.0);

        window_handle.event(Event::Window(WindowEvent::CloseRequested));
        assert_eq!(closed_count.get(), 0);

        window_handle.destroy();

        assert_eq!(closed_count.get(), 1);
    }

    #[test]
    fn test_budgeted_update_quiesces_with_unreachable_style_dirty_view() {
        let root_id = ViewId::new_root();
        set_current_view(root_id);
        let view = Empty::new().style(|s| s.size(100.0, 100.0));
        let mut window_handle =
            WindowHandle::new_headless(root_id, view, Size::new(800.0, 600.0), 1.0);

        // Create a view ID that belongs to this root but is not in the tree.
        let orphan = ViewId::new();
        window_handle
            .window_state
            .mark_style_dirty(orphan.get_element_id());

        // Must quiesce immediately instead of repeatedly trying to style an unreachable view.
        let quiescent =
            window_handle.process_update_budgeted(Instant::now(), Duration::from_millis(10));
        assert!(
            quiescent,
            "process_update_budgeted should quiesce when style dirty contains unreachable views"
        );
        assert!(
            window_handle.window_state.style_dirty.is_empty(),
            "unreachable style dirty entries should be drained"
        );
    }

    #[test]
    fn test_user_window_scale_requests_paint_and_emits_scale_changed() {
        let root_id = ViewId::new_root();
        set_current_view(root_id);

        let observed_scale = Rc::new(Cell::new(0.0));
        let observed_scale_for_listener = observed_scale.clone();

        let view = Empty::new().style(|s| s.size(100.0, 100.0)).on_event_cont(
            listener::WindowScaleChanged,
            move |_cx, scale| {
                observed_scale_for_listener.set(*scale);
            },
        );

        let mut window_handle =
            WindowHandle::new_headless(root_id, view, Size::new(800.0, 600.0), 1.0);
        window_handle.window_state.clear_pending_paint();

        crate::action::set_window_scale(1.5);
        window_handle.process_update_no_paint();

        assert_eq!(window_handle.window_state.user_scale, 1.5);
        assert_eq!(window_handle.window_state.effective_scale(), 1.5);
        assert_eq!(observed_scale.get(), 1.5);
        assert!(window_handle.window_state.has_pending_paint());
    }

    #[test]
    fn test_box_tree_damage_queues_pending_render_without_paint_dirty() {
        let root_id = ViewId::new_root();
        set_current_view(root_id);

        let view = Empty::new().style(|s| s.size(100.0, 100.0));
        let element_id = view.view_id().get_element_id();

        let mut window_handle =
            WindowHandle::new_headless(root_id, view, Size::new(800.0, 600.0), 1.0);

        // Settle the initial layout/box tree state, then clear any bootstrapping paint/render work.
        window_handle.process_update_no_paint();
        window_handle.window_state.clear_pending_paint();
        window_handle.window_state.clear_pending_damage();

        {
            let mut box_tree = window_handle.window_state.box_tree.borrow_mut();
            box_tree.set_world_position(element_id.0, Some(Point::new(25.0, 0.0)));
        }
        window_handle.window_state.needs_box_tree_commit = true;

        window_handle.process_update_no_paint();

        assert!(
            !window_handle.window_state.has_pending_paint(),
            "pure box-tree damage should not require explicit paint dirtiness"
        );
        assert!(
            !window_handle.window_state.pending_damage_rects.is_empty(),
            "box-tree commit damage should be retained as pending render work"
        );
        assert!(
            window_handle.window_state.has_pending_render(),
            "pending box-tree damage alone should keep render submission active"
        );
    }

    #[test]
    fn test_layout_recompute_without_layout_diff_does_not_dirty_paint_tree() {
        let root_id = ViewId::new_root();
        set_current_view(root_id);

        let first = Empty::new().style(|s| s.size(100.0, 100.0));
        let first_id = first.view_id().get_element_id();
        let second = Empty::new().style(|s| s.size(100.0, 100.0));
        let second_id = second.view_id().get_element_id();
        let view = Stack::new((first, second)).style(|s| s.size(300.0, 100.0));

        let mut window_handle =
            WindowHandle::new_headless(root_id, view, Size::new(800.0, 600.0), 1.0);
        window_handle.process_update_no_paint();
        window_handle.window_state.clear_pending_paint();
        window_handle.window_state.clear_pending_damage();

        window_handle.window_state.request_layout();
        window_handle.process_update_no_paint();

        assert!(
            !window_handle
                .window_state
                .dirty_paint_elements
                .contains(&first_id),
            "unchanged first child should not be paint-dirtied by a no-op layout recompute"
        );
        assert!(
            !window_handle
                .window_state
                .dirty_paint_elements
                .contains(&second_id),
            "unchanged second child should not be paint-dirtied by a no-op layout recompute"
        );
    }

    #[test]
    fn test_frame_tick_keeps_animation_progress_alive_without_render_damage() {
        fn test_tick(frame_index: u64) -> subduction_core::timing::FrameTick {
            use subduction_core::{
                output::OutputId,
                time::{Duration as HostDuration, HostTime, Timebase},
                timing::TimingConfidence,
            };
            let now = HostTime::from_nanos(frame_index.saturating_mul(16_666_667), Timebase::NANOS);
            let interval = HostDuration::from_nanos(16_666_667, Timebase::NANOS);
            subduction_core::timing::FrameTick {
                now,
                predicted_present: now.checked_add(interval),
                refresh_interval: Some(interval.0),
                confidence: TimingConfidence::Estimated,
                frame_index,
                output: OutputId(0),
                prev_actual_present: None,
            }
        }

        let root_id = ViewId::new_root();
        set_current_view(root_id);

        let view = Empty::new().style(|s| s.size(100.0, 100.0));
        let mut window_handle =
            WindowHandle::new_headless(root_id, view, Size::new(800.0, 600.0), 1.0);

        window_handle.process_update_no_paint();
        window_handle.window_state.clear_pending_paint();
        window_handle.window_state.clear_pending_damage();

        let runs = Rc::new(Cell::new(0));
        let runs_for_callback = runs.clone();
        window_handle
            .window_state
            .begin_frame_callbacks
            .push(Box::new(move |_| {
                runs_for_callback.set(runs_for_callback.get() + 1);
            }));

        window_handle.process_frame_tick(test_tick(1));
        assert_eq!(runs.get(), 1, "first frame tick should run callback");

        let runs_for_second_callback = runs.clone();
        window_handle
            .window_state
            .begin_frame_callbacks
            .push(Box::new(move |_| {
                runs_for_second_callback.set(runs_for_second_callback.get() + 1);
            }));

        window_handle.process_frame_tick(test_tick(2));

        assert_eq!(
            runs.get(),
            2,
            "second frame tick should still advance deferred begin-frame work"
        );
    }
}
