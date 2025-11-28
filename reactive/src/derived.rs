use std::marker::PhantomData;

use crate::{
    storage::{SyncStorage, UnsyncStorage},
    RwSignal, SignalGet, SignalTrack, SignalUpdate, SignalWith,
};

/// A derived signal backed by a local [`RwSignal`] (UI-thread only).
pub struct DerivedRwSignal<
    T: 'static,
    O,
    GF: Fn(&T) -> O + Clone + 'static,
    UF: Fn(&O) -> T + Clone + 'static,
> {
    signal: RwSignal<T, UnsyncStorage>,
    getter: RwSignal<Box<GF>, UnsyncStorage>,
    setter: RwSignal<Box<UF>, UnsyncStorage>,
    ty: PhantomData<T>,
    ts: PhantomData<()>,
}

impl<T, O, GF, UF> Clone for DerivedRwSignal<T, O, GF, UF>
where
    GF: Fn(&T) -> O + Copy,
    UF: Fn(&O) -> T + Copy,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, O, GF, UF> Copy for DerivedRwSignal<T, O, GF, UF>
where
    GF: Fn(&T) -> O + Copy,
    UF: Fn(&O) -> T + Copy,
{
}

impl<T, O, GF, UF> SignalGet<O> for DerivedRwSignal<T, O, GF, UF>
where
    T: Clone + 'static,
    O: Clone + 'static,
    GF: Fn(&T) -> O + Copy + 'static,
    UF: Fn(&O) -> T + Copy + 'static,
{
    fn id(&self) -> crate::id::Id {
        self.signal.id
    }

    fn get_untracked(&self) -> O
    where
        O: 'static,
    {
        self.try_get_untracked().unwrap()
    }

    fn get(&self) -> O
    where
        O: 'static,
    {
        self.try_get().unwrap()
    }

    fn try_get(&self) -> Option<O>
    where
        O: 'static,
    {
        let sig = self.getter;
        self.signal.id.signal().map(|signal| {
            let func = sig.get_untracked();
            func(&signal.get()).clone()
        })
    }

    fn try_get_untracked(&self) -> Option<O>
    where
        O: 'static,
    {
        let sig = self.getter;
        self.signal.id.signal().map(|signal| {
            let func = sig.get_untracked();
            func(&signal.get_untracked()).clone()
        })
    }
}

impl<T, O, GF, UF> SignalWith<O> for DerivedRwSignal<T, O, GF, UF>
where
    T: Clone + 'static,
    O: Clone,
    GF: Fn(&T) -> O + Copy,
    UF: Fn(&O) -> T + Copy,
{
    fn id(&self) -> crate::id::Id {
        self.signal.id
    }

    fn with<O2>(&self, f: impl FnOnce(&O) -> O2) -> O2
    where
        T: 'static,
    {
        let func = self.getter.get_untracked();
        self.signal.id.signal().unwrap().with(|t| f(&func(t)))
    }

    fn with_untracked<O2>(&self, f: impl FnOnce(&O) -> O2) -> O2
    where
        T: 'static,
    {
        let func = self.getter.get_untracked();
        self.signal
            .id
            .signal()
            .unwrap()
            .with_untracked(|t| f(&func(t)))
    }

    fn try_with<O2>(&self, f: impl FnOnce(Option<&O>) -> O2) -> O2
    where
        T: 'static,
    {
        if let Some(signal) = self.signal.id.signal() {
            let func = self.getter.get_untracked();
            signal.with(|v| f(Some(&func(v))))
        } else {
            f(None)
        }
    }

    fn try_with_untracked<O2>(&self, f: impl FnOnce(Option<&O>) -> O2) -> O2
    where
        T: 'static,
    {
        if let Some(signal) = self.signal.id.signal() {
            let func = self.getter.get_untracked();
            signal.with_untracked(|v| f(Some(&func(v))))
        } else {
            f(None)
        }
    }
}

impl<T, O, GF, UF> SignalTrack<O> for DerivedRwSignal<T, O, GF, UF>
where
    T: Clone + 'static,
    O: Clone,
    GF: Fn(&T) -> O + Copy,
    UF: Fn(&O) -> T + Copy,
{
    fn id(&self) -> crate::id::Id {
        self.signal.id
    }
}

