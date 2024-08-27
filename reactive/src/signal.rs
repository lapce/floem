use std::{
    any::Any,
    cell::{Ref, RefCell},
    collections::HashMap,
    fmt,
    marker::PhantomData,
    rc::Rc,
};

use crate::{
    effect::{run_effect, EffectTrait},
    id::Id,
    read::{SignalRead, SignalWith},
    runtime::RUNTIME,
    write::SignalWrite,
    SignalGet, SignalUpdate,
};

/// A read write Signal which can act as both a Getter and a Setter
pub struct RwSignal<T> {
    pub(crate) id: Id,
    pub(crate) ty: PhantomData<T>,
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

impl<T: 'static> RwSignal<T> {
    pub fn new(value: T) -> Self {
        create_rw_signal(value)
    }
    pub fn new_split(value: T) -> (ReadSignal<T>, WriteSignal<T>) {
        let sig = Self::new(value);
        (sig.read_only(), sig.write_only())
    }
}

/// Creates a new RwSignal which can act both as a setter and a getter.
/// Accessing the signal value in an Effect will make the Effect subscribe
/// to the value change of the Signal. And whenever the signal value changes,
/// it will trigger an effect run.
pub fn create_rw_signal<T>(value: T) -> RwSignal<T>
where
    T: Any + 'static,
{
    let id = Signal::create(value);
    id.set_scope();
    RwSignal {
        id,
        ty: PhantomData,
    }
}

/// A getter only Signal
pub struct ReadSignal<T> {
    pub(crate) id: Id,
    pub(crate) ty: PhantomData<T>,
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

/// A setter only Signal
pub struct WriteSignal<T> {
    pub(crate) id: Id,
    pub(crate) ty: PhantomData<T>,
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

/// Creates a new setter and getter Signal.
/// Accessing the signal value in an Effect will make the Effect subscribe
/// to the value change of the Signal. And whenever the signal value changes,
/// it will trigger an effect run.
pub fn create_signal<T>(value: T) -> (ReadSignal<T>, WriteSignal<T>)
where
    T: Any + 'static,
{
    let s = create_rw_signal(value);
    (s.read_only(), s.write_only())
}

/// The internal Signal where the value is stored, and effects are stored.
#[derive(Clone)]
pub(crate) struct Signal {
    pub(crate) id: Id,
    pub(crate) value: Rc<dyn Any>,
    pub(crate) subscribers: Rc<RefCell<HashMap<Id, Rc<dyn EffectTrait>>>>,
}

impl Signal {
    pub fn create<T>(value: T) -> Id
    where
        T: Any + 'static,
    {
        let id = Id::next();
        let value = RefCell::new(value);
        let signal = Signal {
            id,
            subscribers: Rc::new(RefCell::new(HashMap::new())),
            value: Rc::new(value),
        };
        id.add_signal(signal);
        id
    }

    pub fn borrow<T: 'static>(&self) -> Ref<'_, T> {
        let value = self
            .value
            .downcast_ref::<RefCell<T>>()
            .expect("to downcast signal type");
        value.borrow()
    }

    pub(crate) fn get_untracked<T: Clone + 'static>(&self) -> T {
        let value = self.borrow::<T>();
        value.clone()
    }

    pub(crate) fn get<T: Clone + 'static>(&self) -> T {
        self.subscribe();
        self.get_untracked()
    }

    pub(crate) fn with_untracked<O, T: 'static>(&self, f: impl FnOnce(&T) -> O) -> O {
        let value = self.borrow::<T>();
        f(&value)
    }

    pub(crate) fn with<O, T: 'static>(&self, f: impl FnOnce(&T) -> O) -> O {
        self.subscribe();
        self.with_untracked(f)
    }

    pub(crate) fn update_value<U, T: 'static>(&self, f: impl FnOnce(&mut T) -> U) -> U {
        let result = self
            .value
            .downcast_ref::<RefCell<T>>()
            .expect("to downcast signal type");
        let result = f(&mut result.borrow_mut());
        self.run_effects();
        result
    }

    pub(crate) fn subscribers(&self) -> HashMap<Id, Rc<dyn EffectTrait>> {
        self.subscribers.borrow().clone()
    }

    pub(crate) fn run_effects(&self) {
        // If we are batching then add it as a pending effect
        if RUNTIME.with(|r| r.batching.get()) {
            RUNTIME.with(|r| {
                for (_, subscriber) in self.subscribers() {
                    r.add_pending_effect(subscriber);
                }
            });
            return;
        }

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

impl<T: Clone> SignalGet<T> for RwSignal<T> {
    fn id(&self) -> Id {
        self.id
    }
}

impl<T> SignalWith<T> for RwSignal<T> {
    fn id(&self) -> Id {
        self.id
    }
}

impl<T> SignalRead<T> for RwSignal<T> {
    fn id(&self) -> Id {
        self.id
    }
}

impl<T> SignalUpdate<T> for RwSignal<T> {
    fn id(&self) -> Id {
        self.id
    }
}

impl<T: Clone> SignalGet<T> for ReadSignal<T> {
    fn id(&self) -> Id {
        self.id
    }
}

impl<T> SignalWith<T> for ReadSignal<T> {
    fn id(&self) -> Id {
        self.id
    }
}

impl<T> SignalRead<T> for ReadSignal<T> {
    fn id(&self) -> Id {
        self.id
    }
}

impl<T> SignalUpdate<T> for WriteSignal<T> {
    fn id(&self) -> Id {
        self.id
    }
}

impl<T> SignalWrite<T> for WriteSignal<T> {
    fn id(&self) -> Id {
        self.id
    }
}
