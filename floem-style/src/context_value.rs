//! Deferred, context-resolved style values.
//!
//! [`ContextValue<T>`] packages a closure that computes a value of type `T`
//! from a host-specific style type (typically `floem::style::Style`). The
//! eval closure is stored with the host type erased behind `&dyn Any`, so
//! this type can live in `floem_style` without depending on `floem::style`.
//!
//! Callers running under floem should prefer
//! `floem::style::ContextValueExt::resolve`, which wraps the call in the
//! reactive `with_effect` context. For raw resolution use
//! [`ContextValue::resolve_erased`].

use std::any::Any;
use std::rc::Rc;

pub struct ContextValue<T> {
    pub(crate) eval: Rc<dyn Fn(&dyn Any) -> T>,
}

impl<T> Clone for ContextValue<T> {
    fn clone(&self) -> Self {
        Self {
            eval: self.eval.clone(),
        }
    }
}

impl<T> std::fmt::Debug for ContextValue<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContextValue(..)")
    }
}

impl<T> PartialEq for ContextValue<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.eval, &other.eval)
    }
}

impl<T> Eq for ContextValue<T> {}

impl<T: 'static> ContextValue<T> {
    /// Build a `ContextValue` from a closure that reads the host style type
    /// `S`. The concrete `S` is usually `floem::style::Style`, but any
    /// `'static` type works; the eval closure is stored type-erased and
    /// `resolve_erased` downcasts back to `&S` at call time.
    pub fn new<S: 'static>(eval: impl Fn(&S) -> T + 'static) -> Self {
        Self {
            eval: Rc::new(move |any| {
                eval(
                    any.downcast_ref::<S>()
                        .expect("ContextValue: style type mismatch"),
                )
            }),
        }
    }

    /// Low-level resolve. Prefer `floem::style::ContextValueExt::resolve`
    /// when running under floem, which additionally wraps the call in the
    /// reactive `with_effect` context.
    pub fn resolve_erased(&self, style: &dyn Any) -> T {
        (self.eval)(style)
    }

    pub fn map<U: 'static>(self, f: impl Fn(T) -> U + 'static) -> ContextValue<U> {
        let eval = self.eval;
        ContextValue {
            eval: Rc::new(move |any| f(eval(any))),
        }
    }
}
