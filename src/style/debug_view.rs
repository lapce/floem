//! A side-trait that types opt into when they can render an inspector
//! preview of their value. Kept separate from `StylePropValue` so the
//! engine's value types don't depend on `crate::view::View`.
//!
//! The trait's body receives an `&dyn InspectorRender` (from `floem_style`)
//! and returns a `Box<dyn Any>`. The concrete renderer in `floem` is
//! `FloemInspectorRender`, which builds `Box<dyn View>` widgets and wraps
//! them as `Any`; inspector call sites downcast back to `Box<dyn View>`.
//! Because the signature no longer names `View`, `PropDebugView` (and its
//! impls on types that live in `floem_style`) can be moved into
//! `floem_style` in a subsequent pass.

use std::any::Any;

pub use floem_style::InspectorRender;

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
            impl $crate::style::PropDebugView for $t {}
        )*
    };
}
