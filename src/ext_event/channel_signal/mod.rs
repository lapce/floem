//! Reactive signals for channels that produce multiple values with error handling.

mod builder;
mod signal;

// Re-export everything for convenience
pub use builder::*;
pub use signal::*;
