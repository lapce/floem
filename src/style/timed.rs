use std::time::Instant;

use super::AnimDriver;

#[derive(Clone, Debug)]
pub struct TimedAnimDriver {
    enabled: bool,
    duration_secs: f64,
    // current animation progress from 0 to 1
    progress: f64,
    last_call_instant: Option<Instant>,
    looping: bool,
}

pub fn anim(duration_secs: f64) -> TimedAnimDriver {
    TimedAnimDriver {
        enabled: true,
        duration_secs,
        progress: 0.,
        last_call_instant: None,
        looping: false,
    }
}

pub fn looping(duration_secs: f64) -> TimedAnimDriver {
    TimedAnimDriver {
        enabled: true,
        duration_secs,
        progress: 0.,
        last_call_instant: None,
        looping: true,
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AnimDirection {
    Forward,
    Backward,
}

impl AnimDriver for TimedAnimDriver {
    fn next_value(&mut self) -> f64 {
        let now = Instant::now();
        let mut secs_since_last_call = 0.0;

        if let Some(last_call) = self.last_call_instant {
            secs_since_last_call = now.duration_since(last_call).as_secs_f64();
        }

        self.last_call_instant = Some(now);

        self.next_value_with_elapsed(secs_since_last_call)
    }

    fn requests_next_frame(&self) -> bool {
        let on_not_finished = self.enabled && self.progress != 1.0;
        let off_not_finished = !self.enabled && self.progress != 0.0;
        let on_looping = self.enabled && self.looping;

        on_not_finished || off_not_finished || on_looping
    }

    fn set_enabled(&mut self, enabled: bool, animate: bool) {
        // when finished first clear previous state
        if !self.requests_next_frame() {
            self.last_call_instant = None
        }

        self.enabled = enabled;
        if !animate || self.duration_secs == 0.0 {
            self.progress = if enabled { 1.0 } else { 0.0 };
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl TimedAnimDriver {
    pub fn next_value_with_elapsed(&mut self, elapsed_secs: f64) -> f64 {
        let mut additional_progress = 0.0;
        if self.duration_secs != 0.0 {
            additional_progress = elapsed_secs / self.duration_secs;
        }

        if self.enabled {
            self.progress += additional_progress;
        } else {
            self.progress -= additional_progress;
        };

        if self.looping && self.progress > 1.0 {
            self.progress -= 1.0;
        } else {
            self.progress = self.progress.clamp(0.0, 1.0);
        }

        self.progress
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_animation() {
        let mut driver = anim(10.0);
        let progress = driver.next_value_with_elapsed(0.0);
        assert_eq!(progress, 0.0);
        assert_eq!(driver.requests_next_frame(), true);

        let progress = driver.next_value_with_elapsed(9.0);
        assert_eq!(progress, 0.9);
        assert_eq!(driver.requests_next_frame(), true);

        let progress = driver.next_value_with_elapsed(2.0);
        assert_eq!(progress, 1.0);
        assert_eq!(driver.requests_next_frame(), false);

        // animate back
        driver.set_enabled(false, true);

        let progress = driver.next_value_with_elapsed(0.0);
        assert_eq!(progress, 1.0);
        assert_eq!(driver.requests_next_frame(), true);

        let progress = driver.next_value_with_elapsed(5.0);
        assert_eq!(progress, 0.5);
        assert_eq!(driver.requests_next_frame(), true);

        let progress = driver.next_value_with_elapsed(5.0);
        assert_eq!(progress, 0.0);
        assert_eq!(driver.requests_next_frame(), false);

        // calling again is no problem although driver is done
        let progress = driver.next_value_with_elapsed(5.0);
        assert_eq!(progress, 0.0);
        assert_eq!(driver.requests_next_frame(), false);
    }

    #[test]
    fn test_zero_duration() {
        let mut driver = TimedAnimDriver {
            enabled: true,
            duration_secs: 0.0,
            progress: 1.0,
            last_call_instant: None,
            looping: false,
        };

        let progress = driver.next_value_with_elapsed(0.0);
        assert_eq!(progress, 1.0);
        assert_eq!(driver.should_run(), true);
        assert_eq!(driver.requests_next_frame(), false);

        let progress = driver.next_value_with_elapsed(5.0);
        assert_eq!(progress, 1.0);
        assert_eq!(driver.should_run(), true);
        assert_eq!(driver.requests_next_frame(), false);

        driver.set_enabled(false, true);
        assert_eq!(driver.should_run(), false);
        assert_eq!(driver.requests_next_frame(), false);
        assert_eq!(driver.progress, 0.0);

        let progress = driver.next_value_with_elapsed(5.0);
        assert_eq!(driver.should_run(), false);
        assert_eq!(driver.requests_next_frame(), false);
        assert_eq!(progress, 0.0);
    }

    #[test]
    fn test_skipping_animation() {
        let mut driver = anim(10.0);
        let progress = driver.next_value_with_elapsed(0.0);
        assert_eq!(progress, 0.0);
        assert_eq!(driver.requests_next_frame(), true);

        driver.set_enabled(true, false);
        let progress = driver.next_value_with_elapsed(0.0);
        assert_eq!(progress, 1.0);
        assert_eq!(driver.requests_next_frame(), false);

        driver.set_enabled(false, false);
        let progress = driver.next_value_with_elapsed(5.0);
        assert_eq!(progress, 0.0);
        assert_eq!(driver.requests_next_frame(), false);
    }

    #[test]
    fn test_looping_animation() {
        let mut driver = looping(10.0);
        let progress = driver.next_value_with_elapsed(0.0);
        assert_eq!(progress, 0.0);
        assert_eq!(driver.should_run(), true);
        assert_eq!(driver.requests_next_frame(), true);

        // overshoot
        let progress = driver.next_value_with_elapsed(15.0);
        assert_eq!(progress, 0.5);
        assert_eq!(driver.should_run(), true);
        assert_eq!(driver.requests_next_frame(), true);

        // disable
        driver.set_enabled(false, true);
        let progress = driver.next_value_with_elapsed(4.0);
        assert!(progress - 0.1 < f64::EPSILON);
        assert_eq!(driver.should_run(), true);
        assert_eq!(driver.requests_next_frame(), true);

        // arrive at off
        driver.set_enabled(false, true);
        let progress = driver.next_value_with_elapsed(3.0);
        assert_eq!(progress, 0.0);
        assert_eq!(driver.should_run(), false);
        assert_eq!(driver.requests_next_frame(), false);

        // enable again
        driver.set_enabled(true, true);
        let progress = driver.next_value_with_elapsed(0.0);
        assert_eq!(progress, 0.0);
        assert_eq!(driver.should_run(), true);
        assert_eq!(driver.requests_next_frame(), true);

        driver.set_enabled(true, true);
        let progress = driver.next_value_with_elapsed(10.0);
        assert_eq!(progress, 1.0);
        assert_eq!(driver.should_run(), true);
        assert_eq!(driver.requests_next_frame(), true);
    }
}
