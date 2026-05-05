use crate::platform::{Duration, Instant};
use understory_frame_pacing::{
    DisplayTiming as PacingDisplayTiming, Duration as PacingDuration, TargetFrameCadence,
};

/// Runtime display timing capabilities for the frame target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayTiming {
    /// The display refreshes at a fixed cadence.
    Fixed { interval: Duration },
    /// The display can vary how long a frame remains visible.
    Variable {
        min_frame_interval: Duration,
        max_frame_interval: Duration,
    },
}

impl DisplayTiming {
    #[must_use]
    pub fn fixed(interval: Duration) -> Self {
        Self::Fixed { interval }
    }

    #[must_use]
    pub fn is_variable(self) -> bool {
        matches!(self, Self::Variable { .. })
    }
}

/// Backend present pacing selected for a prepared frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentPacing {
    /// Submit/present as soon as the backend accepts the frame.
    AsSoonAsPossible,
    /// Target an absolute presentation time.
    AtTime(Instant),
    /// On variable-rate displays, keep frames visible for at least this long.
    AfterMinimumDuration(Duration),
}

bitflags::bitflags! {
    /// Reasons the frame is being produced.
    ///
    /// Multiple causes can be pending at once. The frame clock derives an
    /// effective scheduling policy from the full set instead of forcing event
    /// handling to collapse demand to a single priority too early.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct FrameDemand: u8 {
        /// Discrete input such as key presses, clicks, taps, or IME.
        const DISCRETE_INPUT = 1 << 0;
        /// Continuous/coalesced input such as pointer move, scroll, or gesture.
        const CONTINUOUS_INPUT = 1 << 1;
        /// Animation timeline callbacks.
        const ANIMATION = 1 << 2;
        /// Compositor-surface producers requesting frame cadence.
        const COMPOSITOR_SURFACE = 1 << 3;
    }
}

impl FrameDemand {
    /// Take the current demand and leave the source empty.
    #[must_use]
    pub fn take(&mut self) -> Self {
        let demand = *self;
        *self = Self::empty();
        demand
    }
}

/// Renderer timing observations for scheduler feedback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameTimingFeedback {
    pub render_cpu: Option<Duration>,
    pub present_cpu: Option<Duration>,
    pub gpu: Option<Duration>,
}

/// The presentation interval targeted by the current frame opportunity.
#[derive(Clone, Copy, Debug)]
pub struct PresentationInterval {
    pub deadline_min: Instant,
    pub deadline_max: Instant,
    pub predicted_present: Option<Instant>,
    pub display_timing: DisplayTiming,
    pub present_pacing: PresentPacing,
    pub background_rendering: bool,
}

/// Timing information delivered to begin-frame callbacks.
#[derive(Clone, Copy, Debug)]
pub struct FrameTime {
    pub now: Instant,
    pub interval: PresentationInterval,
    pub frame_interval: Duration,
    pub frame_index: u64,
}

#[must_use]
pub(crate) fn target_frame_cadence(target_fps: Option<f64>) -> Option<TargetFrameCadence> {
    target_fps.and_then(TargetFrameCadence::from_fps)
}

#[must_use]
pub(crate) fn target_frame_due(frame_time: FrameTime, target_fps: Option<f64>) -> bool {
    let Some(cadence) = target_frame_cadence(target_fps) else {
        return true;
    };
    cadence.should_deliver(
        frame_time.frame_index,
        pacing_display_timing(frame_time.interval.display_timing),
        pacing_duration_from_std(frame_time.frame_interval),
    )
}

#[must_use]
pub(crate) fn target_frame_interval(
    target_fps: Option<f64>,
    frame_time: Option<FrameTime>,
) -> Option<Duration> {
    let Some(cadence) = target_frame_cadence(target_fps) else {
        return frame_time.map(|frame_time| frame_time.frame_interval);
    };
    let Some(frame_time) = frame_time else {
        return Some(std_duration_from_pacing(cadence.target_interval()));
    };
    Some(std_duration_from_pacing(cadence.effective_interval(
        pacing_display_timing(frame_time.interval.display_timing),
    )))
}

fn pacing_display_timing(display_timing: DisplayTiming) -> PacingDisplayTiming {
    match display_timing {
        DisplayTiming::Fixed { interval } => {
            PacingDisplayTiming::fixed(pacing_duration_from_std(interval))
        }
        DisplayTiming::Variable {
            min_frame_interval,
            max_frame_interval,
        } => PacingDisplayTiming::variable(
            pacing_duration_from_std(min_frame_interval),
            pacing_duration_from_std(max_frame_interval),
            None,
        ),
    }
}

fn pacing_duration_from_std(duration: Duration) -> PacingDuration {
    PacingDuration::from_nanos(duration.as_nanos().min(u64::MAX as u128) as u64)
}

fn std_duration_from_pacing(duration: PacingDuration) -> Duration {
    Duration::from_nanos(duration.as_nanos())
}

/// Outcome information for content prepared during a frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameOutcome {
    pub draw_attempted: bool,
    pub draw_completed: bool,
    pub missed_deadline: Option<bool>,
}
