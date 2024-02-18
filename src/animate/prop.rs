use std::{any::Any, rc::Rc};

use floem_peniko::Color;

use crate::{
    animate::AnimDirection,
    style::{StyleMapValue, StylePropRef},
    unit::Px,
};

use super::{anim_val::AnimValue, assert_valid_time, SizeUnit};

#[derive(Clone, Debug)]
pub enum AnimatedProp {
    Width {
        from: f64,
        to: f64,
        unit: SizeUnit,
    },
    Height {
        from: f64,
        to: f64,
        unit: SizeUnit,
    },
    Scale {
        from: f64,
        to: f64,
    },
    // Opacity { from: f64, to: f64 },
    // TranslateX,
    // TranslateY,
    Prop {
        prop: StylePropRef,
        from: Rc<dyn Any>,
        to: Rc<dyn Any>,
    },
}

impl AnimatedProp {
    pub(crate) fn from(&self) -> AnimValue {
        match self {
            AnimatedProp::Prop { from, .. } => AnimValue::Prop(from.clone()),
            AnimatedProp::Width { from, .. } | AnimatedProp::Height { from, .. } => {
                AnimValue::Float(*from)
            }
            AnimatedProp::Scale { .. } => todo!(),
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
            AnimatedProp::Prop { prop, from, to } => {
                if let Some(from) = from.downcast_ref::<StyleMapValue<Px>>() {
                    let from = from.as_ref().unwrap();
                    let to = to.downcast_ref::<StyleMapValue<Px>>().unwrap();
                    let to = to.as_ref().unwrap();
                    return AnimValue::Prop(Rc::new(StyleMapValue::Val(Px(
                        self.animate_float(from.0, to.0, time, direction)
                    ))));
                }
                if let Some(from) = from.downcast_ref::<StyleMapValue<f64>>() {
                    let from = from.as_ref().unwrap();
                    let to = to.downcast_ref::<StyleMapValue<f64>>().unwrap();
                    let to = to.as_ref().unwrap();
                    return AnimValue::Prop(Rc::new(StyleMapValue::Val(
                        self.animate_float(*from, *to, time, direction),
                    )));
                }
                if let Some(from) = from.downcast_ref::<StyleMapValue<Color>>() {
                    let from = from.as_ref().unwrap();
                    let to = to.downcast_ref::<StyleMapValue<Color>>().unwrap();
                    let to = to.as_ref().unwrap();
                    return AnimValue::Prop(Rc::new(StyleMapValue::Val(
                        self.animate_color(*from, *to, time, direction),
                    )));
                }
                if let Some(from) = from.downcast_ref::<StyleMapValue<Option<Color>>>() {
                    let from = from.as_ref().unwrap();
                    let to = to.downcast_ref::<StyleMapValue<Option<Color>>>().unwrap();
                    let to = to.as_ref().unwrap();
                    //TODO: get rid of this requirement
                    let from = from.expect("Color must be set in the styles to be animated");
                    let to = to.unwrap();
                    return AnimValue::Prop(Rc::new(StyleMapValue::Val(Some(
                        self.animate_color(from, to, time, direction),
                    ))));
                }
                panic!("unknown type for {prop:?}")
            }
            AnimatedProp::Width { from, to, unit: _ }
            | AnimatedProp::Height { from, to, unit: _ } => {
                AnimValue::Float(self.animate_float(*from, *to, time, direction))
            }
            AnimatedProp::Scale { .. } => todo!(),
        }
    }
}

#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub enum AnimPropKind {
    Scale,
    // TranslateX,
    // TranslateY,
    Width,
    Height,
    Prop { prop: StylePropRef },
}
