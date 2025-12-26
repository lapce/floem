//! Layout utilities for screen positioning, responsive design, and layout computation.
//!
//! This module provides:
//! - [`ScreenLayout`] - Tools for computing screen locations from view coordinates
//! - [`responsive`] - Responsive design breakpoints and size flags
//! - [`LayoutCx`] - Context for computing Taffy layout nodes
//! - [`ComputeLayoutCx`] - Context for computing view positions after Taffy layout

mod cx;
pub mod responsive;
mod screen;

pub use cx::{ComputeLayoutCx, LayoutCx};
pub use responsive::{GridBreakpoints, ScreenSize, range};
pub use screen::{ScreenLayout, screen_layout_for_window, try_create_screen_layout};
