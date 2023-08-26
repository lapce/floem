use std::marker::PhantomData;

use crate::{
    effect::create_effect,
    scope::Scope,
    signal::{create_signal, ReadSignal},
};

/// Memo computes the value from the closure on creation, and stores the value.
/// It will act like a Signal when the value is different with the computed value
/// from last run, i.e., it will trigger a effect run when you Get() it whenever the
/// computed value changes to a different value.
pub struct Memo<T> {
    getter: ReadSignal<Option<T>>,
    ty: PhantomData<T>,
}

impl<T> Copy for Memo<T> {}

impl<T> Clone for Memo<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Clone> Memo<T> {
    /// Clones and returns the current value stored in the Memo, and subcribes
    /// to the current runnig effect to this Memo.
    pub fn get(&self) -> T
    where
        T: 'static,
    {
        self.getter.get().unwrap()
    }

    /// Clones and returns the current value stored in the Memo, but it doesn't subcribe
    /// to the current runnig effect.
    pub fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        self.getter.get_untracked().unwrap()
    }
}

impl<T> Memo<T> {
    /// Applies a clsoure to the current value stored in the Memo, and subcribes
    /// to the current runnig effect to this Memo.
    pub fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.getter.with(|value| f(value.as_ref().unwrap()))
    }

    /// Applies a clsoure to the current value stored in the Memo, but it doesn't subcribe
    /// to the current runnig effect.
    pub fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.getter
            .with_untracked(|value| f(value.as_ref().unwrap()))
    }

    /// Only subcribes to the current runnig effect to this Memo.
    pub fn track(&self) {
        let signal = self.getter.id.signal().unwrap();
        signal.subscribe();
    }
}

/// Create a Memo which takes the computed value of the given function, and triggers
/// the reactive system when the computed value is different with the last computed value.
pub fn create_memo<T>(f: impl Fn(Option<&T>) -> T + 'static) -> Memo<T>
where
    T: PartialEq + 'static,
{
    let cx = Scope::current();
    let (getter, setter) = create_signal(None::<T>);

    create_effect(move |_| {
        cx.track();
        let (is_different, new_value) = getter.with_untracked(|value| {
            let new_value = f(value.as_ref());
            (Some(&new_value) != value.as_ref(), new_value)
        });
        if is_different {
            setter.set(Some(new_value));
        }
    });

    Memo {
        getter,
        ty: PhantomData,
    }
}
