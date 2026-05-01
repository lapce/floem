use crate::{
    Application,
    app::UserEvent,
    frame::{
        DisplayTiming, FrameDemand, FrameTime, FrameTimingFeedback, PresentPacing,
        PresentationInterval,
    },
    platform::{Duration, Instant},
};
use winit::window::{Window as WinitWindow, WindowId};

#[cfg(not(target_arch = "wasm32"))]
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
};
#[cfg(target_os = "macos")]
use subduction_backend_apple::{
    DisplayLinkLayer, DisplayLinkThread as CaDisplayLinkThread, DisplayLinkView,
    now as subduction_now, timebase,
};
#[cfg(target_os = "windows")]
use subduction_backend_windows::{now as windows_now, timebase as windows_timebase};
use subduction_core::{
    output::OutputId,
    time::{Duration as HostDuration, HostTime, Timebase},
    timing::{FrameTick, TimingConfidence},
};
use understory_frame_pacing::{
    DisplayTiming as PacingDisplayTiming, Duration as PacingDuration,
    FrameDemand as PacingFrameDemand, FrameOpportunity as PacingFrameOpportunity,
    FramePacingDecision, FrameTimingEstimate as PacingFrameTimingEstimate,
    Presentation as PacingPresentation, Time as PacingTime, plan_frame as pacing_plan_frame,
};
pub(crate) trait FrameClock {
    fn frame_interval(&self, window: &dyn WinitWindow) -> Duration {
        window_frame_interval(window)
    }

    fn current_frame_time(
        &self,
        window: &dyn WinitWindow,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime;
    fn current_external_frame_time(
        &mut self,
        window: &dyn WinitWindow,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
        self.current_frame_time(window, now, background_rendering)
    }
    fn note_begin_frame_callbacks_ran(&mut self);
    fn refresh_schedule(&mut self, _window: &dyn WinitWindow, _now: Instant) {}
    fn note_frame_prepare_started(&mut self, now: Instant);
    fn set_frame_demand(&mut self, _demand: FrameDemand) {}
    fn set_frame_prepared(&mut self, prepared: bool);
    fn needs_frame_prepare(&self, has_next_frame_work: bool) -> bool;
    fn should_defer_scene_work(&self, _now: Instant) -> Option<Instant> {
        None
    }
    fn current_submit_deadline(&self, window: &dyn WinitWindow, now: Instant) -> Instant {
        self.current_frame_time(window, now, false)
            .interval
            .deadline_max
    }
    fn observe_presented(
        &mut self,
        feedback: FrameTimingFeedback,
        submitted_at: Instant,
        presented_at: Instant,
    );
    fn set_active(&mut self, _active: bool) {}
    fn receive_frame_tick(&mut self, _tick: FrameTick) {}
    #[cfg(target_os = "macos")]
    fn set_native_display_id(&mut self, _display_id: Option<u32>) {}
    #[cfg(target_os = "macos")]
    fn set_metal_display_link_layer(&mut self, _layer: Option<DisplayLinkLayer>) {}
    #[cfg(target_os = "macos")]
    fn set_display_link_view(&mut self, _view: Option<DisplayLinkView>) {}
    #[cfg(target_os = "macos")]
    fn set_prefers_metal_display_link(&mut self, _prefers_metal: bool) {}
}

fn force_heuristic_frame_clock_requested() -> bool {
    std::env::var("FLOEM_FORCE_HEURISTIC_FRAME_CLOCK")
        .ok()
        .is_some_and(|value| value.as_str() == "1")
}

pub(crate) fn new_window_frame_clock(window_id: WindowId, output_id: u32) -> Box<dyn FrameClock> {
    if force_heuristic_frame_clock_requested() {
        return Box::new(HeuristicFrameClock::new(window_id, OutputId(output_id)));
    }

    #[cfg(target_os = "macos")]
    {
        Box::new(SubductionFrameClock::new(window_id, OutputId(output_id)))
    }

    #[cfg(target_os = "windows")]
    {
        Box::new(WindowsSubductionFrameClock::new(
            window_id,
            OutputId(output_id),
        ))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Box::new(HeuristicFrameClock::new(window_id, OutputId(output_id)))
    }
}

fn max_duration(a: Duration, b: Duration) -> Duration {
    if a >= b { a } else { b }
}

fn window_frame_interval(window: &dyn WinitWindow) -> Duration {
    window
        .current_monitor()
        .and_then(|monitor| monitor.current_video_mode())
        .and_then(|mode| mode.refresh_rate_millihertz())
        .map(|mhz| Duration::from_nanos(1_000_000_000_000 / mhz.get() as u64))
        .unwrap_or(Duration::from_millis(16))
}

pub(crate) fn frame_pacing_diag_enabled() -> bool {
    std::env::var_os("FLOEM_FRAME_PACING_DIAG").is_some()
}

const DEFAULT_SYNTHETIC_FRAME_INTERVAL_NS: u64 = 16_666_667;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
struct FrameTickDriver {
    running: Arc<AtomicBool>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for FrameTickDriver {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_frame_tick_driver(
    name: &'static str,
    window_id: WindowId,
    output: OutputId,
    timebase: Timebase,
    interval_ns: Arc<AtomicU64>,
    mut now: impl FnMut() -> HostTime + Send + 'static,
) -> FrameTickDriver {
    let running = Arc::new(AtomicBool::new(true));
    let thread_running = running.clone();
    let _ = thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            let mut frame_index = 0_u64;
            while thread_running.load(Ordering::Acquire) {
                let refresh_ns = interval_ns.load(Ordering::Relaxed).max(1_000_000);
                let refresh = HostDuration::from_nanos(refresh_ns, timebase);
                let now = now();
                let tick = FrameTick {
                    now,
                    predicted_present: now.checked_add(refresh),
                    refresh_interval: Some(refresh.0),
                    confidence: TimingConfidence::Estimated,
                    frame_index,
                    output,
                    prev_actual_present: None,
                };
                Application::send_proxy_event(UserEvent::FrameTick { window_id, tick });
                frame_index = frame_index.saturating_add(1);
                thread::sleep(Duration::from_nanos(refresh_ns));
            }
        });
    FrameTickDriver { running }
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn smooth_work_estimate(
    current: Duration,
    observed: Option<Duration>,
    minimum: Duration,
) -> Duration {
    let Some(observed) = observed else {
        return current;
    };
    let observed = observed.max(minimum);
    if observed >= current {
        observed
    } else {
        (current * 7 + observed) / 8
    }
}

#[derive(Debug)]
pub(crate) struct HeuristicFrameClock {
    window_id: Option<WindowId>,
    output: OutputId,
    origin: Instant,
    last_presented_at: Instant,
    estimated_draw_lead_time: Duration,
    estimated_present_lead_time: Duration,
    frame_counter: u64,
    latest_tick: Option<FrameTick>,
    latest_effective_interval: Option<HostDuration>,
    #[cfg(not(target_arch = "wasm32"))]
    tick_driver: Option<FrameTickDriver>,
    #[cfg(not(target_arch = "wasm32"))]
    tick_interval_ns: Arc<AtomicU64>,
    frame_prepared: bool,
    demand: FrameDemand,
    estimate: PacingEstimate,
}

impl Default for HeuristicFrameClock {
    fn default() -> Self {
        Self::new_inner(None, OutputId(0))
    }
}

impl HeuristicFrameClock {
    fn new(window_id: WindowId, output: OutputId) -> Self {
        Self::new_inner(Some(window_id), output)
    }

