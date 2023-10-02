use std::f64::consts::PI;

/// Alters how the easing function behaves, i.e. how the animation interpolates.
#[derive(Debug, Clone, Copy, Default)]
pub enum EasingMode {
    #[default]
    /// Interpolation follows the mathematical formula associated with the easing function.
    In,
    /// Interpolation follows 100% interpolation minus the output of the formula associated with the easing function.
    Out,
    /// Interpolation uses EasingMode::In for the first half of the animation and EasingMode::Out for the second half.
    InOut,
}

#[derive(Debug, Clone, Copy, Default)]
// See https://easings.net/ and
// https://learn.microsoft.com/en-us/dotnet/desktop/wpf/graphics-multimedia/easing-functions
pub enum EasingFn {
    #[default]
    Linear,
    /// Retracts the motion of an animation slightly before it begins to animate in the path indicated.
    Back,
    /// Creates a bouncing effect.
    Bounce,
    /// Creates an animation that accelerates and/or decelerates using a circular function.
    Circle,
    /// Creates an animation that resembles a spring oscillating back and forth until it comes to rest.
    Elastic,
    /// Creates an animation that accelerates and/or decelerates using an exponential formula.
    Exponential,
    /// Creates an animation that accelerates and/or
    /// decelerates using the formula `f(t) = tp` where p is equal to the Power property.
    Power,
    /// Creates an animation that accelerates and/or decelerates using the formula `f(t) = t2`.
    Quadratic,
    /// Creates an animation that accelerates and/or decelerates using the formula `f(t) = t3`.
    Cubic,
    /// Creates an animation that accelerates and/or decelerates using the formula `f(t) = t4`.
    Quartic,
    /// Create an animation that accelerates and/or decelerates using the formula `f(t) = t5`.
    Quintic,
    /// Creates an animation that accelerates and/or decelerates using a sine formula.
    Sine,
}

fn elastic_easing(time: f64) -> f64 {
    let c4: f64 = (2.0 * PI) / 3.0;
    if time == 0.0 {
        0.0
    } else if (1.0 - time).abs() < f64::EPSILON {
        1.0
    } else {
        -(2.0_f64.powf(10.0 * time - 10.0) * ((time * 10.0 - 10.75) * c4).sin())
    }
}

fn apply_easing_fn(v: f64, func: EasingFn) -> f64 {
    match func {
        EasingFn::Linear => v,
        EasingFn::Circle => 1.0 - (1.0 - v.powi(2)).sqrt(),
        EasingFn::Elastic => elastic_easing(v),
        EasingFn::Exponential => {
            if v == 0.0 {
                0.0
            } else {
                2.0f64.powf(10.0 * v - 10.0)
            }
        }
        EasingFn::Power => todo!(),
        EasingFn::Quadratic => v.powf(2.0),
        EasingFn::Cubic => v.powf(3.0),
        EasingFn::Quartic => v.powf(4.0),
        EasingFn::Quintic => v.powf(5.0),
        EasingFn::Sine => 1.0 - ((v * PI) / 2.0).cos(),
        EasingFn::Back => todo!(),
        EasingFn::Bounce => todo!(),
    }
}

pub fn ease(v: f64, mode: EasingMode, func: EasingFn) -> f64 {
    match mode {
        EasingMode::In => apply_easing_fn(v, func),
        EasingMode::Out => 1.0 - apply_easing_fn(1.0 - v, func),
        EasingMode::InOut => {
            if v < 0.5 {
                apply_easing_fn(v * 2.0, func) / 2.0
            } else {
                1.0 - apply_easing_fn(2.0 - v * 2., func) / 2.0
            }
        }
    }
}
