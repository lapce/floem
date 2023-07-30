use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::{HashMap, HashSet},
    fmt,
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

    fn signal(&self) -> Option<Signal> {
        RUNTIME.with(|runtime| runtime.signals.borrow().get(self).cloned())
    }

    fn add_signal(&self, signal: Signal) {
        RUNTIME.with(|runtime| runtime.signals.borrow_mut().insert(*self, signal));
    }

    fn set_scope(&self) {
        RUNTIME.with(|runtime| {
            let scope = runtime.current_scope.borrow();
            let mut children = runtime.children.borrow_mut();
            let children = children.entry(*scope).or_default();
            children.insert(*self);
        });
    }

    fn dispose(&self) {
        RUNTIME.with(|runtime| {
            let children = {
                let mut children = runtime.children.borrow_mut();
                children.remove(self)
            };
            if let Some(children) = children {
                for child in children {
                    child.dispose();
                }
            }
            runtime.signals.borrow_mut().remove(self);
        });
    }
}

pub fn as_child_of_current_scope<T, U>(f: impl Fn(T) -> U + 'static) -> impl Fn(T) -> (U, Scope)
where
    T: 'static,
{
    let scope = Scope::current().create_child();
    move |t| {
        let prev_scope = RUNTIME.with(|runtime| {
            let mut current_scope = runtime.current_scope.borrow_mut();
            let prev_scope = *current_scope;
            *current_scope = scope.0;
            prev_scope
        });

        let result = f(t);

        RUNTIME.with(|runtime| {
            *runtime.current_scope.borrow_mut() = prev_scope;
        });

        (result, scope)
    }
}

pub fn with_scope<T>(scope: Scope, f: impl FnOnce() -> T + 'static) -> T
where
    T: 'static,
{
    let prev_scope = RUNTIME.with(|runtime| {
        let mut current_scope = runtime.current_scope.borrow_mut();
        let prev_scope = *current_scope;
        *current_scope = scope.0;
        prev_scope
    });

    let result = f();

    RUNTIME.with(|runtime| {
        *runtime.current_scope.borrow_mut() = prev_scope;
    });

    result
}

#[derive(Clone, Copy)]
pub struct Scope(Id);

impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("Scope");
        s.field("id", &self.0);
        #[cfg(any(debug_assertions))]
        s.finish()
    }
}

impl Scope {
    pub fn new() -> Self {
        Self(Id::next())
    }

    pub fn current() -> Scope {
        RUNTIME.with(|runtime| Scope(*runtime.current_scope.borrow()))
    }

    pub fn create_child(&self) -> Scope {
        let child = Id::next();
        RUNTIME.with(|runtime| {
            let mut children = runtime.children.borrow_mut();
            let children = children.entry(self.0).or_default();
            children.insert(child);
        });
        Scope(child)
    }

    pub fn create_signal<T>(self, value: T) -> (ReadSignal<T>, WriteSignal<T>)
    where
        T: Any + 'static,
    {
        with_scope(self, || create_signal(value))
    }

    pub fn create_rw_signal<T>(self, value: T) -> RwSignal<T>
    where
        T: Any + 'static,
    {
        with_scope(self, || create_rw_signal(value))
    }

    pub fn create_memo<T>(self, f: impl Fn(Option<&T>) -> T + 'static) -> Memo<T>
    where
        T: PartialEq + 'static,
    {
        with_scope(self, || create_memo(f))
    }

    pub fn create_trigger(self) -> Trigger {
        with_scope(self, create_trigger)
    }

    pub fn dispose(&self) {
        self.0.dispose();
    }
}

struct Runtime {
    current_effect: RefCell<Option<Rc<dyn EffectTrait>>>,
    current_scope: RefCell<Id>,
    children: RefCell<HashMap<Id, HashSet<Id>>>,
    signals: RefCell<HashMap<Id, Signal>>,
    contexts: RefCell<HashMap<TypeId, Box<dyn Any>>>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    fn new() -> Self {
        Self {
            current_effect: RefCell::new(None),
            current_scope: RefCell::new(Id::next()),
            children: RefCell::new(HashMap::new()),
            signals: Default::default(),
            contexts: Default::default(),
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
    let id = Id::next();
    let effect = Rc::new(Effect {
        id,
        f,
        value: Rc::new(RefCell::new(None::<T>)),
        ty: PhantomData,
        observers: Rc::new(RefCell::new(HashMap::new())),
    });
    id.set_scope();

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

    with_scope(Scope(id), move || {
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

fn run_effect(effect: Rc<dyn EffectTrait>) {
    effect.id().dispose();

    observer_clean_up(&effect);

    RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = Some(effect.clone());
    });

    with_scope(Scope(effect.id()), move || {
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
    let result = {
        let mut value = signal.value.borrow_mut();
        value.downcast_mut::<T>().map(f)
    };
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

pub struct Trigger {
    signal: RwSignal<()>,
}

impl Copy for Trigger {}

impl Clone for Trigger {
    fn clone(&self) -> Self {
        Self {
            signal: self.signal,
        }
    }
}

impl Trigger {
    pub fn notify(&self) {
        self.signal.set(());
    }

    pub fn track(&self) {
        self.signal.with(|_| {});
    }
}

pub fn create_trigger() -> Trigger {
    Trigger {
        signal: create_rw_signal(()),
    }
}

pub fn untrack<T>(f: impl FnOnce() -> T) -> T {
    let prev_effect = RUNTIME.with(|runtime| runtime.current_effect.borrow_mut().take());
    let result = f();
    RUNTIME.with(|runtime| {
        *runtime.current_effect.borrow_mut() = prev_effect;
    });
    result
}

pub fn use_context<T>() -> Option<T>
where
    T: Clone + 'static,
{
    let ty = TypeId::of::<T>();
    RUNTIME.with(|runtime| {
        let contexts = runtime.contexts.borrow();
        let context = contexts
            .get(&ty)
            .and_then(|val| val.downcast_ref::<T>())
            .cloned();
        context
    })
}

pub fn provide_context<T>(value: T)
where
    T: Clone + 'static,
{
    let id = value.type_id();

    RUNTIME.with(|runtime| {
        let mut contexts = runtime.contexts.borrow_mut();
        contexts.insert(id, Box::new(value) as Box<dyn Any>);
    });
}
