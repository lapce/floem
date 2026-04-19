//! The interface the style engine uses to talk to the host during the
//! cascade.
//!
//! Every method on `StyleSink` is either a **read** the cascade needs to
//! classify an element (interaction state, theme defaults, frame
//! metadata) or a **write** for a fact the cascade detects but only the
//! host can act on (this element became `position: fixed`, inspector
//! needs a snapshot, layout needs to re-run, an animation tick wants to
//! happen). Purely host-side concerns — cursor overrides, focus-nav
//! caches, paint scheduling, frame-loop waking — stay as inherent
//! methods on the host. They're not on this trait because the engine
//! doesn't call them.
//!
//! All per-element methods take [`ElementId`] so the trait carries no
//! host-specific node-id type. Implementors derive whatever internal
//! identity they need (e.g. floem's `ViewId` via
//! `ElementId::owning_id()`).

use crate::element_id::ElementId;
use crate::interaction::InteractionState;
use crate::recalc::StyleReason;
use crate::responsive::ScreenSizeBp;
use crate::style::Style;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

pub trait StyleSink {
    // --- Frame / root state ---
    fn frame_start(&self) -> Instant;
    fn screen_size_bp(&self) -> ScreenSizeBp;
    fn keyboard_navigation(&self) -> bool;
    fn root_size_width(&self) -> f64;
    fn is_dark_mode(&self) -> bool;

    // --- Theme defaults ---
    fn default_theme_classes(&self) -> &Style;
    fn default_theme_inherited(&self) -> &Style;

    // --- Per-element interaction reads ---
    fn is_hovered(&self, id: ElementId) -> bool;
    fn is_focused(&self, id: ElementId) -> bool;
    fn is_focus_within(&self, id: ElementId) -> bool;
    fn is_active(&self, id: ElementId) -> bool;
    fn is_file_hover(&self, id: ElementId) -> bool;

    // --- Dirty / schedule / invalidate ---
    fn mark_style_dirty_with(&mut self, id: ElementId, reason: StyleReason);

    // --- Host side-effects ---
    fn mark_needs_layout(&mut self);

    /// Called at the end of a style resolution pass so hosts running under
    /// a debugger/inspector can snapshot the computed style. Default no-op;
    /// floem's `WindowState` overrides this to route into the inspector
    /// capture map when one is active.
    fn inspector_capture_style(&mut self, _id: ElementId, _computed_style: &Style) {}

    /// Apply any host-owned animations for `id` on top of the resolved
    /// `combined` style. Called by the tree cascade after classes +
    /// selectors are resolved but before inherited context is merged, so
    /// animated inherited properties propagate to descendants on the same
    /// pass. Animated `display/disabled/selected` bits are OR'd back into
    /// `interact`. Returns `true` iff any animation is still active — the
    /// cascade uses that to schedule another pass. Default no-op.
    fn apply_animations(
        &mut self,
        _id: ElementId,
        _combined: &mut Style,
        _interact: &mut InteractionState,
    ) -> bool {
        false
    }
}
