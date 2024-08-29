use std::{cell::Cell, rc::Rc};

use floem_reactive::{batch, create_effect, create_rw_signal, SignalTrack, SignalUpdate};

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
    assert_eq!(count.get(), 2);

    age.set(21);
    assert_eq!(count.get(), 3);

    // Batching will only update once
    batch(|| {
        name.set("John");
        age.set(20);
    });
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

    assert_eq!(count.get(), 2);
}
