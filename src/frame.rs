use crate::platform::{Duration, Instant};
use understory_frame_pacing::{
    DisplayTiming as PacingDisplayTiming, Duration as PacingDuration,
    FrameRatePlan as PacingFrameRatePlan, FrameRatePreference as PacingFrameRatePreference,
    choose_frame_rate_source_interval,
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
        update_granularity: Option<Duration>,
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

/// Frame-rate hint and acceptable fallback range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameRatePreference(PacingFrameRatePreference);

impl FrameRatePreference {
    /// No throttling by this policy.
    #[must_use]
    pub const fn full() -> Self {
        Self(PacingFrameRatePreference::full())
    }

    /// Runs no faster than `fps`, choosing a clean lower cadence on fixed-rate
    /// displays when `fps` is not directly supported.
    #[must_use]
    pub fn at_most(fps: f64) -> Option<Self> {
        PacingFrameRatePreference::at_most(fps).map(Self)
    }

    /// Starts a preference builder with a preferred FPS.
    #[must_use]
    pub fn preferred(fps: f64) -> Option<FrameRatePreferenceBuilder> {
        PacingFrameRatePreference::preferred(fps).map(FrameRatePreferenceBuilder)
    }

    /// Starts a preference builder with an acceptable FPS range.
    #[must_use]
    pub fn range(min_fps: f64, max_fps: f64) -> Option<FrameRatePreferenceBuilder> {
        PacingFrameRatePreference::range(min_fps, max_fps).map(FrameRatePreferenceBuilder)
    }

    pub(crate) const fn pacing(self) -> PacingFrameRatePreference {
        self.0
    }

    pub(crate) const fn is_full(self) -> bool {
        matches!(self.0, PacingFrameRatePreference::Full)
    }

    pub(crate) fn effective_interval(self, frame_time: Option<FrameTime>) -> Option<Duration> {
        self.plan(frame_time)
            .map(FrameRatePlan::delivery_interval)
            .or_else(|| match frame_time {
                Some(frame_time) => Some(frame_time.frame_interval),
                None => self.0.preferred_interval().map(std_duration_from_pacing),
            })
    }

    pub(crate) fn plan(self, frame_time: Option<FrameTime>) -> Option<FrameRatePlan> {
        let frame_time = frame_time?;
        self.0
            .plan(pacing_display_timing(frame_time.interval.display_timing))
            .map(FrameRatePlan)
    }
}

/// Source and delivery cadence selected for a frame-rate preference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameRatePlan(PacingFrameRatePlan);

impl FrameRatePlan {
    #[must_use]
    pub(crate) fn delivery_interval(self) -> Duration {
        std_duration_from_pacing(self.0.delivery_interval())
    }
}

impl Default for FrameRatePreference {
    fn default() -> Self {
        Self::full()
    }
}

impl From<f64> for FrameRatePreference {
    fn from(fps: f64) -> Self {
        Self::at_most(fps).unwrap_or_else(Self::full)
    }
}

/// Builder for [`FrameRatePreference`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameRatePreferenceBuilder(understory_frame_pacing::FrameRatePreferenceBuilder);

impl FrameRatePreferenceBuilder {
    /// Sets the preferred FPS.
    #[must_use]
    pub fn preferred(self, fps: f64) -> Option<Self> {
        self.0.preferred(fps).map(Self)
    }

    /// Sets the minimum acceptable FPS.
    #[must_use]
    pub fn minimum(self, fps: f64) -> Option<Self> {
        self.0.minimum(fps).map(Self)
    }

    /// Sets the maximum acceptable FPS.
    #[must_use]
    pub fn maximum(self, fps: f64) -> Option<Self> {
        self.0.maximum(fps).map(Self)
    }

    /// Sets an acceptable FPS range.
    #[must_use]
    pub fn range(self, min_fps: f64, max_fps: f64) -> Option<Self> {
        self.0.range(min_fps, max_fps).map(Self)
    }

    /// Builds the preference.
    #[must_use]
    pub const fn build(self) -> FrameRatePreference {
        FrameRatePreference(self.0.build())
    }
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
    pub source_interval: Duration,
    pub frame_interval: Duration,
    pub frame_index: u64,
}

/// Repeat policy for an animation-frame callback.
///
/// Floem removes due callbacks before dispatch, so callbacks may schedule or
/// cancel other callbacks while they run. A repeating callback is reinserted
/// after dispatch unless it has been cancelled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameCallbackRepeat {
    /// Run once at the next eligible begin-frame opportunity.
    None,
    /// Run at each eligible begin-frame opportunity.
    EveryFrame,
}