    fn new_inner(window_id: Option<WindowId>, output: OutputId) -> Self {
        let now = Instant::now();
        Self {
            window_id,
            output,
            origin: now,
            last_presented_at: now,
            estimated_draw_lead_time: Duration::from_millis(1),
            estimated_present_lead_time: Duration::from_millis(1),
            frame_counter: 0,
            latest_tick: None,
            latest_effective_interval: None,
            #[cfg(not(target_arch = "wasm32"))]
            tick_driver: None,
            #[cfg(not(target_arch = "wasm32"))]
            tick_interval_ns: Arc::new(AtomicU64::new(DEFAULT_SYNTHETIC_FRAME_INTERVAL_NS)),
            frame_prepared: false,
            demand: FrameDemand::empty(),
            estimate: PacingEstimate::default(),
        }
    }
}

impl FrameClock for HeuristicFrameClock {
    fn frame_interval(&self, window: &dyn WinitWindow) -> Duration {
        let interval = self
            .latest_effective_interval
            .map(|duration| Duration::from_nanos(duration.to_nanos(Timebase::NANOS)))
            .unwrap_or_else(|| window_frame_interval(window));
        #[cfg(not(target_arch = "wasm32"))]
        self.tick_interval_ns.store(
            interval.as_nanos().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
        interval
    }

    fn current_frame_time(
        &self,
        window: &dyn WinitWindow,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
        let frame_interval = self.frame_interval(window);
        if let Some(tick) = self.latest_tick
            && let Some(predicted_present) = tick.predicted_present
        {
            let semantic_time = self.host_to_instant(predicted_present);
            return FrameTime {
                now: semantic_time,
                interval: PresentationInterval {
                    deadline_min: now,
                    deadline_max: semantic_time,
                    predicted_present: Some(semantic_time),
                    display_timing: DisplayTiming::fixed(frame_interval),
                    present_pacing: PresentPacing::AtTime(semantic_time),
                    background_rendering,
                },
                frame_interval,
                frame_index: tick.frame_index,
            };
        }
        let plan = self.plan_frame(frame_interval, now);
        let semantic_time = plan.target_present_time;
        FrameTime {
            now: semantic_time,
            interval: PresentationInterval {
                deadline_min: now,
                deadline_max: semantic_time,
                predicted_present: Some(semantic_time),
                display_timing: DisplayTiming::fixed(frame_interval),
                present_pacing: self.present_pacing_from_plan(plan),
                background_rendering,
            },
            frame_interval,
            frame_index: self.frame_counter,
        }
    }

    fn note_begin_frame_callbacks_ran(&mut self) {
        self.frame_counter = self.frame_counter.saturating_add(1);
    }

    fn refresh_schedule(&mut self, _window: &dyn WinitWindow, _now: Instant) {}

    fn note_frame_prepare_started(&mut self, _now: Instant) {}

    fn set_frame_demand(&mut self, demand: FrameDemand) {
        self.demand = demand;
    }

    fn set_frame_prepared(&mut self, prepared: bool) {
        self.frame_prepared = prepared;
    }

    fn receive_frame_tick(&mut self, tick: FrameTick) {
        self.latest_effective_interval = tick.refresh_interval.map(HostDuration);
        self.frame_counter = tick.frame_index;
        self.latest_tick = Some(tick);
    }

    fn needs_frame_prepare(&self, has_next_frame_work: bool) -> bool {
        !self.frame_prepared && has_next_frame_work
    }

    fn observe_presented(
        &mut self,
        feedback: FrameTimingFeedback,
        _submitted_at: Instant,
        presented_at: Instant,
    ) {
        let render_cpu = feedback.render_cpu.unwrap_or_default();
        let present_cpu = feedback.present_cpu.unwrap_or(render_cpu);
        self.update_draw_lead_estimate(render_cpu);
        self.estimate.pre_surface_work = self.estimated_draw_lead_time;
        // Surface acquisition may block until the swapchain actually becomes
        // available. If we learn that blocked time into the "ready to present"
        // lead estimate, we wake earlier next frame and recreate the same
        // stall. Learn only the non-blocking present CPU cost here.
        self.update_present_lead_estimate(present_cpu);
        self.estimate.surface_work = self.estimated_present_lead_time;
        self.last_presented_at = presented_at;
    }

    fn set_active(&mut self, active: bool) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if !active {
                self.tick_driver = None;
                return;
            }
            if self.tick_driver.is_some() {
                return;
            }
            let Some(window_id) = self.window_id else {
                return;
            };
            let origin = self.origin;
            let interval_ns = self.tick_interval_ns.clone();
            self.tick_driver = Some(spawn_frame_tick_driver(
                "floem-heuristic-frame-clock",
                window_id,
                self.output,
                Timebase::NANOS,
                interval_ns,
                move || {
                    HostTime::from_nanos(
                        origin.elapsed().as_nanos().min(u64::MAX as u128) as u64,
                        Timebase::NANOS,
                    )
                },
            ));
        }
    }
}

impl HeuristicFrameClock {
    fn host_to_instant(&self, host: HostTime) -> Instant {
        self.origin + Duration::from_nanos(host.to_nanos(Timebase::NANOS))
    }

