use std::marker::PhantomData;

use crate::{
    effect::Effect,
    read::{SignalRead, SignalTrack},
    scope::Scope,
    signal::{ReadSignal, RwSignal},
    storage::{SyncStorage, UnsyncStorage},
    SignalGet, SignalUpdate, SignalWith,
};

/// Memo computes the value from the closure on creation, and stores the value.
///
/// It will act like a Signal when the value is different with the computed value
/// from last run, i.e., it will trigger a effect run when you Get() it whenever the
/// computed value changes to a different value.
pub struct Memo<T, S = UnsyncStorage> {
    getter: ReadSignal<T, S>,
    ty: PhantomData<T>,
    st: PhantomData<S>,
}

pub type SyncMemo<T> = Memo<T, SyncStorage>;

impl<T, S> Copy for Memo<T, S> {}

impl<T, S> Clone for Memo<T, S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Clone, S> SignalGet<T> for Memo<T, S>
where
    ReadSignal<T, S>: SignalGet<T>,
{
    fn id(&self) -> crate::id::Id {
        self.getter.id
    }

    fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        self.getter.get_untracked()
    }

    fn get(&self) -> T
    where
        T: 'static,
    {
        self.getter.get()
    }

    fn try_get(&self) -> Option<T>
    where
        T: 'static,
    {
        self.getter.try_get()
    }

    fn try_get_untracked(&self) -> Option<T>
    where
        T: 'static,
    {
        self.getter.try_get_untracked()
    }
}

impl<T, S> SignalWith<T> for Memo<T, S>
where
    ReadSignal<T, S>: SignalWith<T>,
{
    fn id(&self) -> crate::id::Id {
        self.getter.id
    }

    fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.getter.with(f)
    }

    fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.getter.with_untracked(f)
    }

    fn try_with<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        self.getter.try_with(f)
    }

    fn try_with_untracked<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        self.getter.try_with_untracked(f)
    }
}
impl<T, S> SignalTrack<T> for Memo<T, S>
where
    ReadSignal<T, S>: SignalTrack<T>,
{
    fn id(&self) -> crate::id::Id {
        self.getter.id
    }
}

/// Create a Memo which takes the computed value of the given function, and triggers
/// the reactive system when the computed value is different with the last computed value.
#[deprecated(
    since = "0.2.0",
    note = "Use Memo::new instead; this will be removed in a future release"
)]
pub fn create_memo<T>(f: impl Fn(Option<&T>) -> T + 'static) -> Memo<T>
where
    T: PartialEq + 'static,
{
    Memo::<T, UnsyncStorage>::new(f)
}

impl<T> Memo<T>
where
    T: PartialEq + 'static,
{
    pub fn new(f: impl Fn(Option<&T>) -> T + 'static) -> Self {
        let cx = Scope::current();
        let initial = f(None);
        let (getter, setter) = RwSignal::new_split(initial);
        let reader = getter.read_untracked();

        Effect::new(move |_| {
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
            st: PhantomData,
        }
    }
}

impl<T> Memo<T, SyncStorage>
where
    T: PartialEq + Send + Sync + 'static,
{
    pub fn new(f: impl Fn(Option<&T>) -> T + 'static) -> Self {
        let cx = Scope::current();
        let initial = f(None);
        let (getter, setter) = RwSignal::<T, SyncStorage>::new_sync_split(initial);
        let reader = getter.read_untracked();

        Effect::new(move |_| {
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
            st: PhantomData,
        }
    }
}
