use peniko::Color;

use crate::unit::{Pct, Px, PxPct, PxPctAuto};

pub trait Blendable {
    #[must_use]
    fn blend(&self, to: Self, amount: f64) -> Self;
}

impl Blendable for f64 {
    fn blend(&self, to: Self, amount: f64) -> Self {
        let inverse = 1.0 - amount;
        self * inverse + to * amount
    }
}

impl Blendable for f32 {
    fn blend(&self, to: Self, amount: f64) -> Self {
        let inverse = 1.0 - amount as f32;
        self * inverse + to * amount as f32
    }
}

impl Blendable for u8 {
    fn blend(&self, to: Self, amount: f64) -> Self {
        let blended = (*self as f64).blend(to as f64, amount);
        blended.clamp(0.0, 255.0).round() as u8
    }
}

impl Blendable for Color {
    fn blend(&self, to: Color, amount: f64) -> Color {
        Color::rgba8(
            self.r.blend(to.r, amount),
            self.g.blend(to.g, amount),
            self.b.blend(to.b, amount),
            self.a.blend(to.a, amount),
        )
    }
}

impl Blendable for Px {
    fn blend(&self, to: Self, amount: f64) -> Self {
        Self(self.0.blend(to.0, amount))
    }
}

impl Blendable for Pct {
    fn blend(&self, to: Self, amount: f64) -> Self {
        Self(self.0.blend(to.0, amount))
    }
}

impl Blendable for PxPct {
    fn blend(&self, to: Self, amount: f64) -> Self {
        match self {
            PxPct::Px(v) => {
                let PxPct::Px(to) = to else {
                    eprintln!(
                        "Cannot blend between different units. Got {:?} and {:?}.",
                        self, to
                    );
                    return *self;
                };
                PxPct::Px(v.blend(to, amount))
            }
            PxPct::Pct(v) => {
                let PxPct::Pct(to) = to else {
                    eprintln!(
                        "Cannot blend between different units. Got {:?} and {:?}.",
                        self, to
                    );
                    return *self;
                };
                PxPct::Pct(v.blend(to, amount))
            }
        }
    }
}

impl Blendable for PxPctAuto {
    fn blend(&self, to: Self, amount: f64) -> Self {
        match self {
            PxPctAuto::Px(v) => {
                let PxPctAuto::Px(to) = to else {
                    eprintln!(
                        "Cannot blend between different units. Got {:?} and {:?}.",
                        self, to
                    );
                    return *self;
                };
                PxPctAuto::Px(v.blend(to, amount))
            }
            PxPctAuto::Pct(v) => {
                let PxPctAuto::Pct(to) = to else {
                    eprintln!(
                        "Cannot blend between different units. Got {:?} and {:?}.",
                        self, to
                    );
                    return *self;
                };
                PxPctAuto::Pct(v.blend(to, amount))
            }
            PxPctAuto::Auto => {
                let PxPctAuto::Auto = to else {
                    eprintln!(
                        "Cannot blend between different units. Got {:?} and {:?}.",
                        self, to
                    );
                    return *self;
                };
                PxPctAuto::Auto
            }
        }
    }
}
