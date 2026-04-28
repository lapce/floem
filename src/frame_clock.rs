use crate::{
    frame::{
        DisplayTiming, FrameTime, FrameTimingFeedback, FrameWorkload, PresentPacing,
        PresentationInterval,
    },
    platform::{Duration, Instant},
};
use winit::window::WindowId;

#[cfg(target_os = "macos")]
use crate::{Application, app::UserEvent};
#[cfg(target_os = "macos")]
use subduction_backend_apple::{
    CaDisplayLinkThread, DisplayLinkLayer, MetalDisplayLinkThread, now as subduction_now, timebase,
};
#[cfg(target_os = "windows")]
use subduction_backend_windows::{
    make_tick as windows_make_tick, now as windows_now, timebase as windows_timebase,
};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use subduction_core::{
    output::OutputId,
    time::{Duration as HostDuration, HostTime, Timebase},
    timing::{DisplayTimingCapabilities, FrameTick, PresentPacing as SubductionPresentPacing},
};
use understory_frame_pacing::{
    DisplayTiming as PacingDisplayTiming, Duration as PacingDuration,
    FrameDemand as PacingFrameDemand, FrameOpportunity as PacingFrameOpportunity,
    FramePacingDecision, FrameTimingEstimate as PacingFrameTimingEstimate,
    Presentation as PacingPresentation, Time as PacingTime, plan_frame as pacing_plan_frame,
};
pub(crate) trait FrameClock {
    fn current_frame_time(
        &self,
        frame_interval: Duration,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime;
    fn note_begin_frame_callbacks_ran(&mut self);
    fn refresh_schedule(&mut self, _frame_interval: Duration, _now: Instant) {}
    fn note_frame_prepare_started(&mut self, now: Instant);
    fn set_frame_workload(&mut self, _workload: FrameWorkload) {}
    fn set_frame_prepared(&mut self, prepared: bool);
    fn needs_frame_prepare(&self, has_next_frame_work: bool) -> bool;
    fn redraw_deadline(&self, frame_interval: Duration, now: Instant) -> Instant;
    fn observe_presented(
        &mut self,
        feedback: FrameTimingFeedback,
        submitted_at: Instant,
        presented_at: Instant,
    );
    fn set_active(&mut self, _active: bool) {}
    fn has_external_frame_signal(&self) -> bool {
        false
    }
    #[cfg(target_os = "macos")]
    fn receive_frame_tick(&mut self, _tick: FrameTick) {}
    #[cfg(target_os = "macos")]
    fn set_native_display_id(&mut self, _display_id: Option<u32>) {}
    #[cfg(target_os = "macos")]
    fn set_metal_display_link_layer(&mut self, _layer: Option<DisplayLinkLayer>) {}
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
        let _ = (window_id, output_id);
        return Box::new(HeuristicFrameClock::default());
    }

    #[cfg(target_os = "macos")]
    {
        Box::new(SubductionFrameClock::new(window_id, OutputId(output_id)))
    }

    #[cfg(target_os = "windows")]
    {
        Box::new(WindowsSubductionFrameClock::new(OutputId(output_id)))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (window_id, output_id);
        Box::new(HeuristicFrameClock::default())
    }
}

fn max_duration(a: Duration, b: Duration) -> Duration {
    if a >= b { a } else { b }
}

const MIN_SURFACE_ACQUIRE_GUARD_BAND: Duration = Duration::from_millis(1);

pub(crate) fn frame_pacing_diag_enabled() -> bool {
    std::env::var_os("FLOEM_FRAME_PACING_DIAG").is_some()
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
    origin: Instant,
    last_presented_at: Instant,
    estimated_draw_lead_time: Duration,
    estimated_present_lead_time: Duration,
    frame_counter: u64,
    frame_prepared: bool,
    workload: FrameWorkload,
    estimate: PacingEstimate,
}

impl Default for HeuristicFrameClock {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            origin: now,
            last_presented_at: now,
            estimated_draw_lead_time: Duration::from_millis(1),
            estimated_present_lead_time: Duration::from_millis(1),
            frame_counter: 0,
            frame_prepared: false,
            workload: FrameWorkload::Animation,
            estimate: PacingEstimate::default(),
        }
    }
}

