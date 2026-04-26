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

/// Reason the frame is being produced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameWorkload {
    /// User input or other latency-sensitive work.
    Input,
    /// Smooth animation work where even cadence is preferred.
    #[default]
    Animation,
}

/// Coarse dirty-work class used for frame pacing estimates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameDamageClass {
    /// No window render work is pending.
    #[default]
    CompositorOnly,
    /// Paint/render work without style or layout.
    PaintOnly,
    /// Style may affect paint, but layout is not currently dirty.
    StylePaint,
    /// Layout or box-tree work is dirty.
    Layout,
}

/// Renderer timing observations for scheduler feedback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameTimingFeedback {
    pub render_cpu: Option<Duration>,
    pub present_cpu: Option<Duration>,
    pub gpu: Option<Duration>,
    pub damage_class: FrameDamageClass,
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
