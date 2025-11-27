use std::sync::atomic::AtomicU64;

use crate::{
    effect::observer_clean_up,
    runtime::{Runtime, RUNTIME},
    signal::Signal,
    sync_runtime::SYNC_RUNTIME,
};

/// An internal id which can reference a Signal/Effect/Scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Hash)]
pub struct Id(u64);

impl Id {
    /// Create a new Id that's next in order
    pub(crate) fn next() -> Id {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Id(COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }

    /// Try to get the Signal that links with this Id
    pub(crate) fn signal(&self) -> Option<Signal> {
        if Runtime::is_ui_thread() {
            if let Some(sig) = RUNTIME.with(|runtime| runtime.signals.borrow().get(self).cloned()) {
                return Some(sig);
            }
            SYNC_RUNTIME.get_signal(self).map(Into::into)
        } else {
            SYNC_RUNTIME.get_signal(self).map(Into::into)
        }
    }

    /// Try to set the Signal to be linking with this Id
    pub(crate) fn add_signal(&self, signal: Signal) {
        RUNTIME.with(|runtime| runtime.signals.borrow_mut().insert(*self, signal));
    }

    /// Make this Id a child of the current Scope
    pub(crate) fn set_scope(&self) {
        RUNTIME.with(|runtime| {
            let scope = runtime.current_scope.borrow();
            let mut children = runtime.children.borrow_mut();
            let children = children.entry(*scope).or_default();
            children.insert(*self);
        });
    }

    /// Dispose only the children of this Id without removing resources tied to the Id itself.
    pub(crate) fn dispose_children(&self) {
        if let Ok(Some(children)) =
            RUNTIME.try_with(|runtime| runtime.children.borrow_mut().remove(self))
        {
            for child in children {
                child.dispose();
            }
        }
    }

    /// Dispose the relevant resources that's linking to this Id, and the all the children
    /// and grandchildren.
    pub(crate) fn dispose(&self) {
        if !Runtime::is_ui_thread() {
            // Bounce disposal work to the UI thread so we clean up the correct runtime.
            SYNC_RUNTIME.enqueue_disposals([*self]);
            return;
        }

        if let Ok((children, signal, effect)) = RUNTIME.try_with(|runtime| {
            (
                runtime.children.borrow_mut().remove(self),
                runtime.signals.borrow_mut().remove(self),
                runtime.effects.borrow_mut().remove(self),
            )
        }) {
            if let Some(children) = children {
                for child in children {
                    child.dispose();
                }
            }

            if let Some(effect) = effect {
                observer_clean_up(&effect);
            }

            let mut signal = signal;
            if signal.is_none() {
                signal = SYNC_RUNTIME.remove_signal(self).map(Into::into);
            }
            Self::cleanup_signal(signal);
        } else if let Some(signal) = SYNC_RUNTIME.remove_signal(self) {
            Self::cleanup_signal(Some(signal.into()));
        }
    }

    fn cleanup_signal(signal: Option<Signal>) {
        if let Some(signal) = signal {
            for effect_id in signal.subscriber_ids() {
                // Drop any effect that was subscribed to this signal so it can't linger
                // with dangling dependencies.
                effect_id.dispose();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use crate::{
        create_effect, create_rw_signal,
        runtime::{Runtime, RUNTIME},
        scope::Scope,
        SignalTrack, SignalUpdate,
    };

    #[test]
    fn effect_disposed_when_dependency_signal_disposed() {
        let parent = Scope::new();
        let signal_scope = parent.create_child();
        let (signal, setter) = signal_scope.create_signal(0);

        let count = Rc::new(Cell::new(0));
        parent.enter(|| {
            let count = count.clone();
            create_effect(move |_| {
                signal.track();
                count.set(count.get() + 1);
            });
        });

        assert_eq!(count.get(), 1);

        // Disposing the signal's scope should clean up the subscribing effect.
        signal_scope.dispose();

        // Mutations after disposal should not rerun the effect.
        setter.set(1);
        Runtime::drain_pending_work();
        assert_eq!(count.get(), 1);

        // The effect should be removed from the runtime.
        RUNTIME.with(|runtime| assert!(runtime.effects.borrow().is_empty()));
    }

    #[test]
    fn signals_created_by_effect_are_disposed_with_effect() {
        let parent = Scope::new();
        let dep_scope = parent.create_child();
        let (dep_signal, dep_setter) = dep_scope.create_signal(0);

        let created_signal = Rc::new(std::cell::RefCell::new(None));
        let run_count = Rc::new(Cell::new(0));

        parent.enter(|| {
            let created_signal = created_signal.clone();
            let run_count = run_count.clone();
            create_effect(move |_| {
                dep_signal.track();
                run_count.set(run_count.get() + 1);
                if created_signal.borrow().is_none() {
                    created_signal.replace(Some(create_rw_signal(0)));
                }
            });
        });

        assert_eq!(run_count.get(), 1);
        let inner_signal = created_signal.borrow().clone().expect("signal created");
        assert!(inner_signal.id().signal().is_some());

        // Dispose the dependency scope; the effect should be disposed and clean up its children.
        dep_scope.dispose();
        Runtime::drain_pending_work();

        // Mutating the dependency after disposal should do nothing.
        dep_setter.set(1);
        Runtime::drain_pending_work();
        assert_eq!(run_count.get(), 1);

        assert!(inner_signal.id().signal().is_none());
        RUNTIME.with(|runtime| assert!(runtime.effects.borrow().is_empty()));
    }

    #[test]
    fn disposing_scope_drops_signals_and_effects() {
        let scope = Scope::new();
        let (signal, setter) = scope.create_signal(0);
        let signal_id = signal.id();

        let run_count = Rc::new(Cell::new(0));
        scope.enter(|| {
            let run_count = run_count.clone();
            create_effect(move |_| {
                signal.track();
                run_count.set(run_count.get() + 1);
            });
        });

        // Sanity: effect ran and runtime holds signal/effect.
        assert_eq!(run_count.get(), 1);
        RUNTIME.with(|runtime| {
            assert!(runtime.signals.borrow().contains_key(&signal_id));
            assert_eq!(runtime.effects.borrow().len(), 1);
            assert!(runtime.children.borrow().get(&scope.0).is_some());
        });

        // Dispose the scope; both signal and effect should be cleaned up.
        scope.dispose();
        Runtime::drain_pending_work();

        setter.set(1);
        Runtime::drain_pending_work();
        assert_eq!(run_count.get(), 1);

        RUNTIME.with(|runtime| {
            assert!(runtime.signals.borrow().get(&signal_id).is_none());
            assert!(runtime.effects.borrow().is_empty());
            assert!(runtime.children.borrow().get(&scope.0).is_none());
        });
    }
}
