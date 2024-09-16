use peniko::kurbo::{ParamCurve, Point};

pub trait Easing: std::fmt::Debug {
    fn eval(&self, time: f64) -> f64;
    fn velocity(&self, time: f64) -> Option<f64> {
        let _ = time;
        None
    }
    fn finished(&self, time: f64) -> bool {
        time >= 1. || time <= 0.
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Linear;
impl Easing for Linear {
    fn eval(&self, time: f64) -> f64 {
        time
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepPosition {
    None,
    Both,
    Start,
    End,
}

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
    pub const BOTH: Self = Self {
        num_steps: 1,
        step_position: StepPosition::Both,
    };
    pub const NONE: Self = Self {
        num_steps: 1,
        step_position: StepPosition::None,
    };
    pub const START: Self = Self {
        num_steps: 1,
        step_position: StepPosition::Start,
    };
    pub const END: Self = Self {
        num_steps: 1,
        step_position: StepPosition::End,
    };

    pub fn new(num_steps: usize, step_position: StepPosition) -> Self {
        Step {
            num_steps,
            step_position,
        }
    }

    pub fn new_end(num_steps: usize) -> Self {
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
                ((time / step_size).floor() * step_size + step_size / 2.0).min(1.0)
            }
            StepPosition::Both => {
                let step_size = 1.0 / (self.num_steps - 1) as f64;
                let adjusted_time = ((time / step_size).round() * step_size).min(1.0);
                (adjusted_time / step_size).round() * step_size
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Bezier(pub f64, pub f64, pub f64, pub f64);
impl Bezier {
    const EASE: Self = Bezier(0.25, 0.1, 0.25, 1.);
    const EASE_IN: Self = Bezier(0.42, 0., 1., 1.);
    const EASE_OUT: Self = Bezier(0., 0., 0.58, 1.);
    const EASE_IN_OUT: Self = Bezier(0.42, 0., 0.58, 1.);
    pub fn ease() -> Self {
        Self::EASE
    }
    pub fn ease_in() -> Self {
        Self::EASE_IN
    }
    pub fn ease_out() -> Self {
        Self::EASE_OUT
    }
    pub fn ease_in_out() -> Self {
        Self::EASE_IN_OUT
    }

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

#[derive(Debug, Clone, Copy)]
pub struct Spring {
    mass: f64,
    stiffness: f64,
    damping: f64,
    initial_velocity: f64,
}

impl Spring {
    pub fn new(mass: f64, stiffness: f64, damping: f64, initial_velocity: f64) -> Self {
        Spring {
            mass,
            stiffness,
            damping,
            initial_velocity,
        }
    }
    // TODO: figure out if these are reasonable values.

    /// Slower, smoother motion
    pub fn gentle() -> Self {
        Self::new(1., 50.0, 8.0, 0.0)
    }

    /// More overshoot, longer settling time
    pub fn bouncy() -> Self {
        Self::new(1., 150.0, 5.0, 0.0)
    }

    /// Quick response, minimal overshoot
    pub fn snappy() -> Self {
        Self::new(1., 200.0, 20.0, 0.0)
    }

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
            let omega_d = omega * (1.0 - zeta * zeta).sqrt();
            let e = (-zeta * omega * time).exp();
            let cos_term = (omega_d * time).cos();
            let sin_term = (omega_d * time).sin();

            let a = 1.0;
            let b = (v0 + zeta * omega * a) / omega_d;

            1.0 - e * (a * cos_term + b * sin_term)
        } else if zeta > 1.0 {
            // Overdamped
            let r1 = -omega * (zeta - (zeta * zeta - 1.0).sqrt());
            let r2 = -omega * (zeta + (zeta * zeta - 1.0).sqrt());

            let a = (v0 - r2) / (r1 - r2);
            let b = 1.0 - a;

            1.0 - a * (r1 * time).exp() - b * (r2 * time).exp()
        } else {
            // Critically damped
            let e = (-omega * time).exp();
            let a = 1.0;
            let b = v0 + omega * a;

            1.0 - e * (a + b * time)
        }
    }

    const THRESHOLD: f64 = 0.003;
    pub fn finished(&self, time: f64) -> bool {
        let position = self.eval(time);
        let velocity = self.velocity(time);

        (1.0 - position).abs() < Self::THRESHOLD && velocity.abs() < Self::THRESHOLD
    }

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
            let omega_d = omega * (1.0 - zeta * zeta).sqrt();
            let e = (-zeta * omega * time).exp();
            let cos_term = (omega_d * time).cos();
            let sin_term = (omega_d * time).sin();

            let a = 1.0;
            let b = (v0 + zeta * omega * a) / omega_d;

            e * ((zeta * omega * (a * cos_term + b * sin_term))
                + (a * -omega_d * sin_term + b * omega_d * cos_term))
        } else if zeta > 1.0 {
            // Overdamped
            let r1 = -omega * (zeta - (zeta * zeta - 1.0).sqrt());
            let r2 = -omega * (zeta + (zeta * zeta - 1.0).sqrt());

            let a = (v0 - r2) / (r1 - r2);
            let b = 1.0 - a;

            -a * r1 * (r1 * time).exp() - b * r2 * (r2 * time).exp()
        } else {
            // Critically damped
            let e = (-omega * time).exp();
            let a = 1.0;
            let b = v0 + omega * a;

            e * (b - omega * (a + b * time))
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
