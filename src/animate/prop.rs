use peniko::Color;

use crate::animate::AnimDirection;

use super::{anim_val::AnimValue, assert_valid_time, SizeUnit};

#[derive(Clone, Debug)]
pub enum AnimatedProp {
    Width { from: f64, to: f64, unit: SizeUnit },
    Height { from: f64, to: f64, unit: SizeUnit },
    Scale { from: f64, to: f64 },
    // Opacity { from: f64, to: f64 },
    // TranslateX,
    // TranslateY,
    Background { from: Color, to: Color },
    BorderRadius { from: f64, to: f64 },
    BorderWidth { from: f64, to: f64 },
    BorderColor { from: Color, to: Color },
    Color { from: Color, to: Color },
}

impl AnimatedProp {
    pub(crate) fn from(&self) -> AnimValue {
        match self {
            AnimatedProp::Width { from, .. }
            | AnimatedProp::Height { from, .. }
            | AnimatedProp::BorderWidth { from, .. }
            | AnimatedProp::BorderRadius { from, .. } => AnimValue::Float(*from),
            AnimatedProp::Scale { .. } => todo!(),
            AnimatedProp::Background { from, .. }
            | AnimatedProp::BorderColor { from, .. }
            | AnimatedProp::Color { from, .. } => AnimValue::Color(*from),
        }
    }

    pub(crate) fn animate_float(
        &self,
        from: f64,
        to: f64,
        time: f64,
        direction: AnimDirection,
    ) -> f64 {
        assert_valid_time(time);
        let (from, to) = match direction {
            AnimDirection::Forward => (from, to),
            AnimDirection::Backward => (to, from),
        };
        if time == 0.0 {
            return from;
        }
        if (1.0 - time).abs() < f64::EPSILON {
            return to;
        }
        if (from - to).abs() < f64::EPSILON {
            return from;
        }

        from * (1.0 - time) + to * time
    }

    pub(crate) fn animate_usize(
        &self,
        from: u8,
        to: u8,
        time: f64,
        direction: AnimDirection,
    ) -> u8 {
        assert_valid_time(time);
        let (from, to) = match direction {
            AnimDirection::Forward => (from, to),
            AnimDirection::Backward => (to, from),
        };

        if time == 0.0 {
            return from;
        }
        if (1.0 - time).abs() < f64::EPSILON {
            return to;
        }
        if from == to {
            return from;
        }

        let from = from as f64;
        let to = to as f64;

        let val = from * (1.0 - time) + to * time;
        if to >= from {
            (val + 0.5) as u8
        } else {
            (val - 0.5) as u8
        }
    }

    pub(crate) fn animate_color(
        &self,
        from: Color,
        to: Color,
        time: f64,
        direction: AnimDirection,
    ) -> Color {
        let r = self.animate_usize(from.r, to.r, time, direction);
        let g = self.animate_usize(from.g, to.g, time, direction);
        let b = self.animate_usize(from.b, to.b, time, direction);
        let a = self.animate_usize(from.a, to.a, time, direction);
        Color { r, g, b, a }
    }

    pub(crate) fn animate(&self, time: f64, direction: AnimDirection) -> AnimValue {
        match self {
            AnimatedProp::Width { from, to, unit } | AnimatedProp::Height { from, to, unit } => {
                AnimValue::Float(self.animate_float(*from, *to, time, direction))
            }
            AnimatedProp::Background { from, to }
            | AnimatedProp::BorderColor { from, to }
            | AnimatedProp::Color { from, to } => {
                AnimValue::Color(self.animate_color(*from, *to, time, direction))
            }
            AnimatedProp::Scale { .. } => todo!(),
            AnimatedProp::BorderRadius { from, to } | AnimatedProp::BorderWidth { from, to } => {
                AnimValue::Float(self.animate_float(*from, *to, time, direction))
            }
        }
    }
}

#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub enum AnimPropKind {
    Scale,
    // TranslateX,
    // TranslateY,
    Width,
    Background,
    Color,
    Height,
    BorderRadius,
    BorderColor,
}
