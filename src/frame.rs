use crate::platform::{Duration, Instant};

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
        /// External surface producers requesting frame cadence.
        const EXTERNAL_SURFACE = 1 << 3;
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

/// Outcome information for content prepared during a frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameOutcome {
    pub draw_attempted: bool,
    pub draw_completed: bool,
    pub missed_deadline: Option<bool>,
}
