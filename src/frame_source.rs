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
}

pub(crate) fn new_window_frame_source(window_id: WindowId, output_id: u32) -> FrameSource {
    let inner = SubductionFrameSource::new(OutputId(output_id), move |tick| {
        if frame_pacing_diag_enabled() {
            eprintln!(
                "floem frame pacing display link callback window={:?} tick={} predicted={:?} refresh={:?}",
                window_id, tick.frame_index, tick.predicted_present, tick.refresh_interval,
            );
        }
        Application::send_proxy_event(UserEvent::FrameTick { window_id, tick });
    });
    FrameSource {
        window_id,
        inner,
        target: None,
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
            } => DisplayTiming::Variable {
                min_frame_interval: Duration::from_nanos(min_interval_ns),
                max_frame_interval: Duration::from_nanos(max_interval_ns),
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
            eprintln!(
                "floem frame source target window={:?} monitor={:?} refresh_millihz={:?}",
                self.window_id, target.monitor_name, target.refresh_millihertz,
            );
        }
        let changed = self.target.as_ref() != Some(&target);
        self.target = Some(target.clone());
        self.inner.refresh_target(target);
        changed
    }

    pub(crate) fn current_frame_time(
        &mut self,
        window: &dyn WinitWindow,
        now: Instant,
        background_rendering: bool,
    ) -> FrameTime {
        let frame_interval = self.frame_interval(window);
        let display_timing = self.display_timing(frame_interval);
        if let Some(tick) = self.inner.latest_tick()
            && let Some(predicted_present) = tick.predicted_present
        {
            let predicted_present = self.inner.host_to_instant(predicted_present);
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
            frame_interval,
            frame_index: self.inner.frame_counter(),
        }
    }

    pub(crate) fn receive_frame_tick(&mut self, tick: subduction_core::timing::FrameTick) {
        self.inner.receive_frame_tick(tick);
    }

    pub(crate) fn latest_tick(&self) -> Option<subduction_core::timing::FrameTick> {
        self.inner.latest_tick()
    }

    pub(crate) fn set_active(&mut self, active: bool) {
        self.inner.set_active(active);
    }
}
