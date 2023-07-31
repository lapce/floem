use std::{any::Any, cell::RefCell, collections::HashMap, fmt, rc::Rc};

use crate::{
    id::Id,
    memo::{create_memo, Memo},
    runtime::RUNTIME,
    signal::{create_rw_signal, create_signal, ReadSignal, RwSignal, Signal, WriteSignal},
    trigger::{create_trigger, Trigger},
};

#[derive(Clone, Copy)]
pub struct Scope(pub(crate) Id);

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

    /// This is normally used in create_effect, and it will bind the effect's lifetime
    /// to this scope
    pub fn track(&self) {
        let tracker = if let Some(signal) = self.0.signal() {
            signal
        } else {
            let signal = Signal {
                id: self.0,
                subscribers: Rc::new(RefCell::new(HashMap::new())),
                value: Rc::new(RefCell::new(())),
            };
            self.0.add_signal(signal.clone());
            signal
        };
        tracker.subscribe();
    }

    pub fn dispose(&self) {
        self.0.dispose();
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
