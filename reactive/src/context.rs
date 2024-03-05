use std::any::{Any, TypeId};

use crate::runtime::RUNTIME;

/// Try to retrieve a stored Context value in the reactive system.
/// You can store a Context value anywhere, and retrieve it from anywhere afterwards.
///
/// # Example
/// In a parent component:
/// ```rust
/// # use floem_reactive::provide_context;
/// provide_context(42);
/// provide_context(String::from("Hello world"));
/// ```
///
/// And so in a child component you can retrieve each context data by specifying the type:
/// ```rust
/// # use floem_reactive::use_context;
/// let foo: Option<i32> = use_context();
/// let bar: Option<String> = use_context();
/// ```
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

/// Sets a context value to be stored in the reactive system.
/// The stored context value can be retrieved from anywhere by using [use_context](use_context)
///
/// # Example
/// In a parent component:
/// ```rust
/// # use floem_reactive::provide_context;
/// provide_context(42);
/// provide_context(String::from("Hello world"));
/// ```
///
/// And so in a child component you can retrieve each context data by specifying the type:
/// ```rust
/// # use floem_reactive::use_context;
/// let foo: Option<i32> = use_context();
/// let bar: Option<String> = use_context();
/// ```
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
