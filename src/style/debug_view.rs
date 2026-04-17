//! Re-exports for the inspector debug-view plumbing.
//!
//! The `PropDebugView` trait and `InspectorRender` trait now live in
//! `floem_style`. This module keeps the `floem::style::{InspectorRender,
//! PropDebugView}` API surface stable by re-exporting them.

pub use floem_style::{InspectorRender, PropDebugView};
