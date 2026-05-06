use crate::{
    Application,
    app::UserEvent,
    frame::{DisplayTiming, FrameTime, PresentPacing, PresentationInterval},
    platform::{Duration, Instant},
};
use raw_window_handle::HasWindowHandle;
use subduction::{
    FrameSourceDisplayTiming, FrameSourceTarget, frame_source::FrameSource as SubductionFrameSource,
};
use subduction_core::output::OutputId;
use winit::window::{Window as WinitWindow, WindowId};

pub(crate) fn frame_pacing_diag_enabled() -> bool {
    std::env::var_os("FLOEM_FRAME_PACING_DIAG").is_some()
}

pub(crate) fn diagnostic_logs_enabled() -> bool {
    std::env::var_os("FLOEM_DIAG").is_some()
        || frame_pacing_diag_enabled()
        || std::env::var_os("FLOEM_SUBDUCTION_TIMING_DIAG").is_some()
        || std::env::var_os("FLOEM_CUBE_DIAG").is_some()
        || std::env::var_os("FLOEM_RESIZE_DIAG").is_some()
}

pub(crate) fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

pub(crate) fn duration_hz(duration: Duration) -> f64 {
    let seconds = duration.as_secs_f64();
    if seconds > 0.0 { 1.0 / seconds } else { 0.0 }
}

pub(crate) fn window_frame_interval(window: &dyn WinitWindow) -> Duration {
    window
        .current_monitor()
        .and_then(|monitor| monitor.current_video_mode())
        .and_then(|mode| mode.refresh_rate_millihertz())
        .map(|mhz| Duration::from_nanos(1_000_000_000_000 / mhz.get() as u64))
        .unwrap_or(Duration::from_millis(16))
}

#[derive(Debug)]
pub(crate) struct FrameSource {
    window_id: WindowId,
    inner: SubductionFrameSource,
    target: Option<FrameSourceTarget>,
    preferred_source_interval: Option<Duration>,
    observed_source_interval: Option<Duration>,
    last_logged_observed_source_interval: Option<Duration>,
    latest_tick_received_at: Option<Instant>,
    preferred_source_millihertz: Option<u32>,
    active: bool,
}

pub(crate) fn new_window_frame_source(window_id: WindowId, output_id: u32) -> FrameSource {
    let inner = SubductionFrameSource::new(OutputId(output_id), move |tick| {
        if frame_pacing_diag_enabled() {
            crate::floem_debug_log!(
                "floem frame pacing display link callback window={:?} tick={} predicted={:?} refresh={:?}",
                window_id,
                tick.frame_index,
                tick.predicted_present,
                tick.refresh_interval,
            );
        }
        Application::send_proxy_event(UserEvent::FrameTick { window_id, tick });
    });
    FrameSource {
        window_id,
        inner,
        target: None,
        preferred_source_interval: None,
        observed_source_interval: None,
        last_logged_observed_source_interval: None,
        latest_tick_received_at: None,
        preferred_source_millihertz: None,
        active: false,
    }
}

impl FrameSource {
    pub(crate) fn frame_interval(&mut self, window: &dyn WinitWindow) -> Duration {
        self.inner.frame_interval(window_frame_interval(window))
    }

    pub(crate) fn display_timing(&self, fallback: Duration) -> DisplayTiming {
        match self.inner.display_timing(fallback) {
            FrameSourceDisplayTiming::Fixed { interval_ns } => {
                DisplayTiming::fixed(Duration::from_nanos(interval_ns))
            }
            FrameSourceDisplayTiming::Variable {
                min_interval_ns,
                max_interval_ns,
                update_granularity_ns,
            } => DisplayTiming::Variable {
                min_frame_interval: Duration::from_nanos(min_interval_ns),
                max_frame_interval: Duration::from_nanos(max_interval_ns),
                update_granularity: update_granularity_ns.map(Duration::from_nanos),
            },
        }
    }

    pub(crate) fn refresh_window_target(&mut self, window: &dyn WinitWindow) -> bool {
        let monitor = window.current_monitor();
        let raw_window_handle = window.window_handle().ok().map(|handle| handle.as_raw());
        let target = FrameSourceTarget {
            monitor_name: monitor
                .as_ref()
                .and_then(|monitor| monitor.name().map(|name| name.to_string())),
            refresh_millihertz: monitor
                .as_ref()
                .and_then(|monitor| monitor.current_video_mode())
                .and_then(|mode| mode.refresh_rate_millihertz())
                .map(|rate| rate.get()),
            raw_window_handle,
        };
        if frame_pacing_diag_enabled() {
            crate::floem_debug_log!(
                "floem frame source target window={:?} monitor={:?} refresh_millihz={:?}",
                self.window_id,
                target.monitor_name,
                target.refresh_millihertz,
            );
        }
        let changed = self.target.as_ref() != Some(&target);
        if !changed {
            return false;
        }
        self.target = Some(target.clone());
        self.inner.refresh_target(target);
        if self.active && self.preferred_source_millihertz.is_some() {
            self.inner
                .set_preferred_frame_rate_millihertz(self.preferred_source_millihertz);
        }
        true
    }

