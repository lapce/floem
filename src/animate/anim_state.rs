use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(crate) enum AnimState {
    Idle,
    PassInProgress {
        started_on: Instant,
        elapsed: Duration,
    },
    PassFinished {
        elapsed: Duration,
    },
    // NOTE: If animation has `RepeatMode::LoopForever`, this state will never be reached.
    Completed {
        elapsed: Option<Duration>,
    },
}

#[derive(Debug)]
pub enum AnimStateKind {
    Idle,
    PassInProgress,
    PassFinished,
    Completed,
}
