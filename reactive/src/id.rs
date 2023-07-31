use std::sync::atomic::AtomicU64;

use crate::{effect::observer_clean_up, runtime::RUNTIME, signal::Signal};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Hash)]
/// A stable identifier for an element.
pub(crate) struct Id(u64);

impl Id {
    pub(crate) fn next() -> Id {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Id(COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }

    pub(crate) fn signal(&self) -> Option<Signal> {
        RUNTIME.with(|runtime| runtime.signals.borrow().get(self).cloned())
    }

    pub(crate) fn add_signal(&self, signal: Signal) {
        RUNTIME.with(|runtime| runtime.signals.borrow_mut().insert(*self, signal));
    }

    pub(crate) fn set_scope(&self) {
        RUNTIME.with(|runtime| {
            let scope = runtime.current_scope.borrow();
            let mut children = runtime.children.borrow_mut();
            let children = children.entry(*scope).or_default();
            children.insert(*self);
        });
    }

    pub(crate) fn dispose(&self) {
        let (children, signal) = RUNTIME.with(|runtime| {
            (
                runtime.children.borrow_mut().remove(self),
                runtime.signals.borrow_mut().remove(self),
            )
        });

        if let Some(children) = children {
            for child in children {
                child.dispose();
            }
        }

        if let Some(signal) = signal {
            for (_, effect) in signal.subscribers() {
                observer_clean_up(&effect);
            }
        }
    }
}
