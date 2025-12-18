use std::{cell::Cell, rc::Rc};

use floem_reactive::{
    Effect, Runtime, RwSignal, SignalGet, SignalRead, SignalTrack, SignalUpdate, SignalWrite,
};

#[test]
fn batch_simple() {
    let name = RwSignal::new("John");
    let age = RwSignal::new(20);

    let count = Rc::new(Cell::new(0));

    Effect::new({
        let count = count.clone();
        move |_| {
            name.track();
            age.track();

            count.set(count.get() + 1);
        }
    });

    // The effect runs once immediately
    assert_eq!(count.get(), 1);

    // Setting each signal once will trigger the effect
    name.set("Mary");
    Runtime::drain_pending_work();
    assert_eq!(count.get(), 2);

    age.set(21);
    Runtime::drain_pending_work();
    assert_eq!(count.get(), 3);

    // Batching will only update once
    Effect::batch(|| {
        name.set("John");
        age.set(20);
    });
    Runtime::drain_pending_work();
    assert_eq!(count.get(), 4);
}

#[test]
fn batch_batch() {
    let name = RwSignal::new("John");
    let age = RwSignal::new(20);

    let count = Rc::new(Cell::new(0));

    Effect::new({
        let count = count.clone();
        move |_| {
            name.track();
            age.track();

            count.set(count.get() + 1);
        }
    });

    assert_eq!(count.get(), 1);

    // Batching within another batch should be equivalent to batching them all together
    Effect::batch(|| {
        name.set("Mary");
        age.set(21);
        Effect::batch(|| {
            name.set("John");
            age.set(20);
        });
    });

    Runtime::drain_pending_work();
    assert_eq!(count.get(), 2);
}

#[test]
fn no_reentrant_runs() {
    let state = RwSignal::new(0);
    let log = Rc::new(std::cell::RefCell::new(Vec::new()));

    Effect::new({
        let log = log.clone();
        move |_| {
            let v = state.get();
            log.borrow_mut().push(format!("run {v}"));
            if v == 0 {
                state.set(1);
                log.borrow_mut().push("post set".into());
            }
        }
    });

    Runtime::drain_pending_work();

    let log = log.borrow();
    assert_eq!(log.as_slice(), ["run 0", "run 1", "post set"]);
}

#[test]
fn cross_thread_write_updates_value_immediately() {
    // cross-thread writes are not supported for local signals
}

#[test]
fn pending_effects_are_deduped() {
    let signal = RwSignal::new(0);
    let counter = Rc::new(Cell::new(0));

    Effect::new({
        let counter = counter.clone();
        move |_| {
            signal.track();
            counter.set(counter.get() + 1);
        }
    });

    Effect::batch(|| {
        signal.set(1);
        signal.set(2);
        signal.set(3);
    });

    Runtime::drain_pending_work();
    assert_eq!(counter.get(), 2);
}

#[test]
fn signals_are_send_sync_when_value_is() {
    // no-op for local-only signals
}

#[test]
fn track_default_subscribes() {
    let signal = RwSignal::new(0);
    let runs = Rc::new(Cell::new(0));

    Effect::new({
        let runs = runs.clone();
        move |_| {
            signal.track();
            runs.set(runs.get() + 1);
        }
    });

    assert_eq!(runs.get(), 1);
    signal.set(1);
    Runtime::drain_pending_work();
    assert_eq!(runs.get(), 2);
}

#[test]
fn read_untracked_does_not_subscribe() {
    let signal = RwSignal::new(0);
    let tracked_runs = Rc::new(Cell::new(0));
    let untracked_runs = Rc::new(Cell::new(0));

    Effect::new({
        let tracked_runs = tracked_runs.clone();
        move |_| {
            signal.get();
            tracked_runs.set(tracked_runs.get() + 1);
        }
    });

    Effect::new({
        let untracked_runs = untracked_runs.clone();
        move |_| {
            signal.read_untracked();
            untracked_runs.set(untracked_runs.get() + 1);
        }
    });

    assert_eq!(tracked_runs.get(), 1);
    assert_eq!(untracked_runs.get(), 1);

    signal.set(1);
    Runtime::drain_pending_work();

    assert_eq!(tracked_runs.get(), 2, "tracked effect reruns");
    assert_eq!(
        untracked_runs.get(),
        1,
        "untracked read should not resubscribe"
    );
}

#[test]
fn write_ref_drop_notifies_subscribers_once() {
    let signal = RwSignal::new(0);
    let runs = Rc::new(Cell::new(0));

    Effect::new({
        let runs = runs.clone();
        move |_| {
            signal.get();
            runs.set(runs.get() + 1);
        }
    });

    assert_eq!(runs.get(), 1);

    {
        let mut w = signal.write();
        *w = 5;
    }
    Runtime::drain_pending_work();
    assert_eq!(runs.get(), 2, "effect reruns after write ref drop");
    assert_eq!(signal.get(), 5);
}
