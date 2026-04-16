//! Stable identity for a style target.
//!
//! [`ElementId`] identifies a single rectangle that the style engine can
//! restyle independently. A single host "view" may own multiple elements
//! (e.g. a scroll view with content area + two scrollbars).
//!
//! The owning host-side view is stored as a raw `u64` here rather than a
//! concrete `ViewId` so this crate stays host-agnostic. The host converts
//! between its own id type and this raw bits representation at the crate
//! boundary — typically via `slotmap::KeyData::as_ffi()` / `from_ffi()`.

use understory_box_tree::NodeId;

/// Identifies a rectangle in the box tree that the style engine can
/// restyle independently of its owning view.
///
/// - `.0` — box tree node id
/// - `.1` — raw bits of the owning view id (host-specific encoding)
/// - `.2` — whether this is the primary element for the owning view
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ElementId(pub NodeId, pub u64, pub bool);

impl ElementId {
    /// Returns `true` when this is the primary element for its owning view.
    #[inline]
    pub const fn is_view(&self) -> bool {
        self.2
    }
}
