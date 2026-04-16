//! A side-trait that types opt into when they can render an inspector
//! preview of their value. Kept separate from `StylePropValue` so the
//! engine's value types don't depend on `crate::view::View`; when
//! `floem-style` is extracted, `PropDebugView` stays in floem proper
//! while the value types and `StylePropValue` move out.

use crate::view::View;

pub trait PropDebugView {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        None
    }
}

/// Shorthand for types that don't provide an inspector preview.
/// Emits `impl PropDebugView for T {}` using the default `None` return.
#[macro_export]
macro_rules! no_debug_view {
    ($($t:ty),* $(,)?) => {
        $(
            impl $crate::style::PropDebugView for $t {}
        )*
    };
}
