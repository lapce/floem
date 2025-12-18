#![cfg(debug_assertions)]

use floem_reactive::{RwSignal, SignalRead, SignalWrite};

#[test]
fn borrow_conflict_reports_locations() {
    let result = std::panic::catch_unwind(|| {
        let signal = RwSignal::new(0);
        let _first = signal.read(); // holds a shared borrow

        // Should panic when trying to take a mutable borrow while a shared one is held.
        let _second = signal.write();
    });

    assert!(result.is_err(), "expected borrow conflict panic");
    let panic = result.unwrap_err();
    let msg = if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = panic.downcast_ref::<&str>() {
        s.to_string()
    } else {
        panic!("unexpected panic payload type: {:?}", panic);
    };
    assert!(
        msg.contains("signal value already borrowed"),
        "panic message missing base text: {msg}"
    );
    assert!(
        msg.contains("attempted at"),
        "panic message missing attempt location: {msg}"
    );
}
