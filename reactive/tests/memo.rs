use std::{cell::Cell, rc::Rc};

use floem_reactive::{
    create_effect, create_rw_signal, Memo, Runtime, SignalGet, SignalTrack, SignalUpdate,
};

#[test]
fn memo_recomputes_on_demand() {
    let source = create_rw_signal(0);
    let calls = Rc::new(Cell::new(0));

    let memo = Memo::new({
        let calls = calls.clone();
        move |_| {
            calls.set(calls.get() + 1);
            source.get() * 2
        }
    });

    assert_eq!(calls.get(), 1, "initial compute runs once");

    // Changing the source marks the memo dirty but does not recompute until read.
    source.set(1);
    Runtime::drain_pending_work();
    assert_eq!(calls.get(), 1, "no recompute until accessed");

    assert_eq!(memo.get(), 2);
    assert_eq!(calls.get(), 2, "recomputed on first read after change");

    source.set(2);
    Runtime::drain_pending_work();
    assert_eq!(calls.get(), 2, "still lazy after another change");

    assert_eq!(memo.get(), 4);
    assert_eq!(calls.get(), 3, "recomputed again when accessed");
}

#[test]
fn memo_notifies_dependents_on_change_when_recomputed() {
    let source = create_rw_signal(0);
    let memo = Memo::new(move |_| source.get());
    let runs = Rc::new(Cell::new(0));

    create_effect({
        let memo = memo;
        let runs = runs.clone();
        move |_| {
            memo.track();
            memo.get();
            runs.set(runs.get() + 1);
        }
    });

    assert_eq!(runs.get(), 1, "effect runs once initially");

    // Update the source; memo becomes dirty. Effect should only rerun once
    // the memo is recomputed (e.g., when read).
    source.set(1);
    Runtime::drain_pending_work();
    assert_eq!(runs.get(), 1, "effect not rerun until memo updates");

    memo.get();
    Runtime::drain_pending_work();
    assert_eq!(runs.get(), 2, "effect reruns after memo recompute");
}

#[test]
fn memo_skips_notifications_when_value_unchanged() {
    let source = create_rw_signal(0);
    let memo = Memo::new(move |_| source.get());
    let runs = Rc::new(Cell::new(0));

    create_effect({
        let memo = memo;
        let runs = runs.clone();
        move |_| {
            memo.track();
            memo.get();
            runs.set(runs.get() + 1);
        }
    });

    assert_eq!(runs.get(), 1);

    source.set(0);
    memo.get();
    Runtime::drain_pending_work();
    assert_eq!(runs.get(), 1, "unchanged value does not notify dependents");
}
