//! The Floem style engine.
//!
//! This crate holds the view-agnostic parts of Floem's style system: unit
//! types, pseudo-class selectors, and (in later phases) the cascade, cache,
//! transitions, and per-node style storage. It is consumed by the `floem`
//! crate, and is designed so a second host (e.g. a native-widget backend)
//! can run the same engine over its own node type by implementing the
//! traits this crate exposes.

pub mod element_id;
pub mod interaction;
pub mod prop_value;
pub mod responsive;
pub mod selectors;
pub mod unit;
pub mod value_impls;

pub use element_id::ElementId;
pub use interaction::{InheritedInteractionCx, InteractionState};
pub use prop_value::StylePropValue;
pub use value_impls::AffineLerp;
