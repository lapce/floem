use peniko::kurbo::{ParamCurve, Point};

use super::assert_valid_time;

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
impl From<Step> for Easing {
    fn from(value: Step) -> Self {
        Self::Step(value)
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

    pub fn eval(&self, time: f64) -> f64 {
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
    pub const EASE: Self = Bezier(0.25, 0.1, 0.25, 1.);
    pub const EASE_IN: Self = Bezier(0.42, 0., 1., 1.);
    pub const EASE_OUT: Self = Bezier(0., 0., 0.58, 1.);
    pub const EASE_IN_OUT: Self = Bezier(0.42, 0., 0.58, 1.);
}
impl From<Bezier> for Easing {
    fn from(value: Bezier) -> Self {
        Self::CubicBezier(value)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum Easing {
    #[default]
    Linear,
    CubicBezier(Bezier),
    Step(Step),
    Combined(Vec<Self>),
}

impl Easing {
    pub(crate) fn apply_easing_fn(&self, time: f64) -> f64 {
        assert_valid_time(time);
        match self {
            Easing::Linear => time,
            Easing::CubicBezier(c) => {
                // TODO: Optimize this, don't use kurbo
                let p1 = Point::new(0., 0.);
                let p2 = Point::new(c.0, c.1);
                let p3 = Point::new(c.2, c.3);
                let p4 = Point::new(1., 1.);
                let point = crate::kurbo::CubicBez::new(p1, p2, p3, p4).eval(time);
                point.y
            }
            Easing::Step(step) => step.eval(time),
            Easing::Combined(vec) => vec
                .iter()
                .fold(time, |acc, easing| easing.apply_easing_fn(acc)),
        }
    }
}

impl std::ops::Add for Easing {
    type Output = Self;

    fn add(mut self, mut rhs: Self) -> Self::Output {
        match (&mut self, &mut rhs) {
            (Easing::Linear, _) => rhs,
            (_, Easing::Linear) => self,
            (Easing::Combined(ref mut v1), Easing::Combined(ref mut v2)) => {
                v1.append(v2);
                self
            }
            (Easing::Combined(ref mut vec), _) => {
                vec.push(rhs);
                self
            }
            (lhs, Easing::Combined(ref mut vec)) => {
                vec.push(lhs.clone());
                rhs
            }
            _ => Self::Combined(vec![self, rhs]),
        }
    }
}
