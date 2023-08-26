use std::{any::Any, cell::RefCell, collections::HashMap, fmt, marker::PhantomData, rc::Rc};

use crate::{
    effect::{run_effect, EffectTrait},
    id::Id,
    runtime::RUNTIME,
};

/// A read write Signal which can acts as both a Getter and a Setter
pub struct RwSignal<T> {
    id: Id,
    ty: PhantomData<T>,
}

impl<T> Copy for RwSignal<T> {}

impl<T> Clone for RwSignal<T> {
    fn clone(&self) -> Self {
        *self
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
        s.finish()
    }
}

impl<T> RwSignal<T> {
    /// Sets the new_value to the Signal and triggers effect run
    pub fn set(&self, new_value: T)
    where
        T: 'static,
    {
        if let Some(signal) = self.id.signal() {
            signal_update_value(&signal, |v| *v = new_value);
        }
    }

    /// Update the stored value with the given function and triggers effect run
    pub fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        if let Some(signal) = self.id.signal() {
            signal_update_value(&signal, f);
        }
    }

    /// Update the stored value with the given function, triggers effect run,
    /// and returns the value returned by the function
    pub fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_update_value(&signal, f)
    }

    /// Applies a clsoure to the current value stored in the Signal, and subcribes
    /// to the current runnig effect to this Memo.
    pub fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_with(&signal, f)
    }

    /// Applies a clsoure to the current value stored in the Signal, but it doesn't subcribe
    /// to the current runnig effect.
    pub fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_with_untracked(&signal, f)
    }

    /// If the signal isn't disposed, applies a clsoure to the current value stored in the Signal,
    /// but it doesn't subcribe to the current runnig effect.
    pub fn try_with_untracked<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        if let Some(signal) = self.id.signal() {
            signal_with_untracked(&signal, |v| f(Some(v)))
        } else {
            f(None)
        }
    }

    /// Only subcribes to the current runnig effect to this Signal.
    pub fn track(&self) {
        let signal = self.id.signal().unwrap();
        signal.subscribe();
    }

    /// Create a Getter of this Signal
    pub fn read_only(&self) -> ReadSignal<T> {
        ReadSignal {
            id: self.id,
            ty: PhantomData,
        }
    }

    /// Create a Setter of this Signal
    pub fn write_only(&self) -> WriteSignal<T> {
        WriteSignal {
            id: self.id,
            ty: PhantomData,
        }
    }
}

impl<T: Clone> RwSignal<T> {
    /// Clones and returns the current value stored in the Signal, and subcribes
    /// to the current runnig effect to this Signal.
    pub fn get(&self) -> T
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_get(&signal)
    }

    /// Clones and returns the current value stored in the Signal, but it doesn't subcribe
    /// to the current runnig effect.
    pub fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_get_untracked(&signal)
    }

    /// Try to clone and return the current value stored in the Signal, and returns None
    /// if it's already disposed. It doesn't subcribe to the current runnig effect.
    pub fn try_get_untracked(&self) -> Option<T>
    where
        T: 'static,
    {
        self.id.signal().map(|signal| signal_get_untracked(&signal))
    }
}

/// Creates a new RwSignal which can act both as a setter and a getter.
/// Accessing the signal value in an Effect will make the Effect subscribes
/// to the value change of the Signal. And whenever the signal value changes,
/// it will trigger an effect run.
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

/// A getter only Signal
pub struct ReadSignal<T> {
    pub(crate) id: Id,
    ty: PhantomData<T>,
}

impl<T> Copy for ReadSignal<T> {}

impl<T> Clone for ReadSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Eq for ReadSignal<T> {}

impl<T> PartialEq for ReadSignal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: Clone> ReadSignal<T> {
    /// Clones and returns the current value stored in the Signal, and subcribes
    /// to the current runnig effect to this Signal.
    pub fn get(&self) -> T
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_get(&signal)
    }

    /// Clones and returns the current value stored in the Signal, but it doesn't subcribe
    /// to the current runnig effect.
    pub fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_get_untracked(&signal)
    }
}

impl<T> ReadSignal<T> {
    /// Applies a clsoure to the current value stored in the Signal, and subcribes
    /// to the current runnig effect to this Memo.
    pub fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_with(&signal, f)
    }

    /// Applies a clsoure to the current value stored in the Signal, but it doesn't subcribe
    /// to the current runnig effect.
    pub fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_with_untracked(&signal, f)
    }
}

/// A setter only Signal
pub struct WriteSignal<T> {
    id: Id,
    ty: PhantomData<T>,
}

impl<T> Copy for WriteSignal<T> {}

impl<T> Clone for WriteSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Eq for WriteSignal<T> {}

impl<T> PartialEq for WriteSignal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> WriteSignal<T> {
    /// Sets the new_value to the Signal and triggers effect run
    pub fn set(&self, new_value: T)
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_update_value(&signal, |v| *v = new_value);
    }

    /// When the signal exists, sets the new_value to the Signal and triggers effect run
    pub fn try_set(&self, new_value: T)
    where
        T: 'static,
    {
        if let Some(signal) = self.id.signal() {
            signal_update_value(&signal, |v| *v = new_value);
        }
    }

    /// Update the stored value with the given function and triggers effect run
    pub fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_update_value(&signal, f);
    }

    /// Update the stored value with the given function, triggers effect run,
    /// and returns the value returned by the function
    pub fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static,
    {
        let signal = self.id.signal().unwrap();
        signal_update_value(&signal, f)
    }
}

/// Creates a new setter and getter Signal.
/// Accessing the signal value in an Effect will make the Effect subscribes
/// to the value change of the Signal. And whenever the signal value changes,
/// it will trigger an effect run.
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

/// The interal Signal where the value is stored, and effects are stored.
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
                effect.add_observer(self.id);
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
