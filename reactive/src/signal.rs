use std::{any::Any, cell::RefCell, collections::HashMap, fmt, marker::PhantomData, rc::Rc};

use crate::{
    effect::{run_effect, EffectTrait},
    id::Id,
    runtime::RUNTIME,
};

pub struct RwSignal<T> {
    id: Id,
    ty: PhantomData<T>,
}

impl<T> Copy for RwSignal<T> {}

impl<T> Clone for RwSignal<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            ty: Default::default(),
        }
    }
}

impl<T> Eq for RwSignal<T> {}

impl<T> PartialEq for RwSignal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> fmt::Debug for RwSignal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("RwSignal");
        s.field("id", &self.id);
        s.field("ty", &self.ty);
        #[cfg(any(debug_assertions))]
        s.finish()
    }
}

impl<T> RwSignal<T> {
    pub fn set(&self, new_value: T)
    where
        T: 'static,
    {
        if let Some(signal) = self.id.signal() {
            signal_update_value(&signal, |v| *v = new_value);
        }
    }

    pub fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        if let Some(signal) = self.id.signal() {
            signal_update_value(&signal, f);
        }
    }

    pub fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_update_value(&signal, f)
    }

    pub fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_with(&signal, f)
    }

    pub fn track(&self) {
        let signal = self.id.signal().unwrap();
        signal.subscribe();
    }

    pub fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_with_untracked(&signal, f)
    }

    pub fn read_only(&self) -> ReadSignal<T> {
        ReadSignal {
            id: self.id,
            ty: PhantomData,
        }
    }

    pub fn write_only(&self) -> WriteSignal<T> {
        WriteSignal {
            id: self.id,
            ty: PhantomData,
        }
    }
}

impl<T: Clone> RwSignal<T> {
    pub fn get(&self) -> T
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_get(&signal)
    }

    pub fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_get_untracked(&signal)
    }
}

pub fn create_rw_signal<T>(value: T) -> RwSignal<T>
where
    T: Any + 'static,
{
    let id = Id::next();
    let signal = Signal {
        id,
        subscribers: Rc::new(RefCell::new(HashMap::new())),
        value: Rc::new(RefCell::new(value)),
    };
    id.add_signal(signal);
    id.set_scope();
    RwSignal {
        id,
        ty: PhantomData,
    }
}

pub struct ReadSignal<T> {
    pub(crate) id: Id,
    ty: PhantomData<T>,
}

impl<T> Copy for ReadSignal<T> {}

impl<T> Clone for ReadSignal<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            ty: Default::default(),
        }
    }
}

impl<T> Eq for ReadSignal<T> {}

impl<T> PartialEq for ReadSignal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: Clone> ReadSignal<T> {
    pub fn get(&self) -> T
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_get(&signal)
    }

    pub fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_get_untracked(&signal)
    }
}

impl<T> ReadSignal<T> {
    pub fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_with(&signal, f)
    }

    pub fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_with_untracked(&signal, f)
    }
}

pub struct WriteSignal<T> {
    id: Id,
    ty: PhantomData<T>,
}

impl<T> Copy for WriteSignal<T> {}

impl<T> Clone for WriteSignal<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            ty: Default::default(),
        }
    }
}

impl<T> Eq for WriteSignal<T> {}

impl<T> PartialEq for WriteSignal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> WriteSignal<T> {
    pub fn set(&self, new_value: T)
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_update_value(&signal, |v| *v = new_value);
    }

    pub fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_update_value(&signal, f);
    }

    pub fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_update_value(&signal, f)
    }
}

pub fn create_signal<T>(value: T) -> (ReadSignal<T>, WriteSignal<T>)
where
    T: Any + 'static,
{
    let id = Id::next();
    let signal = Signal {
        id,
        subscribers: Rc::new(RefCell::new(HashMap::new())),
        value: Rc::new(RefCell::new(value)),
    };
    id.add_signal(signal);
    id.set_scope();
    (
        ReadSignal {
            id,
            ty: PhantomData,
        },
        WriteSignal {
            id,
            ty: PhantomData,
        },
    )
}

#[derive(Clone)]
pub(crate) struct Signal {
    pub(crate) id: Id,
    pub(crate) value: Rc<RefCell<dyn Any>>,
    pub(crate) subscribers: Rc<RefCell<HashMap<Id, Rc<dyn EffectTrait>>>>,
}

impl Signal {
    pub(crate) fn subscribers(&self) -> HashMap<Id, Rc<dyn EffectTrait>> {
        self.subscribers.borrow().clone()
    }

    pub(crate) fn run_effects(&self) {
        for (_, subscriber) in self.subscribers() {
            run_effect(subscriber);
        }
    }

    pub(crate) fn subscribe(&self) {
        RUNTIME.with(|runtime| {
            if let Some(effect) = runtime.current_effect.borrow().as_ref() {
                self.subscribers
                    .borrow_mut()
                    .insert(effect.id(), effect.clone());
                effect.add_observer(self.clone());
            }
        });
    }
}

fn signal_get<T: Clone + 'static>(signal: &Signal) -> T {
    signal.subscribe();
    signal_get_untracked(signal)
}

fn signal_get_untracked<T: Clone + 'static>(signal: &Signal) -> T {
    let value = signal.value.borrow();
    let value = value.downcast_ref::<T>().expect("to downcast signal type");
    value.clone()
}

fn signal_with<O, T: 'static>(signal: &Signal, f: impl FnOnce(&T) -> O) -> O {
    signal.subscribe();
    signal_with_untracked(signal, f)
}

fn signal_with_untracked<O, T: 'static>(signal: &Signal, f: impl FnOnce(&T) -> O) -> O {
    let value = signal.value.borrow();
    let value = value.downcast_ref::<T>().expect("to downcast signal type");
    f(value)
}

fn signal_update_value<U, T: 'static>(signal: &Signal, f: impl FnOnce(&mut T) -> U) -> Option<U> {
    let result = {
        let mut value = signal.value.borrow_mut();
        value.downcast_mut::<T>().map(f)
    };
    signal.run_effects();
    result
}
