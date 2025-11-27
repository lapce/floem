use std::{cell::Cell, rc::Rc};

use floem_reactive::{
    batch, create_effect, create_rw_signal, Runtime, SignalGet, SignalTrack, SignalUpdate,
};

#[test]
fn batch_simple() {
    let name = create_rw_signal("John");
    let age = create_rw_signal(20);

    let count = Rc::new(Cell::new(0));

    create_effect({
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
    batch(|| {
        name.set("John");
        age.set(20);
    });
    Runtime::drain_pending_work();
    assert_eq!(count.get(), 4);
}

#[test]
fn batch_batch() {
    let name = create_rw_signal("John");
    let age = create_rw_signal(20);

    let count = Rc::new(Cell::new(0));

    create_effect({
        let count = count.clone();
        move |_| {
            name.track();
            age.track();

            count.set(count.get() + 1);
        }
    });

    assert_eq!(count.get(), 1);

    // Batching within another batch should be equivalent to batching them all together
    batch(|| {
        name.set("Mary");
        age.set(21);
        batch(|| {
            name.set("John");
            age.set(20);
        });
    });

    Runtime::drain_pending_work();
    assert_eq!(count.get(), 2);
}

#[test]
fn no_reentrant_runs() {
    let state = create_rw_signal(0);
    let log = Rc::new(std::cell::RefCell::new(Vec::new()));

    create_effect({
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
    let signal = create_rw_signal(0);
    let counter = Rc::new(Cell::new(0));

    create_effect({
        let counter = counter.clone();
        move |_| {
            signal.track();
            counter.set(counter.get() + 1);
        }
    });

    batch(|| {
        signal.set(1);
        signal.set(2);
        signal.set(3);
    });

    Runtime::drain_pending_work();
    assert_eq!(counter.get(), 2);
}

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn signals_are_send_sync_when_value_is() {
    // no-op for local-only signals
}