    pub(crate) fn set_preferred_source_interval(&mut self, interval: Duration) {
        let nanos = interval.as_nanos();
        let millihertz = (nanos > 0)
            .then(|| ((1_000_000_000_000u128 + (nanos / 2)) / nanos).min(u32::MAX as u128) as u32);
        if self.preferred_source_interval == Some(interval)
            && self.preferred_source_millihertz == millihertz
        {
            return;
        }
        self.preferred_source_interval = Some(interval);
        self.preferred_source_millihertz = millihertz;
        if frame_pacing_diag_enabled() {
            crate::floem_debug_log!(
                "floem frame source preferred window={:?} interval={:.3}ms hz={:.3} millihertz={:?} observed={:?}",
                self.window_id,
                duration_ms(interval),
                duration_hz(interval),
                self.preferred_source_millihertz,
                self.observed_source_interval
                    .map(|interval| (duration_ms(interval), duration_hz(interval))),
            );
        }
        if self.active {
            self.inner
                .set_preferred_frame_rate_millihertz(self.preferred_source_millihertz);
        } else if frame_pacing_diag_enabled() {
            crate::floem_debug_log!(
                "floem frame source preferred deferred window={:?} millihertz={:?}",
                self.window_id,
                self.preferred_source_millihertz,
            );
        }
    }

    pub(crate) fn current_frame_time(
        &mut self,
        window: &dyn WinitWindow,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
        let frame_interval = self.frame_interval(window);
        let source_interval = self
            .preferred_source_interval
            .or(self.observed_source_interval)
            .unwrap_or(frame_interval);
        let display_timing = self.display_timing(frame_interval);
        if let Some(tick) = self.inner.latest_tick()
            && tick.predicted_present.is_some()
        {
            let predicted_present = self.tick_predicted_present_instant(tick, now);
            return FrameTime {
                now: predicted_present,
                interval: PresentationInterval {
                    deadline_min: now,
                    deadline_max: predicted_present,
                    predicted_present: Some(predicted_present),
                    display_timing,
                    present_pacing: PresentPacing::AtTime(predicted_present),
                    background_rendering,
                },
                source_interval,
                frame_interval,
                frame_index: tick.frame_index,
            };
        }

        let predicted_present = now.checked_add(frame_interval).unwrap_or(now);
        FrameTime {
            now: predicted_present,
            interval: PresentationInterval {
                deadline_min: now,
                deadline_max: predicted_present,
                predicted_present: Some(predicted_present),
                display_timing,
                present_pacing: PresentPacing::AtTime(predicted_present),
                background_rendering,
            },
            source_interval,
            frame_interval,
            frame_index: self.inner.frame_counter(),
        }
    }

    pub(crate) fn receive_frame_tick(&mut self, tick: subduction_core::timing::FrameTick) {
        self.latest_tick_received_at = Some(Instant::now());
        self.observed_source_interval = self
            .inner
            .latest_tick()
            .and_then(|previous| {
                previous.predicted_present.zip(tick.predicted_present).map(
                    |(previous_present, predicted_present)| {
                        let previous = self.inner.host_to_instant(previous_present);
                        let predicted = self.inner.host_to_instant(predicted_present);
                        if predicted >= previous {
                            predicted.saturating_duration_since(previous)
                        } else {
                            previous.saturating_duration_since(predicted)
                        }
                    },
                )
            })
            .filter(|interval| !interval.is_zero());
        if frame_pacing_diag_enabled()
            && self.observed_source_interval.is_some()
            && self.last_logged_observed_source_interval != self.observed_source_interval
        {
            let interval = self.observed_source_interval.unwrap();
            crate::floem_debug_log!(
                "floem frame source observed window={:?} interval={:.3}ms hz={:.3} preferred={:?}",
                self.window_id,
                duration_ms(interval),
                duration_hz(interval),
                self.preferred_source_interval
                    .map(|interval| (duration_ms(interval), duration_hz(interval))),
            );
            self.last_logged_observed_source_interval = self.observed_source_interval;
        }
        self.inner.receive_frame_tick(tick);
    }

    fn tick_predicted_present_instant(
        &self,
        tick: subduction_core::timing::FrameTick,
        now: Instant,
    ) -> Instant {
        let Some(predicted_present) = tick.predicted_present else {
            return now;
        };
        let absolute = self.inner.host_to_instant(predicted_present);
        if absolute > now {
            return absolute;
        }

        let relative = if predicted_present >= tick.now {
            let tick_now = self.inner.host_to_instant(tick.now);
            let tick_predicted = self.inner.host_to_instant(predicted_present);
            let delta = tick_predicted.saturating_duration_since(tick_now);
            self.latest_tick_received_at
                .unwrap_or(now)
                .checked_add(delta)
                .unwrap_or(now)
        } else {
            now
        };
        if frame_pacing_diag_enabled() {
            crate::floem_debug_log!(
                "floem frame source stale predicted window={:?} tick={} absolute_late_by={:.3}ms relative_from_tick={:.3}ms",
                self.window_id,
                tick.frame_index,
                now.saturating_duration_since(absolute).as_secs_f64() * 1000.0,
                relative.saturating_duration_since(now).as_secs_f64() * 1000.0,
            );
        }
        relative.max(now)
    }

    pub(crate) fn latest_tick(&self) -> Option<subduction_core::timing::FrameTick> {
        self.inner.latest_tick()
    }

    pub(crate) fn set_active(&mut self, active: bool) {
        let changed = self.active != active;
        self.active = active;
        if changed && active {
            if frame_pacing_diag_enabled() {
                crate::floem_debug_log!(
                    "floem frame source preferred apply-on-active window={:?} millihertz={:?}",
                    self.window_id,
                    self.preferred_source_millihertz,
                );
            }
            self.inner
                .set_preferred_frame_rate_millihertz(self.preferred_source_millihertz);
        }
        self.inner.set_active(active);
    }
}
