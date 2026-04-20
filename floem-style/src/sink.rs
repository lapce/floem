//! The interface the style engine uses to talk to the host during the
//! cascade.
//!
//! Every method on `StyleSink` is either a **read** the cascade needs to
//! classify a node (interaction state, theme defaults, frame metadata)
//! or the single `apply_animations` policy hook a host uses to
//! intercept the animation tick (native backends override it to
//! delegate to their compositor; CPU hosts let the engine's default
//! ticker drive). Every fact the cascade detects but only the host can
//! act on — `position: fixed` transitions, dirtied descendants, layout
//! invalidations — lives in tree-owned state the host drains via
//! `take_*` methods after `compute_style`. Purely host-side concerns
//! (cursor overrides, paint scheduling, frame-loop waking) stay as
//! inherent host methods. They're not on this trait because the engine
//! doesn't call them.
//!
//! Per-node methods take [`StyleNodeId`] — the engine's own handle —
//! not the host's view id. Hosts keep a sidecar mapping
//! (`StyleNodeId ↔ ViewId` in floem's case, same shape as
//! [`taffy::NodeId`] ↔ host-view) and translate at the trait boundary.

use crate::interaction::InteractionState;
use crate::responsive::ScreenSizeBp;
use crate::style::Style;
use crate::tree::StyleNodeId;

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

    // --- Per-node interaction reads ---
    fn is_hovered(&self, node: StyleNodeId) -> bool;
    fn is_focused(&self, node: StyleNodeId) -> bool;
    fn is_focus_within(&self, node: StyleNodeId) -> bool;
    fn is_active(&self, node: StyleNodeId) -> bool;
    fn is_file_hover(&self, node: StyleNodeId) -> bool;

    /// Apply any host-owned animations for `node` on top of the resolved
    /// `combined` style. Called by the tree cascade after classes +
    /// selectors are resolved but before inherited context is merged, so
    /// animated inherited properties propagate to descendants on the same
    /// pass. Animated `display/disabled/selected` bits are OR'd back into
    /// `interact`. Returns `true` iff any animation is still active — the
    /// cascade uses that to schedule another pass. Default no-op.
    fn apply_animations(
        &mut self,
        _node: StyleNodeId,
        _combined: &mut Style,
        _interact: &mut InteractionState,
    ) -> bool {
        false
    }
}
