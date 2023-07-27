use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    marker::PhantomData,
    rc::Rc,
    sync::atomic::AtomicU64,
};

thread_local! {
    static RUNTIME: Runtime = Runtime::new();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Hash)]
/// A stable identifier for an element.
struct Id(u64);

impl Id {
    fn next() -> Id {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Id(COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

struct Runtime {
    current_effect: RefCell<Option<Rc<dyn EffectTrait>>>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            current_effect: RefCell::new(None),
        }
    }
}

trait EffectTrait {
    fn id(&self) -> Id;
    fn run(&self) -> bool;
    fn add_observer(&self, signal: Signal);
    fn current_observers(&self) -> HashMap<Id, Signal>;
    fn clear_observers(&self);
}

struct Effect<T, F>
where
    T: 'static,
    F: Fn(Option<T>) -> T,
{
    id: Id,
    f: F,
    value: Rc<RefCell<dyn Any>>,
    ty: PhantomData<T>,
    observers: Rc<RefCell<HashMap<Id, Signal>>>,
}

impl<T, F> EffectTrait for Effect<T, F>
where
    T: 'static,
    F: Fn(Option<T>) -> T,
{
    fn id(&self) -> Id {
        self.id
    }

    fn run(&self) -> bool {
        let value = self.value.clone();

        let curr_value = {
            // downcast value
            let mut value = value.borrow_mut();
            let value = value
                .downcast_mut::<Option<T>>()
                .expect("to downcast effect value");
            value.take()
        };

        // run the effect
        let new_value = (self.f)(curr_value);

        // set new value
        let mut value = value.borrow_mut();
        let value = value
            .downcast_mut::<Option<T>>()
            .expect("to downcast effect value");
        *value = Some(new_value);

        true
    }

    fn add_observer(&self, signal: Signal) {
        self.observers.borrow_mut().insert(signal.id, signal);
    }

    fn current_observers(&self) -> HashMap<Id, Signal> {
        self.observers.borrow().clone()
    }

    fn clear_observers(&self) {
        self.observers.borrow_mut().clear();
    }
}

pub fn create_effect<T>(f: impl Fn(Option<T>) -> T + 'static)
where
    T: Any + 'static,
{
    let effect = Rc::new(Effect {
        id: Id::next(),
        f,
        value: Rc::new(RefCell::new(None::<T>)),
        ty: PhantomData,
        observers: Rc::new(RefCell::new(HashMap::new())),
    });
    run_effect(effect);
}

pub fn create_signal<T>(value: T) -> (ReadSignal<T>, WriteSignal<T>)
where
    T: Any + 'static,
{
    let signal = Signal {
        id: Id::next(),
        subscribers: Rc::new(RefCell::new(HashMap::new())),
        value: Rc::new(RefCell::new(value)),
    };
    (
        ReadSignal {
            signal: signal.clone(),
            ty: PhantomData,
        },
        WriteSignal {
            signal,
            ty: PhantomData,
        },
    )
}

pub fn create_rw_signal<T>(value: T) -> RwSignal<T>
where
    T: Any + 'static,
{
    RwSignal {
        signal: Signal {
            id: Id::next(),
            subscribers: Rc::new(RefCell::new(HashMap::new())),
            value: Rc::new(RefCell::new(value)),
        },
        ty: PhantomData,
    }
}

fn run_effect(effect: Rc<dyn EffectTrait>) {
    observer_clean_up(&effect);

    RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = Some(effect.clone());
    });

    effect.run();

    RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = None;
    });
}

fn observer_clean_up(effect: &Rc<dyn EffectTrait>) {
    for (_, observer) in effect.current_observers().iter() {
        observer.subscribers.borrow_mut().remove(&effect.id());
    }
    effect.clear_observers();
}

#[derive(Clone)]
pub struct RwSignal<T> {
    signal: Signal,
    ty: PhantomData<T>,
}

impl<T> RwSignal<T> {
    pub fn set(&self, new_value: T)
    where
        T: 'static,
    {
        signal_update_value(&self.signal, |v| *v = new_value);
    }

    pub fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        signal_update_value(&self.signal, f);
    }

    pub fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static,
    {
        signal_update_value(&self.signal, f)
    }

    pub fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        signal_with(&self.signal, f)
    }

    pub fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        signal_with_untracked(&self.signal, f)
    }
}

impl<T: Clone> RwSignal<T> {
    pub fn get(&self) -> T
    where
        T: 'static,
    {
        signal_get(&self.signal)
    }

    pub fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        signal_get_untracked(&self.signal)
    }
}

#[derive(Clone)]
struct Signal {
    id: Id,
    value: Rc<RefCell<dyn Any>>,
    subscribers: Rc<RefCell<HashMap<Id, Rc<dyn EffectTrait>>>>,
}

impl Signal {
    fn subscribers(&self) -> HashMap<Id, Rc<dyn EffectTrait>> {
        self.subscribers.borrow().clone()
    }

    fn run_effects(&self) {
        for (_, subscriber) in self.subscribers() {
            run_effect(subscriber);
        }
    }

    fn subscribe(&self) {
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
    let mut value = signal.value.borrow_mut();
    let result = value.downcast_mut::<T>().map(f);
    signal.run_effects();
    result
}

#[derive(Clone)]
pub struct ReadSignal<T> {
    signal: Signal,
    ty: PhantomData<T>,
}

impl<T: Clone> ReadSignal<T> {
    pub fn get(&self) -> T
    where
        T: 'static,
    {
        signal_get(&self.signal)
    }

    pub fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        signal_get_untracked(&self.signal)
    }
}

impl<T> ReadSignal<T> {
    pub fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        signal_with(&self.signal, f)
    }

    pub fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        signal_with_untracked(&self.signal, f)
    }
}

#[derive(Clone)]
pub struct WriteSignal<T> {
    signal: Signal,
    ty: PhantomData<T>,
}

impl<T> WriteSignal<T> {
    pub fn set(&self, new_value: T)
    where
        T: 'static,
    {
        signal_update_value(&self.signal, |v| *v = new_value);
    }

    pub fn update(&self, f: impl FnOnce(&mut T))
    where
        T: 'static,
    {
        signal_update_value(&self.signal, f);
    }

    pub fn try_update<O>(&self, f: impl FnOnce(&mut T) -> O) -> Option<O>
    where
        T: 'static,
    {
        signal_update_value(&self.signal, f)
    }
}