impl FrameCallbackRepeat {
    /// Creates a one-shot policy.
    #[must_use]
    pub const fn none() -> Self {
        Self::None
    }

    /// Creates a repeating policy.
    #[must_use]
    pub const fn every_frame() -> Self {
        Self::EveryFrame
    }

    /// Returns whether this policy repeats after dispatch.
    #[must_use]
    pub const fn is_repeating(self) -> bool {
        matches!(self, Self::EveryFrame)
    }
}

#[must_use]
pub(crate) fn frame_rate_due(frame_time: FrameTime, preference: FrameRatePreference) -> bool {
    preference.pacing().should_deliver(
        frame_time.frame_index,
        pacing_display_timing(frame_time.interval.display_timing),
        pacing_duration_from_std(frame_time.source_interval),
    )
}

#[must_use]
pub(crate) fn frame_rate_interval(
    preference: FrameRatePreference,
    frame_time: Option<FrameTime>,
) -> Option<Duration> {
    preference.effective_interval(frame_time)
}

#[must_use]
pub(crate) fn frame_rate_source_interval(
    preferences: &[FrameRatePreference],
    display_timing: DisplayTiming,
) -> Duration {
    let preferences = preferences
        .iter()
        .map(|preference| preference.pacing())
        .collect::<Vec<_>>();
    std_duration_from_pacing(choose_frame_rate_source_interval(
        &preferences,
        pacing_display_timing(display_timing),
    ))
}

fn pacing_display_timing(display_timing: DisplayTiming) -> PacingDisplayTiming {
    match display_timing {
        DisplayTiming::Fixed { interval } => {
            PacingDisplayTiming::fixed(pacing_duration_from_std(interval))
        }
        DisplayTiming::Variable {
            min_frame_interval,
            max_frame_interval,
            update_granularity,
        } => PacingDisplayTiming::variable(
            pacing_duration_from_std(min_frame_interval),
            pacing_duration_from_std(max_frame_interval),
            update_granularity.map(pacing_duration_from_std),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn variable_47_to_75hz() -> DisplayTiming {
        DisplayTiming::Variable {
            min_frame_interval: Duration::from_nanos(13_333_333),
            max_frame_interval: Duration::from_nanos(21_276_596),
            update_granularity: None,
        }
    }

    fn frame_time(frame_index: u64, source_interval: Duration) -> FrameTime {
        let now = Instant::now();
        FrameTime {
            now,
            interval: PresentationInterval {
                deadline_min: now,
                deadline_max: now,
                predicted_present: Some(now),
                display_timing: variable_47_to_75hz(),
                present_pacing: PresentPacing::AtTime(now),
                background_rendering: false,
            },
            source_interval,
            frame_interval: Duration::from_nanos(13_333_333),
            frame_index,
        }
    }

    #[test]
    fn throttling_uses_selected_source_interval_not_reported_display_interval() {
        let preference = FrameRatePreference::at_most(60.0).unwrap();
        let source_interval = Duration::from_nanos(16_666_667);

        assert!((0..75).all(|frame_index| frame_rate_due(
            frame_time(frame_index, source_interval),
            preference
        )));
    }

    #[test]
    fn throttling_divides_full_rate_source_when_full_consumer_keeps_source_fast() {
        let source_interval = Duration::from_nanos(13_333_333);
        let at_most_60 = FrameRatePreference::at_most(60.0).unwrap();
        let at_most_30 = FrameRatePreference::at_most(30.0).unwrap();

        let delivered_60 = (0..75)
            .filter(|frame_index| {
                frame_rate_due(frame_time(*frame_index, source_interval), at_most_60)
            })
            .count();
        let delivered_30 = (0..75)
            .filter(|frame_index| {
                frame_rate_due(frame_time(*frame_index, source_interval), at_most_30)
            })
            .count();

        assert_eq!(delivered_60, 60);
        assert_eq!(delivered_30, 30);
    }
}
