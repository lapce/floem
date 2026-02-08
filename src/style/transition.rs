//! Animation and transition types for the style system.
//!
//! This module provides types for animating style property changes over time:
//! - [`Transition`] - Animation parameters (duration, easing)
//! - [`TransitionState`] - Internal state for tracking active transitions
//! - [`DirectTransition`] - Standalone transition controller

use floem_reactive::{RwSignal, SignalGet, SignalUpdate as _};
use floem_renderer::Renderer;
use peniko::color::palette;
use peniko::kurbo::{self, Point, Stroke};
use std::rc::Rc;
use taffy::prelude::{auto, fr};

use crate::platform::{Duration, Instant};

use crate::animate::{Bezier, Easing, Linear, Spring};
use crate::theme::StyleThemeExt;
use crate::view::{IntoView, View};
use crate::views::{ContainerExt, Decorators, Label, Stack, TooltipExt, canvas};

use super::StylePropValue;
use super::values::views;

#[derive(Clone, Debug)]
pub(crate) struct ActiveTransition<T: StylePropValue> {
    pub(crate) start: Instant,
    pub(crate) before: T,
    pub(crate) current: T,
    pub(crate) after: T,
}

#[derive(Clone, Debug)]
pub struct TransitionState<T: StylePropValue> {
    pub(crate) transition: Option<Transition>,
    pub(crate) active: Option<ActiveTransition<T>>,
    pub(crate) initial: bool,
}

impl<T: StylePropValue> TransitionState<T> {
    pub(crate) fn read(&mut self, transition: Option<Transition>) {
        self.transition = transition;
    }

    pub(crate) fn transition(&mut self, before: &T, after: &T) {
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
    pub(crate) fn step(&mut self, now: &Instant, request_transition: &mut bool) -> bool {
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

    pub(crate) fn get(&self, value: &T) -> T {
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
    pub fn debug_view(&self) -> Box<dyn View> {
        let transition = self.clone();
        let easing_clone = transition.easing.clone();

        let curve_color = RwSignal::new(palette::css::BLUE);
        let axis_color = RwSignal::new(palette::css::GRAY);

        // Visual preview of the easing curve
        let preview = canvas(move |cx, size| {
            let width = size.width;
            let height = size.height;
            let padding = 4.0;
            let graph_width = width - padding * 2.0;
            let graph_height = height - padding * 2.0;

            // Sample the easing function
            let sample_count = 50;
            let mut path = kurbo::BezPath::new();

            for i in 0..=sample_count {
                let t = i as f64 / sample_count as f64;
                let eased = easing_clone.eval(t);
                let x = padding + t * graph_width;
                let y = padding + (1.0 - eased) * graph_height;

                if i == 0 {
                    path.move_to(Point::new(x, y));
                } else {
                    path.line_to(Point::new(x, y));
                }
            }

            // Draw the curve
            cx.stroke(
                &path,
                curve_color.get(),
                &Stroke {
                    width: 2.0,
                    ..Default::default()
                },
            );

            // Draw axes
            let axis_stroke = Stroke {
                width: 1.0,
                ..Default::default()
            };

            // X axis
            cx.stroke(
                &kurbo::Line::new(
                    Point::new(padding, height - padding),
                    Point::new(width - padding, height - padding),
                ),
                axis_color.get(),
                &axis_stroke,
            );

            // Y axis
            cx.stroke(
                &kurbo::Line::new(
                    Point::new(padding, padding),
                    Point::new(padding, height - padding),
                ),
                axis_color.get(),
                &axis_stroke,
            );
        })
        .style(|s| s.width(80.0).height(60.0))
        .container()
        .style(move |s| {
            s.padding(4.0)
                .border(1.)
                .border_radius(5.0)
                .with_theme(move |s, t| {
                    curve_color.set(t.primary());
                    axis_color.set(t.text_muted());
                    s.border_color(t.border())
                })
        });

        let tooltip_view = move || {
            let transition = transition.clone();

            let duration_row = views((
                "Duration:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                Label::derived(move || format!("{:.0}ms", transition.duration.as_millis())),
            ));

            let easing_name = format!("{:?}", transition.easing);
            let easing_row = views((
                "Easing:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                Label::derived(move || easing_name.clone()),
            ));

            // Show velocity at key points if available
            let velocity_samples = if transition.easing.velocity(0.0).is_some() {
                let samples = vec![0.0, 0.25, 0.5, 0.75, 1.0]
                    .into_iter()
                    .filter_map(|t| {
                        transition
                            .easing
                            .velocity(t)
                            .map(|v| Label::new(format!("t={:.2}: {:.3}", t, v)))
                    })
                    .collect::<Vec<_>>();

                if !samples.is_empty() {
                    Some(views((
                        "Velocity:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                        Stack::vertical_from_iter(samples).style(|s| s.gap(2.0)),
                    )))
                } else {
                    None
                }
            } else {
                None
            };

            let mut rows = vec![duration_row.into_any(), easing_row.into_any()];

            if let Some(velocity_row) = velocity_samples {
                rows.push(velocity_row.into_any());
            }

            Stack::vertical_from_iter(rows).style(|s| {
                s.grid()
                    .grid_template_columns([auto(), fr(1.)])
                    .justify_center()
                    .items_center()
                    .row_gap(12)
                    .col_gap(10)
                    .padding(20)
            })
        };

        preview
            .tooltip(tooltip_view)
            .style(|s| s.items_center())
            .into_any()
    }
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
/// use floem::style::{DirectTransition, Transition};
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