    fn plan_frame(&self, frame_interval: Duration, now: Instant) -> HeuristicPacingPlan {
        let now_time = self.instant_to_pacing_time(now);
        let predicted_present_time =
            self.instant_to_pacing_time(now.checked_add(frame_interval).unwrap_or(now));
        let decision = pacing_plan_frame(
            PacingDisplayTiming::fixed(pacing_duration(frame_interval)),
            pacing_estimate(self.estimate),
            pacing_demand(self.demand),
            PacingFrameOpportunity {
                now: now_time,
                predicted_present_time: Some(predicted_present_time),
                frame_interval: Some(pacing_duration(frame_interval)),
                last_present_time: Some(self.instant_to_pacing_time(self.last_presented_at)),
                pending_target_present_time: None,
            },
        );
        HeuristicPacingPlan {
            target_present_time: self.pacing_time_to_instant(decision.target_present_time),
            presentation: decision.presentation,
        }
    }

    fn instant_to_pacing_time(&self, instant: Instant) -> PacingTime {
        let nanos = instant
            .checked_duration_since(self.origin)
            .unwrap_or(Duration::ZERO)
            .as_nanos()
            .min(i64::MAX as u128) as i64;
        PacingTime::from_nanos(nanos)
    }

    fn pacing_time_to_instant(&self, time: PacingTime) -> Instant {
        self.origin + platform_time_offset(time)
    }

    fn present_pacing_from_plan(&self, plan: HeuristicPacingPlan) -> PresentPacing {
        match plan.presentation {
            PacingPresentation::AsSoonAsReady => PresentPacing::AsSoonAsPossible,
            PacingPresentation::At(_) => PresentPacing::AtTime(plan.target_present_time),
            PacingPresentation::AfterMinimumDuration(duration) => {
                PresentPacing::AfterMinimumDuration(platform_duration(duration.as_nanos()))
            }
        }
    }

    fn update_draw_lead_estimate(&mut self, observed_cpu_time: Duration) {
        let target = observed_cpu_time + Duration::from_micros(500);
        self.estimated_draw_lead_time = max_duration(self.estimated_draw_lead_time, target);
        self.estimated_draw_lead_time = (self.estimated_draw_lead_time * 7 + target) / 8;
    }

