//! The interface the style engine uses to talk to the host.
//!
//! `StyleCx` interacts with the host (today: floem's `WindowState`) exclusively
//! through this trait. Keeping the engine's outbound surface explicit lets us
//! later generalize `StyleCx` over any sink implementor, which is how a second
//! consumer such as `floem-native` will plug into the same style engine.
//!
//! All per-element methods take [`ElementId`] so the trait carries no host-specific
//! node-id type. Implementors derive whatever internal identity they need
//! (e.g. floem's `ViewId` via `ElementId::owning_id()`).
//!
//! Most methods are currently only invoked by a host's inherent impls rather
//! than through this trait; the trait exists so a second host (`floem-native`,
//! tests, etc.) can plug into `floem_style` without hard-coding floem's
//! `WindowState`. The `#[allow(dead_code)]` on the trait suppresses
//! "unused method" warnings for trait items that floem itself doesn't
//! route through the trait yet.

use crate::element_id::ElementId;
use crate::interaction::InteractionState;
use crate::recalc::StyleReason;
use crate::responsive::ScreenSizeBp;
use crate::style::Style;
use crate::values::CursorStyle;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

#[allow(dead_code)]
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
    fn register_fixed_element(&mut self, id: ElementId);
    fn unregister_fixed_element(&mut self, id: ElementId);
    fn invalidate_focus_nav_cache(&mut self);
    fn mark_needs_cursor_resolution(&mut self);
    fn mark_needs_layout(&mut self);

    /// Override the cursor displayed over `id`. Returns the previous override,
    /// if any.
    fn set_cursor(&mut self, id: ElementId, cursor: CursorStyle) -> Option<CursorStyle>;
    /// Clear any cursor override on `id`. Returns the removed override, if
    /// one was set.
    fn clear_cursor(&mut self, id: ElementId) -> Option<CursorStyle>;

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
