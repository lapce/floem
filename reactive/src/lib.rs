use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    marker::PhantomData,
    rc::Rc,
    sync::atomic::AtomicU64,
};

thread_local! {
    static RUNTIME: Runtime = Runtime::new();
    static SIGNALS: RefCell<HashMap<Id, Signal>> = Default::default();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Hash)]
/// A stable identifier for an element.
struct Id(u64);

impl Id {
    fn next() -> Id {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Id(COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }

    fn signal(&self) -> Option<Signal> {
        SIGNALS.with(|signals| signals.borrow().get(self).cloned())
    }

    fn add_signal(&self, signal: Signal) {
        SIGNALS.with(|signals| signals.borrow_mut().insert(*self, signal));
    }

    fn set_owner(&self) {
        RUNTIME.with(|runtime| {
            if let Some(owner) = runtime.current_owner.borrow().as_ref() {
                let mut children = runtime.children.borrow_mut();
                let children = children.entry(*owner).or_default();
                children.insert(*self);
            }
        });
    }

    fn dispose(&self) {
        RUNTIME.with(|runtime| {
            let mut children = runtime.children.borrow_mut();
            if let Some(children) = children.remove(self) {
                for child in children {
                    child.dispose();
                }
            }
            SIGNALS.with(|signals| {
                signals.borrow_mut().remove(self);
            });
        });
    }
}

fn with_owner<T>(owner: Id, f: impl FnOnce() -> T + 'static) -> T
where
    T: 'static,
{
    let prev_owner = RUNTIME.with(|runtime| {
        let mut current_owner = runtime.current_owner.borrow_mut();
        let prev_owner = current_owner.take();
        *current_owner = Some(owner);
        prev_owner
    });

    let result = f();

    RUNTIME.with(|runtime| {
        *runtime.current_owner.borrow_mut() = prev_owner;
    });

    result
}

#[derive(Clone, Copy)]
pub struct Owner(Id);

impl Default for Owner {
    fn default() -> Self {
        Self::new()
    }
}

impl Owner {
    pub fn new() -> Self {
        Self(Id::next())
    }

    pub fn create_child(&self) -> Owner {
        let child = Id::next();
        RUNTIME.with(|runtime| {
            let mut children = runtime.children.borrow_mut();
            let children = children.entry(self.0).or_default();
            children.insert(child);
        });
        Owner(child)
    }

    pub fn create_signal<T>(&self, value: T) -> (ReadSignal<T>, WriteSignal<T>)
    where
        T: Any + 'static,
    {
        with_owner(self.0, || create_signal(value))
    }

    pub fn create_rw_signal<T>(&self, value: T) -> RwSignal<T>
    where
        T: Any + 'static,
    {
        with_owner(self.0, || create_rw_signal(value))
    }
}

struct Runtime {
    current_effect: RefCell<Option<Rc<dyn EffectTrait>>>,
    current_owner: RefCell<Option<Id>>,
    children: RefCell<HashMap<Id, HashSet<Id>>>,
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
            current_owner: RefCell::new(None),
            children: RefCell::new(HashMap::new()),
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

impl<T, F> Drop for Effect<T, F>
where
    T: 'static,
    F: Fn(Option<T>) -> T,
{
    fn drop(&mut self) {
        self.id.dispose();
    }
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
    println!("run effect when creation ");
    run_effect(effect);
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
    id.set_owner();
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
    id.set_owner();
    RwSignal {
        id,
        ty: PhantomData,
    }
}

fn run_effect(effect: Rc<dyn EffectTrait>) {
    effect.id().dispose();

    observer_clean_up(&effect);

    RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = Some(effect.clone());
    });

    with_owner(effect.id(), move || {
        println!("effect run");
        effect.run();
    });

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

impl<T> RwSignal<T> {
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
        println!("subscribe");
        RUNTIME.with(|runtime| {
            if let Some(effect) = runtime.current_effect.borrow().as_ref() {
                println!("subscribe current effect");
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
    println!("run effects");
    signal.run_effects();
    result
}

pub struct ReadSignal<T> {
    id: Id,
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
