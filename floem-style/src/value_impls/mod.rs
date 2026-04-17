//! `StylePropValue` impls for types not owned by the `floem` crate.
//!
//! These impls live in `floem-style` to satisfy Rust's orphan rule, since
//! both the trait and the types belong to external crates (or to
//! `floem-style`'s own unit module).

mod collections;
mod debug_view_impls;
mod peniko;
mod primitives;
mod taffy;
mod text;
mod unit;

#[cfg(feature = "localization")]
mod localization;

pub use peniko::AffineLerp;
