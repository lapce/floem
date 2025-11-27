use std::any::Any;

use std::{marker::PhantomData, rc::Rc};

use crate::id::Id;

/// Storage marker types for signals.
pub struct UnsyncStorage(PhantomData<Rc<()>>);
pub struct SyncStorage;

/// Internal abstraction over how signals are stored.
pub(crate) trait Storage<T: Any + 'static> {
    fn create(value: T) -> Id;
    fn get(id: Id) -> Option<Self::Signal>
    where
        Self: Sized;
    type Signal: Clone;
}
