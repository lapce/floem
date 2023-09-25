#[derive(Debug, Clone, Copy)]
pub struct Px(pub f64);

#[derive(Debug, Clone, Copy)]
pub struct Pct(pub f64);

impl From<f64> for Px {
    fn from(value: f64) -> Self {
        Px(value)
    }
}

impl From<f32> for Px {
    fn from(value: f32) -> Self {
        Px(value as f64)
    }
}

impl From<i32> for Px {
    fn from(value: i32) -> Self {
        Px(value as f64)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PxOrPct {
    Px(Px),
    Pct(Pct),
}

impl From<Pct> for PxOrPct {
    fn from(value: Pct) -> Self {
        PxOrPct::Pct(value)
    }
}

impl<T> From<T> for PxOrPct
where
    T: Into<Px>,
{
    fn from(value: T) -> Self {
        PxOrPct::Px(value.into())
    }
}

impl PxOrPct {
    pub fn px(value: f64) -> Self {
        PxOrPct::Px(Px(value))
    }
    pub fn pct(value: f64) -> Self {
        PxOrPct::Pct(Pct(value))
    }
}
