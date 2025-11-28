use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashSet,
    marker::PhantomData,
    rc::Rc,
};

use crate::{
    effect::{observer_clean_up, EffectTrait},
    id::Id,
    read::SignalTrack,
    runtime::{Runtime, RUNTIME},
    scope::Scope,
    signal::{ReadSignal, RwSignal, WriteSignal},
    write::SignalUpdate,
    SignalGet, SignalWith,
};

/// A memoized derived value that only recomputes when one of its tracked
/// dependencies changes, and only notifies dependents when its value changes.
///
/// Unlike the previous implementation, this is driven by dependency invalidation
/// rather than an `Effect` that eagerly recomputes.
pub struct Memo<T: PartialEq + 'static> {
    getter: ReadSignal<T>,
    memo_id: Id,
}

impl<T: PartialEq + 'static> Copy for Memo<T> {}

impl<T: PartialEq + 'static> Clone for Memo<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Clone + PartialEq + 'static> SignalGet<T> for Memo<T>
where
    ReadSignal<T>: SignalGet<T>,
{
    fn id(&self) -> crate::id::Id {
        self.getter.id
    }

    fn get_untracked(&self) -> T
    where
        T: 'static,
    {
        self.ensure_fresh();
        self.getter.get_untracked()
    }

    fn get(&self) -> T
    where
        T: 'static,
    {
        self.ensure_fresh();
        self.getter.get()
    }

    fn try_get(&self) -> Option<T>
    where
        T: 'static,
    {
        self.ensure_fresh();
        self.getter.try_get()
    }

    fn try_get_untracked(&self) -> Option<T>
    where
        T: 'static,
    {
        self.ensure_fresh();
        self.getter.try_get_untracked()
    }
}

impl<T: PartialEq + 'static> SignalTrack<T> for Memo<T> {
    fn id(&self) -> crate::id::Id {
        self.getter.id
    }
}

impl<T: PartialEq + 'static> SignalWith<T> for Memo<T>
where
    ReadSignal<T>: SignalWith<T>,
{
    fn id(&self) -> crate::id::Id {
        self.getter.id
    }

    fn with<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.ensure_fresh();
        self.getter.with(f)
    }

    fn with_untracked<O>(&self, f: impl FnOnce(&T) -> O) -> O
    where
        T: 'static,
    {
        self.ensure_fresh();
        self.getter.with_untracked(f)
    }

    fn try_with<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        self.ensure_fresh();
        self.getter.try_with(f)
    }

    fn try_with_untracked<O>(&self, f: impl FnOnce(Option<&T>) -> O) -> O
    where
        T: 'static,
    {
        self.ensure_fresh();
        self.getter.try_with_untracked(f)
    }
}

/// Create a Memo which takes the computed value of the given function, and triggers
/// the reactive system when the computed value is different from the last computed value.
#[deprecated(
    since = "0.2.0",
    note = "Use Memo::new instead; this will be removed in a future release"
)]
pub fn create_memo<T>(f: impl Fn(Option<&T>) -> T + 'static) -> Memo<T>
where
    T: PartialEq + 'static,
{
    Memo::new(f)
}

impl<T: PartialEq + 'static> Memo<T> {
    pub fn new(f: impl Fn(Option<&T>) -> T + 'static) -> Self {
        Runtime::assert_ui_thread();

        let memo_id = Id::next();
        let state = Rc::new(MemoState::new(memo_id, f));

        memo_id.set_scope();
        let effect: Rc<dyn EffectTrait> = state.clone();
        RUNTIME.with(|runtime| runtime.register_effect(&effect));

        let initial = state.compute_initial();
        let (getter, setter) = RwSignal::new_split(initial);
        state.set_signal(setter);
        state.mark_clean();

        Memo { getter, memo_id }
    }

    fn ensure_fresh(&self) {
        self.with_state(|state| state.ensure_fresh(&self.getter));
    }

    fn with_state<O>(&self, f: impl FnOnce(&MemoState<T>) -> O) -> Option<O> {
        RUNTIME.with(|runtime| {
            runtime
                .get_effect(self.memo_id)
                .and_then(|effect| effect.as_any().downcast_ref::<MemoState<T>>().map(f))
        })
    }
}

type ComputeFn<T> = Box<dyn Fn(Option<&T>) -> T>;

struct MemoState<T: PartialEq + 'static> {
    id: Id,
    compute: ComputeFn<T>,
    setter: RefCell<Option<WriteSignal<T>>>,
    dirty: Cell<bool>,
    observers: RefCell<HashSet<Id>>,
    _phantom: PhantomData<T>,
}

impl<T: PartialEq + 'static> MemoState<T> {
    fn new(id: Id, compute: impl Fn(Option<&T>) -> T + 'static) -> Self {
        Self {
            id,
            compute: Box::new(compute),
            setter: RefCell::new(None),
            dirty: Cell::new(true),
            observers: RefCell::new(HashSet::new()),
            _phantom: PhantomData,
        }
    }

    fn compute_initial(&self) -> T {
        let effect = RUNTIME
            .with(|runtime| runtime.get_effect(self.id))
            .expect("memo registered before initial compute");

        let prev_effect =
            RUNTIME.with(|runtime| runtime.current_effect.borrow_mut().replace(effect));
        let scope = Scope(self.id, PhantomData);
        let value = scope.enter(|| (self.compute)(None));

        RUNTIME.with(|runtime| *runtime.current_effect.borrow_mut() = prev_effect);
        value
    }

    fn set_signal(&self, setter: WriteSignal<T>) {
        self.setter.replace(Some(setter));
    }

    fn mark_clean(&self) {
        self.dirty.set(false);
    }

    fn ensure_fresh(&self, getter: &ReadSignal<T>) {
        if !self.dirty.get() {
            return;
        }
        self.recompute(getter);
    }

    fn recompute(&self, getter: &ReadSignal<T>) {
        Runtime::assert_ui_thread();
        let effect = RUNTIME
            .with(|runtime| runtime.get_effect(self.id))
            .expect("memo registered");

        observer_clean_up(&effect);

        let prev_effect =
            RUNTIME.with(|runtime| runtime.current_effect.borrow_mut().replace(effect));
        let scope = Scope(self.id, PhantomData);
        let (changed, new_value) = scope.enter(|| {
            getter.try_with_untracked(|prev| {
                let new_value = (self.compute)(prev);
                let changed = match prev {
                    Some(previous) => new_value != *previous,
                    None => true,
                };
                (changed, new_value)
            })
        });
        RUNTIME.with(|runtime| *runtime.current_effect.borrow_mut() = prev_effect);

        if changed {
            if let Some(setter) = self.setter.borrow().as_ref() {
                setter.set(new_value);
            }
        }

        self.dirty.set(false);
    }
}

impl<T: PartialEq + 'static> Drop for MemoState<T> {
    fn drop(&mut self) {
        if RUNTIME
            .try_with(|runtime| runtime.remove_effect(self.id))
            .is_ok()
        {
            self.id.dispose();
        }
    }
}

impl<T> EffectTrait for MemoState<T>
where
    T: PartialEq + 'static,
{
    fn id(&self) -> Id {
        self.id
    }

    fn run(&self) -> bool {
        self.dirty.set(true);
        true
    }

    fn add_observer(&self, id: Id) {
        self.observers.borrow_mut().insert(id);
    }

    fn clear_observers(&self) -> HashSet<Id> {
        std::mem::take(&mut *self.observers.borrow_mut())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
