#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(crate) enum AnimState {
    Idle,
    Stopped,
    Paused {
        elapsed: Option<Duration>,
    },
    /// How many passes(loops) there will be is controlled by the [`RepeatMode`] of the animation.
    /// By default, the animation will only have a single pass,
    /// but it can be set to [`RepeatMode::LoopForever`] to loop indefinitely.
    PassInProgress {
        started_on: Instant,
        elapsed: Duration,
    },
    /// Depending on the [`RepeatMode`] of the animation, we either go back to `PassInProgress`
    /// or advance to `Completed`.
    PassFinished {
        elapsed: Duration,
    },
    // NOTE: If animation has `RepeatMode::LoopForever`, this state will never be reached.
    Completed {
        elapsed: Option<Duration>,
    },
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AnimStateKind {
    Idle,
    Paused,
    Stopped,
    PassInProgress,
    PassFinished,
    Completed,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AnimStateCommand {
    Pause,
    Resume,
    Start,
    Stop,
}
