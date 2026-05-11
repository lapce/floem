//! Layout utilities for screen positioning, responsive design, and layout computation.
//!
//! This module provides:
//! - [`ScreenLayout`] - Tools for computing screen locations from view coordinates
//! - [`responsive`] - Responsive design breakpoints and size flags
//! - [`LayoutCx`] - Context for computing Taffy layout nodes
//! - [`ComputeLayoutCx`] - Context for computing view positions after Taffy layout

mod screen;

/// Responsive design breakpoints and size flags.
///
/// Re-exported from the `floem_style` crate so `floem::layout::responsive`
/// stays a valid import path for downstream users.
pub use floem_style::responsive;
pub use floem_style::responsive::{GridBreakpoints, ScreenSize, range};
pub use screen::{ScreenLayout, screen_layout_for_window, try_create_screen_layout};
