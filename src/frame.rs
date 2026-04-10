use crate::platform::{Duration, Instant};

/// The presentation interval targeted by the current frame opportunity.
#[derive(Clone, Copy, Debug)]
pub struct PresentationInterval {
    pub deadline_min: Instant,
    pub deadline_max: Instant,
    pub predicted_present: Option<Instant>,
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