impl<T, O, GF, UF> SignalUpdate<O> for DerivedRwSignal<T, O, GF, UF>
where
    T: 'static,
    O: 'static,
    GF: Fn(&T) -> O + Copy + 'static,
    UF: Fn(&O) -> T + Copy + 'static,
{
    fn id(&self) -> crate::id::Id {
        self.signal.id
    }

    fn set(&self, new_value: O)
    where
        O: 'static,
    {
        if let Some(signal) = self.id().signal() {
            let func = self.setter.get_untracked();
            signal.update_value_local::<_, T>(|v| {
                let new = func(&new_value);
                *v = new;
            });
        }
    }

    fn update(&self, f: impl FnOnce(&mut O))
    where
        O: 'static,
    {
        if let Some(signal) = self.id().signal() {
            let get_func = self.getter.get_untracked();
            let set_func = self.setter.get_untracked();
            signal.update_value_local::<_, T>(|cv| {
                let mut new = get_func(cv);
                f(&mut new);
                let new = set_func(&new);
                *cv = new;
            });
        }
    }

    fn try_update<O2>(&self, f: impl FnOnce(&mut O) -> O2) -> Option<O2>
    where
        O: 'static,
    {
        self.id().signal().map(|signal| {
            let get_func = self.getter.get_untracked();
            let set_func = self.setter.get_untracked();
            signal.update_value_local::<_, T>(|cv| {
                let mut new = get_func(cv);
                let ret = f(&mut new);
                let new = set_func(&new);
                *cv = new;
                ret
            })
        })
    }
}

impl<T, O, GF, UF> DerivedRwSignal<T, O, GF, UF>
where
    T: 'static,
    O: 'static,
    GF: Fn(&T) -> O + Clone + 'static,
    UF: Fn(&O) -> T + Clone + 'static,
{
    pub fn new(signal: RwSignal<T, UnsyncStorage>, getter: GF, setter: UF) -> Self {
        let getter = RwSignal::<Box<GF>, UnsyncStorage>::new(Box::new(getter));
        let setter = RwSignal::<Box<UF>, UnsyncStorage>::new(Box::new(setter));
        DerivedRwSignal {
            signal,
            getter,
            setter,
            ty: PhantomData,
            ts: PhantomData,
        }
    }
}

pub fn create_derived_rw_signal<T, O, GF, UF>(
    signal: RwSignal<T, UnsyncStorage>,
    getter: GF,
    setter: UF,
) -> DerivedRwSignal<T, O, GF, UF>
where
    T: 'static,
    O: 'static,
    GF: Fn(&T) -> O + Clone + 'static,
    UF: Fn(&O) -> T + Clone + 'static,
{
    DerivedRwSignal::new(signal, getter, setter)
}

/// A derived signal backed by a thread-safe [`RwSignal`].
pub struct SyncDerivedRwSignal<
    T: Send + Sync + 'static,
    O: Send + Sync,
    GF: Fn(&T) -> O + Clone + Send + Sync + 'static,
    UF: Fn(&O) -> T + Clone + Send + Sync + 'static,
> {
    signal: RwSignal<T, SyncStorage>,
    getter: RwSignal<Box<GF>, SyncStorage>,
    setter: RwSignal<Box<UF>, SyncStorage>,
    ty: PhantomData<T>,
    ts: PhantomData<()>,
}

impl<T, O, GF, UF> Clone for SyncDerivedRwSignal<T, O, GF, UF>
where
    T: Send + Sync,
    O: Send + Sync,
    GF: Fn(&T) -> O + Copy + Send + Sync,
    UF: Fn(&O) -> T + Copy + Send + Sync,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, O, GF, UF> Copy for SyncDerivedRwSignal<T, O, GF, UF>
where
    T: Send + Sync,
    O: Send + Sync,
    GF: Fn(&T) -> O + Copy + Send + Sync,
    UF: Fn(&O) -> T + Copy + Send + Sync,
{
}

impl<T, O, GF, UF> SignalGet<O> for SyncDerivedRwSignal<T, O, GF, UF>
where
    T: Clone + Send + Sync + 'static,
    O: Clone + Send + Sync + 'static,
    GF: Fn(&T) -> O + Copy + Send + Sync,
    UF: Fn(&O) -> T + Copy + Send + Sync,
{
    fn id(&self) -> crate::id::Id {
        self.signal.id
    }

    fn get_untracked(&self) -> O
    where
        O: 'static,
    {
        self.try_get_untracked().unwrap()
    }

    fn get(&self) -> O
    where
        O: 'static,
    {
        self.try_get().unwrap()
    }

    fn try_get(&self) -> Option<O>
    where
        O: 'static,
    {
        let sig = self.getter;
        self.signal.id.signal().map(|signal| {
            let func = sig.get_untracked();
            func(&signal.get()).clone()
        })
    }

    fn try_get_untracked(&self) -> Option<O>
    where
        O: 'static,
    {
        let sig = self.getter;
        self.signal.id.signal().map(|signal| {
            let func = sig.get_untracked();
            func(&signal.get_untracked()).clone()
        })
    }
}

