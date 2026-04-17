//! `ContextValueExt` — floem-side extension trait for
//! [`floem_style::ContextValue`].
//!
//! `ContextValue<T>` lives in `floem_style` and stores its eval closure with
//! the host style type erased behind `&dyn Any`. This trait adds the
//! `resolve(&Style)` convenience which delegates to `Style::resolve_context`
//! (an inherent method) so signal dependencies are tracked inside the reactive
//! `with_effect` context the style was constructed under.

use crate::style::{ContextValue, Style};

pub trait ContextValueExt<T> {
    fn resolve(&self, style: &Style) -> T;
}

impl<T: 'static> ContextValueExt<T> for ContextValue<T> {
    fn resolve(&self, style: &Style) -> T {
        style.resolve_context(self)
    }
}
