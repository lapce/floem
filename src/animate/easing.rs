//! The Easing trait and the built-in easing functions.

use peniko::kurbo::{ParamCurve, Point};

/// A trait for easing functions used in animations.
///
/// Easing functions control how a value changes over time during an animation.
/// They take a normalized time value (0.0 to 1.0) and return a progress value.
pub trait Easing: std::fmt::Debug {
    /// Evaluates the easing function at the given time.
    ///
    /// # Arguments
    /// * `time` - A normalized time value, typically between 0.0 and 1.0
    ///
    /// # Returns
    /// The eased progress value, typically between 0.0 and 1.0
    fn eval(&self, time: f64) -> f64;

    /// Returns the velocity at the given time, if available.
    ///
    /// This is primarily used by spring animations to determine when motion has stopped.
    fn velocity(&self, time: f64) -> Option<f64> {
        let _ = time;
        None
    }

    /// Returns whether the animation has finished at the given time.
    ///
    /// By default, an animation is finished when time is outside the 0.0..1.0 range.
    fn finished(&self, time: f64) -> bool {
        !(0. ..1.).contains(&time)
    }
}

/// Linear easing - no acceleration or deceleration.
#[derive(Debug, Clone, Copy)]
pub struct Linear;
impl Easing for Linear {
    fn eval(&self, time: f64) -> f64 {
        time
    }
}

/// Position of the step change in a step easing function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepPosition {
    /// Step changes occur at the midpoint of each interval.
    None,
    /// Step changes occur at both the start and end of each interval.
    Both,
    /// Step changes occur at the start of each interval.
    Start,
    /// Step changes occur at the end of each interval.
    End,
}

/// Step easing - discrete jumps between values.
///
/// Creates a staircase effect where the animation jumps between discrete values
/// rather than smoothly interpolating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    num_steps: usize,
    step_position: StepPosition,
}
impl Default for Step {
    fn default() -> Self {
        Self::END
    }
}

impl Step {
    /// A single step that changes at both start and end.
    pub const BOTH: Self = Self {
        num_steps: 1,
        step_position: StepPosition::Both,
    };
    /// A single step that changes at the midpoint.
    pub const NONE: Self = Self {
        num_steps: 1,
        step_position: StepPosition::None,
    };
    /// A single step that changes at the start.
    pub const START: Self = Self {
        num_steps: 1,
        step_position: StepPosition::Start,
    };
    /// A single step that changes at the end.
    pub const END: Self = Self {
        num_steps: 1,
        step_position: StepPosition::End,
    };

    /// Creates a new step easing with the given number of steps and position.
    pub const fn new(num_steps: usize, step_position: StepPosition) -> Self {
        Self {
            num_steps,
            step_position,
        }
    }

    /// Creates a new step easing that changes at the end of each step.
    pub const fn new_end(num_steps: usize) -> Self {
        Self {
            num_steps,
            step_position: StepPosition::End,
        }
    }
}

impl Easing for Step {
    fn eval(&self, time: f64) -> f64 {
        match self.step_position {
            StepPosition::Start => {
                let step_size = 1.0 / self.num_steps as f64;
                ((time / step_size).floor() * step_size).min(1.0)
            }
            StepPosition::End => {
                let step_size = 1.0 / self.num_steps as f64;
                ((time / step_size).ceil() * step_size).min(1.0)
            }
            StepPosition::None => {
                let step_size = 1.0 / self.num_steps as f64;
                (time / step_size)
                    .floor()
                    .mul_add(step_size, step_size / 2.0)
                    .min(1.0)
            }
            StepPosition::Both => {
                let step_size = 1.0 / (self.num_steps - 1) as f64;
                let adjusted_time = ((time / step_size).round() * step_size).min(1.0);
                (adjusted_time / step_size).round() * step_size
            }
        }
    }
}

/// Cubic bezier easing curve.
///
/// Defined by two control points (x1, y1) and (x2, y2) that shape the curve.
/// The curve starts at (0, 0) and ends at (1, 1).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Bezier(pub f64, pub f64, pub f64, pub f64);
impl Bezier {
    const EASE: Self = Self(0.25, 0.1, 0.25, 1.);
    const EASE_IN: Self = Self(0.42, 0., 1., 1.);
    const EASE_OUT: Self = Self(0., 0., 0.58, 1.);
    const EASE_IN_OUT: Self = Self(0.42, 0., 0.58, 1.);

    /// Standard ease curve - slow start and end, fast middle.
    pub const fn ease() -> Self {
        Self::EASE
    }

    /// Ease-in curve - slow start, fast end.
    pub const fn ease_in() -> Self {
        Self::EASE_IN
    }

    /// Ease-out curve - fast start, slow end.
    pub const fn ease_out() -> Self {
        Self::EASE_OUT
    }

    /// Ease-in-out curve - slow start and end.
    pub const fn ease_in_out() -> Self {
        Self::EASE_IN_OUT
    }

    /// Evaluates the bezier curve at the given time.
    pub fn eval(&self, time: f64) -> f64 {
        // TODO: Optimize this, don't use kurbo
        let p1 = Point::new(0., 0.);
        let p2 = Point::new(self.0, self.1);
        let p3 = Point::new(self.2, self.3);
        let p4 = Point::new(1., 1.);
        let point = crate::kurbo::CubicBez::new(p1, p2, p3, p4).eval(time);
        point.y
    }
}
impl Easing for Bezier {
    fn eval(&self, time: f64) -> f64 {
        self.eval(time)
    }
}