impl<T, O, GF, UF> SignalWith<O> for SyncDerivedRwSignal<T, O, GF, UF>
where
    T: Clone + Send + Sync + 'static,
    O: Clone + Send + Sync,
    GF: Fn(&T) -> O + Copy + Send + Sync,
    UF: Fn(&O) -> T + Copy + Send + Sync,
{
    fn id(&self) -> crate::id::Id {
        self.signal.id
    }

    fn with<O2>(&self, f: impl FnOnce(&O) -> O2) -> O2
    where
        T: 'static,
    {
        let func = self.getter.get_untracked();
        self.signal.id.signal().unwrap().with(|t| f(&func(t)))
    }

    fn with_untracked<O2>(&self, f: impl FnOnce(&O) -> O2) -> O2
    where
        T: 'static,
    {
        let func = self.getter.get_untracked();
        self.signal
            .id
            .signal()
            .unwrap()
            .with_untracked(|t| f(&func(t)))
    }

    fn try_with<O2>(&self, f: impl FnOnce(Option<&O>) -> O2) -> O2
    where
        T: 'static,
    {
        if let Some(signal) = self.signal.id.signal() {
            let func = self.getter.get_untracked();
            signal.with(|v| f(Some(&func(v))))
        } else {
            f(None)
        }
    }

    fn try_with_untracked<O2>(&self, f: impl FnOnce(Option<&O>) -> O2) -> O2
    where
        T: 'static,
    {
        if let Some(signal) = self.signal.id.signal() {
            let func = self.getter.get_untracked();
            signal.with_untracked(|v| f(Some(&func(v))))
        } else {
            f(None)
        }
    }
}

impl<T, O, GF, UF> SignalTrack<O> for SyncDerivedRwSignal<T, O, GF, UF>
where
    T: Clone + Send + Sync + 'static,
    O: Clone + Send + Sync,
    GF: Fn(&T) -> O + Copy + Send + Sync,
    UF: Fn(&O) -> T + Copy + Send + Sync,
{
    fn id(&self) -> crate::id::Id {
        self.signal.id
    }
}

impl<T, O, GF, UF> SignalUpdate<O> for SyncDerivedRwSignal<T, O, GF, UF>
where
    T: Send + Sync + 'static,
    O: Send + Sync + 'static,
    GF: Fn(&T) -> O + Copy + Send + Sync,
    UF: Fn(&O) -> T + Copy + Send + Sync,
{
    fn id(&self) -> crate::id::Id {
        self.signal.id
    }

    fn set(&self, new_value: O)
    where
        O: 'static,
    {
        if let Some(signal) = self.id().signal() {
            let func = self.setter.get_untracked();
            signal.update_value_sync::<_, T>(|v| {
                let new = func(&new_value);
                *v = new;
            });
        }
    }

    fn update(&self, f: impl FnOnce(&mut O))
    where
        O: 'static,
    {
        if let Some(signal) = self.id().signal() {
            let get_func = self.getter.get_untracked();
            let set_func = self.setter.get_untracked();
            signal.update_value_sync::<_, T>(|cv| {
                let mut new = get_func(cv);
                f(&mut new);
                let new = set_func(&new);
                *cv = new;
            });
        }
    }

    fn try_update<O2>(&self, f: impl FnOnce(&mut O) -> O2) -> Option<O2>
    where
        O: 'static,
    {
        self.id().signal().map(|signal| {
            let get_func = self.getter.get_untracked();
            let set_func = self.setter.get_untracked();
            signal.update_value_sync::<_, T>(|cv| {
                let mut new = get_func(cv);
                let ret = f(&mut new);
                let new = set_func(&new);
                *cv = new;
                ret
            })
        })
    }
}

impl<T, O, GF, UF> SyncDerivedRwSignal<T, O, GF, UF>
where
    T: Send + Sync + 'static,
    O: Send + Sync,
    GF: Fn(&T) -> O + Clone + Send + Sync + 'static,
    UF: Fn(&O) -> T + Clone + Send + Sync + 'static,
{
    pub fn new(signal: RwSignal<T, SyncStorage>, getter: GF, setter: UF) -> Self {
        let getter = RwSignal::<Box<GF>, SyncStorage>::new_sync(Box::new(getter));
        let setter = RwSignal::<Box<UF>, SyncStorage>::new_sync(Box::new(setter));
        SyncDerivedRwSignal {
            signal,
            getter,
            setter,
            ty: PhantomData,
            ts: PhantomData,
        }
    }
}

pub fn create_sync_derived_rw_signal<T, O, GF, UF>(
    signal: RwSignal<T, SyncStorage>,
    getter: GF,
    setter: UF,
) -> SyncDerivedRwSignal<T, O, GF, UF>
where
    T: Send + Sync + 'static,
    O: Send + Sync,
    GF: Fn(&T) -> O + Clone + Send + Sync + 'static,
    UF: Fn(&O) -> T + Clone + Send + Sync + 'static,
{
    SyncDerivedRwSignal::new(signal, getter, setter)
}
