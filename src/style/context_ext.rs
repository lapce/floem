//! `ContextValueExt` — floem-side extension trait for
//! [`floem_style::ContextValue`].
//!
//! `ContextValue<T>` lives in `floem_style` and stores its eval closure with
//! the host style type erased behind `&dyn Any`. This trait adds the
//! `resolve(&Style)` convenience that also wraps the closure in floem's
//! reactive `with_effect` context so signal dependencies are tracked.

use std::any::Any;

use floem_reactive::Runtime;

use crate::style::{ContextValue, Style};

pub trait ContextValueExt<T> {
    fn resolve(&self, style: &Style) -> T;
}

impl<T: 'static> ContextValueExt<T> for ContextValue<T> {
    fn resolve(&self, style: &Style) -> T {
        Runtime::with_effect(style.effect_context.clone(), || {
            self.resolve_erased(style as &dyn Any)
        })
    }
}