impl FrameClock for HeuristicFrameClock {
    fn current_frame_time(
        &self,
        frame_interval: Duration,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
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

    fn refresh_schedule(&mut self, _frame_interval: Duration, _now: Instant) {}

    fn note_frame_prepare_started(&mut self, _now: Instant) {}

    fn set_frame_workload(&mut self, workload: FrameWorkload) {
        self.workload = workload;
    }

    fn set_frame_prepared(&mut self, prepared: bool) {
        self.frame_prepared = prepared;
    }

    fn needs_frame_prepare(&self, has_next_frame_work: bool) -> bool {
        !self.frame_prepared && has_next_frame_work
    }

    fn redraw_deadline(&self, frame_interval: Duration, now: Instant) -> Instant {
        self.plan_frame(frame_interval, now)
            .pre_surface_work_start
            .max(self.earliest_surface_acquire_at())
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
}

impl HeuristicFrameClock {
    fn plan_frame(&self, frame_interval: Duration, now: Instant) -> HeuristicPacingPlan {
        let now_time = self.instant_to_pacing_time(now);
        let predicted_present_time =
            self.instant_to_pacing_time(now.checked_add(frame_interval).unwrap_or(now));
        let decision = pacing_plan_frame(
            PacingDisplayTiming::fixed(pacing_duration(frame_interval)),
            pacing_estimate(self.estimate),
            pacing_demand(self.workload),
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
            pre_surface_work_start: self.pacing_time_to_instant(decision.pre_surface_work_start),
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

    fn earliest_surface_acquire_at(&self) -> Instant {
        self.last_presented_at + MIN_SURFACE_ACQUIRE_GUARD_BAND
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
    pre_surface_work_start: HostTime,
    acquire_surface_at: HostTime,
    present_pacing: SubductionPresentPacing,
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
    pre_surface_work_start: Instant,
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

fn pacing_demand(workload: FrameWorkload) -> PacingFrameDemand {
    match workload {
        FrameWorkload::Input => PacingFrameDemand::Input,
        FrameWorkload::Animation => PacingFrameDemand::Animation,
    }
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
    latest_prepare_start: Option<HostTime>,
    workload: FrameWorkload,
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
            latest_prepare_start: None,
            workload: FrameWorkload::Animation,
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
        self.latest_tick
            .and_then(|tick| tick.refresh_interval)
            .map(|ticks| Duration::from_nanos(self.timebase.ticks_to_nanos(ticks)))
            .unwrap_or(fallback)
    }

    fn display_timing(&self, fallback: Duration) -> DisplayTiming {
        self.latest_tick
            .and_then(|tick| {
                let capabilities = tick.display_capabilities?;
                Some(self.display_timing_from_subduction(capabilities, tick.refresh_interval))
            })
            .unwrap_or_else(|| DisplayTiming::fixed(self.latest_frame_interval(fallback)))
    }

    fn display_timing_from_subduction(
        &self,
        capabilities: DisplayTimingCapabilities,
        refresh_interval: Option<u64>,
    ) -> DisplayTiming {
        if !capabilities.is_variable()
            && let Some(refresh_interval) = refresh_interval
        {
            return DisplayTiming::fixed(Duration::from_nanos(
                self.timebase.ticks_to_nanos(refresh_interval),
            ));
        }

        let min_frame_interval = Duration::from_nanos(
            self.timebase
                .ticks_to_nanos(capabilities.min_frame_interval.0),
        );
        let max_frame_interval = Duration::from_nanos(
            self.timebase
                .ticks_to_nanos(capabilities.max_frame_interval.0),
        );
        if capabilities.is_variable() {
            DisplayTiming::Variable {
                min_frame_interval,
                max_frame_interval,
            }
        } else {
            DisplayTiming::fixed(min_frame_interval)
        }
    }

    fn present_pacing_from_subduction(&self, pacing: SubductionPresentPacing) -> PresentPacing {
        match pacing {
            SubductionPresentPacing::AsSoonAsPossible => PresentPacing::AsSoonAsPossible,
            SubductionPresentPacing::AtTime(host_time) => {
                PresentPacing::AtTime(self.host_to_instant(host_time))
            }
            SubductionPresentPacing::AfterMinimumDuration(duration) => {
                PresentPacing::AfterMinimumDuration(Duration::from_nanos(
                    self.timebase.ticks_to_nanos(duration.0),
                ))
            }
        }
    }

    fn set_frame_workload(&mut self, workload: FrameWorkload) {
        self.workload = workload;
        if !self.heuristic.frame_prepared
            && let Some(tick) = self.latest_tick
        {
            self.latest_plan = self.plan_for_tick(tick);
        }
    }

    #[cfg(target_os = "windows")]
    fn latest_acquire_surface_at(&self) -> Option<Instant> {
        self.latest_plan
            .map(|plan| self.host_to_instant(plan.acquire_surface_at))
    }

    fn pacing_display_timing(
        &self,
        capabilities: DisplayTimingCapabilities,
        refresh_interval: Option<HostDuration>,
    ) -> PacingDisplayTiming {
        if !capabilities.is_variable()
            && let Some(refresh_interval) = refresh_interval
        {
            return PacingDisplayTiming::fixed(self.host_duration_to_pacing(refresh_interval));
        }

        let min_frame_interval = PacingDuration::from_nanos(
            self.timebase
                .ticks_to_nanos(capabilities.min_frame_interval.0),
        );
        let max_frame_interval = PacingDuration::from_nanos(
            self.timebase
                .ticks_to_nanos(capabilities.max_frame_interval.0),
        );
        if capabilities.is_variable() {
            PacingDisplayTiming::variable(min_frame_interval, max_frame_interval, None)
        } else {
            PacingDisplayTiming::fixed(min_frame_interval)
        }
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

    fn present_pacing_from_decision(
        &self,
        decision: FramePacingDecision,
        capabilities: DisplayTimingCapabilities,
    ) -> SubductionPresentPacing {
        match decision.presentation {
            PacingPresentation::AsSoonAsReady => SubductionPresentPacing::AsSoonAsPossible,
            PacingPresentation::At(time) => {
                SubductionPresentPacing::AtTime(self.pacing_time_to_host(time))
            }
            PacingPresentation::AfterMinimumDuration(duration) => {
                if capabilities.is_variable() {
                    SubductionPresentPacing::AfterMinimumDuration(
                        self.pacing_duration_to_host(duration),
                    )
                } else {
                    SubductionPresentPacing::AtTime(
                        self.pacing_time_to_host(decision.target_present_time),
                    )
                }
            }
        }
    }

    fn plan_for_tick(&mut self, tick: FrameTick) -> Option<ActivePacingPlan> {
        let capabilities = tick.display_capabilities?;
        let predicted_present = tick.predicted_present?;
        let refresh_interval = tick
            .refresh_interval
            .or_else(|| self.latest_tick.and_then(|tick| tick.refresh_interval))
            .map(HostDuration)
            .unwrap_or(capabilities.min_frame_interval);
        let pending_target = if self.workload == FrameWorkload::Animation {
            self.pending_animation_target
        } else {
            None
        };
        let decision = pacing_plan_frame(
            self.pacing_display_timing(capabilities, Some(refresh_interval)),
            pacing_estimate(self.estimate),
            pacing_demand(self.workload),
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
        let present_pacing = self.present_pacing_from_decision(decision, capabilities);
        if frame_pacing_diag_enabled() && !capabilities.is_variable() {
            eprintln!(
                "floem frame pacing plan fixed tick={} workload={:?} now_to_pred={:.3}ms refresh={:.3}ms estimate_pre={:.3}ms estimate_surface={:.3}ms estimate_gpu={:.3}ms safety={:.3}ms selected={:.3}ms target_from_now={:.3}ms pre_start_from_now={:.3}ms acquire_from_now={:.3}ms submit_deadline_from_now={:.3}ms pending_in={:?} pending_out={} pacing={:?}",
                tick.frame_index,
                self.workload,
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
        if self.workload == FrameWorkload::Animation && frame_interval > refresh_interval {
            self.pending_animation_target = Some(present_time);
        } else {
            self.pending_animation_target = None;
        }
        Some(ActivePacingPlan {
            semantic_time: present_time,
            present_time,
            pre_surface_work_start: self.pacing_time_to_host(decision.pre_surface_work_start),
            acquire_surface_at: self.pacing_time_to_host(decision.acquire_surface_at),
            present_pacing,
            frame_index: tick.frame_index,
        })
    }

    fn current_frame_time(
        &self,
        frame_interval: Duration,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
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
                    present_pacing: self.present_pacing_from_subduction(plan.present_pacing),
                    background_rendering,
                },
                frame_interval: self.latest_frame_interval(frame_interval),
                frame_index: plan.frame_index,
            };
        }

        self.heuristic
            .current_frame_time(frame_interval, now, background_rendering)
    }

    fn redraw_deadline(&self, frame_interval: Duration, now: Instant) -> Instant {
        if let Some(plan) = self.latest_plan {
            let target = self.host_to_instant(plan.pre_surface_work_start);
            if frame_pacing_diag_enabled()
                && self
                    .latest_tick
                    .and_then(|tick| tick.display_capabilities)
                    .is_some_and(|capabilities| !capabilities.is_variable())
            {
                eprintln!(
                    "floem frame pacing wake fixed tick={} phase={} target_in={:.3}ms now_to_present={:.3}ms now_to_pre={:.3}ms now_to_acquire={:.3}ms pacing={:?}",
                    plan.frame_index,
                    "prepare",
                    duration_ms(target.saturating_duration_since(now)),
                    duration_ms(
                        self.host_to_instant(plan.present_time)
                            .saturating_duration_since(now)
                    ),
                    duration_ms(
                        self.host_to_instant(plan.pre_surface_work_start)
                            .saturating_duration_since(now)
                    ),
                    duration_ms(
                        self.host_to_instant(plan.acquire_surface_at)
                            .saturating_duration_since(now)
                    ),
                    plan.present_pacing,
                );
            }
            return target.max(now);
        }

        self.heuristic.redraw_deadline(frame_interval, now)
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
        if frame_pacing_diag_enabled()
            && self
                .latest_tick
                .and_then(|tick| tick.display_capabilities)
                .is_some_and(|capabilities| !capabilities.is_variable())
        {
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
    Metal(MetalDisplayLinkThread),
    Ca(CaDisplayLinkThread),
}

#[cfg(target_os = "macos")]
impl AppleFrameDisplayLink {
    fn keep_alive(&self) {
        match self {
            Self::Metal(link) => {
                let _ = link;
            }
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
        let callback = move |tick| {
            Application::send_proxy_event(UserEvent::SubductionFrameTick { window_id, tick });
        };
        let display_link = if self.prefers_metal_display_link {
            if let Some(layer) = self.display_link_layer.clone() {
                if frame_pacing_diag_enabled() {
                    eprintln!("floem frame pacing display link source=CAMetalDisplayLink");
                }
                AppleFrameDisplayLink::Metal(MetalDisplayLinkThread::spawn(callback, output, layer))
            } else {
                if frame_pacing_diag_enabled() {
                    eprintln!(
                        "floem frame pacing display link source=CADisplayLink reason=no_metal_layer"
                    );
                }
                AppleFrameDisplayLink::Ca(CaDisplayLinkThread::spawn(callback, output))
            }
        } else {
            if frame_pacing_diag_enabled() {
                eprintln!("floem frame pacing display link source=CADisplayLink reason=preferred");
            }
            AppleFrameDisplayLink::Ca(CaDisplayLinkThread::spawn(callback, output))
        };
        display_link.keep_alive();
        self.display_link = Some(display_link);
    }

    fn recreate_display_link(&mut self) {
        self.display_link = None;
        if self.plan_state.active {
            self.ensure_display_link();
        }
    }
}

#[cfg(target_os = "macos")]
impl FrameClock for SubductionFrameClock {
    fn current_frame_time(
        &self,
        frame_interval: Duration,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
        self.plan_state
            .current_frame_time(frame_interval, now, background_rendering)
    }

    fn note_begin_frame_callbacks_ran(&mut self) {
        self.plan_state.heuristic.note_begin_frame_callbacks_ran();
    }

    fn refresh_schedule(&mut self, _frame_interval: Duration, _now: Instant) {}

    fn note_frame_prepare_started(&mut self, now: Instant) {
        self.plan_state.latest_prepare_start = Some(self.plan_state.instant_to_host(now));
    }

    fn set_frame_workload(&mut self, workload: FrameWorkload) {
        self.plan_state.set_frame_workload(workload);
    }

    fn set_frame_prepared(&mut self, prepared: bool) {
        self.plan_state.heuristic.set_frame_prepared(prepared);
    }

    fn needs_frame_prepare(&self, has_next_frame_work: bool) -> bool {
        self.plan_state
            .heuristic
            .needs_frame_prepare(has_next_frame_work)
    }

    fn redraw_deadline(&self, frame_interval: Duration, now: Instant) -> Instant {
        self.plan_state.redraw_deadline(frame_interval, now)
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

    fn has_external_frame_signal(&self) -> bool {
        true
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
        if self.prefers_metal_display_link {
            self.recreate_display_link();
        }
    }

    fn set_prefers_metal_display_link(&mut self, prefers_metal: bool) {
        if self.prefers_metal_display_link == prefers_metal {
            return;
        }
        self.prefers_metal_display_link = prefers_metal;
        self.recreate_display_link();
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
struct WindowsSubductionFrameClock {
    plan_state: SubductionPlanState,
    next_frame_index: u64,
    prev_present_time: Option<HostTime>,
}

#[cfg(target_os = "windows")]
impl WindowsSubductionFrameClock {
    fn new(output: OutputId) -> Self {
        Self {
            plan_state: SubductionPlanState::new(output, windows_now(), windows_timebase()),
            next_frame_index: 0,
            prev_present_time: None,
        }
    }
}

#[cfg(target_os = "windows")]
impl FrameClock for WindowsSubductionFrameClock {
    fn current_frame_time(
        &self,
        frame_interval: Duration,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
        self.plan_state
            .current_frame_time(frame_interval, now, background_rendering)
    }

    fn note_begin_frame_callbacks_ran(&mut self) {
        self.plan_state.heuristic.note_begin_frame_callbacks_ran();
    }

    fn refresh_schedule(&mut self, frame_interval: Duration, now: Instant) {
        if !self.plan_state.active {
            return;
        }

        let needs_new_plan = self.plan_state.latest_plan.is_none()
            || self
                .plan_state
                .latest_acquire_surface_at()
                .is_some_and(|deadline| now >= deadline);
        if !needs_new_plan {
            return;
        }

        let refresh_ns = frame_interval.as_nanos().min(u64::MAX as u128) as u64;
        let tick = windows_make_tick(refresh_ns, self.next_frame_index, self.prev_present_time);
        self.next_frame_index = self.next_frame_index.saturating_add(1);

        let plan = self.plan_state.plan_for_tick(tick);
        self.plan_state.observe_new_plan(
            FrameTick {
                output: self.plan_state.output,
                ..tick
            },
            plan,
        );
    }

    fn note_frame_prepare_started(&mut self, now: Instant) {
        self.plan_state.latest_prepare_start = Some(self.plan_state.instant_to_host(now));
    }

    fn set_frame_workload(&mut self, workload: FrameWorkload) {
        self.plan_state.set_frame_workload(workload);
    }

    fn set_frame_prepared(&mut self, prepared: bool) {
        self.plan_state.heuristic.set_frame_prepared(prepared);
    }

    fn needs_frame_prepare(&self, has_next_frame_work: bool) -> bool {
        self.plan_state
            .heuristic
            .needs_frame_prepare(has_next_frame_work)
    }

    fn redraw_deadline(&self, frame_interval: Duration, now: Instant) -> Instant {
        self.plan_state.redraw_deadline(frame_interval, now)
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
        self.plan_state.active = active;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_frame_time_samples_predicted_present() {
        let clock = HeuristicFrameClock::default();
        let now = Instant::now();
        let interval = Duration::from_millis(8);

        let frame_time = clock.current_frame_time(interval, now, false);

        assert_eq!(frame_time.now, now + interval);
        assert_eq!(frame_time.interval.predicted_present, Some(now + interval));
    }
}
