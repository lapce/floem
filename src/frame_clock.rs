use crate::{
    frame::{FrameTime, PresentationInterval},
    platform::{Duration, Instant},
};
use winit::window::WindowId;

#[cfg(all(feature = "subduction", target_os = "macos"))]
use crate::{Application, app::UserEvent};
#[cfg(all(feature = "subduction", target_os = "macos"))]
use objc2::MainThreadMarker;
#[cfg(all(feature = "subduction", target_os = "macos"))]
use subduction_backend_apple::{DisplayLink, compute_present_hints, now as subduction_now, timebase};
#[cfg(all(feature = "subduction", target_os = "windows"))]
use subduction_backend_windows::{
    compute_present_hints as windows_compute_present_hints, make_tick as windows_make_tick,
    now as windows_now, timebase as windows_timebase,
};
#[cfg(all(feature = "subduction", target_os = "macos"))]
use subduction_core::{
    output::OutputId,
    scheduler::{Scheduler, SchedulerConfig},
    time::{Duration as HostDuration, HostTime, Timebase},
    timing::{FramePlan, FrameTick, PendingFeedback, PresentHints},
};
#[cfg(all(feature = "subduction", target_os = "windows"))]
use subduction_core::{
    output::OutputId,
    scheduler::{Scheduler, SchedulerConfig},
    time::{Duration as HostDuration, HostTime, Timebase},
    timing::{FramePlan, FrameTick, PendingFeedback, PresentHints},
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
    fn mark_frame_prepared(&mut self);
    fn clear_prepared_frame(&mut self);
    fn has_preparable_frame_work(&self, has_next_frame_work: bool) -> bool;
    fn frame_prepare_deadline(&self, frame_interval: Duration, now: Instant) -> Instant;
    fn redraw_deadline(&self, frame_interval: Duration, now: Instant) -> Instant;
    fn observe_presented(
        &mut self,
        update_cpu_time: Duration,
        draw_cpu_time_excluding_acquire: Duration,
        presented_at: Instant,
    );
    fn set_active(&mut self, _active: bool) {}
    #[cfg(all(feature = "subduction", target_os = "macos"))]
    fn receive_frame_tick(&mut self, _tick: FrameTick) {}
}

pub(crate) fn new_window_frame_clock(window_id: WindowId, output_id: u32) -> Box<dyn FrameClock> {
    #[cfg(all(feature = "subduction", target_os = "macos"))]
    {
        if let Some(mtm) = MainThreadMarker::new() {
            return Box::new(SubductionFrameClock::new(window_id, OutputId(output_id), mtm));
        }
    }

    #[cfg(all(feature = "subduction", target_os = "windows"))]
    {
        return Box::new(WindowsSubductionFrameClock::new(OutputId(output_id)));
    }

    let _ = (window_id, output_id);
    Box::new(HeuristicFrameClock::default())
}

fn max_duration(a: Duration, b: Duration) -> Duration {
    if a >= b { a } else { b }
}

fn min_duration(a: Duration, b: Duration) -> Duration {
    if a <= b { a } else { b }
}

const SURFACE_ACQUIRE_GUARD_BAND: Duration = Duration::from_micros(300);

#[derive(Debug)]
pub(crate) struct HeuristicFrameClock {
    last_presented_at: Instant,
    last_frame_opportunity_at: Instant,
    estimated_frame_prepare_lead_time: Duration,
    estimated_draw_lead_time: Duration,
    frame_counter: u64,
    frame_prepared: bool,
}

