//! Common types and functionality shared across all async signal types.

pub mod executors;
mod traits;
mod type_state;

// Re-export everything for convenience
pub use traits::*;
pub(crate) use type_state::*;
