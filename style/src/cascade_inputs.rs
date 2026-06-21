//! Inputs and the animation-backend policy hook the cascade takes at
//! call time.
//!
//! [`compute_style`](crate::StyleTree::compute_style) matches taffy's
//! shape: reads flow in through [`CascadeInputs`] (values + two closures —
//! one for per-node interaction state, one for the animation tick), and
//! the only host-pluggable during-cascade callback is the
//! [`animations`](CascadeInputs::animations) hook — a closure a native
//! host can point at the platform's compositor to delegate animation
//! ticking instead of the tree's CPU ticker.
//!
//! Everything else the cascade might want to tell the host — fixed-
//! element transitions, dirtied descendants, layout invalidations,
//! schedule-me-next-frame, animation lifecycle events — lands in
//! tree-owned state the host drains via `tree.take_*` calls after
//! `compute_style` returns. No mid-cascade callbacks for pure
//! notifications.

use crate::interaction::InteractionState;
use crate::responsive::ScreenSizeBp;
use crate::style::Style;
use crate::tree::StyleNodeId;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

/// The five pseudo-class input bits the cascade reads per node.
///
/// Hosts materialize one of these for each dirty node the cascade
/// visits, via the [`CascadeInputs::interactions`] closure. Splitting
/// the per-node state from global reads (dark-mode, theme, etc.) keeps
/// the hot callback narrow.
#[derive(Debug, Default, Clone, Copy)]
pub struct PerNodeInteraction {
    pub is_hovered: bool,
    pub is_focused: bool,
    pub is_focus_within: bool,
    pub is_active: bool,
    pub is_file_hover: bool,
}

/// Everything [`StyleTree::compute_style`](crate::StyleTree::compute_style)
/// needs from the host.
///
/// Every read is a plain value (`Copy` or `&Style`) except the two
/// callbacks that reach back into host state — the per-node interaction
/// lookup and the animation tick — which come through closures. The
/// animation tick ([`animations`](Self::animations)) is the only active
/// policy callback; all other engine-detected facts land in tree-owned
/// state the host drains after `compute_style` returns.
pub struct CascadeInputs<'a> {
    /// Frame-wide time snapshot. Used for transition progress.
    pub frame_start: Instant,
    /// Responsive breakpoint the root matched this frame.
    pub screen_size_bp: ScreenSizeBp,
    /// Whether the host is in keyboard-navigation mode (for
    /// `:focus-visible` resolution).
    pub keyboard_navigation: bool,
    /// Root width in logical pixels. Used by relative-unit resolution
    /// (`Pct`) that takes the viewport as its basis.
    pub root_size_width: f64,
    /// System-dark-mode flag.
    pub is_dark_mode: bool,
    /// Class map the root inherits from (floem: default theme's classes).
    pub default_theme_classes: &'a Style,
    /// Inherited-style map the root inherits from.
    pub default_theme_inherited: &'a Style,
    /// Per-node interaction state. Called for every dirty node the
    /// cascade visits; returns the five pseudo-class bits.
    pub interactions: &'a dyn Fn(StyleNodeId) -> PerNodeInteraction,
    /// Animation policy hook. Runs during
    /// [`compute_style`](crate::StyleTree::compute_style) on every dirty
    /// node after the cascade resolves `combined` and before inherited
    /// context is derived: it folds any host-driven animation for the
    /// node into `combined`/`interact` and returns `true` while that
    /// animation is still active (the cascade then reschedules the node).
    /// Hosts that keep their own per-view registry (floem, via
    /// `ViewState.animations`) tick it here; native-offload hosts hand the
    /// animation to a compositor and return `false`. Mutation through a
    /// shared `&` closure is fine via interior mutability (floem's view
    /// state is `RefCell`-backed). Hosts that don't animate pass a no-op
    /// (`&|_, _, _| false`).
    pub animations: &'a dyn Fn(StyleNodeId, &mut Style, &mut InteractionState) -> bool,
}