impl Default for HeuristicFrameClock {
    fn default() -> Self {
        Self {
            last_presented_at: Instant::now(),
            last_frame_opportunity_at: Instant::now(),
            estimated_frame_prepare_lead_time: Duration::from_millis(1),
            estimated_draw_lead_time: Duration::from_millis(1),
            frame_counter: 0,
            frame_prepared: false,
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
        let predicted_present = now.checked_add(frame_interval);
        FrameTime {
            now,
            interval: PresentationInterval {
                deadline_min: now,
                deadline_max: predicted_present.unwrap_or(now),
                predicted_present,
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

    fn mark_frame_prepared(&mut self) {
        self.frame_prepared = true;
    }

    fn clear_prepared_frame(&mut self) {
        self.frame_prepared = false;
    }

    fn has_preparable_frame_work(&self, has_next_frame_work: bool) -> bool {
        !self.frame_prepared && has_next_frame_work
    }

    fn frame_prepare_deadline(&self, frame_interval: Duration, now: Instant) -> Instant {
        let earliest_present = self.last_presented_at + frame_interval;
        let max_total_lead = frame_interval
            .checked_sub(Duration::from_millis(1))
            .unwrap_or(frame_interval);
        let lead_time = min_duration(
            max_duration(
                self.estimated_frame_prepare_lead_time + self.estimated_draw_lead_time,
                Duration::from_millis(1),
            ),
            max_total_lead,
        );

        earliest_present.checked_sub(lead_time).unwrap_or(now)
    }

    fn redraw_deadline(&self, frame_interval: Duration, now: Instant) -> Instant {
        let earliest_present = self.last_presented_at + frame_interval;
        let max_lead = frame_interval
            .checked_div(2)
            .unwrap_or(Duration::from_millis(1));
        let lead_time = min_duration(
            max_duration(self.estimated_draw_lead_time, Duration::from_millis(1)),
            max_lead,
        );

        earliest_present
            .checked_sub(lead_time)
            .unwrap_or(now)
            .max(self.earliest_surface_acquire_at())
    }

    fn observe_presented(
        &mut self,
        update_cpu_time: Duration,
        draw_cpu_time_excluding_acquire: Duration,
        presented_at: Instant,
    ) {
        self.update_frame_prepare_lead_estimate(update_cpu_time);
        self.update_draw_lead_estimate(draw_cpu_time_excluding_acquire);
        self.last_presented_at = presented_at;
        self.last_frame_opportunity_at = presented_at;
    }
}

impl HeuristicFrameClock {
    fn earliest_surface_acquire_at(&self) -> Instant {
        self.last_presented_at + SURFACE_ACQUIRE_GUARD_BAND
    }

    #[cfg(all(feature = "subduction", target_os = "macos"))]
    fn estimated_frame_prepare_lead_time(&self) -> Duration {
        self.estimated_frame_prepare_lead_time
    }

    #[cfg(all(feature = "subduction", target_os = "macos"))]
    fn estimated_draw_lead_time(&self) -> Duration {
        self.estimated_draw_lead_time
    }

    fn update_frame_prepare_lead_estimate(&mut self, observed_cpu_time: Duration) {
        let target = observed_cpu_time + Duration::from_micros(500);
        self.estimated_frame_prepare_lead_time =
            max_duration(self.estimated_frame_prepare_lead_time, target);
        self.estimated_frame_prepare_lead_time =
            (self.estimated_frame_prepare_lead_time * 7 + target) / 8;
    }

    fn update_draw_lead_estimate(&mut self, observed_cpu_time: Duration) {
        let target = observed_cpu_time + Duration::from_micros(500);
        self.estimated_draw_lead_time = max_duration(self.estimated_draw_lead_time, target);
        self.estimated_draw_lead_time = (self.estimated_draw_lead_time * 7 + target) / 8;
    }
}

#[cfg(any(
    all(feature = "subduction", target_os = "macos"),
    all(feature = "subduction", target_os = "windows")
))]
#[derive(Debug)]
struct SubductionPlanState {
    heuristic: HeuristicFrameClock,
    scheduler: Scheduler,
    output: OutputId,
    timebase: Timebase,
    host_origin: HostTime,
    instant_origin: Instant,
    latest_tick: Option<FrameTick>,
    latest_hints: Option<PresentHints>,
    latest_plan: Option<FramePlan>,
    pending_feedback: Option<PendingFeedback>,
    latest_prepare_start: Option<HostTime>,
    active: bool,
}

#[cfg(any(
    all(feature = "subduction", target_os = "macos"),
    all(feature = "subduction", target_os = "windows")
))]
impl SubductionPlanState {
    fn new(output: OutputId, scheduler: Scheduler, now: HostTime, timebase: Timebase) -> Self {
        Self {
            heuristic: HeuristicFrameClock::default(),
            scheduler,
            output,
            timebase,
            host_origin: now,
            instant_origin: Instant::now(),
            latest_tick: None,
            latest_hints: None,
            latest_plan: None,
            pending_feedback: None,
            latest_prepare_start: None,
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

    fn latest_commit_deadline(&self) -> Option<Instant> {
        self.latest_plan
            .map(|plan| self.host_to_instant(plan.commit_deadline))
    }

    fn current_frame_time(
        &self,
        frame_interval: Duration,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
        if let Some(plan) = self.latest_plan {
            let predicted_present = plan.present_time.map(|present| self.host_to_instant(present));
            return FrameTime {
                now: self.host_to_instant(plan.semantic_time),
                interval: PresentationInterval {
                    deadline_min: now,
                    deadline_max: predicted_present
                        .unwrap_or_else(|| self.host_to_instant(plan.commit_deadline)),
                    predicted_present,
                    background_rendering,
                },
                frame_interval: self.latest_frame_interval(frame_interval),
                frame_index: plan.frame_index,
            };
        }

        self.heuristic
            .current_frame_time(frame_interval, now, background_rendering)
    }

    fn frame_prepare_deadline(&self, frame_interval: Duration, now: Instant) -> Instant {
        if let Some(commit_deadline) = self.latest_commit_deadline() {
            let max_total_lead = frame_interval
                .checked_sub(Duration::from_millis(1))
                .unwrap_or(frame_interval);
            let lead_time = min_duration(
                max_duration(
                    self.heuristic.estimated_frame_prepare_lead_time()
                        + self.heuristic.estimated_draw_lead_time(),
                    Duration::from_millis(1),
                ),
                max_total_lead,
            );
            return commit_deadline.checked_sub(lead_time).unwrap_or(now);
        }

        self.heuristic.frame_prepare_deadline(frame_interval, now)
    }

    fn redraw_deadline(&self, frame_interval: Duration, now: Instant) -> Instant {
        if let Some(commit_deadline) = self.latest_commit_deadline() {
            let max_lead = frame_interval
                .checked_div(2)
                .unwrap_or(Duration::from_millis(1));
            let lead_time = min_duration(
                max_duration(
                    self.heuristic.estimated_draw_lead_time(),
                    Duration::from_millis(1),
                ),
                max_lead,
            );
            return commit_deadline
                .checked_sub(lead_time)
                .unwrap_or(now)
                .max(self.heuristic.earliest_surface_acquire_at());
        }

        self.heuristic.redraw_deadline(frame_interval, now)
    }

    fn observe_presented(
        &mut self,
        update_cpu_time: Duration,
        draw_cpu_time_excluding_acquire: Duration,
        presented_at: Instant,
    ) {
        self.heuristic.observe_presented(
            update_cpu_time,
            draw_cpu_time_excluding_acquire,
            presented_at,
        );

        if let (Some(hints), Some(build_start)) = (self.latest_hints, self.latest_prepare_start) {
            self.pending_feedback = Some(PendingFeedback {
                hints,
                build_start,
                submitted_at: self.instant_to_host(presented_at),
            });
        }
    }

    fn observe_new_plan(&mut self, tick: FrameTick, hints: PresentHints, plan: FramePlan) {
        if self
            .latest_plan
            .is_some_and(|latest_plan| latest_plan.frame_index != plan.frame_index)
        {
            // A newer platform frame opportunity arrived before draw.
            // Drop the "prepared" latch so Floem can re-prepare against the freshest plan.
            self.heuristic.clear_prepared_frame();
            self.latest_prepare_start = None;
        }

        self.latest_tick = Some(tick);
        self.latest_hints = Some(hints);
        self.latest_plan = Some(plan);
    }
}

#[cfg(all(feature = "subduction", target_os = "macos"))]
#[derive(Debug)]
struct SubductionFrameClock {
    plan_state: SubductionPlanState,
    display_link: DisplayLink,
}

#[cfg(all(feature = "subduction", target_os = "macos"))]
impl SubductionFrameClock {
    fn new(window_id: WindowId, output: OutputId, mtm: MainThreadMarker) -> Self {
        let display_link = DisplayLink::new(
            move |tick| {
                Application::send_proxy_event(UserEvent::SubductionFrameTick { window_id, tick });
            },
            output,
            mtm,
        );
        display_link.start();
        display_link.set_paused(true);

        Self {
            plan_state: SubductionPlanState::new(
                output,
                Scheduler::new(SchedulerConfig::macos()),
                subduction_now(),
                timebase(),
            ),
            display_link,
        }
    }
}

#[cfg(all(feature = "subduction", target_os = "macos"))]
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

    fn mark_frame_prepared(&mut self) {
        self.plan_state.heuristic.mark_frame_prepared();
    }

    fn clear_prepared_frame(&mut self) {
        self.plan_state.heuristic.clear_prepared_frame();
    }

    fn has_preparable_frame_work(&self, has_next_frame_work: bool) -> bool {
        self.plan_state
            .heuristic
            .has_preparable_frame_work(has_next_frame_work)
    }

    fn frame_prepare_deadline(&self, frame_interval: Duration, now: Instant) -> Instant {
        self.plan_state.frame_prepare_deadline(frame_interval, now)
    }

    fn redraw_deadline(&self, frame_interval: Duration, now: Instant) -> Instant {
        self.plan_state.redraw_deadline(frame_interval, now)
    }

    fn observe_presented(
        &mut self,
        update_cpu_time: Duration,
        draw_cpu_time_excluding_acquire: Duration,
        presented_at: Instant,
    ) {
        self.plan_state
            .observe_presented(update_cpu_time, draw_cpu_time_excluding_acquire, presented_at);
    }

    fn set_active(&mut self, active: bool) {
        if self.plan_state.active == active {
            return;
        }
        self.plan_state.active = active;
        self.display_link.set_paused(!active);
    }

    fn receive_frame_tick(&mut self, tick: FrameTick) {
        if let Some(pending_feedback) = self.plan_state.pending_feedback.take() {
            let feedback = pending_feedback.resolve(tick.prev_actual_present);
            self.plan_state.scheduler.observe(&feedback);
        }

        let safety = HostDuration(self.plan_state.scheduler.safety_margin_ticks());
        let hints = compute_present_hints(&tick, safety);
        let plan = self.plan_state.scheduler.plan(&tick, &hints);
        self.plan_state.observe_new_plan(
            FrameTick {
                output: self.plan_state.output,
                ..tick
            },
            hints,
            plan,
        );
    }
}

#[cfg(all(feature = "subduction", target_os = "windows"))]
#[derive(Debug)]
struct WindowsSubductionFrameClock {
    plan_state: SubductionPlanState,
    next_frame_index: u64,
    prev_present_time: Option<HostTime>,
}

#[cfg(all(feature = "subduction", target_os = "windows"))]
impl WindowsSubductionFrameClock {
    fn new(output: OutputId) -> Self {
        Self {
            plan_state: SubductionPlanState::new(
                output,
                Scheduler::new(SchedulerConfig::windows()),
                windows_now(),
                windows_timebase(),
            ),
            next_frame_index: 0,
            prev_present_time: None,
        }
    }
}

#[cfg(all(feature = "subduction", target_os = "windows"))]
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
                .latest_commit_deadline()
                .is_some_and(|deadline| now >= deadline);
        if !needs_new_plan {
            return;
        }

