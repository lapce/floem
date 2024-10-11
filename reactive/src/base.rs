use std::marker::PhantomData;

use crate::{
    id::Id, signal::Signal, ReadSignal, RwSignal, SignalGet, SignalUpdate, SignalWith, WriteSignal,
};

/// BaseSignal gives you another way to control the lifetime of a Signal
/// apart from Scope.
///
/// When BaseSignal is dropped, it will dispose the underlying Signal as well.
/// The signal isn't put in any Scope when a BaseSignal is created, so that
/// the lifetime of the signal can only be determined by BaseSignal rather than
/// Scope dependencies
pub struct BaseSignal<T> {
    id: Id,
    ty: PhantomData<T>,
}

impl<T> Eq for BaseSignal<T> {}

impl<T> PartialEq for BaseSignal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Drop for BaseSignal<T> {
    fn drop(&mut self) {
        self.id.dispose();
    }
}

pub fn create_base_signal<T: 'static>(value: T) -> BaseSignal<T> {
    let id = Signal::create(value);
    BaseSignal {
        id,
        ty: PhantomData,
    }
}

impl<T> BaseSignal<T> {
    /// Create a RwSignal of this Signal
    pub fn rw(&self) -> RwSignal<T> {
        RwSignal {
            id: self.id,
            ty: PhantomData,
        }
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

impl<T: Clone> SignalGet<T> for BaseSignal<T> {
    fn id(&self) -> Id {
        self.id
    }
}

impl<T> SignalWith<T> for BaseSignal<T> {
    fn id(&self) -> Id {
        self.id
    }
}

impl<T> SignalUpdate<T> for BaseSignal<T> {
    fn id(&self) -> Id {
        self.id
    }
}