    fn update_present_lead_estimate(&mut self, observed_cpu_time: Duration) {
        let target = observed_cpu_time + Duration::from_micros(500);
        self.estimated_present_lead_time = max_duration(self.estimated_present_lead_time, target);
        self.estimated_present_lead_time = (self.estimated_present_lead_time * 7 + target) / 8;
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[derive(Clone, Copy, Debug)]
struct ActivePacingPlan {
    semantic_time: HostTime,
    present_time: HostTime,
    submit_deadline: HostTime,
    present_pacing: PresentPacing,
    frame_index: u64,
}

#[derive(Clone, Copy, Debug)]
struct PacingEstimate {
    pre_surface_work: Duration,
    surface_work: Duration,
    gpu_work: Duration,
    safety_margin: Duration,
}

impl Default for PacingEstimate {
    fn default() -> Self {
        Self {
            pre_surface_work: Duration::from_millis(1),
            surface_work: Duration::from_millis(1),
            gpu_work: Duration::ZERO,
            safety_margin: Duration::from_micros(500),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct HeuristicPacingPlan {
    target_present_time: Instant,
    presentation: PacingPresentation,
}

fn pacing_duration(duration: Duration) -> PacingDuration {
    PacingDuration::from_nanos(duration.as_nanos().min(u64::MAX as u128) as u64)
}

fn platform_time_offset(time: PacingTime) -> Duration {
    Duration::from_nanos(time.as_nanos().max(0) as u64)
}

fn platform_duration(nanos: u64) -> Duration {
    Duration::from_nanos(nanos)
}

fn pacing_estimate(estimate: PacingEstimate) -> PacingFrameTimingEstimate {
    PacingFrameTimingEstimate {
        pre_surface_work: pacing_duration(estimate.pre_surface_work),
        surface_work: pacing_duration(estimate.surface_work),
        gpu_work: pacing_duration(estimate.gpu_work),
        safety_margin: pacing_duration(estimate.safety_margin),
    }
}

fn pacing_demand(demand: FrameDemand) -> PacingFrameDemand {
    if demand.contains(FrameDemand::DISCRETE_INPUT) {
        PacingFrameDemand::Input
    } else {
        PacingFrameDemand::Animation
    }
}

fn is_frame_cadenced_demand(demand: FrameDemand) -> bool {
    !demand.contains(FrameDemand::DISCRETE_INPUT)
        && demand.intersects(
            FrameDemand::ANIMATION | FrameDemand::CONTINUOUS_INPUT | FrameDemand::EXTERNAL_SURFACE,
        )
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[derive(Debug)]
struct SubductionPlanState {
    heuristic: HeuristicFrameClock,
    output: OutputId,
    timebase: Timebase,
    host_origin: HostTime,
    instant_origin: Instant,
    latest_tick: Option<FrameTick>,
    latest_plan: Option<ActivePacingPlan>,
    latest_effective_interval: Option<HostDuration>,
    previous_predicted_present: Option<HostTime>,
    latest_prepare_start: Option<HostTime>,
    demand: FrameDemand,
    estimate: PacingEstimate,
    last_present_time: Option<HostTime>,
    pending_animation_target: Option<HostTime>,
    active: bool,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl SubductionPlanState {
    fn new(output: OutputId, now: HostTime, timebase: Timebase) -> Self {
        Self {
            heuristic: HeuristicFrameClock::default(),
            output,
            timebase,
            host_origin: now,
            instant_origin: Instant::now(),
            latest_tick: None,
            latest_plan: None,
            latest_effective_interval: None,
            previous_predicted_present: None,
            latest_prepare_start: None,
            demand: FrameDemand::empty(),
            estimate: PacingEstimate::default(),
            last_present_time: None,
            pending_animation_target: None,
            active: false,
        }
    }

    fn host_to_instant(&self, host: HostTime) -> Instant {
        let nanos = host
            .saturating_duration_since(self.host_origin)
            .to_nanos(self.timebase);
        self.instant_origin + Duration::from_nanos(nanos)
    }

    fn instant_to_host(&self, instant: Instant) -> HostTime {
        let nanos = instant
            .checked_duration_since(self.instant_origin)
            .unwrap_or(Duration::ZERO)
            .as_nanos()
            .min(u64::MAX as u128) as u64;
        self.host_origin + HostDuration::from_nanos(nanos, self.timebase)
    }

    fn latest_frame_interval(&self, fallback: Duration) -> Duration {
        self.latest_effective_interval
            .map(|duration| Duration::from_nanos(duration.to_nanos(self.timebase)))
            .unwrap_or(fallback)
    }

    fn effective_interval_for_tick(&self, tick: FrameTick) -> Option<HostDuration> {
        tick.refresh_interval.map(HostDuration).or_else(|| {
            let predicted_present = tick.predicted_present?;
            let previous = self.previous_predicted_present?;
            (predicted_present > previous)
                .then(|| predicted_present.saturating_duration_since(previous))
        })
    }

    fn display_timing(&self, fallback: Duration) -> DisplayTiming {
        DisplayTiming::fixed(self.latest_frame_interval(fallback))
    }

    fn set_frame_demand(&mut self, demand: FrameDemand) {
        self.demand = demand;
        if !self.heuristic.frame_prepared
            && let Some(tick) = self.latest_tick
        {
            self.latest_plan = self.plan_for_tick(tick);
        }
    }

    fn pacing_display_timing(&self, refresh_interval: HostDuration) -> PacingDisplayTiming {
        PacingDisplayTiming::fixed(self.host_duration_to_pacing(refresh_interval))
    }

    fn host_to_pacing_time(&self, host: HostTime) -> PacingTime {
        PacingTime::from_nanos(
            host.saturating_duration_since(self.host_origin)
                .to_nanos(self.timebase) as i64,
        )
    }

    fn pacing_time_to_host(&self, time: PacingTime) -> HostTime {
        self.host_origin + HostDuration::from_nanos(time.as_nanos().max(0) as u64, self.timebase)
    }

    fn host_duration_to_pacing(&self, duration: HostDuration) -> PacingDuration {
        PacingDuration::from_nanos(duration.to_nanos(self.timebase))
    }

    fn pacing_duration_to_host(&self, duration: PacingDuration) -> HostDuration {
        HostDuration::from_nanos(duration.as_nanos(), self.timebase)
    }

    fn host_duration_ms(&self, duration: HostDuration) -> f64 {
        duration.to_nanos(self.timebase) as f64 / 1_000_000.0
    }

    fn host_delta_ms(&self, from: HostTime, to: HostTime) -> f64 {
        if to >= from {
            self.host_duration_ms(to.saturating_duration_since(from))
        } else {
            -(self.host_duration_ms(from.saturating_duration_since(to)))
        }
    }

    fn present_pacing_from_decision(&self, decision: FramePacingDecision) -> PresentPacing {
        match decision.presentation {
            PacingPresentation::AsSoonAsReady => PresentPacing::AsSoonAsPossible,
            PacingPresentation::At(time) => {
                PresentPacing::AtTime(self.host_to_instant(self.pacing_time_to_host(time)))
            }
            PacingPresentation::AfterMinimumDuration(duration) => {
                let _ = duration;
                PresentPacing::AtTime(
                    self.host_to_instant(self.pacing_time_to_host(decision.target_present_time)),
                )
            }
        }
    }

    fn plan_for_tick(&mut self, tick: FrameTick) -> Option<ActivePacingPlan> {
        let predicted_present = tick.predicted_present?;
        let refresh_interval = self
            .effective_interval_for_tick(tick)
            .or(self.latest_effective_interval)
            .unwrap_or_else(|| HostDuration::from_nanos(16_666_667, self.timebase));
        let frame_cadenced = is_frame_cadenced_demand(self.demand);
        let pending_target = if frame_cadenced {
            self.pending_animation_target
        } else {
            None
        };
        let decision = pacing_plan_frame(
            self.pacing_display_timing(refresh_interval),
            pacing_estimate(self.estimate),
            pacing_demand(self.demand),
            PacingFrameOpportunity {
                now: self.host_to_pacing_time(tick.now),
                predicted_present_time: Some(self.host_to_pacing_time(predicted_present)),
                frame_interval: Some(self.host_duration_to_pacing(refresh_interval)),
                last_present_time: self
                    .last_present_time
                    .map(|time| self.host_to_pacing_time(time)),
                pending_target_present_time: pending_target
                    .map(|time| self.host_to_pacing_time(time)),
            },
        );
        let frame_interval = self.pacing_duration_to_host(decision.frame_interval);
        let present_time = self.pacing_time_to_host(decision.target_present_time);
        let present_pacing = self.present_pacing_from_decision(decision);
        if frame_pacing_diag_enabled() {
            eprintln!(
                "floem frame pacing plan fixed tick={} demand={:?} now_to_pred={:.3}ms refresh={:.3}ms estimate_pre={:.3}ms estimate_surface={:.3}ms estimate_gpu={:.3}ms safety={:.3}ms selected={:.3}ms target_from_now={:.3}ms pre_start_from_now={:.3}ms acquire_from_now={:.3}ms submit_deadline_from_now={:.3}ms pending_in={:?} pending_out={} pacing={:?}",
                tick.frame_index,
                self.demand,
                self.host_delta_ms(tick.now, predicted_present),
                self.host_duration_ms(refresh_interval),
                duration_ms(self.estimate.pre_surface_work),
                duration_ms(self.estimate.surface_work),
                duration_ms(self.estimate.gpu_work),
                duration_ms(self.estimate.safety_margin),
                self.host_duration_ms(frame_interval),
                self.host_delta_ms(tick.now, present_time),
                self.host_delta_ms(
                    tick.now,
                    self.pacing_time_to_host(decision.pre_surface_work_start)
                ),
                self.host_delta_ms(
                    tick.now,
                    self.pacing_time_to_host(decision.acquire_surface_at)
                ),
                self.host_delta_ms(tick.now, self.pacing_time_to_host(decision.submit_deadline)),
                pending_target.map(|target| self.host_delta_ms(tick.now, target)),
                frame_interval > refresh_interval,
                present_pacing,
            );
        }
        if frame_cadenced && frame_interval > refresh_interval {
            self.pending_animation_target = Some(present_time);
        } else {
            self.pending_animation_target = None;
        }
        Some(ActivePacingPlan {
            semantic_time: present_time,
            present_time,
            submit_deadline: self.pacing_time_to_host(decision.submit_deadline),
            present_pacing,
            frame_index: tick.frame_index,
        })
    }

    fn current_frame_time(
        &self,
        window: &dyn WinitWindow,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
        let frame_interval = self.latest_frame_interval(window_frame_interval(window));
        if let Some(plan) = self.latest_plan {
            let semantic_time = self.host_to_instant(plan.semantic_time);
            let predicted_present = self.host_to_instant(plan.present_time);
            return FrameTime {
                now: semantic_time,
                interval: PresentationInterval {
                    deadline_min: now,
                    deadline_max: predicted_present,
                    predicted_present: Some(predicted_present),
                    display_timing: self.display_timing(frame_interval),
                    present_pacing: plan.present_pacing,
                    background_rendering,
                },
                frame_interval: self.latest_frame_interval(frame_interval),
                frame_index: plan.frame_index,
            };
        }

        self.heuristic
            .current_frame_time(window, now, background_rendering)
    }

    fn current_external_frame_time(
        &mut self,
        window: &dyn WinitWindow,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
        self.current_frame_time(window, now, background_rendering)
    }

    fn should_defer_scene_work(&self, now: Instant) -> Option<Instant> {
        let plan = self.latest_plan?;
        let budget = self.estimate.pre_surface_work + self.estimate.surface_work;
        let budget = HostDuration::from_nanos(
            budget.as_nanos().min(u64::MAX as u128) as u64,
            self.timebase,
        );
        let estimated_finish = self.instant_to_host(now) + budget;
        (estimated_finish > plan.submit_deadline)
            .then(|| self.host_to_instant(plan.submit_deadline))
    }

    fn current_submit_deadline(&self, window: &dyn WinitWindow, now: Instant) -> Instant {
        self.latest_plan
            .map(|plan| self.host_to_instant(plan.submit_deadline))
            .unwrap_or_else(|| {
                self.current_frame_time(window, now, false)
                    .interval
                    .deadline_max
            })
    }

    fn observe_presented(
        &mut self,
        feedback: FrameTimingFeedback,
        submitted_at: Instant,
        presented_at: Instant,
    ) {
        self.heuristic
            .observe_presented(feedback, submitted_at, presented_at);
        self.estimate.pre_surface_work = smooth_work_estimate(
            self.estimate.pre_surface_work,
            feedback.render_cpu,
            Duration::from_micros(500),
        );
        self.estimate.surface_work = smooth_work_estimate(
            self.estimate.surface_work,
            feedback.present_cpu,
            Duration::from_micros(500),
        );
        self.estimate.gpu_work =
            smooth_work_estimate(self.estimate.gpu_work, feedback.gpu, Duration::ZERO);
        let presented_host = self.instant_to_host(presented_at);
        if frame_pacing_diag_enabled() {
            let last_gap = self
                .last_present_time
                .map(|last| self.host_duration_ms(presented_host.saturating_duration_since(last)));
            eprintln!(
                "floem frame pacing feedback fixed submitted_to_present={:.3}ms present_gap={:?} render_cpu={:.3}ms present_cpu={:.3}ms gpu={:.3}ms next_estimate_pre={:.3}ms next_estimate_surface={:.3}ms",
                duration_ms(presented_at.saturating_duration_since(submitted_at)),
                last_gap,
                duration_ms(feedback.render_cpu.unwrap_or(Duration::ZERO)),
                duration_ms(feedback.present_cpu.unwrap_or(Duration::ZERO)),
                duration_ms(feedback.gpu.unwrap_or(Duration::ZERO)),
                duration_ms(self.estimate.pre_surface_work),
                duration_ms(self.estimate.surface_work),
            );
        }
        self.last_present_time = Some(presented_host);

        self.latest_plan = None;
        self.latest_prepare_start = None;
    }

    fn observe_new_plan(&mut self, tick: FrameTick, plan: Option<ActivePacingPlan>) {
        self.latest_effective_interval = self
            .effective_interval_for_tick(tick)
            .or(self.latest_effective_interval);
        if let Some(predicted_present) = tick.predicted_present {
            self.previous_predicted_present = Some(predicted_present);
        }
        self.latest_tick = Some(tick);
        if self.heuristic.frame_prepared {
            if frame_pacing_diag_enabled() {
                eprintln!(
                    "floem frame pacing keep prepared plan fixed tick={} latest_plan={:?}",
                    tick.frame_index,
                    self.latest_plan.map(|plan| plan.frame_index),
                );
            }
            return;
        }

        if self.latest_plan.is_some_and(|latest_plan| {
            plan.is_none_or(|plan| latest_plan.frame_index != plan.frame_index)
        }) {
            // A newer platform frame opportunity arrived before draw.
            // Drop the "prepared" latch so Floem can re-prepare against the freshest plan.
            self.heuristic.set_frame_prepared(false);
            self.latest_prepare_start = None;
        }

        self.latest_plan = plan;
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
enum AppleFrameDisplayLink {
    Ca(CaDisplayLinkThread),
}

#[cfg(target_os = "macos")]
impl AppleFrameDisplayLink {
    fn keep_alive(&self) {
        match self {
            Self::Ca(link) => {
                let _ = link;
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct SubductionFrameClock {
    plan_state: SubductionPlanState,
    window_id: WindowId,
    native_display_id: Option<u32>,
    display_link: Option<AppleFrameDisplayLink>,
    display_link_view: Option<DisplayLinkView>,
    display_link_layer: Option<DisplayLinkLayer>,
    prefers_metal_display_link: bool,
}

#[cfg(target_os = "macos")]
impl SubductionFrameClock {
    fn new(window_id: WindowId, fallback_output: OutputId) -> Self {
        Self {
            plan_state: SubductionPlanState::new(fallback_output, subduction_now(), timebase()),
            window_id,
            native_display_id: None,
            display_link: None,
            display_link_view: None,
            display_link_layer: None,
            prefers_metal_display_link: false,
        }
    }

    fn ensure_display_link(&mut self) {
        if self.display_link.is_some() {
            return;
        }

        let output = self.plan_state.output;
        let window_id = self.window_id;
        let callback = move |tick: FrameTick| {
            if frame_pacing_diag_enabled() {
                eprintln!(
                    "floem frame pacing display link callback window={:?} tick={} predicted={:?} refresh={:?}",
                    window_id, tick.frame_index, tick.predicted_present, tick.refresh_interval,
                );
            }
            Application::send_proxy_event(UserEvent::FrameTick { window_id, tick });
        };
        if frame_pacing_diag_enabled() {
            eprintln!(
                "floem frame pacing display link source={} mode=NSRunLoopCommonModes",
                if self.display_link_view.is_some() {
                    "AppKitCADisplayLinkThread"
                } else {
                    "CADisplayLinkThread"
                }
            );
        }
        let display_link = if let Some(view) = self.display_link_view.clone() {
            CaDisplayLinkThread::spawn_for_view(callback, output, view)
        } else {
            CaDisplayLinkThread::spawn(callback, output)
        };
        let display_link = AppleFrameDisplayLink::Ca(display_link);
        display_link.keep_alive();
        self.display_link = Some(display_link);
    }
}

#[cfg(target_os = "macos")]
impl FrameClock for SubductionFrameClock {
    fn frame_interval(&self, window: &dyn WinitWindow) -> Duration {
        self.plan_state
            .latest_frame_interval(window_frame_interval(window))
    }

    fn current_frame_time(
        &self,
        window: &dyn WinitWindow,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
        self.plan_state
            .current_frame_time(window, now, background_rendering)
    }

    fn current_external_frame_time(
        &mut self,
        window: &dyn WinitWindow,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
        self.plan_state
            .current_external_frame_time(window, now, background_rendering)
    }

    fn note_begin_frame_callbacks_ran(&mut self) {
        self.plan_state.heuristic.note_begin_frame_callbacks_ran();
    }

    fn refresh_schedule(&mut self, _window: &dyn WinitWindow, _now: Instant) {}

    fn note_frame_prepare_started(&mut self, now: Instant) {
        self.plan_state.latest_prepare_start = Some(self.plan_state.instant_to_host(now));
    }

    fn set_frame_demand(&mut self, demand: FrameDemand) {
        self.plan_state.set_frame_demand(demand);
    }

    fn set_frame_prepared(&mut self, prepared: bool) {
        self.plan_state.heuristic.set_frame_prepared(prepared);
    }

    fn needs_frame_prepare(&self, has_next_frame_work: bool) -> bool {
        self.plan_state
            .heuristic
            .needs_frame_prepare(has_next_frame_work)
    }

    fn should_defer_scene_work(&self, now: Instant) -> Option<Instant> {
        self.plan_state.should_defer_scene_work(now)
    }

    fn current_submit_deadline(&self, window: &dyn WinitWindow, now: Instant) -> Instant {
        self.plan_state.current_submit_deadline(window, now)
    }

    fn observe_presented(
        &mut self,
        feedback: FrameTimingFeedback,
        submitted_at: Instant,
        presented_at: Instant,
    ) {
        self.plan_state
            .observe_presented(feedback, submitted_at, presented_at);
    }

    fn set_active(&mut self, active: bool) {
        if self.plan_state.active == active {
            return;
        }
        self.plan_state.active = active;
        if active {
            self.ensure_display_link();
        }
    }

    fn receive_frame_tick(&mut self, tick: FrameTick) {
        let tick = FrameTick {
            output: self.plan_state.output,
            ..tick
        };
        if frame_pacing_diag_enabled()
            && let Some(predicted_present) = tick.predicted_present
        {
            let refresh_ms = tick
                .refresh_interval
                .map(HostDuration)
                .map(|duration| duration.to_nanos(self.plan_state.timebase) as f64 / 1_000_000.0);
            eprintln!(
                "floem frame pacing raw tick={} now_to_pred={:.3}ms refresh={:?} confidence={:?}",
                tick.frame_index,
                self.plan_state.host_delta_ms(tick.now, predicted_present),
                refresh_ms,
                tick.confidence,
            );
        }
        let plan = self.plan_state.plan_for_tick(tick);
        self.plan_state.observe_new_plan(tick, plan);
    }

    fn set_native_display_id(&mut self, display_id: Option<u32>) {
        if self.native_display_id == display_id {
            return;
        }

        self.native_display_id = display_id;
    }

    fn set_metal_display_link_layer(&mut self, layer: Option<DisplayLinkLayer>) {
        let changed = match (&self.display_link_layer, &layer) {
            (Some(old), Some(new)) => !old.is_same_layer(new),
            (None, None) => false,
            _ => true,
        };
        if !changed {
            return;
        }
        self.display_link_layer = layer;
    }

    fn set_display_link_view(&mut self, view: Option<DisplayLinkView>) {
        let changed = match (&self.display_link_view, &view) {
            (Some(old), Some(new)) => !old.is_same_view(new),
            (None, None) => false,
            _ => true,
        };
        if !changed {
            return;
        }
        self.display_link_view = view;
        if self.display_link.is_some() {
            self.display_link = None;
            if self.plan_state.active {
                self.ensure_display_link();
            }
        }
    }

    fn set_prefers_metal_display_link(&mut self, prefers_metal: bool) {
        self.prefers_metal_display_link = prefers_metal;
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
struct WindowsSubductionFrameClock {
    plan_state: SubductionPlanState,
    prev_present_time: Option<HostTime>,
    window_id: WindowId,
    tick_driver: Option<FrameTickDriver>,
    tick_interval_ns: Arc<AtomicU64>,
}

#[cfg(target_os = "windows")]
impl WindowsSubductionFrameClock {
    fn new(window_id: WindowId, output: OutputId) -> Self {
        Self {
            plan_state: SubductionPlanState::new(output, windows_now(), windows_timebase()),
            prev_present_time: None,
            window_id,
            tick_driver: None,
            tick_interval_ns: Arc::new(AtomicU64::new(DEFAULT_SYNTHETIC_FRAME_INTERVAL_NS)),
        }
    }
}

#[cfg(target_os = "windows")]
impl FrameClock for WindowsSubductionFrameClock {
    fn frame_interval(&self, window: &dyn WinitWindow) -> Duration {
        let interval = self
            .plan_state
            .latest_frame_interval(window_frame_interval(window));
        self.tick_interval_ns.store(
            interval.as_nanos().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
        interval
    }

    fn current_frame_time(
        &self,
        window: &dyn WinitWindow,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
        self.plan_state
            .current_frame_time(window, now, background_rendering)
    }

    fn current_external_frame_time(
        &mut self,
        window: &dyn WinitWindow,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
        self.plan_state
            .current_external_frame_time(window, now, background_rendering)
    }

    fn note_begin_frame_callbacks_ran(&mut self) {
        self.plan_state.heuristic.note_begin_frame_callbacks_ran();
    }

    fn refresh_schedule(&mut self, _window: &dyn WinitWindow, _now: Instant) {}

    fn note_frame_prepare_started(&mut self, now: Instant) {
        self.plan_state.latest_prepare_start = Some(self.plan_state.instant_to_host(now));
    }

    fn set_frame_demand(&mut self, demand: FrameDemand) {
        self.plan_state.set_frame_demand(demand);
    }

    fn set_frame_prepared(&mut self, prepared: bool) {
        self.plan_state.heuristic.set_frame_prepared(prepared);
    }

    fn needs_frame_prepare(&self, has_next_frame_work: bool) -> bool {
        self.plan_state
            .heuristic
            .needs_frame_prepare(has_next_frame_work)
    }

    fn should_defer_scene_work(&self, now: Instant) -> Option<Instant> {
        self.plan_state.should_defer_scene_work(now)
    }

    fn current_submit_deadline(&self, window: &dyn WinitWindow, now: Instant) -> Instant {
        self.plan_state.current_submit_deadline(window, now)
    }

    fn observe_presented(
        &mut self,
        feedback: FrameTimingFeedback,
        submitted_at: Instant,
        presented_at: Instant,
    ) {
        self.prev_present_time = Some(self.plan_state.instant_to_host(presented_at));
        self.plan_state
            .observe_presented(feedback, submitted_at, presented_at);
    }

    fn set_active(&mut self, active: bool) {
        if self.plan_state.active == active {
            return;
        }
        self.plan_state.active = active;
        if !active {
            self.tick_driver = None;
            return;
        }
        if self.tick_driver.is_some() {
            return;
        }
        let interval_ns = self.tick_interval_ns.clone();
        self.tick_driver = Some(spawn_frame_tick_driver(
            "floem-windows-frame-clock",
            self.window_id,
            self.plan_state.output,
            windows_timebase(),
            interval_ns,
            windows_now,
        ));
    }

    fn receive_frame_tick(&mut self, tick: FrameTick) {
        let tick = FrameTick {
            output: self.plan_state.output,
            ..tick
        };
        let plan = self.plan_state.plan_for_tick(tick);
        self.plan_state.observe_new_plan(tick, plan);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_frame_time_samples_predicted_present() {
        let clock = HeuristicFrameClock::default();
        let now = Instant::now();
        let interval = clock
            .plan_frame(Duration::from_millis(8), now)
            .target_present_time
            - now;

        assert_eq!(interval, Duration::from_millis(8));
    }
}
