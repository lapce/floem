use std::marker::PhantomData;

use crate::{
    effect::create_effect,
    scope::{with_scope, Scope},
    signal::{create_signal, ReadSignal},
};

pub struct Memo<T> {
    getter: ReadSignal<Option<T>>,
    ty: PhantomData<T>,
}

impl<T> Copy for Memo<T> {}

impl<T> Clone for Memo<T> {
    fn clone(&self) -> Self {
        Self {
            getter: self.getter,
            ty: Default::default(),
        }
    }
}

impl<T: Clone> Memo<T> {
    pub fn get(&self) -> T
    where
        T: 'static,
    {
        self.getter.get().unwrap()
    }

    pub fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        self.getter.get_untracked().unwrap()
    }
}

impl<T> Memo<T> {
    pub fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.getter.with(|value| f(value.as_ref().unwrap()))
    }

    pub fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.getter
            .with_untracked(|value| f(value.as_ref().unwrap()))
    }

    pub fn track(&self) {
        let signal = self.getter.id.signal().unwrap();
        signal.subscribe();
    }
}

pub fn create_memo<T>(f: impl Fn(Option<&T>) -> T + 'static) -> Memo<T>
where
    T: PartialEq + 'static,
{
    let (getter, setter) = create_signal(None::<T>);
    let id = getter.id;

    with_scope(Scope(id).create_child(), move || {
        create_effect(move |_| {
            let (is_different, new_value) = getter.with_untracked(|value| {
                let new_value = f(value.as_ref());
                (Some(&new_value) != value.as_ref(), new_value)
            });
            if is_different {
                setter.set(Some(new_value));
            }
        });
    });

    Memo {
        getter,
        ty: PhantomData,
    }
}
