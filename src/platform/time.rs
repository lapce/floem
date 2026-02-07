//! Platform-agnostic time types.
//!
//! This module provides unified Duration and Instant types that work across
//! all platforms, using std::time on native platforms and web_time on wasm32.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::{Duration, Instant};

#[cfg(target_arch = "wasm32")]
pub use web_time::{Duration, Instant};
