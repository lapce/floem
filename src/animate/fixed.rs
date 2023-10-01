use super::AnimDriver;

#[derive(Clone)]
pub struct FixedAnimDriver(bool);

pub fn fixed() -> FixedAnimDriver {
    FixedAnimDriver(true)
}

impl AnimDriver for FixedAnimDriver {
    fn next_value(&mut self) -> f64 {
        if self.0 {
            1.0
        } else {
            0.0
        }
    }

    fn requests_next_frame(&self) -> bool {
        false
    }

    fn set_enabled(&mut self, enabled: bool, _: bool) {
        self.0 = enabled
    }

    fn is_enabled(&self) -> bool {
        self.0
    }
}
