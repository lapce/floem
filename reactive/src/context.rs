use std::any::{Any, TypeId};

use crate::runtime::RUNTIME;

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
