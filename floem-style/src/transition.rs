//! Animation and transition types for the style system.
//!
//! This module provides types for animating style property changes over time:
//! - [`Transition`] - Animation parameters (duration, easing)
//! - [`TransitionState`] - Internal state for tracking active transitions
//! - [`DirectTransition`] - Standalone transition controller
//!
//! The inspector preview for [`Transition`] (`Transition::debug_view`) lives
//! in the `floem` crate behind the `TransitionDebugViewExt` extension trait,
//! because it constructs views and so depends on `floem::view::View`.

use std::rc::Rc;

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

use crate::easing::{Bezier, Easing, Linear, Spring};
use crate::prop_value::StylePropValue;

#[derive(Clone, Debug)]
pub struct ActiveTransition<T: StylePropValue> {
    pub start: Instant,
    pub before: T,
    pub current: T,
    pub after: T,
}

#[derive(Clone, Debug)]
pub struct TransitionState<T: StylePropValue> {
    pub transition: Option<Transition>,
    pub active: Option<ActiveTransition<T>>,
    pub initial: bool,
}

impl<T: StylePropValue> TransitionState<T> {
    pub fn read(&mut self, transition: Option<Transition>) {
        self.transition = transition;
    }

    pub fn transition(&mut self, before: &T, after: &T) {
        if !self.initial {
            return;
        }
        if self.transition.is_some() {
            self.active = Some(ActiveTransition {
                start: Instant::now(),
                before: before.clone(),
                current: before.clone(),
                after: after.clone(),
            });
        }
    }

    /// Returns true if changed
    pub fn step(&mut self, now: &Instant, request_transition: &mut bool) -> bool {
        if !self.initial {
            // We have observed the initial value. Any further changes may trigger animations.
            self.initial = true;
        }
        if let Some(active) = &mut self.active {
            if let Some(transition) = &self.transition {
                let time = now.saturating_duration_since(active.start);
                let time_percent = time.as_secs_f64() / transition.duration.as_secs_f64();
                if (time < transition.duration || !transition.easing.finished(time_percent))
                    && let Some(i) = T::interpolate(
                        &active.before,
                        &active.after,
                        transition.easing.eval(time_percent),
                    )
                {
                    active.current = i;
                    *request_transition = true;
                    return true;
                }
            }
            // time has past duration, or the value is not interpolatable
            self.active = None;

            true
        } else {
            false
        }
    }

    pub fn get(&self, value: &T) -> T {
        if let Some(active) = &self.active {
            active.current.clone()
        } else {
            value.clone()
        }
    }
}

