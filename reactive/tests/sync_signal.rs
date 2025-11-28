use std::thread;

use floem_reactive::{SignalGet, SignalUpdate, SyncRwSignal};

#[test]
fn sync_signal_reads_and_writes_across_threads() {
    let signal: SyncRwSignal<i32> = SyncRwSignal::new_sync(0);
    let writer = signal;

    let handle = thread::spawn(move || {
        writer.set(42);
        writer.get()
    });

    let thread_value = handle.join().expect("thread success");
    assert_eq!(thread_value, 42);
    assert_eq!(signal.get(), 42);
}
