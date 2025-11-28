use floem_reactive::{create_rw_signal, SignalRead};

#[test]
fn read_ref_derefs_to_inner_value() {
    let signal = create_rw_signal(vec![10, 20, 30]);

    let read = signal.read();
    assert_eq!(read.len(), 3);
    assert_eq!(read[0], 10);
    assert_eq!(read[2], 30);
}

#[test]
fn read_untracked_also_derefs() {
    let signal = create_rw_signal(String::from("hello"));

    let read = signal.read_untracked();
    assert_eq!(read.as_str(), "hello");
}