/// Physics-based spring animation.
///
/// Simulates a damped harmonic oscillator to create natural-feeling motion
/// with overshoot and settling behavior.
#[derive(Debug, Clone, Copy)]
pub struct Spring {
    mass: f64,
    stiffness: f64,
    damping: f64,
    initial_velocity: f64,
}

impl Spring {
    /// Creates a new spring with the given physics parameters.
    ///
    /// # Arguments
    /// * `mass` - The mass of the spring (affects momentum)
    /// * `stiffness` - How "tight" the spring is (higher = faster oscillation)
    /// * `damping` - How quickly oscillations die down (higher = less bouncy)
    /// * `initial_velocity` - Starting velocity of the animation
    pub const fn new(mass: f64, stiffness: f64, damping: f64, initial_velocity: f64) -> Self {
        Self {
            mass,
            stiffness,
            damping,
            initial_velocity,
        }
    }
    // TODO: figure out if these are reasonable values.

    /// Slower, smoother motion
    pub const fn gentle() -> Self {
        Self::new(1., 50.0, 8.0, 0.0)
    }

    /// More overshoot, longer settling time
    pub const fn bouncy() -> Self {
        Self::new(1., 150.0, 5.0, 0.0)
    }

    /// Quick response, minimal overshoot
    pub const fn snappy() -> Self {
        Self::new(1., 200.0, 20.0, 0.0)
    }

    /// Evaluates the spring position at the given time.
    pub fn eval(&self, time: f64) -> f64 {
        if time <= 0.0 {
            return 0.0;
        }

        let m = self.mass;
        let k = self.stiffness;
        let c = self.damping;
        let v0 = self.initial_velocity;

        let omega = (k / m).sqrt();
        let zeta = c / (2.0 * (k * m).sqrt());

        if zeta < 1.0 {
            // Underdamped
            let omega_d = omega * zeta.mul_add(-zeta, 1.0).sqrt();
            let e = (-zeta * omega * time).exp();
            let cos_term = (omega_d * time).cos();
            let sin_term = (omega_d * time).sin();

            let a = 1.0;
            let b = (zeta * omega).mul_add(a, v0) / omega_d;

            e.mul_add(-a.mul_add(cos_term, b * sin_term), 1.0)
        } else if zeta > 1.0 {
            // Overdamped
            let r1 = -omega * (zeta - zeta.mul_add(zeta, -1.0).sqrt());
            let r2 = -omega * (zeta + zeta.mul_add(zeta, -1.0).sqrt());

            let a = (v0 - r2) / (r1 - r2);
            let b = 1.0 - a;

            b.mul_add(-(r2 * time).exp(), a.mul_add(-(r1 * time).exp(), 1.0))
        } else {
            // Critically damped
            let e = (-omega * time).exp();
            let a = 1.0;
            let b = omega.mul_add(a, v0);

            e.mul_add(-b.mul_add(time, a), 1.0)
        }
    }

    /// Threshold for determining when the spring has settled.
    pub const THRESHOLD: f64 = 0.005;

    /// Returns whether the spring has settled at the given time.
    pub fn finished(&self, time: f64) -> bool {
        let position = self.eval(time);
        let velocity = self.velocity(time);

        (1.0 - position).abs() < Self::THRESHOLD && velocity.abs() < Self::THRESHOLD
    }

    /// Returns the velocity of the spring at the given time.
    pub fn velocity(&self, time: f64) -> f64 {
        if time <= 0.0 {
            return self.initial_velocity;
        }

        let m = self.mass;
        let k = self.stiffness;
        let c = self.damping;
        let v0 = self.initial_velocity;

        let omega = (k / m).sqrt();
        let zeta = c / (2.0 * (k * m).sqrt());

        if zeta < 1.0 {
            // Underdamped
            let omega_d = omega * zeta.mul_add(-zeta, 1.0).sqrt();
            let e = (-zeta * omega * time).exp();
            let cos_term = (omega_d * time).cos();
            let sin_term = (omega_d * time).sin();

            let a = 1.0;
            let b = (zeta * omega).mul_add(a, v0) / omega_d;

            e * (zeta * omega).mul_add(
                a.mul_add(cos_term, b * sin_term),
                (a * -omega_d).mul_add(sin_term, b * omega_d * cos_term),
            )
        } else if zeta > 1.0 {
            // Overdamped
            let r1 = -omega * (zeta - zeta.mul_add(zeta, -1.0).sqrt());
            let r2 = -omega * (zeta + zeta.mul_add(zeta, -1.0).sqrt());

            let a = (v0 - r2) / (r1 - r2);
            let b = 1.0 - a;

            (-a * r1).mul_add((r1 * time).exp(), -(b * r2 * (r2 * time).exp()))
        } else {
            // Critically damped
            let e = (-omega * time).exp();
            let a = 1.0;
            let b = omega.mul_add(a, v0);

            e * omega.mul_add(-b.mul_add(time, a), b)
        }
    }
}

impl Default for Spring {
    fn default() -> Self {
        Self::new(1.0, 100.0, 15.0, 0.0)
    }
}

// TODO: The finished function is quite inneficient as it will result in repeated work.
// Can't cache it here because making this mutable is weird.
// Need to find a way to cache the work in the animation.
impl Easing for Spring {
    fn eval(&self, time: f64) -> f64 {
        self.eval(time)
    }

    fn velocity(&self, time: f64) -> Option<f64> {
        Some(self.velocity(time))
    }

    fn finished(&self, time: f64) -> bool {
        self.finished(time)
    }
}
