//! Reactive signals for futures that resolve to a single value.

mod builder;
mod signal;

// Re-export everything for convenience
pub use builder::*;
pub use signal::*;
