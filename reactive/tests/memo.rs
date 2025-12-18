use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use floem_reactive::{Effect, Memo, Runtime, RwSignal, SignalGet, SignalTrack, SignalUpdate};

#[test]
fn memo_recomputes_eagerly_on_change() {
    let source = RwSignal::new(0);
    let calls = Rc::new(Cell::new(0));

    let memo = Memo::new({
        let calls = calls.clone();
        move |_| {
            calls.set(calls.get() + 1);
            source.get() * 2
        }
    });

    assert_eq!(calls.get(), 1, "initial compute runs once");

    // Changing the source recomputes immediately.
    source.set(1);
    Runtime::drain_pending_work();
    assert_eq!(calls.get(), 2, "recomputed eagerly on change");
    assert_eq!(memo.get(), 2);
    assert_eq!(calls.get(), 2, "get does not recompute when fresh");

    source.set(2);
    Runtime::drain_pending_work();
    assert_eq!(calls.get(), 3, "recomputed again on change");
    assert_eq!(memo.get(), 4);
    assert_eq!(calls.get(), 3, "get still does not recompute when fresh");
}

#[test]
fn memo_notifies_dependents_on_change_when_recomputed() {
    let source = RwSignal::new(0);
    let memo = Memo::new(move |_| source.get());
    let runs = Rc::new(Cell::new(0));

    Effect::new({
        let runs = runs.clone();
        move |_| {
            memo.track();
            memo.get();
            runs.set(runs.get() + 1);
        }
    });

    assert_eq!(runs.get(), 1, "effect runs once initially");

    source.set(1);
    Runtime::drain_pending_work();
    assert_eq!(runs.get(), 2, "effect reruns after memo recompute");

    source.set(2);
    Runtime::drain_pending_work();
    assert_eq!(runs.get(), 3, "effect reruns on each source change");
}

#[test]
fn memo_skips_notifications_when_value_unchanged() {
    let source = RwSignal::new(0);
    let memo = Memo::new(move |_| source.get());
    let runs = Rc::new(Cell::new(0));

    Effect::new({
        let runs = runs.clone();
        move |_| {
            memo.track();
            memo.get();
            runs.set(runs.get() + 1);
        }
    });

    assert_eq!(runs.get(), 1);

    source.set(0);
    Runtime::drain_pending_work();
    assert_eq!(runs.get(), 1, "unchanged value does not notify dependents");
}

#[test]
fn memo_skips_when_derived_value_equal_after_signal_update() {
    let source = RwSignal::new(0);
    let memo = Memo::new(move |_| source.get() % 2 == 0);
    let runs = Rc::new(Cell::new(0));

    Effect::new({
        let runs = runs.clone();
        move |_| {
            memo.track();
            memo.get();
            runs.set(runs.get() + 1);
        }
    });

    assert_eq!(runs.get(), 1);

    source.set(2); // signal changed, derived parity is still even
    Runtime::drain_pending_work();
    assert_eq!(
        runs.get(),
        1,
        "memo should not notify when derived value stays equal"
    );
}

#[test]
fn memo_notifies_when_derived_value_changes_after_signal_update() {
    let source = RwSignal::new(0);
    let memo = Memo::new(move |_| source.get() % 2 == 0);
    let runs = Rc::new(Cell::new(0));

    Effect::new({
        let runs = runs.clone();
        move |_| {
            memo.track();
            memo.get();
            runs.set(runs.get() + 1);
        }
    });

    assert_eq!(runs.get(), 1);

    source.set(1); // parity flips to odd
    Runtime::drain_pending_work();
    assert_eq!(
        runs.get(),
        2,
        "memo should notify dependents when derived value changes"
    );
}

#[test]
fn memo_recomputes_before_dependents_use_value() {
    let source = RwSignal::new(0);
    let memo = Memo::new(move |_| source.get());
    let seen = Rc::new(RefCell::new(Vec::new()));

    Effect::new({
        let seen = seen.clone();
        move |_| {
            let value = source.get();
            let memo_value = memo.get();
            seen.borrow_mut().push((value, memo_value));
        }
    });

    assert_eq!(*seen.borrow(), vec![(0, 0)]);

    source.set(1);
    Runtime::drain_pending_work();
    assert_eq!(seen.borrow().last(), Some(&(1, 1)));

    source.set(2);
    Runtime::drain_pending_work();
    assert_eq!(seen.borrow().last(), Some(&(2, 2)));
    assert_eq!(seen.borrow().len(), 3);
}

#[test]
fn memo_stays_in_lockstep_with_signal_for_add_assign_updates() {
    let mut counter = RwSignal::new(0);
    let memo_runs = Rc::new(RefCell::new(Vec::new()));

    let memo = Memo::new({
        let memo_runs = memo_runs.clone();
        move |_| {
            let value = counter.get();
            memo_runs.borrow_mut().push(value);
            value
        }
    });

    let view_runs = Rc::new(RefCell::new(Vec::new()));
    Effect::new({
        let view_runs = view_runs.clone();
        move |_| {
            let value = counter.get();
            let memo_value = memo.get();
            view_runs.borrow_mut().push((value, memo_value));
        }
    });

    assert_eq!(*memo_runs.borrow(), vec![0]);
    assert_eq!(*view_runs.borrow(), vec![(0, 0)]);

    counter += 1;
    Runtime::drain_pending_work();
    counter += 1;
    Runtime::drain_pending_work();
    counter += 1;
    Runtime::drain_pending_work();

    assert_eq!(*memo_runs.borrow(), vec![0, 1, 2, 3]);
    assert_eq!(*view_runs.borrow(), vec![(0, 0), (1, 1), (2, 2), (3, 3)]);
}

#[test]
fn memo_high_priority_runs_before_normal_dependents() {
    let source = RwSignal::new(0);
    let log = Rc::new(RefCell::new(Vec::new()));

    let memo = Memo::new({
        let log = log.clone();
        move |_| {
            log.borrow_mut().push("memo");
            source.get()
        }
    });

    log.borrow_mut().clear();

    Effect::new({
        let log = log.clone();
        move |_| {
            let _ = source.get();
            let _ = memo.get();
            log.borrow_mut().push("effect");
        }
    });

    log.borrow_mut().clear();

    source.set(1);
    Runtime::drain_pending_work();

    assert_eq!(*log.borrow(), vec!["memo", "effect"]);
}
