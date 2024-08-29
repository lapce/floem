use std::marker::PhantomData;

use crate::{read::SignalTrack, RwSignal, SignalGet, SignalUpdate, SignalWith};

pub struct GetUpdateFn<T, O, GF: Fn(&T) -> O + Clone + 'static, UF: Fn(&O) -> T + 'static> {
    signal: RwSignal<T>,
    getter: RwSignal<Box<GF>>,
    setter: RwSignal<Box<UF>>,
    ty: PhantomData<T>,
}

impl<T, O, GF: Fn(&T) -> O + Copy, UF: Fn(&O) -> T + Copy> Clone for GetUpdateFn<T, O, GF, UF> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, O, GF: Fn(&T) -> O + Copy, UF: Fn(&O) -> T + Copy> Copy for GetUpdateFn<T, O, GF, UF> {}

impl<T: Clone + 'static, O: Clone, GF: Fn(&T) -> O + Copy, UF: Fn(&O) -> T + Copy> SignalGet<O>
    for GetUpdateFn<T, O, GF, UF>
{
    fn id(&self) -> crate::id::Id {
        self.signal.id
    }

    fn try_get(&self) -> Option<O>
    where
        O: 'static,
    {
        let sig = self.getter;
        SignalGet::id(self).signal().map(|signal| {
            let func = sig.get_untracked();
            func(&signal.get()).clone()
        })
    }

    fn try_get_untracked(&self) -> Option<O>
    where
        O: 'static,
    {
        let sig = self.getter;
        SignalGet::id(self).signal().map(|signal| {
            let func = sig.get_untracked();
            func(&signal.get_untracked()).clone()
        })
    }
}

impl<T: Clone + 'static, O: Clone, GF: Fn(&T) -> O + Copy, UF: Fn(&O) -> T + Copy> SignalWith<O>
    for GetUpdateFn<T, O, GF, UF>
{
    fn id(&self) -> crate::id::Id {
        self.signal.id
    }

    fn with<O2>(&self, f: impl FnOnce(&O) -> O2) -> O2
    where
        T: 'static,
    {
        let func = self.getter.get_untracked();
        SignalWith::id(self).signal().unwrap().with(|t| f(&func(t)))
    }

    fn with_untracked<O2>(&self, f: impl FnOnce(&O) -> O2) -> O2
    where
        T: 'static,
    {
        let func = self.getter.get_untracked();
        SignalWith::id(self)
            .signal()
            .unwrap()
            .with_untracked(|t| f(&func(t)))
    }

    fn try_with<O2>(&self, f: impl FnOnce(Option<&O>) -> O2) -> O2
    where
        T: 'static,
    {
        if let Some(signal) = SignalWith::id(self).signal() {
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
        if let Some(signal) = SignalWith::id(self).signal() {
            let func = self.getter.get_untracked();
            signal.with_untracked(|v| f(Some(&func(v))))
        } else {
            f(None)
        }
    }
}

impl<T: Clone + 'static, O: Clone, GF: Fn(&T) -> O + Copy, UF: Fn(&O) -> T + Copy> SignalTrack<O>
    for GetUpdateFn<T, O, GF, UF>
{
    fn id(&self) -> crate::id::Id {
        self.signal.id
    }
    fn track(&self) {
        SignalWith::id(self).signal().unwrap().subscribe();
    }

    fn try_track(&self) {
        if let Some(signal) = SignalWith::id(self).signal() {
            signal.subscribe();
        }
    }
}

impl<T: 'static, O, GF: Fn(&T) -> O + Copy, UF: Fn(&O) -> T + Copy> SignalUpdate<O>
    for GetUpdateFn<T, O, GF, UF>
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
            signal.update_value::<_, T>(|v| {
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
            signal.update_value::<_, T>(|cv| {
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
            signal.update_value::<_, T>(|cv| {
                let mut new = get_func(cv);
                let ret = f(&mut new);
                let new = set_func(&new);
                *cv = new;
                ret
            })
        })
    }
}

pub fn create_get_update<T, O, GF, UF>(
    signal: RwSignal<T>,
    getter: GF,
    setter: UF,
) -> GetUpdateFn<T, O, GF, UF>
where
    GF: Fn(&T) -> O + Clone + 'static,
    UF: Fn(&O) -> T + 'static,
{
    let getter = RwSignal::new(Box::new(getter));
    let setter = RwSignal::new(Box::new(setter));
    GetUpdateFn {
        signal,
        getter,
        setter,
        ty: PhantomData,
    }
}
