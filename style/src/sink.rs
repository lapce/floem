//! Inputs and the animation-backend policy hook the cascade takes at
//! call time.
//!
//! [`compute_style`](crate::StyleTree::compute_style) matches taffy's
//! shape: reads flow in through [`CascadeInputs`] (values + one closure
//! for per-node interaction state), and the only host-pluggable
//! during-cascade callback is [`AnimationBackend`] — the hook a native
//! host overrides to delegate animation ticking to the platform's
//! compositor instead of the tree's CPU ticker.
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

/// The animation policy hook.
///
/// `apply` runs during [`StyleTree::compute_style`] on every dirty node
/// after the cascade has resolved `combined`, before inherited context
/// is derived. Hosts that keep their own per-view animation registry
/// (floem today, via `ViewState.animations`) tick them here and fold
/// results into `combined`. Native-offload backends register the
/// animation with the platform's compositor on first sight and return
/// `false` afterwards — the compositor drives interpolation separately.
///
/// Returns `true` while any animation is still active. The cascade
/// records the node in its schedule set so the host brings it back
/// next frame.
///
/// Receives `&self` rather than `&mut self` because floem's impl
/// mutates through `RefCell`-backed view state. Backends that need
/// internal mutation use interior mutability (`RefCell`, `Cell`,
/// atomics for native-shared state).
///
/// The default impl is a no-op — standalone hosts and tests that
/// don't animate anything can omit it and the cascade sees no
/// animation work.
pub trait AnimationBackend {
    fn apply(
        &self,
        _node: StyleNodeId,
        _combined: &mut Style,
        _interact: &mut InteractionState,
    ) -> bool {
        false
    }
}

/// An [`AnimationBackend`] that does nothing. Handy when a host wants
/// to use tree-stored animations exclusively, or for tests that don't
/// exercise animations.
pub struct NoAnimationBackend;
impl AnimationBackend for NoAnimationBackend {}

/// Everything [`StyleTree::compute_style`](crate::StyleTree::compute_style)
/// needs from the host.
///
/// Every read is a plain value (`Copy` or `&Style`) except the per-node
/// interaction lookup, which has to call back into host state — that
/// one comes through a closure. The single active policy callback is
/// [`animations`](Self::animations); all other engine-detected facts
/// land in tree-owned state the host drains after `compute_style`
/// returns.
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
    /// Animation policy hook. Defaulting to `NoAnimationBackend` is
    /// fine — hosts that don't animate don't need to wire anything
    /// beyond that.
    pub animations: &'a dyn AnimationBackend,
}