        let refresh_ns = frame_interval.as_nanos().min(u64::MAX as u128) as u64;
        let tick = windows_make_tick(refresh_ns, self.next_frame_index, self.prev_present_time);
        self.next_frame_index = self.next_frame_index.saturating_add(1);

        if let Some(pending_feedback) = self.plan_state.pending_feedback.take() {
            let feedback = pending_feedback.resolve(tick.prev_actual_present);
            self.plan_state.scheduler.observe(&feedback);
        }

        let safety_ns = self
            .plan_state
            .scheduler
            .safety_margin_ticks()
            .saturating_mul(u64::from(self.plan_state.timebase.numer))
            / u64::from(self.plan_state.timebase.denom);
        let hints = windows_compute_present_hints(&tick, safety_ns);
        let plan = self.plan_state.scheduler.plan(&tick, &hints);
        self.plan_state.observe_new_plan(
            FrameTick {
                output: self.plan_state.output,
                ..tick
            },
            hints,
            plan,
        );
    }

    fn note_frame_prepare_started(&mut self, now: Instant) {
        self.plan_state.latest_prepare_start = Some(self.plan_state.instant_to_host(now));
    }

    fn mark_frame_prepared(&mut self) {
        self.plan_state.heuristic.mark_frame_prepared();
    }

    fn clear_prepared_frame(&mut self) {
        self.plan_state.heuristic.clear_prepared_frame();
    }

    fn has_preparable_frame_work(&self, has_next_frame_work: bool) -> bool {
        self.plan_state
            .heuristic
            .has_preparable_frame_work(has_next_frame_work)
    }

    fn frame_prepare_deadline(&self, frame_interval: Duration, now: Instant) -> Instant {
        self.plan_state.frame_prepare_deadline(frame_interval, now)
    }

    fn redraw_deadline(&self, frame_interval: Duration, now: Instant) -> Instant {
        self.plan_state.redraw_deadline(frame_interval, now)
    }

    fn observe_presented(
        &mut self,
        update_cpu_time: Duration,
        draw_cpu_time_excluding_acquire: Duration,
        presented_at: Instant,
    ) {
        self.prev_present_time = Some(self.plan_state.instant_to_host(presented_at));
        self.plan_state
            .observe_presented(update_cpu_time, draw_cpu_time_excluding_acquire, presented_at);
    }

    fn set_active(&mut self, active: bool) {
        self.plan_state.active = active;
    }
}
