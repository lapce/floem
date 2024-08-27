use std::marker::PhantomData;

use crate::{
    effect::create_effect,
    read::SignalRead,
    scope::Scope,
    signal::{create_signal, ReadSignal},
    SignalGet, SignalUpdate, SignalWith,
};

/// Memo computes the value from the closure on creation, and stores the value.
/// It will act like a Signal when the value is different with the computed value
/// from last run, i.e., it will trigger a effect run when you Get() it whenever the
/// computed value changes to a different value.
pub struct Memo<T> {
    getter: ReadSignal<T>,
    ty: PhantomData<T>,
}

impl<T> Copy for Memo<T> {}

impl<T> Clone for Memo<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Clone> SignalGet<T> for Memo<T> {
    fn id(&self) -> crate::id::Id {
        self.getter.id
    }
}

impl<T> SignalWith<T> for Memo<T> {
    fn id(&self) -> crate::id::Id {
        self.getter.id
    }
}

/// Create a Memo which takes the computed value of the given function, and triggers
/// the reactive system when the computed value is different with the last computed value.
pub fn create_memo<T>(f: impl Fn(Option<&T>) -> T + 'static) -> Memo<T>
where
    T: PartialEq + 'static,
{
    let cx = Scope::current();
    let inital = f(None);
    let (getter, setter) = create_signal(inital);
    let reader = getter.read_untracked();

    create_effect(move |_| {
        cx.track();
        let (is_different, new_value) = {
            let last_value = reader.borrow();
            let new_value = f(Some(&last_value));
            (new_value != *last_value, new_value)
        };
        if is_different {
            setter.set(new_value);
        }
    });

    Memo {
        getter,
        ty: PhantomData,
    }
}