impl<T: StylePropValue> Default for TransitionState<T> {
    fn default() -> Self {
        Self {
            transition: None,
            active: None,
            initial: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Transition {
    pub duration: Duration,
    pub easing: Rc<dyn Easing>,
}

impl Transition {
    pub fn new(duration: Duration, easing: impl Easing + 'static) -> Self {
        Self {
            duration,
            easing: Rc::new(easing),
        }
    }

    pub fn linear(duration: Duration) -> Self {
        Self::new(duration, Linear)
    }

    pub fn ease_in_out(duration: Duration) -> Self {
        Self::new(duration, Bezier::ease_in_out())
    }

    pub fn spring(duration: Duration) -> Self {
        Self::new(duration, Spring::default())
    }
}

/// Direct transition controller using TransitionState without the Style system.
///
/// This allows you to animate any value that implements `StylePropValue` by managing
/// the transition state directly. You control when transitions start, how they're
/// configured, and when to step them forward.
///
/// # Example
///
/// ```rust
/// use std::time::{Duration, Instant};
/// use floem_style::{DirectTransition, Transition};
///
/// // Create a transition for animating opacity
/// let mut opacity = DirectTransition::new(1., None);
///
/// // Configure transition timing and easing
/// opacity.set_transition(Some(
///     Transition::ease_in_out(Duration::from_millis(300))
/// ));
///
/// // Start transition to new value
/// opacity.transition_to(0.0);
///
/// // Animation loop - call this every frame
/// let start_time = Instant::now();
/// loop {
///     let now = Instant::now();
///
///     // Step the transition forward
///     let changed = opacity.step(&now);
///
///     // Get current interpolated value
///     let current_opacity = opacity.get();
///
///     // Only update rendering if value changed
///     if changed {
///         println!("Current opacity: {:.3}", current_opacity);
///         // render_with_opacity(current_opacity);
///     }
///
///     // Exit when transition completes
///     if !opacity.is_active() {
///         println!("Transition complete!");
///         break;
///     }
///
///     // Wait for next frame (~60fps)
///     std::thread::sleep(Duration::from_millis(16));
///
///     // Safety timeout
///     if now.duration_since(start_time) > Duration::from_secs(2) {
///         break;
///     }
/// }
///
/// // Chain multiple transitions
/// opacity.transition_to(0.5); // Start new transition from current position
/// // ... repeat animation loop
///
/// // Or jump immediately without animation
/// opacity.set_immediate(1.0);
/// ```
#[derive(Debug, Clone)]
pub struct DirectTransition<T: StylePropValue> {
    pub current_value: T,
    transition_state: TransitionState<T>,
}

impl<T: StylePropValue> DirectTransition<T> {
    /// Create a new transition starting at the given value
    pub fn new(initial_value: T, transition: Option<Transition>) -> Self {
        let mut t = Self {
            current_value: initial_value,
            transition_state: TransitionState::default(),
        };
        t.transition_state.read(transition);
        t
    }

    /// Configure the transition timing and easing function
    ///
    /// Pass `None` to disable transitions (values will change immediately)
    pub fn set_transition(&mut self, transition: Option<Transition>) {
        // If we're currently transitioning, preserve the current interpolated state
        // as the new baseline instead of reverting to the original target
        if self.transition_state.active.is_some() {
            let current_interpolated = self.get();
            self.current_value = current_interpolated;
            self.transition_state.active = None;
        }

        self.transition_state.read(transition);
    }

    /// Start transitioning to a new target value
    ///
    /// Returns `true` if a transition was started, `false` if no transition
    /// is configured or the target equals the current value
    pub fn transition_to(&mut self, target: T) -> bool {
        let before = if self.transition_state.active.is_some() {
            // If already transitioning, start from current interpolated position
            self.get()
        } else {
            self.current_value.clone()
        };

        self.current_value = target;

        // Ensure transitions can start by marking as initialized
        if !self.transition_state.initial {
            self.transition_state.initial = true;
        }

        self.transition_state
            .transition(&before, &self.current_value);
        self.transition_state.active.is_some()
    }

    /// Step the transition forward in time
    ///
    /// Call this every frame with the current time. Returns `true` if the
    /// interpolated value changed this frame, `false` otherwise.
    ///
    /// You can use the return value to optimize rendering - only update
    /// when something actually changed.
    pub fn step(&mut self, now: &Instant) -> bool {
        let mut request_transition = false;
        self.transition_state.step(now, &mut request_transition)
    }

    /// Get the current interpolated value
    ///
    /// During a transition, this returns the smoothly interpolated value.
    /// When no transition is active, returns the target value.
    pub fn get(&self) -> T {
        self.transition_state.get(&self.current_value)
    }

    /// Check if a transition is currently active
    pub fn is_active(&self) -> bool {
        self.transition_state.active.is_some()
    }

    /// Get the target value (final destination of current/last transition)
    pub fn target(&self) -> &T {
        &self.current_value
    }

    /// Set value immediately without any transition
    ///
    /// This cancels any active transition and jumps directly to the new value
    pub fn set_immediate(&mut self, value: T) {
        self.current_value = value;
        self.transition_state.active = None;
    }

    /// Get the progress of the current transition as a value from 0.0 to 1.0
    ///
    /// Returns `None` if no transition is active or configured
    pub fn progress(&self, now: &Instant) -> Option<f64> {
        if let Some(active) = &self.transition_state.active {
            if let Some(transition) = &self.transition_state.transition {
                let elapsed = now.saturating_duration_since(active.start);
                let progress = elapsed.as_secs_f64() / transition.duration.as_secs_f64();
                Some(progress.min(1.0))
            } else {
                None
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    //! Standalone transition tests exercising the interpolation machinery
    //! without any floem/view scaffolding.

    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    fn instant_plus(base: Instant, millis: u64) -> Instant {
        base + Duration::from_millis(millis)
    }
    #[cfg(target_arch = "wasm32")]
    fn instant_plus(base: Instant, millis: u64) -> Instant {
        base + Duration::from_millis(millis)
    }

    #[test]
    fn no_transition_set_ignores_trigger() {
        let mut state: TransitionState<f64> = TransitionState::default();
        state.initial = true; // bypass first-read gate
        state.transition(&0.0, &100.0);
        assert!(
            state.active.is_none(),
            "TransitionState::transition should not activate when no transition is configured"
        );
        assert_eq!(state.get(&100.0), 100.0);
    }

    #[test]
    fn first_read_does_not_animate_initial_value() {
        // The `initial` flag exists so the very first value observation does not
        // fire a spurious transition (e.g. baseline→default on mount).
        let mut state: TransitionState<f64> = TransitionState::default();
        state.read(Some(Transition::linear(Duration::from_millis(100))));
        state.transition(&0.0, &100.0);
        assert!(
            state.active.is_none(),
            "initial transition() call should be suppressed until initial is observed"
        );
    }

    #[test]
    fn linear_transition_midpoint_is_interpolated() {
        let mut state: DirectTransition<f64> =
            DirectTransition::new(0.0, Some(Transition::linear(Duration::from_millis(100))));
        let started = Instant::now();
        // Trigger transition 0 → 100.
        assert!(state.transition_to(100.0));
        assert!(state.is_active());

        // Step forward to ~50% of the duration.
        let mid = instant_plus(started, 50);
        let _changed = state.step(&mid);
        let current = state.get();
        assert!(
            (30.0..=70.0).contains(&current),
            "midpoint value should lie between before/after, got {current}"
        );
    }

    #[test]
    fn linear_transition_completes_at_target() {
        let mut state: DirectTransition<f64> =
            DirectTransition::new(0.0, Some(Transition::linear(Duration::from_millis(50))));
        let started = Instant::now();
        state.transition_to(100.0);

        // Step past duration — transition should clear and `get` returns the target.
        let after = instant_plus(started, 200);
        state.step(&after);
        assert!(!state.is_active());
        assert_eq!(state.get(), 100.0);
    }

    #[test]
    fn direct_transition_reports_progress() {
        let mut state: DirectTransition<f64> =
            DirectTransition::new(0.0, Some(Transition::linear(Duration::from_millis(100))));
        let started = Instant::now();
        state.transition_to(100.0);

        let quarter = instant_plus(started, 25);
        let prog = state.progress(&quarter).unwrap();
        assert!(
            (0.15..=0.40).contains(&prog),
            "quarter-elapsed progress should be roughly 0.25, got {prog}"
        );
    }

    #[test]
    fn set_immediate_cancels_active_transition() {
        let mut state: DirectTransition<f64> =
            DirectTransition::new(0.0, Some(Transition::linear(Duration::from_millis(100))));
        state.transition_to(100.0);
        assert!(state.is_active());

        state.set_immediate(42.0);
        assert!(!state.is_active());
        assert_eq!(state.get(), 42.0);
    }

}
