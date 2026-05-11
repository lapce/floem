//! A side-trait that types opt into when they can render an inspector
//! preview of their value. Kept separate from `StylePropValue` so the
//! engine's value types don't depend on `crate::view::View`.
//!
//! The trait's body receives an `&dyn InspectorRender` and returns a
//! `Box<dyn Any>`. Concrete renderers (e.g. `FloemInspectorRender` in the
//! `floem` crate) build their host view type and box it as `Any`; inspector
//! call sites downcast back to the host view type.

use std::any::Any;

use crate::InspectorRender;

pub trait PropDebugView {
    fn debug_view(&self, _r: &dyn InspectorRender) -> Option<Box<dyn Any>> {
        None
    }
}

/// Shorthand for types that don't provide an inspector preview.
/// Emits `impl PropDebugView for T {}` using the default `None` return.
#[macro_export]
macro_rules! no_debug_view {
    ($($t:ty),* $(,)?) => {
        $(
            impl $crate::PropDebugView for $t {}
        )*
    };
}
