use crate::{signal::RwSignal, SignalUpdate, SignalWith};

#[derive(Debug)]
pub struct Trigger {
    signal: RwSignal<()>,
}

impl Copy for Trigger {}

impl Clone for Trigger {
    fn clone(&self) -> Self {
        *self
    }
}

impl Trigger {
    pub fn notify(&self) {
        self.signal.set(());
    }

    pub fn track(&self) {
        self.signal.with(|_| {});
    }

    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Trigger {
            signal: RwSignal::new(()),
        }
    }
}

#[deprecated(
    since = "0.2.0",
    note = "Use Trigger::new instead; this will be removed in a future release"
)]
pub fn create_trigger() -> Trigger {
    Trigger {
        signal: RwSignal::new(()),
    }
}
